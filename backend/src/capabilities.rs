use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::domain::{ExtraAction, ReapplyAction};
use crate::process::execute_output;

/// Device facts are separate from the user's strictly versioned settings.
/// The installer writes a probe result without touching user settings.
/// Missing records use live detection; a record can restrict, never grant,
/// access to the legacy XML/HAL controls.
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

pub(crate) fn detect(module_dir: &Path) -> DeviceCapabilities {
    match fs::read_to_string(module_dir.join("device-capabilities.conf")) {
        Ok(record) => match record_restriction(&record) {
            Ok(Some(caps)) if caps.audio_hal != "unknown" && record_is_current(&record) => {
                return caps
            }
            Ok(Some(_)) => {} // Recovery/early-boot probes and OTA records are advisory.
            Ok(None) => {}    // Full mode must still be established from live evidence.
            Err(()) => {
                return DeviceCapabilities::limited("unknown", "invalid_device_capabilities")
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return DeviceCapabilities::limited("unknown", "device_capabilities_unreadable"),
    }
    detect_live()
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
fn record_restriction(record: &str) -> Result<Option<DeviceCapabilities>, ()> {
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
        Ok(Some(DeviceCapabilities::limited(
            hal,
            "device_record_restricts_legacy_controls",
        )))
    } else {
        Ok(None)
    }
}

/// Read-only installer probe: no state layout, policy generation or restart.
pub(crate) fn installation_record() -> String {
    let caps = detect_live();
    let build = query("getprop", &["ro.build.fingerprint"]).unwrap_or_default();
    let vendor = query("getprop", &["ro.vendor.build.fingerprint"]).unwrap_or_default();
    format!("version=1\naudio_hal={}\npolicy_xml_supported={}\nreason={}\nbuild_fingerprint={}\nvendor_fingerprint={}\n",
        caps.audio_hal, if caps.legacy_controls { "1" } else { "0" }, caps.reason,
        build.replace(['\n', '\r'], ""), vendor.replace(['\n', '\r'], ""))
}

fn record_is_current(record: &str) -> bool {
    record_matches_fingerprints(record, |property| query("getprop", &[property]))
}

fn record_matches_fingerprints(record: &str, mut read: impl FnMut(&str) -> Option<String>) -> bool {
    for (key, property) in [
        ("build_fingerprint", "ro.build.fingerprint"),
        ("vendor_fingerprint", "ro.vendor.build.fingerprint"),
    ] {
        let saved = record
            .lines()
            .map(str::trim)
            .filter_map(|line| line.split_once('='))
            .find_map(|(name, value)| (name == key).then_some(value));
        if let Some(saved) = saved.filter(|value| !value.is_empty()) {
            if !read(property).is_some_and(|current| current.trim() == saved) {
                return false;
            }
        }
    }
    true
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
    fn ota_and_unavailable_fingerprints_require_a_live_probe() {
        let record = "version=1\naudio_hal=aidl\npolicy_xml_supported=0\nbuild_fingerprint=build-a\nvendor_fingerprint=vendor-a\nreason=aidl\n";
        assert!(record_restriction(record).unwrap().is_some());
        assert!(record_matches_fingerprints(record, |name| Some(
            if name == "ro.build.fingerprint" {
                "build-a\n"
            } else {
                "vendor-a\n"
            }
            .into()
        )));
        assert!(!record_matches_fingerprints(record, |_| Some(
            "updated".into()
        )));
        assert!(!record_matches_fingerprints(record, |_| None));
    }
    #[test]
    fn records_only_restrict_and_are_strict() {
        assert!(
            record_restriction("version=1\naudio_hal=aidl\npolicy_xml_supported=1\n")
                .unwrap()
                .is_some()
        );
        assert!(
            record_restriction("version=1\naudio_hal=hidl\npolicy_xml_supported=1\n")
                .unwrap()
                .is_none()
        );
        for bad in [
            "",
            "version=1\naudio_hal=aidl",
            "version=1\nversion=1\naudio_hal=hidl\npolicy_xml_supported=1",
            "version=1\naudio_hal=aidl\npolicy_xml_supported=yes",
        ] {
            assert!(record_restriction(bad).is_err());
        }
    }
}
