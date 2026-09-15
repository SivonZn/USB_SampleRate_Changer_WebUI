use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

#[test]
fn uninstall_delegates_once_to_cleanup_regardless_of_state_health() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root
        .join("target")
        .join(format!(".uninstall-test-{}", std::process::id()));
    fs::create_dir_all(&fixture).unwrap();
    fs::copy(
        root.join("../module/uninstall.sh"),
        fixture.join("uninstall.sh"),
    )
    .unwrap();
    let controller = fixture.join("usbsrctl");
    fs::write(&controller, "#!/bin/sh\nif [ \"$1\" = status ]; then cat \"$TEST_STATUS\"; else printf '%s\\n' \"$*\" >> \"$TEST_CALLS\"; fi\n").unwrap();
    fs::set_permissions(&controller, fs::Permissions::from_mode(0o755)).unwrap();
    // Never let a host test perform the script's absolute Android removals.
    let rm = fixture.join("rm");
    fs::write(&rm, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&rm, fs::Permissions::from_mode(0o755)).unwrap();
    for (status, expected) in [
        ("state_degraded=0\n", "settings audioserver-priority disable\n_audioserver-priority-stop\ncleanup\n"),
        ("state_degraded=0\nresampler_configured=1\nusb_period_configured=1\nbluetooth_hal_configured=1\njitter_effect_configured=1\njitter_wifi_configured=1\npolicy_configured=1\n",
         "settings audioserver-priority disable\n_audioserver-priority-stop\ncleanup\n"),
        ("state_degraded=1\nresampler_configured=1\njitter_effect_configured=1\n", "settings audioserver-priority disable\n_audioserver-priority-stop\ncleanup\n"),
    ] {
        fs::write(fixture.join("status"), status).unwrap();
        fs::write(fixture.join("calls"), "").unwrap();
        let output = Command::new("/bin/sh").arg(fixture.join("uninstall.sh"))
            .env("PATH", format!("{}:/usr/bin:/bin", fixture.display()))
            .env("TEST_STATUS", fixture.join("status"))
            .env("TEST_CALLS", fixture.join("calls"))
            .output().unwrap();
        assert!(output.status.success());
        assert_eq!(fs::read_to_string(fixture.join("calls")).unwrap(), expected);
    }
    fs::remove_dir_all(fixture).unwrap();
}
