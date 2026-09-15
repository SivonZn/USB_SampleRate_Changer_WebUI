use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn executable(path: &Path, script: &str) {
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn boot_resolves_capabilities_and_restores_priority_independently_of_reapply() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(".boot-service-test-{}", std::process::id()));
    fs::create_dir_all(root.join("state")).unwrap();
    // Redirect Android paths before running the real lifecycle script on host.
    let script = include_str!("../../module/service.sh")
        .replace(
            "/data/adb/usb_samplerate_changer_webui",
            &root.join("state").to_string_lossy(),
        )
        .replace(
            "/data/local/tmp/usb_samplerate_changer_webui",
            &root.join("logs").to_string_lossy(),
        );
    fs::write(root.join("service.sh"), script).unwrap();
    executable(&root.join("getprop"), "#!/bin/sh\ncase \"$1\" in sys.boot_completed) echo 1;; init.svc.audioserver) echo running;; esac\n");
    executable(&root.join("chown"), "#!/bin/sh\nexit 0\n");
    executable(&root.join("usbsrctl"), "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$TEST_CALLS\"\nif [ \"$1\" = _resolve-device ]; then exit \"$TEST_RESOLVE_EXIT\"; fi\n");
    let calls = root.join("calls");
    let settings = root.join("state/settings.conf");
    for (contents, resolve_exit, reapply) in [
        (None, "0", false),
        (Some("auto_reapply=0\n"), "0", false),
        (Some("auto_reapply=1\n"), "0", true),
        (Some("auto_reapply=1\n"), "1", true),
    ] {
        if let Some(contents) = contents {
            fs::write(&settings, contents).unwrap();
        }
        fs::write(&calls, "").unwrap();
        // Explicitly wait for the sourced service's background worker.
        let output = Command::new("/bin/sh")
            .args(["-c", ". \"$0\"; wait"])
            .arg(root.join("service.sh"))
            .env("PATH", format!("{}:/usr/bin:/bin", root.display()))
            .env("TEST_CALLS", &calls)
            .env("TEST_RESOLVE_EXIT", resolve_exit)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let expected = format!(
            "_audioserver-priority-stop\n_resolve-device\n_audioserver-priority-start\n{}",
            if reapply { "reapply\n" } else { "" }
        );
        assert_eq!(fs::read_to_string(&calls).unwrap(), expected);
    }
    fs::remove_dir_all(root).unwrap();
}
