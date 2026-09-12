use std::{fs, path::PathBuf, process::Command};

struct Module(PathBuf);
impl Module {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(".capabilities-cli-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        fs::copy(env!("CARGO_BIN_EXE_usbsrctl"), path.join("usbsrctl")).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(self.0.join("usbsrctl"))
            .args(args)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap()
    }
}
impl Drop for Module {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cli_restricts_aidl_before_any_script_or_state_access() {
    let module = Module::new();
    fs::write(
        module.0.join("device-capabilities.conf"),
        "version=1\naudio_hal=aidl\npolicy_xml_supported=0\n",
    )
    .unwrap();
    // No core scripts or Android commands exist in this fixture. Capability
    // rejection must happen before either is touched, including the internal CLI.
    for args in [
        vec!["apply"],
        vec!["preview"],
        vec!["_dynamic-direct", "--policy", "offload-direct-dynamic"],
        vec!["extra", "bluetooth-hal", "offload"],
        vec!["extra", "usb-period", "2250"],
    ] {
        let output = module.run(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("unsupported_device_capability"),
            "{args:?}: {stderr}"
        );
        assert!(!stderr.contains("script not found"));
        if args[0] == "apply" || args[0] == "extra" {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("operation_applied=0"));
            assert!(stdout.contains("operation_result=not_started"));
        }
    }
    let output = module.run(&["schema", "--json"]);
    assert!(output.status.success());
    let schema = String::from_utf8(output.stdout).unwrap();
    assert!(schema.contains("\"mode\":\"limited\""));
    assert!(schema.contains("\"policy\":{\"available\":false,\"default\":\"auto\",\"options\":[]}"));
    assert!(schema.contains("\"bluetooth_hal\":{\"available\":false"));
    assert!(schema.contains("\"usb_period\":{\"available\":false"));
    assert!(schema.contains("\"jitter\":{\"available\":true"));
    assert!(schema.contains("\"value\":\"effect\""));
    fs::remove_file(module.0.join("device-capabilities.conf")).unwrap();
    let missing = module.run(&["schema", "--json"]);
    assert!(missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stdout).contains("\"mode\":\"limited\""));
}
