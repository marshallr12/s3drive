use super::*;
use namespace::{Apply, Mutation, ROOT, Receipt, Volume};
use std::{
    fs,
    io::{BufRead, Write},
    process::{Child, Command, Stdio},
    time::Duration,
};
const POLICY: &str = "portable-collision-v1";
fn seed(path: &Path) -> (Volume, Apply, Receipt) {
    let mut state = State::open_managed(path).unwrap();
    let v = state.namespace_initialize_with_policy(POLICY).unwrap();
    let request = Apply {
        volume: v.id.clone(),
        operation: "1".repeat(32),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "2".repeat(64),
            root: "managed/.s3drive/".into(),
        },
    };
    let receipt = state.namespace_apply(&request).unwrap();
    (v, request, receipt)
}
type OperationBytes = Vec<(Vec<u8>, Vec<u8>)>;
fn rows(path: &Path) -> (i32, OperationBytes) {
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    (
        db.pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap(),
        db.prepare("SELECT request,outcome FROM namespace_operations ORDER BY operation_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect(),
    )
}

#[test]
fn existing_authority_missing_paths_and_files_are_never_created() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("missing");
    assert!(State::open_existing_authority(&path, &"1".repeat(32), POLICY).is_err());
    assert!(!path.exists());
    let (v, _, _) = seed(&path);
    for missing in ["metadata.sqlite3", "volume.lock"] {
        let saved = fs::read(path.join(missing)).unwrap();
        fs::remove_file(path.join(missing)).unwrap();
        let entries = fs::read_dir(&path)
            .unwrap()
            .map(|r| r.unwrap().file_name())
            .collect::<Vec<_>>();
        assert!(State::inspect_existing_authority(&path, &v.id, POLICY).is_err());
        assert!(State::open_existing_authority(&path, &v.id, POLICY).is_err());
        assert!(!path.join(missing).exists());
        assert_eq!(
            entries,
            fs::read_dir(&path)
                .unwrap()
                .map(|r| r.unwrap().file_name())
                .collect::<Vec<_>>()
        );
        // Restore only this owned test fixture with its original private mode.
        let file = s3drive_test_private_restore(&path, missing, &saved);
        assert!(file.exists());
    }
}
fn s3drive_test_private_restore(path: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    // This root was created by State with private inheritable Windows ACLs.
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = path.join(name);
    let mut out = options.open(&file).unwrap();
    out.write_all(bytes).unwrap();
    out.sync_all().unwrap();
    file
}

#[test]
fn existing_authority_wrong_identity_does_not_migrate_or_change_operation_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let (v, request, receipt) = seed(&path);
    {
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        db.execute_batch("DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans")
            .unwrap();
        db.pragma_update(None, "user_version", 22).unwrap();
    }
    let before = rows(&path);
    for (id, policy) in [
        (&"3".repeat(32), POLICY),
        (&v.id, "exact-utf8-v1"),
        (&ROOT.to_string(), POLICY),
    ] {
        assert!(State::open_existing_authority(&path, id, policy).is_err());
        assert!(State::inspect_existing_authority(&path, id, policy).is_err());
        assert_eq!(rows(&path), before);
    }
    assert_eq!(
        State::inspect_existing_authority(&path, &v.id, POLICY)
            .unwrap()
            .sequence,
        1
    );
    assert_eq!(rows(&path), before, "inspection performed a migration");
    let mut state = State::open_existing_authority(&path, &v.id, POLICY).unwrap();
    assert_eq!(state.info().unwrap().schema_version, SCHEMA_VERSION);
    assert_eq!(state.namespace_apply(&request).unwrap(), receipt);
    assert_eq!(state.namespace_volume().unwrap().sequence, 1);
    drop(state);
    assert_eq!(rows(&path).1, before.1);
}

#[test]
fn existing_authority_local_state_and_uninitialized_database_are_not_adopted() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("client");
    let mut local = State::open_managed(&path).unwrap();
    local.register_source(&[7; 32], &"f".repeat(64)).unwrap();
    drop(local);
    let before = fs::read(path.join("metadata.sqlite3")).unwrap();
    assert!(State::open_existing_authority(&path, &"1".repeat(32), POLICY).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM namespace_volume", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM source_bindings", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(db);
    assert_eq!(fs::read(path.join("metadata.sqlite3")).unwrap(), before);
    fs::write(path.join("metadata.sqlite3"), []).unwrap();
    assert!(State::open_existing_authority(&path, &"1".repeat(32), POLICY).is_err());
    assert!(fs::read(path.join("metadata.sqlite3")).unwrap().is_empty());
}

#[test]
fn existing_authority_rechecks_identity_inside_migration_transaction() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let (v, _, _) = seed(&path);
    let owner = directory::OwnedDirectory::acquire_existing(&path).unwrap();
    State::inspect_authority(&owner, &v.id, POLICY).unwrap();
    let replacement = tmp.path().join("replacement");
    let (other, _, _) = seed(&replacement);
    assert_ne!(other.id, v.id);
    let db = Connection::open(replacement.join("metadata.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE chunk_publications; DROP TABLE chunk_client_role; DROP TABLE chunk_objects; DROP TABLE chunk_plans; DROP TABLE multipart_parts; DROP TABLE multipart_plans").unwrap();
    db.pragma_update(None, "user_version", 22).unwrap();
    drop(db);
    // Simulate a noncooperating external replacement AFTER the read-only
    // inspection, while our legitimate advisory owner still holds its lock.
    fs::copy(
        replacement.join("metadata.sqlite3"),
        path.join("metadata.sqlite3"),
    )
    .unwrap();
    let before = rows(&path);
    assert!(State::open_mode(&path.join("metadata.sqlite3"), Some((&v.id, POLICY))).is_err());
    assert_eq!(rows(&path), before, "wrong replacement was migrated");
    fs::write(path.join("metadata.sqlite3"), []).unwrap();
    assert!(State::open_mode(&path.join("metadata.sqlite3"), Some((&v.id, POLICY))).is_err());
    assert!(
        fs::read(path.join("metadata.sqlite3")).unwrap().is_empty(),
        "empty replacement was initialized"
    );
}

#[test]
fn existing_authority_real_schema_eight_migrates_without_rewriting_old_receipt() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("schema-eight");
    let owner = directory::OwnedDirectory::acquire(&path).unwrap();
    let db = Connection::open(owner.database_path()).unwrap();
    for sql in [
        include_str!("../migrations/001.sql"),
        include_str!("../migrations/002.sql"),
        include_str!("../migrations/003.sql"),
        include_str!("../migrations/004.sql"),
        include_str!("../migrations/005.sql"),
        include_str!("../migrations/006.sql"),
        include_str!("../migrations/007.sql"),
        include_str!("../migrations/008.sql"),
    ] {
        db.execute_batch(sql).unwrap();
    }
    db.pragma_update(None, "application_id", APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", 8).unwrap();
    let volume = "4".repeat(32);
    let request = Apply {
        volume: volume.clone(),
        operation: "1".repeat(32),
        expected_sequence: 0,
        mutation: Mutation::Create {
            parent: ROOT.into(),
            name: "old café".into(),
            node_kind: namespace::Kind::Directory,
        },
    };
    let receipt = Receipt {
        sequence: 1,
        node: namespace::Node {
            id: request.operation.clone(),
            parent: Some(ROOT.into()),
            name: "old café".into(),
            kind: namespace::Kind::Directory,
            revision: 1,
            deleted: false,
            payload: None,
            placement: None,
        },
        artifact: None,
    };
    db.execute(
        "INSERT INTO namespace_volume VALUES(1,?1,1,'exact-utf8-v1')",
        [&volume],
    )
    .unwrap();
    db.execute(
        "INSERT INTO namespace_nodes VALUES(?1,NULL,'','directory',0,0)",
        [ROOT],
    )
    .unwrap();
    db.execute(
        "INSERT INTO namespace_nodes VALUES(?1,?2,'old café','directory',1,0)",
        params![request.operation, ROOT],
    )
    .unwrap();
    let outcome: std::result::Result<Receipt, namespace::Error> = Ok(receipt.clone());
    db.execute(
        "INSERT INTO namespace_operations VALUES(?1,?2,?3)",
        params![
            request.operation,
            serde_json::to_vec(&request).unwrap(),
            serde_json::to_vec(&outcome).unwrap()
        ],
    )
    .unwrap();
    drop(db);
    drop(owner);
    let before = rows(&path);
    assert_eq!(
        State::inspect_existing_authority(&path, &volume, "exact-utf8-v1")
            .unwrap()
            .sequence,
        1
    );
    assert_eq!(rows(&path), before, "inspection migrated schema eight");
    let mut state = State::open_existing_authority(&path, &volume, "exact-utf8-v1").unwrap();
    assert_eq!(state.info().unwrap().schema_version, SCHEMA_VERSION);
    assert_eq!(state.namespace_apply(&request).unwrap(), receipt);
    assert_eq!(state.namespace_volume().unwrap().sequence, 1);
    drop(state);
    assert_eq!(rows(&path).1, before.1);
}

#[test]
fn existing_authority_refuses_corrupt_root_future_schema_and_another_owner() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let (v, _, _) = seed(&path);
    let state = State::open_existing_authority(&path, &v.id, POLICY).unwrap();
    assert!(State::inspect_existing_authority(&path, &v.id, POLICY).is_err());
    assert!(State::open_existing_authority(&path, &v.id, POLICY).is_err());
    drop(state);
    for case in 0..6 {
        let path = tmp.path().join(format!("corrupt-{case}"));
        let (v, _, _) = seed(&path);
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        db.pragma_update(None, "ignore_check_constraints", true)
            .unwrap();
        match case {
            0 => db
                .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .unwrap(),
            1 => {
                db.execute("DELETE FROM namespace_nodes WHERE node_id=?1", [ROOT])
                    .unwrap();
            }
            2 => {
                db.execute(
                    "UPDATE namespace_nodes SET revision=999 WHERE node_id=?1",
                    [ROOT],
                )
                .unwrap();
            }
            3 => {
                db.execute(
                    "UPDATE namespace_nodes SET payload=?1 WHERE node_id=?2",
                    params![b"{}".as_slice(), ROOT],
                )
                .unwrap();
            }
            4 => {
                db.execute(
                    "UPDATE namespace_nodes SET placement=?1 WHERE node_id=?2",
                    params![b"{}".as_slice(), ROOT],
                )
                .unwrap();
            }
            _ => db.pragma_update(None, "application_id", 42).unwrap(),
        }
        drop(db);
        let before = rows(&path);
        assert!(
            State::open_existing_authority(&path, &v.id, POLICY).is_err(),
            "accepted corruption {case}"
        );
        assert!(
            State::inspect_existing_authority(&path, &v.id, POLICY).is_err(),
            "inspection accepted corruption {case}"
        );
        assert_eq!(rows(&path), before);
    }
}

#[test]
fn existing_authority_wal_child() {
    let Ok(path) = std::env::var("S3DRIVE_AUTHORITY_WAL_CHILD") else {
        return;
    };
    let mut state = State::open_managed(Path::new(&path)).unwrap();
    state
        .connection
        .pragma_update(None, "wal_autocheckpoint", 0)
        .unwrap();
    let v = state.namespace_initialize_with_policy(POLICY).unwrap();
    let request = Apply {
        volume: v.id.clone(),
        operation: "1".repeat(32),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "2".repeat(64),
            root: "managed/.s3drive/".into(),
        },
    };
    let receipt = state.namespace_apply(&request).unwrap();
    println!(
        "AUTHORITY_WAL {}",
        serde_json::to_string(&(v, request, receipt)).unwrap()
    );
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}
struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn existing_authority_crash_reads_wal_only_identity_and_replays_original_receipt() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let mut child = ChildOwner(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "authority_restart_tests::existing_authority_wal_child",
                "--nocapture",
            ])
            .env("S3DRIVE_AUTHORITY_WAL_CHILD", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let line = line.unwrap();
            if let Some(v) = line.strip_prefix("AUTHORITY_WAL ") {
                let _ = send.send(v.to_owned());
                break;
            }
        }
    });
    let data = receive.recv_timeout(Duration::from_secs(20)).unwrap();
    let (v, request, receipt): (Volume, Apply, Receipt) = serde_json::from_str(&data).unwrap();
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(
        fs::metadata(path.join("metadata.sqlite3-wal"))
            .unwrap()
            .len()
            > 0
    );
    let copy = tmp.path().join("main-only.sqlite3");
    fs::copy(path.join("metadata.sqlite3"), &copy).unwrap();
    let main = Connection::open(&copy).unwrap();
    assert_eq!(
        main.query_row("SELECT count(*) FROM namespace_volume", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0,
        "fixture was checkpointed; does not prove WAL-only recovery"
    );
    drop(main);
    assert_eq!(
        State::inspect_existing_authority(&path, &v.id, POLICY)
            .unwrap()
            .sequence,
        1
    );
    let mut state = State::open_existing_authority(&path, &v.id, POLICY).unwrap();
    assert_eq!(state.namespace_apply(&request).unwrap(), receipt);
    assert_eq!(state.namespace_volume().unwrap().sequence, 1);
}
