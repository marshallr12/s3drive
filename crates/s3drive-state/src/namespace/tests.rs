use super::*;
use std::{
    io::{BufRead, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn request(v: &Volume, id: u64) -> Apply {
    Apply {
        volume: v.id.clone(),
        operation: format!("{id:032x}"),
        expected_sequence: v.sequence,
        mutation: Mutation::Create {
            parent: ROOT.into(),
            name: format!("file-{id} 🦀"),
            node_kind: Kind::EmptyFile,
        },
    }
}
#[test]
fn migration_preserves_local_rows_and_refuses_role_mixing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    s.register_source(&[1; 32], &"a".repeat(64)).unwrap();
    s.connection.execute_batch("DROP TABLE windows_open_events; DROP TABLE retained_windows_opens; DROP TABLE windows_open_journal; DROP TABLE namespace_removal_events; ALTER TABLE namespace_removals DROP COLUMN mode; ALTER TABLE namespace_removals DROP COLUMN event_id; DROP TABLE logical_conflicts; DROP TABLE managed_rejections; DROP TABLE retained_client_identity; DROP TABLE client_identity; DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; DROP TABLE managed_publications; DROP TABLE namespace_uploads; DROP TABLE namespace_stores; DROP TABLE namespace_operations; DROP TABLE namespace_nodes; DROP TABLE namespace_volume; PRAGMA user_version=7;").unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(s.namespace_initialize(), Err(Error::Invalid));
    let count: i64 = s
        .connection
        .query_row("SELECT count(*) FROM source_bindings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    drop(s);
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let v = s.namespace_initialize().unwrap();
    assert!(s.register_source(&[1; 32], &"a".repeat(64)).is_err());
    assert!(crate::journal::Journal::new(&mut s, crate::journal::Options::default()).is_err());
    assert_eq!(s.namespace_volume().unwrap(), v);
}
#[test]
fn migration_failure_keeps_schema_seven_and_original_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    s.register_source(&[1; 32], &"a".repeat(64)).unwrap();
    s.connection.execute_batch("DROP TABLE windows_open_events; DROP TABLE retained_windows_opens; DROP TABLE windows_open_journal; DROP TABLE namespace_removal_events; ALTER TABLE namespace_removals DROP COLUMN mode; ALTER TABLE namespace_removals DROP COLUMN event_id; DROP TABLE logical_conflicts; DROP TABLE managed_rejections; DROP TABLE retained_client_identity; DROP TABLE client_identity; DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; DROP TABLE logical_publication_links; DROP TABLE logical_edits; DROP TABLE logical_files; DROP TABLE managed_publications; DROP TABLE namespace_uploads; DROP TABLE namespace_stores; DROP TABLE namespace_operations; DROP TABLE namespace_nodes; DROP TABLE namespace_volume; CREATE TABLE namespace_nodes(keep TEXT); INSERT INTO namespace_nodes(keep) VALUES('untouched'); PRAGMA user_version=7;").unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        7
    );
    let original: String = db
        .query_row("SELECT keep FROM namespace_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(original, "untouched");
    assert!(db.prepare("SELECT * FROM namespace_volume").is_err());
}
#[test]
fn failed_result_insert_rolls_back_name_and_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let v = s.namespace_initialize().unwrap();
    let r = request(&v, 1);
    s.connection.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON namespace_operations BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END;").unwrap();
    assert_eq!(s.namespace_apply(&r), Err(Error::Internal));
    assert_eq!(s.namespace_volume().unwrap(), v);
    assert!(s.namespace_list(&v.id, ROOT).unwrap().children.is_empty());
    assert_eq!(s.namespace_operation(&v.id, &r.operation), Ok(None));
    s.connection
        .execute_batch("DROP TRIGGER fail_receipt")
        .unwrap();
    let receipt = s.namespace_apply(&r).unwrap();
    assert_eq!(receipt.sequence, 1);
    // Refused deletion leaves the exact prior node and all names intact.
    let dir_request = Apply {
        volume: v.id.clone(),
        expected_sequence: 1,
        operation: format!("{:032x}", 2),
        mutation: Mutation::Create {
            parent: ROOT.into(),
            name: "folder".into(),
            node_kind: Kind::Directory,
        },
    };
    let folder = s.namespace_apply(&dir_request).unwrap();
    let mv = Apply {
        volume: v.id.clone(),
        expected_sequence: 2,
        operation: format!("{:032x}", 3),
        mutation: Mutation::Move {
            node: receipt.node.id.clone(),
            parent: folder.node.id.clone(),
            name: "moved".into(),
        },
    };
    s.connection.execute_batch("CREATE TRIGGER fail_sequence BEFORE UPDATE ON namespace_volume BEGIN SELECT RAISE(ABORT,'injected after move'); END;").unwrap();
    assert_eq!(s.namespace_apply(&mv), Err(Error::Internal));
    assert_eq!(
        s.namespace_node(&v.id, &receipt.node.id).unwrap(),
        receipt.node
    );
    assert_eq!(s.namespace_volume().unwrap().sequence, 2);
    assert!(
        s.namespace_list(&v.id, &folder.node.id)
            .unwrap()
            .children
            .is_empty()
    );
    assert_eq!(s.namespace_operation(&v.id, &mv.operation), Ok(None));
}
#[test]
fn sqlite_full_never_commits_partial_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let mut v = s.namespace_initialize().unwrap();
    s.connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let mut full = false;
    for id in 1..200 {
        let r = request(&v, id);
        match s.namespace_apply(&r) {
            Ok(receipt) => v.sequence = receipt.sequence,
            Err(Error::Internal) => {
                full = true;
                assert_eq!(s.namespace_volume().unwrap(), v);
                assert_eq!(s.namespace_operation(&v.id, &r.operation), Ok(None));
                assert_eq!(s.namespace_node(&v.id, &r.operation), Err(Error::NotFound));
                assert_eq!(
                    s.namespace_list(&v.id, ROOT).unwrap().children.len() as u64,
                    v.sequence
                );
                break;
            }
            e => panic!("unexpected: {e:?}"),
        }
    }
    assert!(full, "test must reach real SQLite full condition");
}
#[test]
fn retained_limits_and_listing_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let v = s.namespace_initialize().unwrap();
    {
        let tx = s.connection.transaction().unwrap();
        for id in 1..MAX_NODES {
            tx.execute(
                "INSERT INTO namespace_nodes(node_id,parent_id,name,kind,revision,deleted) VALUES(?1,?2,?3,'empty-file',0,0)",
                params![format!("{id:032x}"), ROOT, format!("name{id}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    assert_eq!(s.namespace_list(&v.id, ROOT), Err(Error::Limit));
    let r = request(&v, 99999);
    assert_eq!(s.namespace_apply(&r), Err(Error::Limit));
    assert_eq!(
        s.namespace_operation(&v.id, &r.operation),
        Ok(Some(Err(Error::Limit)))
    );
    assert_eq!(s.namespace_volume().unwrap(), v);
    {
        let tx = s.connection.transaction().unwrap();
        for id in 1..MAX_OPERATIONS {
            tx.execute(
                "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
                [format!("{:032x}", 100_000 + id)],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let r = request(&v, 99998);
    assert_eq!(s.namespace_apply(&r), Err(Error::Limit));
    assert_eq!(s.namespace_operation(&v.id, &r.operation), Ok(None));
}
#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_AUTHORITY_CRASH_CHILD") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let v = s.namespace_initialize().unwrap();
    s.namespace_apply(&request(&v, 77)).unwrap();
    println!("namespace-committed");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    panic!("parent should terminate child before graceful close");
}
#[test]
fn forced_process_death_retains_metadata_and_operation_result() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authority");
    let v = {
        let mut s = State::open_managed(&path).unwrap();
        s.namespace_initialize().unwrap()
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "namespace::tests::crash_child", "--nocapture"])
        .env("S3DRIVE_AUTHORITY_CRASH_CHILD", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            let line = line.unwrap();
            if line == "namespace-committed" {
                send.send(()).unwrap();
                return;
            }
        }
    });
    let signaled = receive.recv_timeout(Duration::from_secs(10));
    child.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "child termination timed out");
        std::thread::yield_now();
    }
    reader.join().unwrap();
    signaled.unwrap();
    let mut s = State::open_managed(&path).unwrap();
    let r = request(&v, 77);
    let receipt = s.namespace_apply(&r).unwrap();
    assert_eq!(receipt.sequence, 1);
    assert_eq!(
        s.namespace_operation(&v.id, &r.operation),
        Ok(Some(Ok(receipt)))
    );
    assert_eq!(s.namespace_list(&v.id, ROOT).unwrap().children.len(), 1);
}

#[test]
fn content_cas_allows_independent_files_and_renames_but_rejects_stale_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let volume = s.namespace_initialize().unwrap().id;
    let id = |n: u64| format!("{n:032x}");
    let apply = |s: &mut State, operation: u64, mutation: Mutation| {
        let seq = s.namespace_volume().unwrap().sequence;
        s.namespace_apply(&Apply {
            volume: volume.clone(),
            operation: id(operation),
            expected_sequence: seq,
            mutation,
        })
        .unwrap()
    };
    apply(
        &mut s,
        200,
        Mutation::RegisterStore {
            route: "a".repeat(64),
            root: "internal/".into(),
        },
    );
    for n in [10, 11] {
        apply(
            &mut s,
            n,
            Mutation::Create {
                parent: ROOT.into(),
                name: format!("file-{n}"),
                node_kind: Kind::EmptyFile,
            },
        );
    }
    let seq = s.namespace_volume().unwrap().sequence;
    let reserve = |op, node| Apply {
        volume: volume.clone(),
        operation: id(op),
        expected_sequence: seq,
        mutation: Mutation::ReserveContent {
            node: id(node),
            expected_revision: 1,
            store: id(200),
            size: 0,
            sha256: "b".repeat(64),
            expected_payload: None,
        },
    };
    let first = reserve(1000, 10);
    let second = reserve(1001, 11);
    let loser = reserve(1002, 10);
    let r = s.namespace_apply(&first).unwrap();
    assert_eq!(s.namespace_apply(&first), Ok(r));
    s.namespace_apply(&second).unwrap();
    s.namespace_apply(&loser).unwrap();
    apply(
        &mut s,
        2000,
        Mutation::Move {
            node: id(10),
            parent: ROOT.into(),
            name: "renamed 🦀".into(),
        },
    );
    let publish = |op, reservation| Apply {
        volume: volume.clone(),
        operation: id(op),
        expected_sequence: seq,
        mutation: Mutation::Publish {
            reservation: id(reservation),
            upload_operation: "c".repeat(64),
            etag: "\"tag\"".into(),
            version: None,
        },
    };
    let winner = s.namespace_apply(&publish(3000, 1000)).unwrap();
    assert_eq!(winner.node.name, "renamed 🦀");
    assert!(winner.sequence > seq + 1);
    assert!(s.namespace_apply(&publish(3001, 1001)).is_ok());
    assert_eq!(
        s.namespace_apply(&publish(3002, 1002)),
        Err(Error::Conflict)
    );
    let stale = Apply {
        volume: volume.clone(),
        operation: id(4000),
        expected_sequence: seq,
        mutation: Mutation::Reserve {
            node: id(11),
            expected_revision: 1,
            store: id(200),
            size: 0,
            sha256: "d".repeat(64),
        },
    };
    assert_eq!(s.namespace_apply(&stale), Err(Error::Conflict));
    let reference = payload_reference(&winner.node.payload).unwrap();
    assert_eq!(reference.len(), 64);
    // Full payload reference differs for a new publication of identical bytes.
    let mut other = winner.node.payload.clone();
    other.as_mut().unwrap().reservation.id = id(9999);
    assert_ne!(payload_reference(&other), Some(reference));
    let mut mismatch = first.clone();
    mismatch.expected_sequence += 1;
    assert_eq!(s.namespace_apply(&mismatch), Err(Error::OperationMismatch));
    drop(s);
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    assert_eq!(s.namespace_apply(&publish(3000, 1000)), Ok(winner));
}

#[test]
fn portable_creation_guards_parent_and_name_without_unrelated_sequence_conflicts() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let mut s = State::open_managed(&path).unwrap();
    let v = s.namespace_initialize_with_policy(names::PORTABLE).unwrap();
    s.namespace_apply(&Apply {
        volume: v.id.clone(),
        operation: format!("{:032x}", 200),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "a".repeat(64),
            root: "writable/.s3drive/".into(),
        },
    })
    .unwrap();
    let make = |op: u64, name: &str| Apply {
        volume: v.id.clone(),
        operation: format!("{op:032x}"),
        expected_sequence: 0,
        mutation: Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: Some(format!("{:032x}", 200)),
            name: name.into(),
            node_kind: Kind::EmptyFile,
        },
    };
    let a = make(1, "café");
    let r = s.namespace_apply(&a).unwrap();
    assert_eq!(s.namespace_apply(&a), Ok(r));
    assert!(s.namespace_apply(&make(2, "independent 🦀")).is_ok());
    assert_eq!(
        s.namespace_apply(&make(3, "cafe\u{301}")),
        Err(Error::AlreadyExists)
    );
    assert_eq!(
        s.namespace_apply(&make(4, "CAFÉ")),
        Err(Error::AlreadyExists)
    );
    assert_eq!(s.namespace_apply(&make(5, "con.txt")), Err(Error::Invalid));
    let mut legacy = make(6, "ＣＡＦÉ");
    legacy.expected_sequence = s.namespace_volume().unwrap().sequence;
    legacy.mutation = Mutation::Create {
        parent: ROOT.into(),
        name: "ＣＡＦÉ".into(),
        node_kind: Kind::Directory,
    };
    assert_eq!(
        s.namespace_apply(&legacy),
        Err(Error::AlreadyExists),
        "legacy clients must not bypass a portable volume's policy"
    );
    let d = Apply {
        volume: v.id.clone(),
        operation: format!("{:032x}", 10),
        expected_sequence: s.namespace_volume().unwrap().sequence,
        mutation: Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: None,
            name: "dir".into(),
            node_kind: Kind::Directory,
        },
    };
    let directory = s.namespace_apply(&d).unwrap().node;
    let stale = Apply {
        volume: v.id.clone(),
        operation: format!("{:032x}", 12),
        expected_sequence: s.namespace_volume().unwrap().sequence,
        mutation: Mutation::CreateEntry {
            parent: directory.id.clone(),
            parent_revision: directory.revision,
            store: Some(format!("{:032x}", 200)),
            name: "nested".into(),
            node_kind: Kind::EmptyFile,
        },
    };
    s.namespace_apply(&Apply {
        volume: v.id.clone(),
        operation: format!("{:032x}", 11),
        expected_sequence: s.namespace_volume().unwrap().sequence,
        mutation: Mutation::Move {
            node: directory.id.clone(),
            parent: ROOT.into(),
            name: "moved".into(),
        },
    })
    .unwrap();
    assert_eq!(s.namespace_apply(&stale), Err(Error::Conflict));
    assert!(
        s.namespace_initialize().is_err(),
        "no implicit policy conversion"
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        s.namespace_initialize_with_policy(names::PORTABLE)
            .unwrap()
            .id,
        v.id
    );
    assert_eq!(s.namespace_list(&v.id, ROOT).unwrap().children.len(), 3);
}

#[test]
fn portable_name_key_corruption_is_not_hidden_by_listing_or_lookup() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
    let v = s.namespace_initialize_with_policy(names::PORTABLE).unwrap();
    s.namespace_apply(&Apply {
        volume: v.id.clone(),
        operation: format!("{:032x}", 200),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "a".repeat(64),
            root: "writable/.s3drive/".into(),
        },
    })
    .unwrap();
    let r = s
        .namespace_apply(&Apply {
            volume: v.id.clone(),
            operation: format!("{:032x}", 1),
            expected_sequence: 0,
            mutation: Mutation::CreateEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                store: Some(format!("{:032x}", 200)),
                name: "café".into(),
                node_kind: Kind::EmptyFile,
            },
        })
        .unwrap();
    s.connection
        .execute(
            "UPDATE namespace_nodes SET name_key='wrong' WHERE node_id=?",
            [&r.node.id],
        )
        .unwrap();
    assert_eq!(s.namespace_node(&v.id, &r.node.id), Err(Error::Internal));
    assert_eq!(s.namespace_list(&v.id, ROOT), Err(Error::Internal));
    assert_eq!(
        s.namespace_apply(&Apply {
            volume: v.id.clone(),
            operation: format!("{:032x}", 2),
            expected_sequence: 1,
            mutation: Mutation::CreateEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                store: Some(format!("{:032x}", 200)),
                name: "CAFÉ".into(),
                node_kind: Kind::EmptyFile
            }
        }),
        Err(Error::Internal)
    );
}
#[test]
fn schema11_upgrade_preserves_exact_names_and_refuses_incompatible_tables_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    for broken in [false, true] {
        let path = tmp.path().join(if broken { "bad" } else { "good" });
        let mut s = State::open_managed(&path).unwrap();
        let v = s.namespace_initialize().unwrap();
        let created = s.namespace_apply(&request(&v, 1)).unwrap();
        s.connection.execute_batch("DROP TABLE windows_open_events; DROP TABLE retained_windows_opens; DROP TABLE windows_open_journal; DROP TABLE namespace_removal_events; ALTER TABLE namespace_removals DROP COLUMN mode; ALTER TABLE namespace_removals DROP COLUMN event_id; DROP TABLE logical_conflicts; DROP TABLE managed_rejections; DROP TABLE retained_client_identity; DROP TABLE client_identity; DROP TABLE namespace_removals; DROP TABLE namespace_intents; DROP INDEX namespace_portable_names; ALTER TABLE namespace_nodes DROP COLUMN name_key; ALTER TABLE namespace_nodes DROP COLUMN placement; PRAGMA user_version=11").unwrap();
        if broken {
            s.connection.execute_batch("CREATE TABLE namespace_intents(keep TEXT); INSERT INTO namespace_intents VALUES('untouched')").unwrap();
        }
        drop(s);
        if broken {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                11
            );
            assert_eq!(
                db.query_row::<String, _, _>("SELECT keep FROM namespace_intents", [], |r| r
                    .get(0))
                    .unwrap(),
                "untouched"
            );
            assert!(db.prepare("SELECT name_key FROM namespace_nodes").is_err());
        } else {
            let mut s = State::open_managed(&path).unwrap();
            assert_eq!(s.namespace_initialize().unwrap().id, v.id);
            assert_eq!(
                s.namespace_node(&v.id, &created.node.id).unwrap(),
                created.node
            );
            assert!(s.namespace_initialize_with_policy(names::PORTABLE).is_err());
        }
    }
}

#[test]
fn portable_empty_file_placement_is_authoritative_before_first_publication() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
    let v = s.namespace_initialize_with_policy(names::PORTABLE).unwrap();
    for id in [200u64, 201] {
        s.namespace_apply(&Apply {
            volume: v.id.clone(),
            operation: format!("{id:032x}"),
            expected_sequence: s.namespace_volume().unwrap().sequence,
            mutation: Mutation::RegisterStore {
                route: format!("{id:064x}"),
                root: "writable/.s3drive/".into(),
            },
        })
        .unwrap();
    }
    let created = s
        .namespace_apply(&Apply {
            volume: v.id.clone(),
            operation: format!("{:032x}", 10),
            expected_sequence: 2,
            mutation: Mutation::CreateEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                name: "empty 🦀".into(),
                node_kind: Kind::EmptyFile,
                store: Some(format!("{:032x}", 200)),
            },
        })
        .unwrap();
    assert!(created.node.payload.is_none());
    assert_eq!(
        created.node.placement.as_ref().unwrap().id,
        format!("{:032x}", 200)
    );
    assert_eq!(
        s.namespace_apply(&Apply {
            volume: v.id.clone(),
            operation: format!("{:032x}", 20),
            expected_sequence: 3,
            mutation: Mutation::ReserveContent {
                node: created.node.id,
                expected_revision: created.node.revision,
                store: format!("{:032x}", 201),
                size: 0,
                sha256: "a".repeat(64),
                expected_payload: None
            }
        }),
        Err(Error::Conflict)
    );
}
