use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::domain::{ExtraAction, ReapplyAction};
use crate::process::execute_output;

/// Device facts are separate from the user's strictly versioned settings.
/// The installer writes a probe result without touching user settings.
/// Resolved installation records are authoritative until the next install.
/// Only a valid unknown record may be probed and resolved after boot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeviceCapabilities {
    pub(crate) audio_hal: String,
    pub(crate) legacy_controls: bool,
    pub(crate) reason: String,
}

impl DeviceCapabilities {
    pub(crate) fn limited(hal: &str, reason: &str) -> Self {
        Self {
            audio_hal: hal.into(),
            legacy_controls: false,
            reason: reason.into(),
        }
    }

    pub(crate) fn from_evidence(policy_readable: bool, services: Option<&str>) -> Self {
        let Some(services) = services else {
            return Self::limited("unknown", "audio_service_detection_unavailable");
        };
        if services.contains("android.hardware.audio.core.IModule/") {
            return Self::limited("aidl", "aidl_legacy_controls_unavailable");
        }
        if policy_readable {
            // This proves the legacy XML path, not a particular HIDL version.
            Self {
                audio_hal: "legacy-xml".into(),
                legacy_controls: true,
                reason: String::new(),
            }
        } else {
            Self::limited("unknown", "traditional_policy_xml_unavailable")
        }
    }

    pub(crate) fn require_policy(&self) -> Result<(), String> {
        if self.legacy_controls {
            Ok(())
        } else {
            Err(format!(
                "unsupported_device_capability: policy ({})",
                self.reason
            ))
        }
    }

    pub(crate) fn allows_extra(&self, action: &ExtraAction) -> bool {
        self.legacy_controls
            || !matches!(action,
            ExtraAction::BluetoothHal { action } if action != "status")
                && !matches!(action, ExtraAction::UsbPeriodSet { .. })
    }

    pub(crate) fn allows_reapply(&self, action: &ReapplyAction) -> bool {
        match action {
            ReapplyAction::Policy(_) => self.legacy_controls,
            ReapplyAction::Extra(action) => self.allows_extra(action),
        }
    }
}

fn query(program: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args);
    let result = execute_output(&mut command, Duration::from_secs(3), 1024 * 1024).ok()?;
    if result.timed_out || result.stdout_truncated || !result.output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&result.output.stdout).into_owned())
}

fn read_record(module_dir: &Path) -> Result<DeviceCapabilities, &'static str> {
    match fs::read_to_string(module_dir.join("device-capabilities.conf")) {
        Ok(record) => parse_record(&record).map_err(|_| "invalid_device_capabilities"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err("device_capabilities_missing")
        }
        Err(_) => Err("device_capabilities_unreadable"),
    }
}

pub(crate) fn detect(module_dir: &Path) -> DeviceCapabilities {
    let caps = match read_record(module_dir) {
        Ok(caps) => caps,
        Err(reason) => return DeviceCapabilities::limited("unknown", reason),
    };
    if caps.audio_hal != "unknown" {
        return caps;
    }
    // No sleeps or recurring checks: an unresolved installation can recover
    // on boot or on the next controller request once AudioService is ready.
    let ready = query("getprop", &["sys.boot_completed"]).is_some_and(|v| v.trim() == "1")
        && query("getprop", &["init.svc.audioserver"]).is_some_and(|v| v.trim() == "running");
    if !ready {
        return caps;
    }
    // Separate from the audio mutation lock: reapply may already own that lock.
    let Ok(_lock) =
        crate::operation::acquire_operation_lock_at(&module_dir.join(".device-capabilities.lock"))
    else {
        return DeviceCapabilities::limited("unknown", "device_capabilities_retry_pending");
    };
    match read_record(module_dir) {
        Ok(current) if current.audio_hal != "unknown" => return current,
        Err(reason) => return DeviceCapabilities::limited("unknown", reason),
        _ => {}
    }
    let resolved = detect_live();
    if resolved.audio_hal == "unknown" {
        return resolved;
    }
    if crate::paths::atomic_write(
        &module_dir.join("device-capabilities.conf"),
        render_record(&resolved).as_bytes(),
        0o644,
    )
    .is_err()
    {
        return DeviceCapabilities::limited("unknown", "device_capabilities_save_failed");
    }
    resolved
}

pub(crate) fn detect_live() -> DeviceCapabilities {
    let policy = query("dumpsys", &["media.audio_policy"]);
    let readable = policy
        .as_deref()
        .and_then(|dump| {
            dump.lines()
                .find_map(|line| line.trim().strip_prefix("Config source:").map(str::trim))
        })
        .is_some_and(|path| {
            path.starts_with('/')
                && path.ends_with(".xml")
                && fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
                && fs::File::open(path).is_ok()
        });
    let services = query("service", &["list"]);
    DeviceCapabilities::from_evidence(readable, services.as_deref())
}

/// Installation record contract. Strict parsing prevents a
/// damaged restriction file from accidentally enabling controls. No shell eval.
fn parse_record(record: &str) -> Result<DeviceCapabilities, ()> {
    let mut entries = std::collections::BTreeMap::new();
    for line in record
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (key, value) = line.split_once('=').ok_or(())?;
        if !matches!(
            key,
            "version"
                | "audio_hal"
                | "policy_xml_supported"
                // Accepted for old installations, never compared or re-emitted.
                | "build_fingerprint"
                | "vendor_fingerprint"
                | "reason"
        ) || entries.insert(key, value).is_some()
        {
            return Err(());
        }
    }
    if entries.get("version") != Some(&"1") {
        return Err(());
    }
    let hal = *entries.get("audio_hal").ok_or(())?;
    if !matches!(hal, "hidl" | "legacy-xml" | "aidl" | "mixed" | "unknown") {
        return Err(());
    }
    let xml = *entries.get("policy_xml_supported").ok_or(())?;
    if !matches!(xml, "0" | "1") {
        return Err(());
    }
    if matches!(hal, "aidl" | "mixed" | "unknown") || xml == "0" {
        Ok(DeviceCapabilities::limited(
            hal,
            entries
                .get("reason")
                .copied()
                .filter(|reason| !reason.is_empty())
                .unwrap_or("device_record_restricts_legacy_controls"),
        ))
    } else {
        Ok(DeviceCapabilities {
            audio_hal: hal.into(),
            legacy_controls: true,
            reason: String::new(),
        })
    }
}

/// Read-only installer probe: no state layout, policy generation or restart.
pub(crate) fn installation_record() -> String {
    render_record(&detect_live())
}

pub(crate) fn render_record(caps: &DeviceCapabilities) -> String {
    format!(
        "version=1\naudio_hal={}\npolicy_xml_supported={}\nreason={}\n",
        caps.audio_hal,
        if caps.legacy_controls { "1" } else { "0" },
        caps.reason
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aidl_and_unknown_never_enable_legacy_controls_even_with_xml() {
        for (xml, services) in [
            (true, None),
            (false, Some("")),
            (true, Some("0 android.hardware.audio.core.IModule/default")),
        ] {
            assert!(!DeviceCapabilities::from_evidence(xml, services).legacy_controls);
        }
        assert!(
            DeviceCapabilities::from_evidence(true, Some("media.audio_policy")).legacy_controls
        );
        assert!(
            DeviceCapabilities::from_evidence(
                true,
                Some("android.hardware.bluetooth.audio.IBluetoothAudioProviderFactory/default")
            )
            .legacy_controls
        );
    }
    #[test]
    fn old_fingerprint_fields_are_accepted_but_ignored() {
        let record = "version=1\naudio_hal=hidl\npolicy_xml_supported=1\nbuild_fingerprint=build-a\nvendor_fingerprint=vendor-a\nreason=\n";
        let caps = parse_record(record).unwrap();
        assert!(caps.legacy_controls);
        assert!(!render_record(&caps).contains("fingerprint"));
    }
    #[test]
    fn records_preserve_full_and_limited_modes_and_are_strict() {
        assert!(
            !parse_record("version=1\naudio_hal=aidl\npolicy_xml_supported=1\n")
                .unwrap()
                .legacy_controls
        );
        assert!(
            parse_record("version=1\naudio_hal=hidl\npolicy_xml_supported=1\n")
                .unwrap()
                .legacy_controls
        );
        for bad in [
            "",
            "version=1\naudio_hal=aidl",
            "version=1\nversion=1\naudio_hal=hidl\npolicy_xml_supported=1",
            "version=1\naudio_hal=aidl\npolicy_xml_supported=yes",
        ] {
            assert!(parse_record(bad).is_err());
        }
    }
}
