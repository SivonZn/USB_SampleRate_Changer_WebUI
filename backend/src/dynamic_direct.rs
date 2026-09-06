//! Experimental, separate Direct PCM template. Never edits upstream templates.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::process::Command;

use roxmltree::{Document, Node};
use sha2::{Digest, Sha256};

use crate::domain::Settings;
use crate::paths::{atomic_write, state_root};

const GENERATED: &str = "/data/local/tmp/audio_conf_generated.xml";
const MARKER: &str = "<!-- usbsrctl:direct-pcm-dynamic:v1 -->";
const BT_PLACEHOLDER: &str = "dynamic_bluetooth_placeholder";
const A2DP: &[&str] = &[
    "AUDIO_DEVICE_OUT_BLUETOOTH_A2DP",
    "AUDIO_DEVICE_OUT_BLUETOOTH_A2DP_HEADPHONES",
    "AUDIO_DEVICE_OUT_BLUETOOTH_A2DP_SPEAKER",
];

pub(crate) fn generated_is_dynamic() -> bool {
    fs::read_to_string(GENERATED).is_ok_and(|text| text.contains(MARKER))
}

fn parse(text: &str) -> Result<Document<'_>, String> {
    Document::parse(text).map_err(|e| format!("invalid audio policy XML: {e}"))
}
fn unique<'a, 'i>(
    nodes: impl Iterator<Item = Node<'a, 'i>>,
    label: &str,
) -> Result<Node<'a, 'i>, String> {
    let values: Vec<_> = nodes.collect();
    if values.len() != 1 {
        return Err(format!("expected one {label}, found {}", values.len()));
    }
    Ok(values[0])
}
fn module<'a, 'i>(doc: &'a Document<'i>, name: &str) -> Result<Node<'a, 'i>, String> {
    unique(
        doc.descendants()
            .filter(|n| n.has_tag_name("module") && n.attribute("name") == Some(name)),
        name,
    )
}
fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Result<Node<'a, 'i>, String> {
    unique(node.children().filter(|n| n.has_tag_name(name)), name)
}
fn attr<'a>(node: Node<'_, 'a>, key: &str) -> Result<String, String> {
    node.attribute(key)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("missing {key} on {}", node.tag_name().name()))
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn edit(text: &str, mut edits: Vec<(Range<usize>, String)>) -> Result<String, String> {
    edits.sort_by_key(|(r, _)| r.start);
    if edits.windows(2).any(|w| w[0].0.end > w[1].0.start) {
        return Err("overlapping XML edits".into());
    }
    let mut output = text.to_owned();
    for (range, value) in edits.into_iter().rev() {
        output.replace_range(range, &value);
    }
    Ok(output)
}
fn renamed_node(text: &str, node: Node<'_, '_>, key: &str, value: &str) -> Result<String, String> {
    let attribute = node
        .attributes()
        .find(|a| a.name() == key)
        .ok_or_else(|| format!("missing {key}"))?;
    let range = node.range();
    let value_range = attribute.range_value();
    edit(
        &text[range.clone()],
        vec![(
            value_range.start - range.start..value_range.end - range.start,
            escape(value),
        )],
    )
}
fn no_includes(node: Node<'_, '_>) -> Result<(), String> {
    if node
        .descendants()
        .any(|n| n.has_tag_name(("http://www.w3.org/2001/XInclude", "include")))
    {
        return Err("Direct PCM dynamic v1 requires inline Primary/Bluetooth modules; nested XInclude is not supported".into());
    }
    Ok(())
}

/// Replace only A2DP sinks/routes and the Bluetooth module in the separate copy.
/// Other template nodes, including direct_pcm/compressed_offload, are untouched.
fn inherit(stock: &str, template: &str) -> Result<String, String> {
    let source = parse(stock)?;
    let target = parse(template)?;
    if source.root_element().attribute("version") != Some("7.0") {
        return Err("Direct PCM dynamic v1 supports HIDL policy XML version 7.0 only".into());
    }
    let sp = module(&source, "primary")?;
    let tp = module(&target, "primary")?;
    no_includes(sp)?;
    let source_ports = child(sp, "devicePorts")?;
    let target_ports = child(tp, "devicePorts")?;
    let source_mixes = child(sp, "mixPorts")?;
    let target_mixes = child(tp, "mixPorts")?;
    let mut edits = Vec::new();
    let mut inherited_aux = BTreeSet::new();
    for device_type in A2DP {
        let matches = |n: &Node<'_, '_>| {
            n.has_tag_name("devicePort")
                && n.attribute("type") == Some(*device_type)
                && n.attribute("role") == Some("sink")
        };
        let sd = unique(source_ports.children().filter(matches), device_type)?;
        let td = unique(target_ports.children().filter(matches), device_type)?;
        let formats = attr(sd, "encodedFormats")?;
        if !formats
            .split_whitespace()
            .any(|f| f.starts_with("AUDIO_FORMAT_") && f != "AUDIO_FORMAT_FORCE_AOSP")
        {
            return Err(format!("{device_type}: no system Primary offload codecs"));
        }
        let source_tag = attr(sd, "tagName")?;
        let target_tag = attr(td, "tagName")?;
        edits.push((td.range(), renamed_node(stock, sd, "tagName", &target_tag)?));
        let sr = unique(
            child(sp, "routes")?.children().filter(|n| {
                n.has_tag_name("route") && n.attribute("sink") == Some(source_tag.as_str())
            }),
            "system A2DP route",
        )?;
        let tr = unique(
            child(tp, "routes")?.children().filter(|n| {
                n.has_tag_name("route") && n.attribute("sink") == Some(target_tag.as_str())
            }),
            "template A2DP route",
        )?;
        if sr.attribute("type") != Some("mix") {
            return Err("unsupported A2DP route type".into());
        }
        let mut mapped = Vec::new();
        for name in attr(sr, "sources")?.split(',').map(str::trim) {
            let sm = unique(
                source_mixes.children().filter(|n| {
                    n.has_tag_name("mixPort")
                        && n.attribute("name") == Some(name)
                        && n.attribute("role") == Some("source")
                }),
                "system route source",
            )?;
            let exact: Vec<_> = target_mixes
                .children()
                .filter(|n| n.has_tag_name("mixPort") && n.attribute("name") == Some(name))
                .collect();
            let mapped_name = if exact.len() == 1 {
                name
            } else if name == "deep_buffer" {
                "deep buffer"
            } else {
                return Err(format!(
                    "cannot map system A2DP source {name}; original policy retained"
                ));
            };
            let tm = unique(
                target_mixes.children().filter(|n| {
                    n.has_tag_name("mixPort")
                        && n.attribute("name") == Some(mapped_name)
                        && n.attribute("role") == Some("source")
                }),
                "mapped route source",
            )?;
            let flags = |n: Node<'_, '_>| {
                n.attribute("flags")
                    .unwrap_or("")
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<BTreeSet<_>>()
            };
            if name == "voip_rx" {
                if inherited_aux.insert(name.to_owned()) {
                    edits.push((tm.range(), stock[sm.range()].to_owned()));
                }
            } else if flags(sm) != flags(tm) {
                return Err(format!(
                    "incompatible output flags for {name} -> {mapped_name}"
                ));
            }
            mapped.push(mapped_name.to_owned());
        }
        edits.push((
            tr.range(),
            format!(
                "<route type=\"mix\" sink=\"{}\" sources=\"{}\"/>",
                escape(&target_tag),
                escape(&mapped.join(","))
            ),
        ));
    }
    // Preserve vendor fallback semantics (e.g. FORCE_AOSP) and dynamic profiles.
    // No generic module-name guess or codec list is substituted here.
    let bluetooth: Vec<_> = source
        .descendants()
        .filter(|n| {
            n.has_tag_name("module")
                && *n != sp
                && n.descendants().any(|d| {
                    d.has_tag_name("devicePort")
                        && d.attribute("type").is_some_and(|t| A2DP.contains(&t))
                })
        })
        .collect();
    if bluetooth.is_empty() {
        return Err("system inline Bluetooth A2DP module not found".into());
    }
    for node in &bluetooth {
        no_includes(*node)?;
    }
    let bt = module(&target, BT_PLACEHOLDER)?;
    edits.push((
        bt.range(),
        bluetooth
            .iter()
            .map(|n| &stock[n.range()])
            .collect::<Vec<_>>()
            .join("\n"),
    ));
    let output = edit(template, edits)?;
    validate_graph(&output)?;
    Ok(format!("{output}\n{MARKER}\n"))
}

fn validate_graph(text: &str) -> Result<(), String> {
    let doc = parse(text)?;
    let mut modules = BTreeSet::new();
    for m in doc.descendants().filter(|n| n.has_tag_name("module")) {
        let name = attr(m, "name")?;
        if !modules.insert(name.clone()) {
            return Err(format!("duplicate module: {name}"));
        }
        let mut ports = BTreeMap::new();
        for p in m
            .descendants()
            .filter(|n| n.has_tag_name("mixPort") || n.has_tag_name("devicePort"))
        {
            let tag = attr(
                p,
                if p.has_tag_name("mixPort") {
                    "name"
                } else {
                    "tagName"
                },
            )?;
            if ports.insert(tag.clone(), attr(p, "role")?).is_some() {
                return Err(format!("duplicate port: {name}/{tag}"));
            }
        }
        for r in m.descendants().filter(|n| n.has_tag_name("route")) {
            let sink = attr(r, "sink")?;
            if ports.get(&sink).map(String::as_str) != Some("sink") {
                return Err(format!("invalid route sink: {name}/{sink}"));
            }
            for source in attr(r, "sources")?.split(',').map(str::trim) {
                if ports.get(source).map(String::as_str) != Some("source") {
                    return Err(format!("invalid route source: {name}/{source}"));
                }
            }
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<String, String> {
    let data =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if data.len() > 2 * 1024 * 1024 {
        return Err(format!("XML/state file too large: {}", path.display()));
    }
    Ok(data)
}
fn command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn mounted_roots(mountinfo: &str, target: &str) -> Vec<String> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.get(4).copied() == Some(target)).then(|| fields[3].to_string())
        })
        .collect()
}
fn baseline(
    target: &Path,
    roots: &[String],
    identity: &str,
    directory: &Path,
) -> Result<String, String> {
    let key = digest(&format!("{}\n{identity}", target.display()));
    let snapshot = directory.join(format!("{key}.xml"));
    if roots.is_empty() {
        let original = read(target)?;
        if original.contains(MARKER) {
            return Err("dynamic policy is not a valid system baseline; reset first".into());
        }
        parse(&original)?;
        atomic_write(&snapshot, original.as_bytes(), 0o600)?;
        atomic_write(
            &snapshot.with_extension("sha256"),
            digest(&original).as_bytes(),
            0o600,
        )?;
        return Ok(original);
    }
    if roots.len() != 1 || !roots[0].ends_with("/audio_conf_generated.xml") {
        return Err(
            "audio policy has an unrecognized overlay; reset it before using Direct PCM dynamic"
                .into(),
        );
    }
    if !snapshot.is_file() {
        return Err("no original policy snapshot for this boot/ROM; reset the existing policy before first dynamic apply".into());
    }
    let original = read(&snapshot)?;
    if read(&snapshot.with_extension("sha256"))? != digest(&original) {
        return Err("original policy snapshot checksum mismatch; reset first".into());
    }
    Ok(original)
}
fn include_path(stock: &str, target: &Path, suffix: &str) -> Result<String, String> {
    let doc = parse(stock)?;
    let includes: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name(("http://www.w3.org/2001/XInclude", "include")))
        .filter_map(|n| n.attribute("href"))
        .filter(|href| href.ends_with(suffix))
        .collect();
    if includes.len() != 1 {
        return Err(format!("expected one system {suffix} include"));
    }
    let path = target
        .parent()
        .ok_or("missing policy directory")?
        .join(includes[0]);
    let path =
        fs::canonicalize(&path).map_err(|e| format!("cannot resolve {}: {e}", path.display()))?;
    if !["/vendor/", "/odm/", "/system/", "/system_ext/", "/product/"]
        .iter()
        .any(|prefix| path.to_string_lossy().starts_with(prefix))
    {
        return Err("volume include outside system partitions".into());
    }
    Ok(path.to_string_lossy().into_owned())
}
fn render(
    template: &str,
    settings: &Settings,
    usb: &str,
    volume: &str,
    default_volume: &str,
) -> Result<String, String> {
    let format = match settings.bit_depth.as_str() {
        "16" => "AUDIO_FORMAT_PCM_16_BIT",
        "24" => "AUDIO_FORMAT_PCM_24_BIT_PACKED",
        "32" => "AUDIO_FORMAT_PCM_32_BIT",
        "float" => "AUDIO_FORMAT_PCM_FLOAT",
        _ => return Err("unsupported PCM format".into()),
    };
    let mut output = template.to_owned();
    for (key, value) in [
        ("%DRC_ENABLED%", if settings.drc { "true" } else { "false" }),
        ("%USB_MODULE%", usb),
        ("%BT_MODULE%", BT_PLACEHOLDER),
        ("%SAMPLING_RATE%", &settings.sample_rate.to_string()),
        ("%AUDIO_FORMAT%", format),
        ("%VOLUME_FILE%", volume),
        ("%DEFAULT_VOLUME_FILE%", default_volume),
    ] {
        output = output.replace(key, &escape(value));
    }
    if output.contains('%') {
        return Err("unresolved dynamic template placeholder".into());
    }
    Ok(output)
}

pub(crate) fn apply(settings: &Settings, module_dir: &Path) -> Result<(), String> {
    crate::scripts::validate_settings(settings, module_dir)?;
    if settings.policy != "offload-direct-dynamic" {
        return Err("internal dynamic entry requires dynamic policy".into());
    }
    let ns = crate::android::namespace_info();
    if ns.self_ns.is_none() || ns.self_ns != ns.audio_ns {
        return Err("dynamic mount namespace mismatch".into());
    }
    let dump = command("dumpsys", &["media.audio_policy"])?;
    let path = dump
        .lines()
        .find_map(|l| l.trim().strip_prefix("Config source: "))
        .ok_or("audio policy source unavailable")?;
    let target =
        fs::canonicalize(path).map_err(|e| format!("cannot resolve policy source: {e}"))?;
    let target_text = target.to_string_lossy();
    if !["/vendor/", "/odm/"]
        .iter()
        .any(|prefix| target_text.starts_with(prefix))
    {
        return Err("dynamic v1 requires a vendor/odm HIDL XML policy".into());
    }
    let roots = mounted_roots(&read(Path::new("/proc/self/mountinfo"))?, &target_text);
    let fingerprint = command("getprop", &["ro.build.fingerprint"])?;
    let boot = read(Path::new("/proc/sys/kernel/random/boot_id"))?;
    if fingerprint.is_empty() || boot.trim().is_empty() {
        return Err("cannot identify ROM/boot for policy snapshot".into());
    }
    let directory = state_root().join("direct-pcm-dynamic");
    let stock = baseline(
        &target,
        &roots,
        &format!("{fingerprint}\n{boot}"),
        &directory,
    )?;
    let volume = include_path(&stock, &target, "audio_policy_volumes.xml")?;
    let default_volume = include_path(&stock, &target, "default_volume_tables.xml")?;
    let exists_hal = |name: &str| {
        ["/vendor/lib64/hw", "/vendor/lib/hw"].iter().any(|dir| {
            Path::new(dir)
                .join(format!("audio.{name}.default.so"))
                .is_file()
        })
    };
    let usb = if settings.force_usbv2 || (!exists_hal("usb") && exists_hal("usbv2")) {
        "usbv2"
    } else {
        "usb"
    };
    let template = read(&module_dir.join("core/templates/offload_direct_dynamic_template.xml"))?;
    let rendered = render(&template, settings, usb, &volume, &default_volume)?;
    let generated = inherit(&stock, &rendered)?;
    let candidate = directory.join("candidate.xml");
    atomic_write(&candidate, generated.as_bytes(), 0o644)?;
    // Validate and persist the candidate before touching any live mount.
    // The shared generated path keeps the existing reset/uninstall compatible.
    let old = if roots.is_empty() {
        None
    } else {
        Some(read(&target)?)
    };
    if old.is_some() {
        command("umount", &[&target_text])?;
    }
    let install = || -> Result<(), String> {
        atomic_write(Path::new(GENERATED), generated.as_bytes(), 0o644)?;
        command("chcon", &["u:object_r:vendor_configs_file:s0", GENERATED])?;
        command("mount", &["-o", "bind", GENERATED, &target_text])?;
        Ok(())
    };
    if let Err(error) = install() {
        if let Some(old) = old {
            let restore = || -> Result<(), String> {
                atomic_write(Path::new(GENERATED), old.as_bytes(), 0o644)?;
                command("chcon", &["u:object_r:vendor_configs_file:s0", GENERATED])?;
                command("mount", &["-o", "bind", GENERATED, &target_text])?;
                Ok(())
            };
            restore()
                .map_err(|restore| format!("{error}; previous mount restore failed: {restore}"))?;
        }
        return Err(error);
    }
    println!("dynamic_direct=1\ndynamic_source={}\ndynamic_source_sha256={}\ndynamic_a2dp_ports=3\ndynamic_routes=3\ndynamic_candidate={}\ndynamic_xml_sha256={}", target.display(), digest(&stock), candidate.display(), digest(&generated));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const STOCK: &str = include_str!("../tests/fixtures/thor-stock-policy.xml");
    // Read the exact new-file payload installed by the build patch.
    fn template() -> String {
        include_str!("../../patches/0005-add-dynamic-direct-pcm-template.patch")
            .lines()
            .skip_while(|line| !line.starts_with("@@ "))
            .skip(1)
            .map(|line| format!("{}\n", line.strip_prefix('+').expect("new-file patch line")))
            .collect()
    }
    fn rendered() -> String {
        render(
            &template(),
            &Settings::default(),
            "usb",
            "/vendor/etc/audio_policy_volumes.xml",
            "/vendor/etc/default_volume_tables.xml",
        )
        .unwrap()
    }
    #[test]
    fn thor_inherits_lhdc_profiles_routes_and_vendor_fallback() {
        let generated = inherit(STOCK, &rendered()).unwrap();
        let doc = parse(&generated).unwrap();
        let primary = module(&doc, "primary").unwrap();
        for p in primary.descendants().filter(|n| {
            n.has_tag_name("devicePort") && n.attribute("type").is_some_and(|t| A2DP.contains(&t))
        }) {
            assert!(p
                .attribute("encodedFormats")
                .unwrap()
                .contains("AUDIO_FORMAT_LHDC"));
            assert_eq!(
                child(p, "profile").unwrap().attribute("format"),
                Some("AUDIO_FORMAT_PCM_16_BIT")
            );
        }
        assert!(
            generated.contains("primary output,deep buffer,direct_pcm,compressed_offload,voip_rx")
        );
        let bt = module(&doc, "bluetooth_qti").unwrap();
        assert!(generated[bt.range()].contains("AUDIO_FORMAT_FORCE_AOSP"));
        let a2dp = bt
            .descendants()
            .find(|n| n.has_tag_name("mixPort") && n.attribute("name") == Some("a2dp output"))
            .unwrap();
        assert_eq!(a2dp.children().filter(|n| n.is_element()).count(), 0);
    }
    #[test]
    fn keeps_direct_outputs_and_non_bluetooth_template_nodes_unchanged() {
        let rendered = rendered();
        let generated = inherit(STOCK, &rendered).unwrap();
        let before = parse(&rendered).unwrap();
        let after = parse(&generated).unwrap();
        for name in [
            "primary output",
            "deep buffer",
            "direct_pcm",
            "compressed_offload",
            "usb_playback",
        ] {
            let find = |d: &Document<'_>| {
                d.descendants()
                    .find(|n| n.has_tag_name("mixPort") && n.attribute("name") == Some(name))
                    .unwrap()
                    .range()
            };
            assert_eq!(&rendered[find(&before)], &generated[find(&after)]);
        }
    }
    #[test]
    fn rejects_unknown_sources_duplicate_ports_and_nested_includes() {
        assert!(inherit(
            &STOCK.replace("deep_buffer", "unknown_vendor_output"),
            &rendered()
        )
        .unwrap_err()
        .contains("cannot map"));
        let duplicate = STOCK.replace("<devicePorts>", "<devicePorts><devicePort tagName=\"duplicate\" type=\"AUDIO_DEVICE_OUT_BLUETOOTH_A2DP\" role=\"sink\"/>");
        assert!(inherit(&duplicate, &rendered()).is_err());
        let included = STOCK.replacen(
            "<mixPorts>",
            "<xi:include href=\"ports.xml\"/><mixPorts>",
            1,
        );
        assert!(inherit(&included, &rendered())
            .unwrap_err()
            .contains("XInclude"));
    }
    #[test]
    fn rejects_missing_codec_and_dangling_template_route() {
        assert!(inherit(
            &STOCK.replace("encodedFormats=", "removedFormats="),
            &rendered()
        )
        .is_err());
        assert!(inherit(
            STOCK,
            &rendered().replace(
                "sources=\"incall playback,voice call tx\"",
                "sources=\"nonexistent\""
            )
        )
        .is_err());
        assert!(inherit(
            &STOCK.replace("version=\"7.0\"", "version=\"1.0\""),
            &rendered()
        )
        .is_err());
    }
    #[test]
    fn repeated_apply_uses_snapshot_and_new_boot_cannot_reuse_it() {
        let dir = std::env::temp_dir().join(format!("direct-pcm-snapshot-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("policy.xml");
        fs::write(&target, STOCK).unwrap();
        assert_eq!(baseline(&target, &[], "boot1", &dir).unwrap(), STOCK);
        fs::write(&target, "generated override").unwrap();
        let roots = vec![GENERATED.to_string()];
        assert_eq!(baseline(&target, &roots, "boot1", &dir).unwrap(), STOCK);
        assert!(baseline(&target, &roots, "boot2", &dir).is_err());
        assert!(baseline(&target, &["/another-module.xml".into()], "boot1", &dir).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
