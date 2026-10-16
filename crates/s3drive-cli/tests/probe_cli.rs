use s3drive_s3::probe::Plan;
use std::{fs, path::Path, process::Command};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_s3drive"))
}
fn config(path: &Path) {
    fs::write(path,"version=1\nmode='read-only'\n[[sources]]\nalias='fixture'\nbucket='bucket'\nregion='us-east-1'\nendpoint='http://127.0.0.1:9'\nprofile='deliberately-unconfigured'\npath_style=true\nallow_http_loopback=true\n").unwrap();
}
#[test]
fn offline_plan_and_confirmation_guards_preserve_existing_local_files() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("drive.toml");
    config(&config_path);
    let plan_path = temp.path().join("probe.toml");
    let output = command()
        .args(["probe-writes", "plan"])
        .arg(&config_path)
        .arg("fixture")
        .arg("--output")
        .arg(&plan_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("No credentials resolved"));
    let text = fs::read_to_string(&plan_path).unwrap();
    let plan = Plan::from_toml(&text).unwrap();
    assert_eq!(plan.keys().len(), 6);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&plan_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    // Refuse replacing an existing plan; no provider credentials are required.
    let output = command()
        .args(["probe-writes", "plan"])
        .arg(&config_path)
        .arg("fixture")
        .arg("--output")
        .arg(&plan_path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), text);
    let report = temp.path().join("report.toml");
    let output = command()
        .args(["probe-writes", "run"])
        .arg(&config_path)
        .arg("fixture")
        .arg("--plan")
        .arg(&plan_path)
        .args(["--confirm", "invalid"])
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("probe-plan-confirmation"));
    assert!(!report.exists());
    // A bad output destination must also fail before credential discovery/PUT.
    fs::write(&report, "preserve this report").unwrap();
    let output = command()
        .args(["probe-writes", "run"])
        .arg(&config_path)
        .arg("fixture")
        .arg("--plan")
        .arg(&plan_path)
        .arg("--confirm")
        .arg(plan.digest().unwrap())
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&report).unwrap(), "preserve this report");
    // Even a new digest cannot authorize arbitrary edited keys in the file.
    let changed = text.replace(&plan.keys()[0], "ordinary-user-data");
    fs::write(&plan_path, &changed).unwrap();
    let changed = Plan::from_toml(&changed).unwrap();
    let report = temp.path().join("changed-report.toml");
    let output = command()
        .args(["probe-writes", "cleanup"])
        .arg(&config_path)
        .arg("fixture")
        .arg("--plan")
        .arg(&plan_path)
        .arg("--confirm")
        .arg(changed.digest().unwrap())
        .arg("--report")
        .arg(&report)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("probe-plan-confirmation"));
    assert!(!report.exists());
}
