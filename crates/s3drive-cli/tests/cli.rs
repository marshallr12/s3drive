use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_s3drive"))
}

#[test]
fn unfinished_mount_never_reports_success() {
    let result = cli().arg("mount").output().unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("planned"), "{error}");
}

#[test]
fn config_and_state_commands_work_without_credentials() {
    let config =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/s3drive.toml");
    assert!(
        cli()
            .arg("config-check")
            .arg(config)
            .status()
            .unwrap()
            .success()
    );
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("state");
    for _ in 0..2 {
        assert!(
            cli()
                .arg("init-state")
                .arg("--directory")
                .arg(&state)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(state.join("metadata.sqlite3").is_file());
}
