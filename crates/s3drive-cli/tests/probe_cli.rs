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

#[test]
fn policy_check_is_offline_and_never_rewrites_its_inputs() {
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("drive.toml");
    config(&config_path);
    let source = s3drive_core::Config::parse(&fs::read_to_string(&config_path).unwrap())
        .unwrap()
        .sources
        .remove(0);
    let plan = Plan::prepare(&source).unwrap();
    let plan_text = plan.to_toml().unwrap();
    let value: toml::Value = toml::from_str(&plan_text).unwrap();
    let fingerprint = value["source_fingerprint"].as_str().unwrap();
    let observed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut report = format!(
        "version=1\nprobe_version='conditional-put-v1'\nobserved_unix_seconds={observed}\nplan_digest='{}'\nsource_fingerprint='{fingerprint}'\ncredential_generation='{}'\npassed=true\nerrors=[]\nremaining_keys=[]\n",
        plan.digest().unwrap(),
        "01".repeat(32)
    );
    for name in [
        "conditional-create",
        "existing-key-create-rejected",
        "conditional-replace",
        "stale-replace-rejected",
        "wrong-validator-rejected",
        "concurrent-create-one-winner",
        "concurrent-replace-one-winner",
        "unicode-literal-key-replace",
        "empty-object-round-trip",
        "receipt-independent-observation",
    ] {
        report.push_str(&format!(
            "[[checks]]\nname='{name}'\npassed=true\ndetail='local fixture'\n"
        ));
    }
    let report_digest = Sha256::digest(report.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let policy = format!(
        "version=1\nsource_fingerprint='{fingerprint}'\nreport_sha256='{report_digest}'\nwritable_prefix='documents/'\nmax_object_bytes=4096\noperations='create-only'\nobject_metadata='s3drive-markers-only'\nevidence_max_age_seconds=86400\n",
    );
    let plan_path = temp.path().join("plan.toml");
    let report_path = temp.path().join("report.toml");
    let policy_path = temp.path().join("policy.toml");
    fs::write(&plan_path, &plan_text).unwrap();
    fs::write(&report_path, &report).unwrap();
    fs::write(&policy_path, &policy).unwrap();
    let run = || {
        command()
            .arg("upload-policy-check")
            .arg(&config_path)
            .arg("fixture")
            .arg("--policy")
            .arg(&policy_path)
            .arg("--plan")
            .arg(&plan_path)
            .arg("--report")
            .arg(&report_path)
            .output()
            .unwrap()
    };
    let result = run();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("No credentials resolved"));
    assert_eq!(fs::read_to_string(&plan_path).unwrap(), plan_text);
    assert_eq!(fs::read_to_string(&report_path).unwrap(), report);
    assert_eq!(fs::read_to_string(&policy_path).unwrap(), policy);
    fs::write(&report_path, format!("{report}\n# changed")).unwrap();
    let result = run();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Evidence"));
    assert_eq!(fs::read_to_string(&policy_path).unwrap(), policy);
}
