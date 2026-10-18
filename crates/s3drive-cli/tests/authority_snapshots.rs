//! Actual offline CLI processes over private fixture databases. No S3, service
//! installation or restoration. Platform ACL acceptance remains separate.
use s3drive_state::{
    State,
    namespace::{Apply, Kind, Mutation, ROOT},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::Duration,
};
use tokio::process::Command;
const POLICY: &str = "portable-collision-v1";

fn seed(path: &Path) -> String {
    let mut state = State::open_managed(path).unwrap();
    let volume = state.namespace_initialize_with_policy(POLICY).unwrap();
    state
        .namespace_apply(&Apply {
            volume: volume.id.clone(),
            operation: "1".repeat(32),
            expected_sequence: 0,
            mutation: Mutation::Create {
                parent: ROOT.into(),
                name: "private namespace 資料 café 🦀".into(),
                node_kind: Kind::Directory,
            },
        })
        .unwrap();
    volume.id
}
fn cli(verb: &str, directory: &Path, volume: &str) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_s3drive"));
    c.args(["authority-snapshot", verb])
        .arg("--directory")
        .arg(directory)
        .args(["--expect-volume", volume, "--name-policy", POLICY])
        .env(
            "AWS_SHARED_CREDENTIALS_FILE",
            directory.join("absent-credentials"),
        )
        .env("AWS_CONFIG_FILE", directory.join("absent-config"))
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    c
}
async fn run(c: &mut Command) -> Output {
    tokio::time::timeout(Duration::from_secs(20), c.output())
        .await
        .expect("offline CLI deadline")
        .unwrap()
}
fn ok(out: Output) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("private namespace"));
    serde_json::from_slice(&out.stdout).unwrap()
}
fn refused(out: Output) {
    assert!(!out.status.success() && out.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("private namespace"));
}
fn rows(directory: &Path) -> Vec<(Vec<u8>, Vec<u8>)> {
    let db = rusqlite::Connection::open_with_flags(
        directory.join("metadata.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    db.prepare("SELECT request,outcome FROM namespace_operations ORDER BY operation_id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_snapshot_round_trip_preserves_receipts_and_never_authorizes_restore() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source 資料 café 🦀");
    let output = tmp.path().join("snapshot 資料 café 🦀");
    let volume = seed(&source);
    let before = rows(&source);
    let created = ok(run(cli("create", &source, &volume).arg("--output").arg(&output)).await);
    assert_eq!(created["manifest"]["volume"]["id"], volume);
    assert_eq!(created["manifest"]["volume"]["sequence"], 1);
    assert_eq!(created["history_continuity_proven"], false);
    assert_eq!(created["restore_authorized"], false);
    assert_eq!(
        created,
        ok(run(&mut cli("inspect", &output, &volume)).await)
    );
    assert_eq!(rows(&source), before);
    let marker = fs::read(output.join("snapshot.json")).unwrap();
    let data = fs::read(output.join("snapshot.sqlite3")).unwrap();
    refused(run(cli("create", &source, &volume).arg("--output").arg(&output)).await);
    assert_eq!(fs::read(output.join("snapshot.json")).unwrap(), marker);
    assert_eq!(fs::read(output.join("snapshot.sqlite3")).unwrap(), data);
    assert!(State::open_existing_authority(&output, &volume, POLICY).is_err());
    assert!(!output.join("metadata.sqlite3").exists());
    assert!(!output.join("volume.lock").exists());
    // The operator surface has no restore command or token/config inputs.
    refused(run(&mut cli("restore", &output, &volume)).await);
    let mut manifest: serde_json::Value = serde_json::from_slice(&marker).unwrap();
    manifest["private namespace secret"] = "never echo manifest contents".into();
    fs::write(
        output.join("snapshot.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    refused(run(&mut cli("inspect", &output, &volume)).await);
    assert_eq!(rows(&source), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_snapshot_refuses_active_wrong_missing_nested_and_limited_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    let volume = seed(&source);
    let before = rows(&source);
    let active = State::open_existing_authority(&source, &volume, POLICY).unwrap();
    let output = tmp.path().join("active");
    refused(run(cli("create", &source, &volume).arg("--output").arg(&output)).await);
    assert!(!output.exists());
    drop(active);
    for (name, path, id, extra) in [
        ("wrong-volume", source.clone(), "f".repeat(32), vec![]),
        (
            "missing",
            tmp.path().join("missing-source"),
            volume.clone(),
            vec![],
        ),
        (
            "limit",
            source.clone(),
            volume.clone(),
            vec!["--max-bytes", "4096"],
        ),
        (
            "invalid-limit",
            source.clone(),
            volume.clone(),
            vec!["--max-bytes", "0"],
        ),
        (
            "invalid-deadline",
            source.clone(),
            volume.clone(),
            vec!["--timeout-seconds", "0"],
        ),
    ] {
        let output = tmp.path().join(name);
        refused(
            run(cli("create", &path, &id)
                .arg("--output")
                .arg(&output)
                .args(extra))
            .await,
        );
        assert!(!output.exists(), "refusal created {name}");
    }
    let nested = source.join("nested");
    refused(run(cli("create", &source, &volume).arg("--output").arg(&nested)).await);
    assert!(!nested.exists());
    assert_eq!(rows(&source), before);
    let client = tmp.path().join("client");
    drop(State::open_managed(&client).unwrap());
    let output: PathBuf = tmp.path().join("wrong-role");
    refused(run(cli("create", &client, &volume).arg("--output").arg(&output)).await);
    assert!(!output.exists());
}
