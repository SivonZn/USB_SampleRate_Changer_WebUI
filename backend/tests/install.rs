use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn installer_accepts_aidl_recovery_and_upgrades_and_writes_runtime_readable_records() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = Fixture(
        root.join("target")
            .join(format!(".install-test-{}", std::process::id())),
    );
    let path = &fixture.0;
    for directory in ["", "bin", "core/templates", "core/extras", "webroot"] {
        fs::create_dir_all(path.join(directory)).unwrap();
    }
    for name in ["core/USB_SampleRate_Changer.sh", "core/functions3.shlib"] {
        fs::write(path.join(name), "").unwrap();
    }
    fs::write(path.join("module.prop"), "version=1.2.2\n").unwrap();
    fs::write(
        path.join("policy.xml"),
        "<audioPolicyConfiguration version=\"7.0\"/>",
    )
    .unwrap();
    fs::copy(
        root.join("../module/customize.sh"),
        path.join("customize.sh"),
    )
    .unwrap();
    fs::copy(env!("CARGO_BIN_EXE_usbsrctl"), path.join("usbsrctl")).unwrap();
    for (name, source) in [
        ("dumpsys", "#!/bin/sh\n[ \"$TEST_MODE\" = unavailable ] && exit 1\nprintf '  Config source: %s\\n' \"$MODPATH/policy.xml\"\n"),
        ("service", "#!/bin/sh\ncase \"$TEST_MODE\" in aidl) echo '0 android.hardware.audio.core.IModule/default';; full) echo '0 media.audio_policy';; *) exit 1;; esac\n"),
        ("getprop", "#!/bin/sh\ncase \"$1\" in sys.boot_completed) echo 1;; init.svc.audioserver) echo running;; *) exit 1;; esac\n"),
        // Keep host tests away from all absolute Android migration removals.
        ("rm", "#!/bin/sh\nexit 0\n"),
    ] {
        let file = path.join("bin").join(name);
        fs::write(&file, source).unwrap();
        fs::set_permissions(file, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let run = |mode: &str| {
        Command::new("/bin/sh").arg("-c").arg(
            "ui_print() { printf '%s\\n' \"$*\"; }; abort() { echo \"$*\" >&2; exit 1; }; set_perm() { chmod \"$4\" \"$1\"; }; set_perm_recursive() { :; }; . \"$MODPATH/customize.sh\""
        ).env("MODPATH", path).env("TEST_MODE", mode).env("TEST_FINGERPRINT", "build-a")
         .env("PATH", format!("{}:/usr/bin:/bin", path.join("bin").display())).output().unwrap()
    };
    // The same module directory is reused to verify upgrades overwrite flags.
    for (mode, expected) in [
        ("aidl", "audio_hal=aidl\npolicy_xml_supported=0"),
        ("full", "audio_hal=legacy-xml\npolicy_xml_supported=1"),
        ("unavailable", "audio_hal=unknown\npolicy_xml_supported=0"),
    ] {
        let result = run(mode);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let record = fs::read_to_string(path.join("device-capabilities.conf")).unwrap();
        assert!(record.contains(expected), "{record}");
        assert!(!record.contains("fingerprint"));
        assert_eq!(
            fs::metadata(path.join("device-capabilities.conf"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        assert!(!path.join(".device-capabilities.conf.tmp").exists());
        assert!(!path.join("core/.config").exists());
    }
    // A recovery-time unknown result is not a permanent restriction after boot.
    let schema = Command::new(path.join("usbsrctl"))
        .args(["schema", "--json"])
        .env("MODPATH", path)
        .env("TEST_MODE", "full")
        .env("TEST_FINGERPRINT", "build-a")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", path.join("bin").display()),
        )
        .output()
        .unwrap();
    assert!(schema.status.success());
    assert!(String::from_utf8_lossy(&schema.stdout).contains("\"mode\":\"full\""));
    assert!(fs::read_to_string(path.join("device-capabilities.conf"))
        .unwrap()
        .contains("audio_hal=legacy-xml"));
    // Once resolved, runtime uses the file even if every probe is unavailable.
    let cached = Command::new(path.join("usbsrctl"))
        .args(["schema", "--json"])
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&cached.stdout).contains("\"mode\":\"full\""));
    // Resolved records never run device probes, even if an OTA changes the
    // environment to report a different HAL. Missing/corrupt records also do
    // not probe or rewrite themselves.
    let record_path = path.join("device-capabilities.conf");
    let trace = path.join("probe-calls");
    for name in ["getprop", "service", "dumpsys"] {
        let file = path.join("bin").join(name);
        let previous = fs::read_to_string(&file).unwrap();
        fs::write(
            &file,
            previous.replacen(
                "#!/bin/sh\n",
                "#!/bin/sh\necho called >> \"$MODPATH/probe-calls\"\n",
                1,
            ),
        )
        .unwrap();
    }
    for (record, expected) in [
        (
            Some(
                "version=1\naudio_hal=legacy-xml\npolicy_xml_supported=1\nbuild_fingerprint=old\n",
            ),
            "\"mode\":\"full\"",
        ),
        (
            Some("version=1\naudio_hal=aidl\npolicy_xml_supported=0\n"),
            "\"mode\":\"limited\"",
        ),
        (Some("broken"), "invalid_device_capabilities"),
        (None, "device_capabilities_missing"),
    ] {
        if let Some(record) = record {
            fs::write(&record_path, record).unwrap();
        } else {
            fs::remove_file(&record_path).unwrap();
        }
        fs::write(&trace, "").unwrap();
        let result = Command::new(path.join("usbsrctl"))
            .args(["schema", "--json"])
            .env("MODPATH", path)
            .env("TEST_MODE", "aidl")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", path.join("bin").display()),
            )
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains(expected));
        assert_eq!(fs::read_to_string(&trace).unwrap(), "");
        assert_eq!(fs::read_to_string(&record_path).ok().as_deref(), record);
    }
    // Unknown records remain unchanged until boot AND audioserver are ready.
    let unknown = "version=1\naudio_hal=unknown\npolicy_xml_supported=0\n";
    fs::write(&record_path, unknown).unwrap();
    fs::write(path.join("bin/getprop"), "#!/bin/sh\necho 0\n").unwrap();
    fs::write(&trace, "").unwrap();
    let pending = Command::new(path.join("usbsrctl"))
        .arg("_resolve-device")
        .env("MODPATH", path)
        .env("TEST_MODE", "aidl")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", path.join("bin").display()),
        )
        .output()
        .unwrap();
    assert!(pending.status.success());
    assert_eq!(fs::read_to_string(&record_path).unwrap(), unknown);
    assert_eq!(fs::read_to_string(&trace).unwrap(), "");
    // A failed executable still installs a conservative, parseable fallback.
    fs::write(path.join("usbsrctl"), "#!/bin/sh\nexit 1\n").unwrap();
    assert!(run("full").status.success());
    let record = fs::read_to_string(path.join("device-capabilities.conf")).unwrap();
    assert!(record.contains("audio_hal=unknown\npolicy_xml_supported=0"));
    assert!(record.contains("reason=install_probe_unavailable"));
}
