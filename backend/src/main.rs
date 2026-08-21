use std::env;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CONTROLLER_VERSION: &str = env!("CARGO_PKG_VERSION");
const STATE_ROOT: &str = "/data/adb/usb_samplerate_changer_webui";
const SETTINGS_VERSION: &str = "2";

const POLICIES: &[(&str, &str, &str)] = &[
    ("auto", "--auto", "自动检测"),
    ("offload", "--offload", "USB 与蓝牙硬件卸载"),
    (
        "offload-hifi-playback",
        "--offload-hifi-playback",
        "USB hifi_playback 硬件卸载",
    ),
    (
        "offload-direct",
        "--offload-direct",
        "Direct PCM / 压缩卸载",
    ),
    ("offload-safer", "--offload-safer", "较安全的 USB 硬件卸载"),
    ("bypass", "--bypass-offload", "绕过 USB 与蓝牙硬件卸载"),
    (
        "bypass-safer",
        "--bypass-offload-safer",
        "较安全地绕过 USB 与蓝牙硬件卸载",
    ),
    ("legacy", "--legacy", "旧版 A2DP HAL"),
    ("safe", "--safe", "安全兼容模式"),
    ("safest", "--safest", "最安全兼容模式"),
    ("safest-auto", "--safest-auto", "最安全并自动检测 USB 上限"),
    ("usb", "--usb-only", "仅修改 USB 音频策略"),
];

const DOCUMENTED_RATES: &[(u32, &str)] = &[
    (44_100, "44.1 kHz"),
    (48_000, "48 kHz"),
    (88_200, "88.2 kHz"),
    (96_000, "96 kHz"),
    (176_400, "176.4 kHz"),
    (192_000, "192 kHz"),
    (352_800, "352.8 kHz"),
    (384_000, "384 kHz"),
    (705_600, "705.6 kHz"),
    (768_000, "768 kHz"),
];

const BIT_DEPTHS: &[(&str, &str)] = &[
    ("16", "16-bit PCM"),
    ("24", "24-bit packed PCM"),
    ("32", "32-bit PCM"),
    ("float", "32-bit float PCM"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Settings {
    policy: String,
    sample_rate: u32,
    bit_depth: String,
    drc: bool,
    force_usbv2: bool,
    force_bluetooth_qti: bool,
    amzm: bool,
    test: bool,
    test_template: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            policy: "auto".to_string(),
            sample_rate: 44_100,
            bit_depth: "32".to_string(),
            drc: false,
            force_usbv2: false,
            force_bluetooth_qti: false,
            amzm: false,
            test: false,
            test_template: None,
        }
    }
}

#[derive(Clone, Debug)]
struct NamespaceInfo {
    self_ns: Option<String>,
    audio_ns: Option<String>,
    audio_pid: Option<u32>,
}

impl NamespaceInfo {
    fn matches(&self) -> Option<bool> {
        match (&self.self_ns, &self.audio_ns) {
            (Some(left), Some(right)) => Some(left == right),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Apply,
    Reset,
}

impl Action {
    fn script_name(self) -> &'static str {
        match self {
            Self::Apply => "apply.sh",
            Self::Reset => "reset.sh",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Reset => "reset",
        }
    }
}

struct OperationLock {
    path: PathBuf,
}

impl Drop for OperationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn main() {
    restore_default_sigpipe();
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("ERROR: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(unix)]
fn restore_default_sigpipe() {
    // Rust ignores SIGPIPE and turns a closed stdout pipe into a panic from
    // println!. CLI output is intentionally pipe-friendly (`schema | head`).
    unsafe extern "C" {
        fn signal(signal: i32, handler: usize) -> usize;
    }
    const SIGPIPE: i32 = 13;
    const SIG_DFL: usize = 0;
    unsafe {
        signal(SIGPIPE, SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_default_sigpipe() {}

fn run() -> Result<i32, String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("schema") if args.len() == 2 => {
            print_schema(&module_dir()?);
            Ok(0)
        }
        Some("status") if args.len() == 2 => {
            print_status(&module_dir()?);
            Ok(0)
        }
        Some("logs") if args.len() == 2 => {
            print_logs()?;
            Ok(0)
        }
        Some("generated") if args.len() == 2 => {
            print_generated()?;
            Ok(0)
        }
        Some("preview") => {
            let settings = parse_settings_args(&args[2..])?;
            validate_settings(&settings, &module_dir()?)?;
            print!(
                "{}",
                render_script(&settings, &module_dir()?, Action::Apply)
            );
            Ok(0)
        }
        Some("apply") => {
            let settings = parse_settings_args(&args[2..])?;
            run_operation(settings, Action::Apply)
        }
        Some("reset") if args.len() == 2 => run_operation(Settings::default(), Action::Reset),
        Some("extra") => run_extra(&args[2..]),
        _ => Err(format!(
            "usage: {} {{schema|status|logs|generated|preview OPTIONS|apply OPTIONS|reset|extra TOOL ACTION}}",
            args.first().map(String::as_str).unwrap_or("usbsrctl")
        )),
    }
}

struct ExtraCommand {
    tool: String,
    script: &'static str,
    args: Vec<String>,
    reconnect_bluetooth: bool,
}

const RESAMPLER_PRESETS: &[(&str, &[&str])] = &[
    ("default", &[]),
    ("159-480-92", &["159", "480", "92"]),
    ("165-360-104", &["--bypass", "--cheat", "165", "360", "104"]),
    ("179-408-99", &["--cheat", "179", "408", "99"]),
    ("194-520-100", &["194", "520", "100"]),
    ("ultra-hifi", &["--cheat", "194", "520", "98"]),
    ("cheap-44", &["194", "520", "92"]),
    ("cheap-44-low", &["194", "520", "91"]),
    ("cheap-48", &["194", "520", "84"]),
    ("cheap-48-low", &["194", "520", "83"]),
    ("cheap-96", &["--bypass-hires", "194", "520", "42"]),
    ("mock-dac-a", &["--bypass", "--cheat", "150", "80", "109"]),
    ("mock-dac-b", &["120", "80", "97"]),
    ("mock-dac-c", &["--bypass", "--cheat", "100", "80", "104"]),
    ("mock-mastering", &["--cheat", "159", "240", "99"]),
];

fn run_extra(args: &[String]) -> Result<i32, String> {
    let command = parse_extra_command(args)?;
    let module_dir = module_dir()?;
    let script_path = module_dir.join("extras").join(command.script);
    if !script_path.is_file() {
        return Err(format!(
            "extras script not found: {}",
            script_path.display()
        ));
    }
    ensure_state_layout()?;
    let _lock = acquire_lock()?;
    let generated_path = state_root()
        .join("generated")
        .join(format!("extra-{}.sh", command.tool));
    let generated = render_extra_script(&script_path, &command.args);
    atomic_write(&generated_path, generated.as_bytes(), 0o700)?;

    let a2dp_was_connected = command.reconnect_bluetooth && bluetooth_a2dp_connected();
    let namespace = namespace_info();
    let (route, mut output) = execute_generated(&generated_path, &namespace)?;
    let upstream_code = output.status.code().unwrap_or(1);
    let mut code = upstream_code;
    if upstream_code == 0 && a2dp_was_connected {
        output
            .stdout
            .extend_from_slice(b"bluetooth_a2dp_before=1\n");
        match restore_bluetooth_a2dp() {
            Ok(recovery) => output.stdout.extend_from_slice(
                format!("bluetooth_recovery={recovery}\nbluetooth_a2dp_after=1\n").as_bytes(),
            ),
            Err(error) => {
                code = 72;
                output.stderr.extend_from_slice(
                    format!("Bluetooth A2DP recovery failed: {error}\n").as_bytes(),
                );
            }
        }
    }
    let label = format!("extra-{}", command.tool);
    write_named_operation_log(&label, route, code, &generated_path, &output)?;
    println!("controller_action={label}");
    println!("controller_route={route}");
    println!("generated_script={}", generated_path.display());
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(code)
}

fn parse_extra_command(args: &[String]) -> Result<ExtraCommand, String> {
    let tool = args.first().map(String::as_str).ok_or_else(extra_usage)?;
    let action = args.get(1).map(String::as_str).ok_or_else(extra_usage)?;
    match tool {
        "bluetooth-hal" => {
            if args.len() != 2
                || !matches!(action, "status" | "aosp" | "legacy" | "offload" | "sysbta")
            {
                return Err(
                    "bluetooth-hal action must be status, aosp, legacy, offload, or sysbta"
                        .to_string(),
                );
            }
            Ok(ExtraCommand {
                tool: tool.to_string(),
                script: "change-bluetooth-hal.sh",
                args: vec![if action == "status" {
                    "--status"
                } else {
                    action
                }
                .to_string()],
                reconnect_bluetooth: action != "status",
            })
        }
        "resampler" => {
            if action == "custom" {
                return parse_custom_resampler(args);
            }
            if args.len() != 2 {
                return Err(
                    "resampler action must be status, reset, a preset name, or custom parameters"
                        .to_string(),
                );
            }
            let mapped = match action {
                "status" => vec!["--status".to_string()],
                "reset" => vec!["--reset".to_string()],
                preset => RESAMPLER_PRESETS
                    .iter()
                    .find(|(name, _)| *name == preset)
                    .map(|(_, values)| values.iter().map(|value| (*value).to_string()).collect())
                    .ok_or_else(|| format!("unsupported resampler preset: {preset}"))?,
            };
            Ok(ExtraCommand {
                tool: tool.to_string(),
                script: "change-resampling-quality.sh",
                args: mapped,
                reconnect_bluetooth: action != "status",
            })
        }
        "usb-period" => {
            if args.len() != 2 {
                return Err(
                    "usb-period action must be status, reset, or a period in usec".to_string(),
                );
            }
            let value = match action {
                "status" => "--status".to_string(),
                "reset" => "--reset".to_string(),
                raw => {
                    let period = raw
                        .parse::<u32>()
                        .map_err(|_| format!("invalid USB period: {raw}"))?;
                    if !(125..=50_000).contains(&period) || period % 125 != 0 {
                        return Err(
                            "USB period must be a multiple of 125 between 125 and 50000 usec"
                                .to_string(),
                        );
                    }
                    period.to_string()
                }
            };
            Ok(ExtraCommand {
                tool: tool.to_string(),
                script: "change-usb-period.sh",
                args: vec![value],
                reconnect_bluetooth: action != "status",
            })
        }
        "jitter" => parse_jitter_command(args),
        "diagnose" => parse_diagnostic_command(args),
        _ => Err(extra_usage()),
    }
}

fn parse_jitter_command(args: &[String]) -> Result<ExtraCommand, String> {
    if args.get(1).map(String::as_str) == Some("status") && args.len() == 2 {
        return Ok(ExtraCommand {
            tool: "jitter".to_string(),
            script: "jitter-reducer.sh",
            args: vec!["--status".to_string()],
            reconnect_bluetooth: false,
        });
    }
    if args.len() < 3 || !matches!(args[1].as_str(), "enable" | "disable") {
        return Err(
            "jitter usage: jitter status | jitter enable|disable FEATURE [SCHEDULER TONE]"
                .to_string(),
        );
    }
    let enable = args[1] == "enable";
    let feature = args[2].as_str();
    let mut flag = match (enable, feature) {
        (true, "all") => "--all",
        (false, "all") => "++all",
        (true, "selinux") => "--selinux",
        (false, "selinux") => "++selinux",
        (true, "thermal") => "--thermal",
        (false, "thermal") => "++thermal",
        (true, "doze") => "--doze",
        (false, "doze") => "++doze",
        (true, "governor") => "--governor",
        (false, "governor") => "++governor",
        (true, "camera") => "--camera",
        (false, "camera") => "++camera",
        (true, "logd") => "--logd",
        (false, "logd") => "++logd",
        (true, "io") => "--io",
        (false, "io") => "++io",
        (true, "vm") => "--vm",
        (false, "vm") => "++vm",
        (true, "wifi") => "--wifi",
        (false, "wifi") => "++wifi",
        (true, "battery") => "--battery",
        (false, "battery") => "++battery",
        (true, "effect") => "--effect",
        (false, "effect") => "++effect",
        _ => return Err(format!("unsupported jitter feature: {feature}")),
    };
    if feature == "wifi" && enable && args.get(3).map(String::as_str) == Some("no-restart") {
        flag = "--wifi-no-restart";
    }
    let mut mapped = vec![flag.to_string()];
    if feature == "io" && enable {
        let scheduler = args.get(3).map(String::as_str).unwrap_or("*");
        let tone = args.get(4).map(String::as_str).unwrap_or("medium");
        const SCHEDULERS: &[&str] = &[
            "*",
            "none",
            "noop",
            "deadline",
            "mq-deadline",
            "cfq",
            "bfq",
            "kyber",
        ];
        const TONES: &[&str] = &["light", "m-light", "medium", "boost", "exp"];
        if !SCHEDULERS.contains(&scheduler) || !TONES.contains(&tone) || args.len() > 5 {
            return Err("unsupported I/O scheduler or tone".to_string());
        }
        mapped.push(scheduler.to_string());
        mapped.push(tone.to_string());
    } else if !(args.len() == 3
        || (feature == "wifi" && enable && args.len() == 4 && args[3] == "no-restart"))
    {
        return Err("only enabled I/O accepts scheduler and tone arguments".to_string());
    }
    mapped.push("--status".to_string());
    Ok(ExtraCommand {
        tool: "jitter".to_string(),
        script: "jitter-reducer.sh",
        args: mapped,
        reconnect_bluetooth: false,
    })
}

fn parse_custom_resampler(args: &[String]) -> Result<ExtraCommand, String> {
    if args.len() != 7 {
        return Err("custom resampler usage: resampler custom none|48|96 cutoff|cheat STOP_DB HALF_LENGTH PERCENT".to_string());
    }
    let bypass = args[2].as_str();
    let mode = args[3].as_str();
    if !matches!(bypass, "none" | "48" | "96") || !matches!(mode, "cutoff" | "cheat") {
        return Err("invalid resampler bypass or cutoff mode".to_string());
    }
    let stop = args[4]
        .parse::<u32>()
        .map_err(|_| "invalid stop-band value".to_string())?;
    let half = args[5]
        .parse::<u32>()
        .map_err(|_| "invalid half-filter length".to_string())?;
    let percent = args[6]
        .parse::<u32>()
        .map_err(|_| "invalid cutoff/cheat percent".to_string())?;
    if !(20..=242).contains(&stop)
        || !(8..=640).contains(&half)
        || half % 8 != 0
        || (mode == "cutoff" && !(1..=100).contains(&percent))
        || (mode == "cheat" && !(1..=200).contains(&percent))
    {
        return Err("custom resampler parameters are outside upstream ranges".to_string());
    }
    let mut mapped = Vec::new();
    match bypass {
        "48" => mapped.push("--bypass".to_string()),
        "96" => mapped.push("--bypass-hires".to_string()),
        _ => {}
    }
    if mode == "cheat" {
        mapped.push("--cheat".to_string());
    }
    mapped.extend([stop.to_string(), half.to_string(), percent.to_string()]);
    Ok(ExtraCommand {
        tool: "resampler".to_string(),
        script: "change-resampling-quality.sh",
        args: mapped,
        reconnect_bluetooth: true,
    })
}

fn parse_diagnostic_command(args: &[String]) -> Result<ExtraCommand, String> {
    if !(2..=3).contains(&args.len()) {
        return Err("diagnose usage: diagnose audio|bluetooth|config|alsa [all]".to_string());
    }
    let script = match args[1].as_str() {
        "audio" => "dumpsys-filtered.sh",
        "bluetooth" => "dumpsys-bluetooth-filtered.sh",
        "config" => "getConfig.sh",
        "alsa" => "alsa-hw-params.sh",
        other => return Err(format!("unsupported diagnostic: {other}")),
    };
    let mapped = if args.get(2).map(String::as_str) == Some("all") {
        vec!["--all".to_string()]
    } else if args.len() == 2 {
        Vec::new()
    } else {
        return Err("diagnostic detail must be all".to_string());
    };
    Ok(ExtraCommand {
        tool: format!("diagnose-{}", args[1]),
        script,
        args: mapped,
        reconnect_bluetooth: false,
    })
}

fn extra_usage() -> String {
    "extra tool must be bluetooth-hal, resampler, usb-period, jitter, or diagnose".to_string()
}

fn render_extra_script(script: &Path, args: &[String]) -> String {
    let command = std::iter::once("/system/bin/sh".to_string())
        .chain(std::iter::once(script.to_string_lossy().into_owned()))
        .chain(args.iter().cloned())
        .map(|argument| shell_quote(&argument))
        .collect::<Vec<_>>()
        .join(" ");
    format!("#!/system/bin/sh\nset -u\nexec {command}\n")
}

fn module_dir() -> Result<PathBuf, String> {
    let executable =
        env::current_exe().map_err(|error| format!("cannot resolve executable: {error}"))?;
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "cannot resolve module directory".to_string())
}

fn state_root() -> PathBuf {
    PathBuf::from(STATE_ROOT)
}

fn ensure_state_layout() -> Result<(), String> {
    let root = state_root();
    let generated = root.join("generated");
    fs::create_dir_all(&generated)
        .map_err(|error| format!("cannot create state directory: {error}"))?;
    set_mode(&root, 0o700)?;
    set_mode(&generated, 0o700)?;
    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| format!("cannot set permissions on {}: {error}", path.display()))
}

fn parse_settings_args(args: &[String]) -> Result<Settings, String> {
    let mut settings = Settings::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--policy" => {
                settings.policy = take_value(args, &mut index, "--policy")?.to_string();
            }
            "--sample-rate" => {
                let value = take_value(args, &mut index, "--sample-rate")?;
                settings.sample_rate = normalize_sample_rate(value)?;
            }
            "--bit-depth" => {
                settings.bit_depth = take_value(args, &mut index, "--bit-depth")?.to_string();
            }
            "--drc" => settings.drc = true,
            "--force-usbv2" => settings.force_usbv2 = true,
            "--force-bluetooth-qti" => settings.force_bluetooth_qti = true,
            "--amzm" => settings.amzm = true,
            "--test" => settings.test = true,
            "--test-template" => {
                settings.test_template =
                    Some(take_value(args, &mut index, "--test-template")?.to_string());
            }
            unknown => return Err(format!("unknown controller option: {unknown}")),
        }
        index += 1;
    }
    Ok(settings)
}

fn take_value<'a>(args: &'a [String], index: &mut usize, option: &str) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}

fn normalize_sample_rate(value: &str) -> Result<u32, String> {
    let normalized = match value {
        "44k" | "44.1k" => 44_100,
        "48k" => 48_000,
        "88k" | "88.2k" => 88_200,
        "96k" => 96_000,
        "176k" | "176.4k" => 176_400,
        "192k" => 192_000,
        "352k" | "353k" | "352.8k" => 352_800,
        "384k" => 384_000,
        "705k" | "706k" | "705.6k" => 705_600,
        "768k" => 768_000,
        other => other
            .parse::<u32>()
            .map_err(|_| format!("invalid sample rate: {other}"))?,
    };
    if !(44_100..=768_000).contains(&normalized) {
        return Err(format!(
            "sample rate must be between 44100 and 768000 Hz: {normalized}"
        ));
    }
    Ok(normalized)
}

fn validate_settings(settings: &Settings, module_dir: &Path) -> Result<(), String> {
    if policy_flag(&settings.policy).is_none() {
        return Err(format!("unsupported policy mode: {}", settings.policy));
    }
    normalize_sample_rate(&settings.sample_rate.to_string())?;
    if !BIT_DEPTHS
        .iter()
        .any(|(value, _)| *value == settings.bit_depth)
    {
        return Err(format!("unsupported bit depth: {}", settings.bit_depth));
    }
    if settings.amzm && settings.test {
        return Err("--amzm and --test are mutually exclusive in the WebUI".to_string());
    }
    match (&settings.test, &settings.test_template) {
        (true, Some(template)) => validate_template(template, module_dir)?,
        (true, None) => {
            return Err(
                "--test requires a template because the upstream default is absent".to_string(),
            )
        }
        (false, Some(_)) => return Err("--test-template requires --test".to_string()),
        (false, None) => {}
    }
    let script = module_dir.join("USB_SampleRate_Changer.sh");
    if !script.is_file() {
        return Err(format!("upstream script not found: {}", script.display()));
    }
    Ok(())
}

fn validate_template(template: &str, module_dir: &Path) -> Result<(), String> {
    if template.is_empty()
        || template.starts_with('/')
        || template
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !template.ends_with(".xml")
        || !template
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./-".contains(&byte))
    {
        return Err(format!("unsafe template path: {template}"));
    }
    let path = module_dir.join("templates").join(template);
    if !path.is_file() {
        return Err(format!("template does not exist: {template}"));
    }
    Ok(())
}

fn policy_flag(policy: &str) -> Option<&'static str> {
    POLICIES
        .iter()
        .find(|(value, _, _)| *value == policy)
        .map(|(_, flag, _)| *flag)
}

fn upstream_args(settings: &Settings, action: Action) -> Vec<String> {
    if matches!(action, Action::Reset) {
        return vec!["--reset".to_string()];
    }

    let mut args = vec![policy_flag(&settings.policy)
        .unwrap_or("--auto")
        .to_string()];
    if settings.drc {
        args.push("--drc".to_string());
    }
    if settings.force_usbv2 {
        args.push("--force-usbv2".to_string());
    }
    if settings.force_bluetooth_qti {
        args.push("--force-bluetooth-qti".to_string());
    }
    if settings.amzm {
        args.push("--amzm".to_string());
    }
    if settings.test {
        args.push("--test".to_string());
        if let Some(template) = &settings.test_template {
            args.push("--test-template".to_string());
            args.push(template.clone());
        }
    }
    args.push(settings.sample_rate.to_string());
    args.push(settings.bit_depth.clone());
    args
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn render_script(settings: &Settings, module_dir: &Path, action: Action) -> String {
    let script = module_dir.join("USB_SampleRate_Changer.sh");
    let command = std::iter::once("/system/bin/sh".to_string())
        .chain(std::iter::once(script.to_string_lossy().into_owned()))
        .chain(upstream_args(settings, action))
        .map(|argument| shell_quote(&argument))
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        "#!/system/bin/sh\n\
set -u\n\
audio_pid=\"$(pidof audioserver 2>/dev/null)\"\n\
audio_pid=\"${{audio_pid%% *}}\"\n\
if [ -z \"$audio_pid\" ]; then\n\
    audio_pid=\"$(getprop init.svc_debug_pid.audioserver 2>/dev/null)\"\n\
fi\n\
[ -n \"$audio_pid\" ] || {{ echo \"audioserver is not running\" >&2; exit 70; }}\n\
self_ns=\"$(readlink /proc/self/ns/mnt 2>/dev/null)\"\n\
audio_ns=\"$(readlink \"/proc/$audio_pid/ns/mnt\" 2>/dev/null)\"\n\
if [ -z \"$self_ns\" ] || [ -z \"$audio_ns\" ] || [ \"$self_ns\" != \"$audio_ns\" ]; then\n\
    echo \"mount namespace mismatch: self=$self_ns audio=$audio_ns\" >&2\n\
    exit 71\n\
fi\n\
echo \"namespace verified: $self_ns\"\n\
exec {command}\n"
    )
}

fn run_operation(settings: Settings, action: Action) -> Result<i32, String> {
    let module_dir = module_dir()?;
    validate_settings_for_action(&settings, &module_dir, action)?;
    ensure_state_layout()?;
    let _lock = acquire_lock()?;

    let generated_path = state_root().join("generated").join(action.script_name());
    let generated = render_script(&settings, &module_dir, action);
    atomic_write(&generated_path, generated.as_bytes(), 0o700)?;

    let a2dp_was_connected = bluetooth_a2dp_connected();

    let namespace = namespace_info();
    let (route, mut output) = execute_generated(&generated_path, &namespace)?;
    let upstream_code = output.status.code().unwrap_or(1);
    let mut code = upstream_code;

    if upstream_code == 0 && a2dp_was_connected {
        output
            .stdout
            .extend_from_slice(b"bluetooth_a2dp_before=1\n");
        match restore_bluetooth_a2dp() {
            Ok(route) => {
                output.stdout.extend_from_slice(
                    format!("bluetooth_recovery={route}\nbluetooth_a2dp_after=1\n").as_bytes(),
                );
            }
            Err(error) => {
                code = 72;
                output.stderr.extend_from_slice(
                    format!("Bluetooth A2DP recovery failed: {error}\n").as_bytes(),
                );
            }
        }
    }

    write_operation_log(action, route, code, &generated_path, &output)?;
    if upstream_code == 0 {
        match action {
            Action::Apply => save_settings(&settings)?,
            Action::Reset => {
                let settings_path = state_root().join("settings.conf");
                if settings_path.exists() {
                    fs::remove_file(&settings_path)
                        .map_err(|error| format!("cannot clear saved settings: {error}"))?;
                }
            }
        }
    }

    println!("controller_action={}", action.label());
    println!("controller_route={route}");
    println!("generated_script={}", generated_path.display());
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(code)
}

fn bluetooth_a2dp_connected() -> bool {
    let Ok(output) = Command::new("dumpsys").arg("audio").output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    bluetooth_a2dp_connected_in_dump(&String::from_utf8_lossy(&output.stdout))
}

fn bluetooth_a2dp_connected_in_dump(dump: &str) -> bool {
    let Some((_, connected_and_rest)) = dump.split_once("Connected devices:") else {
        return false;
    };
    let connected = connected_and_rest
        .split_once("APM Connected device")
        .map(|(section, _)| section)
        .unwrap_or(connected_and_rest);
    if !connected.contains("(bt_a2dp)") {
        return false;
    }

    let Some((_, music_and_rest)) = dump.split_once("- STREAM_MUSIC:") else {
        return false;
    };
    let music = music_and_rest
        .split_once("\n- STREAM_")
        .map(|(section, _)| section)
        .unwrap_or(music_and_rest);
    music
        .lines()
        .any(|line| line.trim_start().starts_with("Devices:") && line.contains("bt_a2dp"))
}

fn wait_for_bluetooth_a2dp(attempts: usize, delay: Duration) -> bool {
    for _ in 0..attempts {
        if bluetooth_a2dp_connected() {
            return true;
        }
        std::thread::sleep(delay);
    }
    false
}

fn restore_bluetooth_a2dp() -> Result<&'static str, String> {
    // AudioService can retain the old device list briefly after audioserver has
    // restarted. Let the new policy instance settle before deciding that A2DP
    // recovered without intervention.
    std::thread::sleep(Duration::from_secs(3));
    if wait_for_bluetooth_a2dp(8, Duration::from_millis(500)) {
        return Ok("automatic");
    }
    Err(
        "audio policy was applied, but the connected headset must be explicitly disconnected and reconnected before STREAM_MUSIC returns to A2DP"
            .to_string(),
    )
}

fn validate_settings_for_action(
    settings: &Settings,
    module_dir: &Path,
    action: Action,
) -> Result<(), String> {
    match action {
        Action::Apply => validate_settings(settings, module_dir),
        Action::Reset => {
            let script = module_dir.join("USB_SampleRate_Changer.sh");
            if script.is_file() {
                Ok(())
            } else {
                Err(format!("upstream script not found: {}", script.display()))
            }
        }
    }
}

fn acquire_lock() -> Result<OperationLock, String> {
    let path = state_root().join("operation.lock");
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            let _ = writeln!(file, "{}", std::process::id());
            Ok(OperationLock { path })
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let stale = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
                .map(|elapsed| elapsed > Duration::from_secs(180))
                .unwrap_or(false);
            if stale {
                fs::remove_file(&path).map_err(|remove_error| {
                    format!("cannot remove stale operation lock: {remove_error}")
                })?;
                acquire_lock()
            } else {
                Err("another audio operation is still running".to_string())
            }
        }
        Err(error) => Err(format!("cannot acquire operation lock: {error}")),
    }
}

fn execute_generated(
    path: &Path,
    namespace: &NamespaceInfo,
) -> Result<(&'static str, Output), String> {
    if namespace.matches() == Some(true) {
        let output = Command::new("/system/bin/sh")
            .arg(path)
            .output()
            .map_err(|error| format!("cannot execute generated script: {error}"))?;
        return Ok(("current-namespace", output));
    }

    let command = format!("/system/bin/sh {}", shell_quote(&path.to_string_lossy()));
    let output = Command::new("su")
        .args(["--mount-master", "-c", command.as_str()])
        .output()
        .map_err(|error| format!("cannot execute mount-master shell: {error}"))?;
    Ok(("mount-master", output))
}

fn atomic_write(path: &Path, content: &[u8], mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid output path: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let temp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name().and_then(OsStr::to_str).unwrap_or("output"),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temp)
        .map_err(|error| format!("cannot create {}: {error}", temp.display()))?;
    if let Err(error) = file.write_all(content).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(format!("cannot write {}: {error}", temp.display()));
    }
    set_mode(&temp, mode)?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("cannot replace {}: {error}", path.display())
    })
}

fn save_settings(settings: &Settings) -> Result<(), String> {
    let content = format!(
        "version={SETTINGS_VERSION}\npolicy={}\nsample_rate={}\nbit_depth={}\ndrc={}\nforce_usbv2={}\nforce_bluetooth_qti={}\namzm={}\ntest={}\ntest_template={}\n",
        settings.policy,
        settings.sample_rate,
        settings.bit_depth,
        bool_number(settings.drc),
        bool_number(settings.force_usbv2),
        bool_number(settings.force_bluetooth_qti),
        bool_number(settings.amzm),
        bool_number(settings.test),
        settings.test_template.as_deref().unwrap_or("")
    );
    atomic_write(
        &state_root().join("settings.conf"),
        content.as_bytes(),
        0o600,
    )
}

fn load_settings() -> Settings {
    let path = state_root().join("settings.conf");
    let Ok(content) = fs::read_to_string(path) else {
        return Settings::default();
    };
    let mut settings = Settings::default();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "policy" => settings.policy = value.to_string(),
            "sample_rate" => {
                if let Ok(rate) = normalize_sample_rate(value) {
                    settings.sample_rate = rate;
                }
            }
            "bit_depth" => settings.bit_depth = value.to_string(),
            "drc" => settings.drc = value == "1",
            "force_usbv2" => settings.force_usbv2 = value == "1",
            "force_bluetooth_qti" => settings.force_bluetooth_qti = value == "1",
            "amzm" => settings.amzm = value == "1",
            "test" => settings.test = value == "1",
            "test_template" if !value.is_empty() => {
                settings.test_template = Some(value.to_string())
            }
            _ => {}
        }
    }
    settings
}

fn bool_number(value: bool) -> u8 {
    if value {
        1
    } else {
        0
    }
}

fn write_operation_log(
    action: Action,
    route: &str,
    code: i32,
    generated_path: &Path,
    output: &Output,
) -> Result<(), String> {
    write_named_operation_log(action.label(), route, code, generated_path, output)
}

fn write_named_operation_log(
    action: &str,
    route: &str,
    code: i32,
    generated_path: &Path,
    output: &Output,
) -> Result<(), String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let status = format!(
        "last_action={}\nlast_route={route}\nlast_exit={code}\nlast_time={timestamp}\n",
        action
    );
    atomic_write(&state_root().join("last.status"), status.as_bytes(), 0o600)?;

    let log = format!(
        "time={timestamp}\naction={}\nroute={route}\nexit={code}\ngenerated={}\n\n[stdout]\n{}\n\n[stderr]\n{}\n",
        action,
        generated_path.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    atomic_write(&state_root().join("last.log"), log.as_bytes(), 0o600)
}

fn print_status(module_dir: &Path) {
    let settings = load_settings();
    let namespace = namespace_info();
    println!("controller_version={CONTROLLER_VERSION}");
    println!("script_version={}", upstream_script_version(module_dir));
    println!("module_dir={}", module_dir.display());
    print_settings(&settings);
    println!(
        "audioserver_pid={}",
        namespace
            .audio_pid
            .map(|pid| pid.to_string())
            .unwrap_or_default()
    );
    println!("self_ns={}", namespace.self_ns.as_deref().unwrap_or(""));
    println!("audio_ns={}", namespace.audio_ns.as_deref().unwrap_or(""));
    println!(
        "namespace_ok={}",
        match namespace.matches() {
            Some(true) => "1",
            Some(false) => "0",
            None => "unknown",
        }
    );
    if let Ok(last_status) = fs::read_to_string(state_root().join("last.status")) {
        print!("{last_status}");
    }
    print_templates(module_dir);
}

fn upstream_script_version(module_dir: &Path) -> String {
    fs::read_to_string(module_dir.join("USB_SampleRate_Changer.sh"))
        .ok()
        .and_then(|source| {
            source.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("# Version:")
                    .map(str::trim)
                    .filter(|version| !version.is_empty())
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn print_settings(settings: &Settings) {
    println!("policy={}", settings.policy);
    println!("sample_rate={}", settings.sample_rate);
    println!("bit_depth={}", settings.bit_depth);
    println!("drc={}", bool_number(settings.drc));
    println!("force_usbv2={}", bool_number(settings.force_usbv2));
    println!(
        "force_bluetooth_qti={}",
        bool_number(settings.force_bluetooth_qti)
    );
    println!(
        "bluetooth_a2dp_connected={}",
        bool_number(bluetooth_a2dp_connected())
    );
    println!("amzm={}", bool_number(settings.amzm));
    println!("test={}", bool_number(settings.test));
    println!(
        "test_template={}",
        settings.test_template.as_deref().unwrap_or("")
    );
}

fn print_schema(module_dir: &Path) {
    println!("schema_version=1");
    println!("custom_rate_min=44100");
    println!("custom_rate_max=768000");
    println!("policies_begin");
    for (value, flag, label) in POLICIES {
        println!("policy={value}|{flag}|{label}");
    }
    println!("policies_end");
    println!("rates_begin");
    for (value, label) in DOCUMENTED_RATES {
        println!("rate={value}|{label}");
    }
    println!("rates_end");
    println!("bit_depths_begin");
    for (value, label) in BIT_DEPTHS {
        println!("bit_depth={value}|{label}");
    }
    println!("bit_depths_end");
    println!("switch=drc|--drc");
    println!("switch=force_usbv2|--force-usbv2");
    println!("switch=force_bluetooth_qti|--force-bluetooth-qti");
    println!("switch=amzm|--amzm");
    println!("switch=test|--test");
    println!("action=reset|--reset");
    print_templates(module_dir);
}

fn print_templates(module_dir: &Path) {
    println!("templates_begin");
    for template in collect_templates(&module_dir.join("templates")) {
        println!("template={template}");
    }
    println!("templates_end");
}

fn collect_templates(root: &Path) -> Vec<String> {
    fn walk(root: &Path, current: &Path, output: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                walk(root, &path, output);
            } else if file_type.is_file() && path.extension() == Some(OsStr::new("xml")) {
                if let Ok(relative) = path.strip_prefix(root) {
                    output.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }

    let mut templates = Vec::new();
    walk(root, root, &mut templates);
    templates.sort();
    templates
}

fn namespace_info() -> NamespaceInfo {
    let audio_pid = audioserver_pid();
    NamespaceInfo {
        self_ns: read_namespace_link(Path::new("/proc/self/ns/mnt")),
        audio_ns: audio_pid
            .and_then(|pid| read_namespace_link(Path::new(&format!("/proc/{pid}/ns/mnt")))),
        audio_pid,
    }
}

fn read_namespace_link(path: &Path) -> Option<String> {
    fs::read_link(path)
        .ok()
        .map(|value| value.to_string_lossy().into_owned())
}

fn audioserver_pid() -> Option<u32> {
    let from_pidof = Command::new("pidof")
        .arg("audioserver")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
                .and_then(|pid| pid.parse().ok())
        });
    if from_pidof.is_some() {
        return from_pidof;
    }
    Command::new("getprop")
        .arg("init.svc_debug_pid.audioserver")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8_lossy(&output.stdout).trim().parse().ok())
}

fn print_logs() -> Result<(), String> {
    let path = state_root().join("last.log");
    if !path.exists() {
        println!("No operation has been recorded yet.");
        return Ok(());
    }
    let mut file = File::open(&path).map_err(|error| format!("cannot open log: {error}"))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|error| format!("cannot read log: {error}"))?;
    print!("{content}");
    Ok(())
}

fn print_generated() -> Result<(), String> {
    let path = state_root().join("generated").join("apply.sh");
    if !path.exists() {
        return Err("no generated apply script exists".to_string());
    }
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read generated script: {error}"))?;
    print!("{content}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    #[test]
    fn normalizes_every_documented_alias_family() {
        assert_eq!(normalize_sample_rate("44.1k").unwrap(), 44_100);
        assert_eq!(normalize_sample_rate("353k").unwrap(), 352_800);
        assert_eq!(normalize_sample_rate("706k").unwrap(), 705_600);
        assert_eq!(normalize_sample_rate("123456").unwrap(), 123_456);
        assert!(normalize_sample_rate("44099").is_err());
        assert!(normalize_sample_rate("768001").is_err());
    }

    #[test]
    fn parses_complete_controller_configuration() {
        let args = [
            "--policy",
            "offload-direct",
            "--sample-rate",
            "96k",
            "--bit-depth",
            "float",
            "--drc",
            "--force-usbv2",
            "--force-bluetooth-qti",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        let parsed = parse_settings_args(&args).unwrap();
        assert_eq!(parsed.policy, "offload-direct");
        assert_eq!(parsed.sample_rate, 96_000);
        assert_eq!(parsed.bit_depth, "float");
        assert!(parsed.drc && parsed.force_usbv2 && parsed.force_bluetooth_qti);
    }

    #[test]
    fn renders_all_upstream_flags_in_a_guarded_script() {
        let settings = Settings {
            policy: "offload-direct".to_string(),
            sample_rate: 96_000,
            bit_depth: "24".to_string(),
            drc: true,
            force_usbv2: true,
            force_bluetooth_qti: true,
            amzm: false,
            test: false,
            test_template: None,
        };
        let script = render_script(&settings, &module_fixture(), Action::Apply);
        assert!(script.contains("readlink /proc/self/ns/mnt"));
        assert!(script.contains("--offload-direct"));
        assert!(script.contains("--drc"));
        assert!(script.contains("--force-usbv2"));
        assert!(script.contains("--force-bluetooth-qti"));
        assert!(script.contains("'96000' '24'"));
    }

    #[test]
    fn rejects_conflicting_template_modes() {
        let settings = Settings {
            amzm: true,
            test: true,
            test_template: Some("offload_template.xml".to_string()),
            ..Settings::default()
        };
        assert!(validate_settings(&settings, &module_fixture()).is_err());
    }

    #[test]
    fn quotes_single_quotes_for_shell() {
        assert_eq!(shell_quote("a'b"), "'a'\"'\"'b'");
    }

    #[test]
    fn reset_has_no_configuration_arguments() {
        assert_eq!(
            upstream_args(&Settings::default(), Action::Reset),
            ["--reset"]
        );
    }

    #[test]
    fn detects_a2dp_only_inside_connected_device_section() {
        let connected = "volume bt_a2dp\n- STREAM_MUSIC:\n  Devices: bt_a2dp(80)\n- STREAM_ALARM:\nConnected devices:\n  [DeviceInfo: type:0x80 (bt_a2dp) name:Headphones]\nAPM Connected device (A2DP sink only):\n";
        let disconnected = "volume bt_a2dp\n- STREAM_MUSIC:\n  Devices: speaker(2)\n- STREAM_ALARM:\nConnected devices:\n  [DeviceInfo: type:0x10 (bt_sco) name:Headphones]\nAPM Connected device (A2DP sink only):\n";
        let connected_but_unrouted = "- STREAM_MUSIC:\n  Devices: speaker(2)\n- STREAM_ALARM:\nConnected devices:\n  [DeviceInfo: type:0x80 (bt_a2dp) name:Headphones]\nAPM Connected device (A2DP sink only):\n";
        assert!(bluetooth_a2dp_connected_in_dump(connected));
        assert!(!bluetooth_a2dp_connected_in_dump(disconnected));
        assert!(!bluetooth_a2dp_connected_in_dump(connected_but_unrouted));
    }

    #[test]
    fn extra_commands_are_strictly_whitelisted() {
        let bluetooth = ["bluetooth-hal", "offload"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let parsed = parse_extra_command(&bluetooth).unwrap();
        assert_eq!(parsed.script, "change-bluetooth-hal.sh");
        assert_eq!(parsed.args, ["offload"]);

        let injection = ["bluetooth-hal", "offload;id"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(parse_extra_command(&injection).is_err());
    }

    #[test]
    fn validates_usb_period_and_jitter_parameters() {
        let valid = ["usb-period", "2250"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(parse_extra_command(&valid).is_ok());
        let invalid = ["usb-period", "2251"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(parse_extra_command(&invalid).is_err());

        let io = ["jitter", "enable", "io", "*", "boost"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            parse_extra_command(&io).unwrap().args,
            ["--io", "*", "boost", "--status"]
        );

        let custom = ["resampler", "custom", "96", "cheat", "194", "520", "98"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            parse_extra_command(&custom).unwrap().args,
            ["--bypass-hires", "--cheat", "194", "520", "98"]
        );

        let wifi = ["jitter", "enable", "wifi", "no-restart"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            parse_extra_command(&wifi).unwrap().args,
            ["--wifi-no-restart", "--status"]
        );
    }
}
