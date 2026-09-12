use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

#[test]
fn default_packaged_restart_only_restarts_audioserver_and_reports_failure() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root
        .join("target")
        .join(format!(".restart-test-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    // This patch is the source of the script actually shipped in the module.
    let patch =
        fs::read_to_string(root.join("../patches/0004-controller-owns-audio-restart.patch"))
            .unwrap();
    let section = patch
        .split("+++ b/extras/reload-audio-servers.sh\n")
        .nth(1)
        .unwrap()
        .split("diff --git ")
        .next()
        .unwrap();
    let script = section
        .lines()
        .filter_map(|line| line.strip_prefix('+'))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(path.join("reload.sh"), script).unwrap();
    for (name, source) in [
        ("getprop", "#!/bin/sh\ncase \"$1\" in sys.boot_completed) echo 1;; init.svc.audioserver) if [ \"$TEST_FAIL\" = 1 ]; then echo stopped; else echo running; fi;; init.svc_debug_pid.audioserver) :;; *) echo \"unexpected-getprop $*\" >> \"$TEST_CALLS\";; esac\n"),
        ("setprop", "#!/bin/sh\necho \"setprop $*\" >> \"$TEST_CALLS\"\n"),
        ("pidof", "#!/bin/sh\n[ \"$TEST_FAIL\" = 1 ] || echo 42\n"),
        ("sleep", "#!/bin/sh\nexit 0\n"),
        ("start", "#!/bin/sh\necho \"start $*\" >> \"$TEST_CALLS\"\n"),
        ("stop", "#!/bin/sh\necho \"stop $*\" >> \"$TEST_CALLS\"\n"),
    ] {
        fs::write(path.join(name), source).unwrap();
        fs::set_permissions(path.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
    for fail in [false, true] {
        fs::write(path.join("calls"), "").unwrap();
        let output = Command::new("/bin/sh")
            .arg(path.join("reload.sh"))
            .env("PATH", &path)
            .env("TEST_CALLS", path.join("calls"))
            .env("TEST_FAIL", if fail { "1" } else { "0" })
            .output()
            .unwrap();
        assert_eq!(
            fs::read_to_string(path.join("calls")).unwrap(),
            "setprop ctl.restart audioserver\n"
        );
        assert_eq!(output.status.success(), !fail);
        if fail {
            assert!(String::from_utf8_lossy(&output.stderr).contains("audioserver reload failed"));
        } else {
            assert!(String::from_utf8_lossy(&output.stdout).contains("audioserver reload complete"));
        }
    }
    fs::remove_dir_all(path).unwrap();
}
