//! Modeled receipts only. SDK HTTP and native acceptance are separate gates.
use super::*;
fn id(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}
fn fixture(path: &Path, save: bool) -> (Input, Receipt, opens::OpenRecord) {
    let mut state = State::open_managed(path).unwrap();
    let control = Control {
        store: Store {
            id: id(3),
            route: "a".repeat(64),
            root: "managed/".into(),
        },
        source: [4; 32],
        fingerprint: "b".repeat(64),
    };
    let key = chunk_mount::admit(
        &mut state,
        chunk_mount::Input {
            volume: id(1),
            controls: vec![control.clone()],
        },
    )
    .unwrap();
    let node = namespace::Node {
        id: id(5),
        parent: Some(namespace::ROOT.into()),
        name: "日本語 🦀".into(),
        kind: namespace::Kind::Directory,
        revision: 1,
        deleted: false,
        payload: None,
        placement: None,
    };
    let request = Input {
        request: Apply {
            volume: id(1),
            operation: id(6),
            expected_sequence: 1,
            mutation: Mutation::ArmRemoval {
                node: namespace::NodeGuard {
                    id: node.id.clone(),
                    revision: 1,
                },
                parent: namespace::NodeGuard {
                    id: namespace::ROOT.into(),
                    revision: 0,
                },
                owner: key.owner().into(),
            },
        },
        store: control.store.clone(),
        source: control.source,
        fingerprint: control.fingerprint.clone(),
        entries: vec![Entry::from_node(&node).unwrap()],
        destination_control: None,
    };
    let mut log = opens::Journal::for_chunks(&mut state, &key).unwrap();
    log.start_session(&id(7), &id(1)).unwrap();
    log.prepare(opens::OpenInput {
        id: id(8),
        session: id(7),
        volume: id(1),
        node: node.id.clone(),
        store: control.store.clone(),
        source: control.source,
        fingerprint: control.fingerprint.clone(),
        delete_on_close: false,
        controlled: true,
        session_bound: true,
        posix: true,
        admission: None,
        creation: None,
    })
    .unwrap();
    log.activate(&id(8), &id(9)).unwrap();
    log.callback(&id(8), &id(10), opens::Callback::SetDelete { delete: true })
        .unwrap();
    let open = log.link(&id(8), &id(11), &id(10), request.clone()).unwrap();
    if save {
        Journal::for_chunks(&mut state, crate::journal::Options::default(), &key)
            .unwrap()
            .begin(request.clone())
            .unwrap();
    }
    let binding = crate::sparse_views::Binding::new(
        s3drive_core::manifest::base::Descriptor::from_hex(
            &id(1),
            &id(20),
            &id(21),
            &"c".repeat(64),
        )
        .unwrap(),
        [3; 16],
        control.source,
        control.fingerprint,
        control.store.root,
        4,
    )
    .unwrap();
    let mut views =
        crate::sparse_views::Journal::new(&mut state, crate::sparse_views::Options::default())
            .unwrap();
    let view = views.begin([1; 32], binding, 0).unwrap();
    views
        .write(
            [2; 32],
            view.id(),
            11 * 1024 * 1024 + 3,
            "日本語 🦀".as_bytes(),
        )
        .unwrap();
    let receipt = Receipt {
        sequence: 2,
        node: namespace::Node {
            revision: 2,
            ..node
        },
        artifact: Some(namespace::Artifact::Removal(namespace::RemovalFence {
            intent: id(6),
            owner: key.owner().into(),
        })),
    };
    (request, receipt, open)
}
fn source(db: &Connection) -> Vec<String> {
    [
        "sparse_view_files",
        "sparse_views",
        "sparse_view_blocks",
        "windows_mount_sessions",
        "windows_session_recoveries",
        "retired_windows_opens",
    ]
    .into_iter()
    .flat_map(|table| {
        let mut q = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
            .unwrap();
        let cols = q.column_count();
        q.query_map([], |row| {
            Ok((0..cols)
                .map(|n| format!("{:?}", row.get_ref(n).unwrap()))
                .collect::<Vec<_>>()
                .join("|"))
        })
        .unwrap()
        .map(|r| format!("{table}:{}", r.unwrap()))
        .collect::<Vec<_>>()
    })
    .collect()
}
#[test]
fn namespace_cold_exact_outcome_and_link_ack_leave_callbacks_and_sources_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let (input, receipt, before) = fixture(&path, true);
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    assert!(RecoveryStore::open(&path, &id(1)).is_err());
    let bytes = source(&r.connection);
    assert!(r.acknowledge_open(&id(8), &id(12), &id(6)).is_err());
    let saved = r.acknowledge(&input.request, Ok(receipt.clone())).unwrap();
    assert_eq!(saved.input, input);
    assert_eq!(r.acknowledge(&input.request, Ok(receipt)).unwrap(), saved);
    let after = r.acknowledge_open(&id(8), &id(12), &id(6)).unwrap();
    assert_eq!(after.input, before.input);
    assert_eq!(&after.events[..before.events.len()], &before.events);
    assert_eq!(after.events.len(), before.events.len() + 1);
    assert!(after.pending().is_empty());
    assert!(after.reply(&id(10)).is_none());
    assert!(!after.completed(&id(10)));
    assert!(after.cleanup().is_none());
    assert!(after.close().is_none());
    assert_eq!(r.acknowledge_open(&id(8), &id(12), &id(6)).unwrap(), after);
    assert!(r.acknowledge_open(&id(8), &id(13), &id(6)).is_err());
    assert_eq!(source(&r.connection), bytes);
    drop(r);
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    assert_eq!(r.open_record(&id(8)).unwrap(), after);
}
#[test]
fn namespace_cold_corrupt_body_does_not_block_exact_metadata_replay_or_repair_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let (input, receipt, _) = fixture(&path, true);
    let r = RecoveryStore::open(&path, &id(1)).unwrap();
    assert!(
        r.connection
            .execute(
                "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
                []
            )
            .unwrap()
            > 0
    );
    let bytes = source(&r.connection);
    drop(r);
    assert!(State::open_managed(&path).is_err());
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    r.acknowledge(&input.request, Ok(receipt)).unwrap();
    r.acknowledge_open(&id(8), &id(12), &id(6)).unwrap();
    assert_eq!(source(&r.connection), bytes);
    drop(r);
    assert!(State::open_managed(&path).is_err());
}
#[test]
fn namespace_cold_refuses_missing_changed_unknown_and_unlinked_effects() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let (input, receipt, _) = fixture(&path, true);
    assert!(RecoveryStore::open(&path, &id(99)).is_err());
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    let mut wrong = input.request.clone();
    wrong.expected_sequence += 1;
    assert!(r.acknowledge(&wrong, Ok(receipt.clone())).is_err());
    for e in [
        namespace::Error::Unavailable,
        namespace::Error::Internal,
        namespace::Error::OperationMismatch,
        namespace::Error::DeletePending,
    ] {
        assert!(r.acknowledge(&input.request, Err(e)).is_err());
        assert!(r.get(&id(6)).unwrap().outcome.is_none());
    }
    let mut wrong = receipt;
    wrong.node.name = "other".into();
    assert!(r.acknowledge(&input.request, Ok(wrong)).is_err());
    r.acknowledge(&input.request, Err(namespace::Error::Conflict))
        .unwrap();
    r.acknowledge_open(&id(8), &id(12), &id(6)).unwrap();
    assert_eq!(
        r.get(&id(6)).unwrap().outcome,
        Some(Err(namespace::Error::Conflict))
    );
    assert!(r.open_record(&id(8)).unwrap().reply(&id(10)).is_none());
    let path = temp.path().join("missing");
    let (input, receipt, _) = fixture(&path, false);
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    assert!(r.get(&id(6)).is_err());
    assert!(r.acknowledge(&input.request, Ok(receipt)).is_err());
    assert!(r.acknowledge_open(&id(8), &id(12), &id(6)).is_err());
    assert!(r.records().unwrap().is_empty());
}
#[test]
fn namespace_cold_checks_whole_role_and_late_triggers_without_migration() {
    for sql in [
        "PRAGMA user_version=46",
        "INSERT INTO chunk_client_role VALUES(1,1,zeroblob(32))",
        "DELETE FROM chunk_mount_role",
        "PRAGMA application_id=1",
        "UPDATE windows_open_journal SET state_hash=zeroblob(32)",
        "UPDATE source_bindings SET config_fingerprint=replace(config_fingerprint,'b','c')",
        "UPDATE sparse_view_files SET state_hash=zeroblob(32)",
        "CREATE TRIGGER late_recovery AFTER UPDATE ON namespace_intents BEGIN SELECT 1; END",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        let (input, receipt, _) = fixture(&path, true);
        let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
        r.connection.execute_batch(sql).unwrap();
        assert!(r.records().is_err(), "{sql}");
        assert!(r.acknowledge(&input.request, Ok(receipt)).is_err(), "{sql}");
        drop(r);
        assert!(RecoveryStore::open(&path, &id(1)).is_err(), "{sql}");
    }
}
#[test]
fn namespace_cold_real_sqlite_full_preserves_original_outcome_and_retry() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("state");
    let (input, receipt, _) = fixture(&path, true);
    let key = RecoveryStore::open(&path, &id(1)).unwrap().key.clone();
    let mut state = State::open_managed(&path).unwrap();
    let mut candidates = Vec::new();
    for n in 100..132 {
        let mut i = input.clone();
        i.request.operation = id(n);
        i.entries[0].id = id(n);
        let Mutation::ArmRemoval { node, .. } = &mut i.request.mutation else {
            panic!()
        };
        node.id = id(n);
        Journal::for_chunks(&mut state, crate::journal::Options::default(), &key)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        let mut out = receipt.clone();
        out.node.id = id(n);
        out.sequence = u64::from(n);
        out.node.revision = out.sequence;
        out.artifact = Some(namespace::Artifact::Removal(namespace::RemovalFence {
            intent: id(n),
            owner: key.owner().into(),
        }));
        candidates.push((i, out));
    }
    drop(state);
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    r.connection
        .execute_batch("CREATE TABLE cold_recovery_filler(data BLOB)")
        .unwrap();
    let pages: i64 = r
        .connection
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .unwrap();
    r.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    loop {
        match r.connection.execute(
            "INSERT INTO cold_recovery_filler VALUES(?1)",
            [vec![99u8; 3500]],
        ) {
            Ok(_) => (),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DiskFull =>
            {
                break;
            }
            Err(e) => panic!("{e}"),
        }
    }
    let mut failed = None;
    for (input, receipt) in candidates {
        let before = r.records().unwrap();
        match r.acknowledge(&input.request, Ok(receipt.clone())) {
            Ok(_) => (),
            Err(e) => {
                assert!(
                    matches!(e.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::DiskFull),
                    "{e}"
                );
                assert_eq!(r.records().unwrap(), before);
                failed = Some((input, receipt));
                break;
            }
        }
    }
    let (input, receipt) = failed.expect("actual SQLiteFull must interrupt outcome growth");
    r.connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    assert_eq!(
        r.acknowledge(&input.request, Ok(receipt.clone()))
            .unwrap()
            .outcome,
        Some(Ok(receipt))
    );
}
#[test]
fn namespace_cold_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_NAMESPACE_RECOVERY_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let (input, receipt, _) = fixture(&path, true);
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    r.acknowledge(&input.request, Ok(receipt)).unwrap();
    r.acknowledge_open(&id(8), &id(12), &id(6)).unwrap();
    panic!("expected crash checkpoint missing");
}
#[test]
fn namespace_cold_killed_owners_preserve_atomic_outcome_and_link() {
    use std::{
        process::{Command, Stdio},
        time::Instant,
    };
    for point in [
        "OutcomeStored",
        "OutcomeCommitted",
        "OpenStored",
        "OpenCommitted",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("killed 日本語");
        let ready = temp.path().join("ready");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "namespace_client::recovery::tests::namespace_cold_crash_child",
                "--nocapture",
            ])
            .env("S3DRIVE_NAMESPACE_RECOVERY_DIR", &path)
            .env("S3DRIVE_NAMESPACE_RECOVERY_STOP", point)
            .env("S3DRIVE_NAMESPACE_RECOVERY_READY", &ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                use std::io::Read;
                let mut error = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut error)
                    .unwrap();
                panic!("{point}: {status}: {error}");
            }
            if start.elapsed() > Duration::from_secs(45) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{point}: timeout");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(std::fs::read_to_string(&ready).unwrap(), point);
        assert!(RecoveryStore::open(&path, &id(1)).is_err());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
        let saved = r.get(&id(6)).unwrap();
        assert_eq!(saved.outcome.is_some(), point != "OutcomeStored");
        let open = r.open_record(&id(8)).unwrap();
        assert_eq!(open.pending().is_empty(), point == "OpenCommitted");
        assert!(open.reply(&id(10)).is_none());
        assert!(!open.completed(&id(10)));
        assert!(open.cleanup().is_none());
        assert!(open.close().is_none());
    }
}

#[test]
fn namespace_cold_requires_existing_wal_role_and_checks_unselected_records() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("never-created");
    assert!(RecoveryStore::open(&path, &id(1)).is_err());
    assert!(!path.exists());
    let state = State::open_managed(&path).unwrap();
    drop(state);
    assert!(RecoveryStore::open(&path, &id(1)).is_err());
    let path = temp.path().join("state");
    let (mut input, _, _) = fixture(&path, true);
    let key = RecoveryStore::open(&path, &id(1)).unwrap().key.clone();
    let mut state = State::open_managed(&path).unwrap();
    input.request.operation = id(100);
    input.entries[0].id = id(100);
    let Mutation::ArmRemoval { node, .. } = &mut input.request.mutation else {
        panic!()
    };
    node.id = id(100);
    Journal::for_chunks(&mut state, crate::journal::Options::default(), &key)
        .unwrap()
        .begin(input)
        .unwrap();
    drop(state);
    let mut r = RecoveryStore::open(&path, &id(1)).unwrap();
    assert_eq!(
        r.connection
            .execute(
                "UPDATE namespace_intents SET state_hash=zeroblob(32) WHERE operation_id=?",
                [id(100)]
            )
            .unwrap(),
        1
    );
    assert!(r.get(&id(6)).is_err());
    assert!(r.open_record(&id(8)).is_err());
    drop(r);
    assert!(RecoveryStore::open(&path, &id(1)).is_err());
    let path = temp.path().join("not-wal");
    fixture(&path, true);
    let r = RecoveryStore::open(&path, &id(1)).unwrap();
    let mode: String = r
        .connection
        .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "delete");
    drop(r);
    assert!(RecoveryStore::open(&path, &id(1)).is_err());
}

#[test]
fn namespace_cold_startup_classifies_without_migration_or_source_reads() {
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty");
    assert!(RecoveryStore::needs_startup_replay(&empty).is_err());
    drop(State::open_managed(&empty).unwrap());
    assert!(!RecoveryStore::needs_startup_replay(&empty).unwrap());
    let path = temp.path().join("state");
    let (_, _, open) = fixture(&path, true);
    assert!(RecoveryStore::needs_startup_replay(&path).unwrap());
    let mut store = RecoveryStore::open(&path, &id(1)).unwrap();
    assert!(RecoveryStore::needs_startup_replay(&path).is_err());
    assert_eq!(store.open_records().unwrap(), vec![open.clone()]);
    store
        .connection
        .execute(
            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
            [],
        )
        .unwrap();
    let before = source(&store.connection);
    drop(store);
    assert!(RecoveryStore::needs_startup_replay(&path).unwrap());
    let mut store = RecoveryStore::open(&path, &id(1)).unwrap();
    assert_eq!(store.open_records().unwrap(), vec![open]);
    assert_eq!(source(&store.connection), before);
    store
        .connection
        .pragma_update(None, "user_version", 47)
        .unwrap();
    drop(store);
    assert!(!RecoveryStore::needs_startup_replay(&path).unwrap());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        47
    );
    assert_eq!(source(&db), before);
}
#[test]
fn namespace_cold_startup_never_falls_back_from_current_role_corruption() {
    for sql in [
        "DELETE FROM chunk_mount_role",
        "PRAGMA application_id=1",
        "PRAGMA user_version=49",
        "UPDATE windows_open_journal SET state_hash=zeroblob(32)",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state");
        fixture(&path, true);
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        assert!(RecoveryStore::needs_startup_replay(&path).is_err(), "{sql}");
    }
}
