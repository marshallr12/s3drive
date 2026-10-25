use super::*;
use crate::{cache, namespace_client::opens, sparse_views};
fn id(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}
fn control(n: u8) -> Control {
    Control {
        store: ns::Store {
            id: id(n),
            route: "a".repeat(64),
            root: format!("managed/{n}/"),
        },
        source: [n; 32],
        fingerprint: "a".repeat(64),
    }
}
fn input() -> Input {
    Input {
        volume: id(1),
        controls: vec![control(3)],
    }
}
fn sparse(input: &Input, node: u8) -> sparse_views::Binding {
    let c = &input.controls[0];
    let store: [u8; 16] =
        std::array::from_fn(|i| u8::from_str_radix(&c.store.id[2 * i..2 * i + 2], 16).unwrap());
    sparse_views::Binding::new(
        s3drive_core::manifest::base::Descriptor::from_hex(
            &input.volume,
            &id(node),
            &id(90),
            &"a".repeat(64),
        )
        .unwrap(),
        store,
        c.source,
        c.fingerprint.clone(),
        c.store.root.clone(),
        4,
    )
    .unwrap()
}
fn creation(input: &Input, op: u8) -> crate::manifest_creation::Input {
    let c = &input.controls[0];
    crate::manifest_creation::Input {
        volume: input.volume.clone(),
        operation: id(op),
        expected_sequence: 0,
        parent: ns::NodeGuard {
            id: ns::ROOT.into(),
            revision: 0,
        },
        name: "café 日本語 🦀.bin".into(),
        store: c.store.clone(),
        source: c.source,
        fingerprint: c.fingerprint.clone(),
        owner: None,
        open: None,
    }
}
fn directory(input: &Input, op: u8) -> names::Input {
    let c = &input.controls[0];
    names::Input {
        request: ns::Apply {
            volume: input.volume.clone(),
            operation: id(op),
            expected_sequence: 0,
            mutation: ns::Mutation::CreateEntry {
                parent: ns::ROOT.into(),
                parent_revision: 0,
                name: "日本語 folder".into(),
                node_kind: ns::Kind::Directory,
                store: None,
            },
        },
        store: c.store.clone(),
        source: c.source,
        fingerprint: c.fingerprint.clone(),
        entries: vec![],
        destination_control: None,
    }
}
fn rows(state: &State) -> Vec<String> {
    let mut out = Vec::new();
    for table in [
        "chunk_mount_role",
        "client_identity",
        "retained_client_identity",
        "source_bindings",
    ] {
        let columns = if table == "chunk_mount_role" {
            "quote(singleton)||quote(record)||quote(state_hash)"
        } else if table == "client_identity" {
            "quote(singleton)||quote(owner_id)||quote(state_hash)"
        } else if table == "retained_client_identity" {
            "quote(singleton)||quote(state_hash)"
        } else {
            "quote(source_id)||quote(config_fingerprint)"
        };
        let mut q = state
            .connection
            .prepare(&format!("SELECT {columns} FROM {table} ORDER BY 1"))
            .unwrap();
        for row in q.query_map([], |r| r.get::<_, String>(0)).unwrap() {
            out.push(format!("{table}:{}", row.unwrap()));
        }
    }
    out
}
#[test]
fn role_admission_is_stable_append_only_and_reciprocally_excludes_old_roles() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut state = State::open_managed(&path).unwrap();
    let declared = input();
    let key = admit(&mut state, declared.clone()).unwrap();
    let before = rows(&state);
    assert_eq!(admit(&mut state, declared.clone()).unwrap(), key);
    assert_eq!(rows(&state), before);
    assert!(names::Journal::new(&mut state).is_err());
    assert!(opens::Journal::new(&mut state).is_err());
    assert!(crate::logical::Journal::new(&mut state, journal::Options::default()).is_err());
    assert!(crate::managed::Journal::new(&mut state, journal::Options::default()).is_err());
    assert!(state.namespace_initialize().is_err());
    let mut second = declared.clone();
    second.controls.push(control(4));
    assert_eq!(admit(&mut state, second.clone()).unwrap(), key);
    assert_eq!(load(&state.connection).unwrap().unwrap().controls.len(), 2);
    let mut changed = second.clone();
    changed.controls[0].store.root = "elsewhere/".into();
    let before = rows(&state);
    assert!(admit(&mut state, changed).is_err());
    assert_eq!(rows(&state), before);
    let mut changed = second.clone();
    changed.volume = id(2);
    assert!(admit(&mut state, changed).is_err());
    assert_eq!(rows(&state), before);
    let tables: i64 = state
        .connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(tables <= 65, "actual table count {tables}");
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    assert_eq!(admit(&mut state, declared).unwrap(), key);
}
#[test]
fn fresh_and_preexisting_sparse_creation_intents_compose_in_both_admission_orders() {
    for before in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        let input = input();
        if before {
            sparse_views::Journal::new(&mut state, sparse_views::Options::default())
                .unwrap()
                .begin([1; 32], sparse(&input, 10), 4)
                .unwrap();
            crate::manifest_creation::Journal::new(
                &mut state,
                crate::manifest_creation::Limits::default(),
            )
            .unwrap()
            .begin(creation(&input, 20))
            .unwrap();
        }
        let key = admit(&mut state, input.clone()).unwrap();
        if !before {
            sparse_views::Journal::new(&mut state, sparse_views::Options::default())
                .unwrap()
                .begin([1; 32], sparse(&input, 10), 4)
                .unwrap();
            crate::manifest_creation::Journal::new(
                &mut state,
                crate::manifest_creation::Limits::default(),
            )
            .unwrap()
            .begin(creation(&input, 20))
            .unwrap();
        }
        let mut n =
            names::Journal::for_chunks(&mut state, journal::Options::default(), &key).unwrap();
        n.begin(directory(&input, 30)).unwrap();
        assert!(names::Journal::new(&mut state).is_err());
        let mut bad = creation(&input, 21);
        bad.volume = id(2);
        assert!(
            crate::manifest_creation::Journal::new(
                &mut state,
                crate::manifest_creation::Limits::default()
            )
            .unwrap()
            .begin(bad)
            .is_err()
        );
        let mut bad = input.clone();
        bad.controls = vec![control(4)];
        assert!(
            sparse_views::Journal::new(&mut state, sparse_views::Options::default())
                .unwrap()
                .begin([2; 32], sparse(&bad, 11), 4)
                .is_err()
        );
        drop(state);
        let mut state = State::open_managed(&path).unwrap();
        names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
            .unwrap()
            .get(&id(30))
            .unwrap();
    }
}
#[test]
fn shared_namespace_receipts_and_open_sessions_survive_restart_without_old_writer_adoption() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("client");
    let mut authority = State::open_managed(&tmp.path().join("authority")).unwrap();
    let volume = authority
        .namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
        .unwrap();
    let mut input = input();
    input.volume = volume.id.clone();
    let mut state = State::open_managed(&path).unwrap();
    let key = admit(&mut state, input.clone()).unwrap();
    let request = directory(&input, 30);
    names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
        .unwrap()
        .begin(request.clone())
        .unwrap();
    let receipt = authority.namespace_apply(&request.request).unwrap();
    names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
        .unwrap()
        .acknowledge(&id(30), Ok(receipt.clone()))
        .unwrap();
    let c = &input.controls[0];
    let open = opens::OpenInput {
        id: id(40),
        session: id(41),
        volume: input.volume.clone(),
        node: id(30),
        store: c.store.clone(),
        source: c.source,
        fingerprint: c.fingerprint.clone(),
        delete_on_close: false,
        controlled: true,
        session_bound: true,
        posix: false,
        admission: None,
        creation: None,
    };
    let mut journal = opens::Journal::for_chunks(&mut state, &key).unwrap();
    journal.start_session(&id(41), &input.volume).unwrap();
    journal.prepare(open.clone()).unwrap();
    assert!(journal.activate(&id(40), &id(42)).unwrap().active());
    drop(state);
    let mut state = State::open_managed(&path).unwrap();
    let reopened = admit(&mut state, input.clone()).unwrap();
    assert_eq!(reopened, key);
    let journal = opens::Journal::for_chunks(&mut state, &key).unwrap();
    assert!(journal.obsolete_session(&id(41), &input.volume).unwrap());
    assert_eq!(journal.get(&id(40)).unwrap().input, open);
    assert_eq!(
        names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
            .unwrap()
            .get(&id(30))
            .unwrap()
            .outcome,
        Some(Ok(receipt))
    );
}
#[test]
fn populated_legacy_or_wrong_scope_and_late_triggers_never_gain_a_combined_role() {
    let tmp = tempfile::tempdir().unwrap();
    let mut old = State::open_managed(&tmp.path().join("old")).unwrap();
    names::Journal::new(&mut old)
        .unwrap()
        .client_identity()
        .unwrap();
    let before = rows(&old);
    assert!(admit(&mut old, input()).is_err());
    assert_eq!(rows(&old), before);
    let mut state = State::open_managed(&tmp.path().join("state")).unwrap();
    let declared = input();
    let key = admit(&mut state, declared.clone()).unwrap();
    let mut other = State::open_managed(&tmp.path().join("other")).unwrap();
    admit(&mut other, declared.clone()).unwrap();
    assert!(names::Journal::for_chunks(&mut other, journal::Options::default(), &key).is_err());
    let mut journal =
        names::Journal::for_chunks(&mut state, journal::Options::default(), &key).unwrap();
    let bad = directory(
        &Input {
            volume: id(2),
            ..declared.clone()
        },
        10,
    );
    assert!(journal.begin(bad).is_err());
    let mut bad = directory(&declared, 10);
    if let ns::Mutation::CreateEntry { node_kind, .. } = &mut bad.request.mutation {
        *node_kind = ns::Kind::EmptyFile;
    }
    assert!(journal.begin(bad).is_err());
    state
        .connection
        .execute_batch(
            "CREATE TRIGGER late_profile AFTER INSERT ON namespace_intents BEGIN SELECT 1; END;",
        )
        .unwrap();
    let before = rows(&state);
    assert!(admit(&mut state, declared).is_err());
    assert_eq!(rows(&state), before);
}
#[tokio::test]
async fn capability_clients_use_exact_storage_owner_and_close_with_all_clones() {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let storage = Storage::open(
            &path,
            cache::Options::default(),
            journal::Options::default(),
        )
        .await
        .unwrap();
        let input = input();
        let client = storage.chunk_mount(input.clone()).await.unwrap();
        assert!(client.same_storage(&storage));
        assert_eq!(
            client.namespace().client_identity().await.unwrap(),
            client.key().owner()
        );
        client
            .namespace()
            .begin(directory(&input, 30))
            .await
            .unwrap();
        client
            .opens()
            .start_session(id(40), input.volume.clone())
            .await
            .unwrap();
        assert!(storage.namespace().get(id(30)).await.is_err());
        assert!(storage.opens().records().await.is_err());
        storage.close().await.unwrap();
        assert!(client.namespace().get(id(30)).await.is_err());
        let storage = Storage::open(
            &path,
            cache::Options::default(),
            journal::Options::default(),
        )
        .await
        .unwrap();
        let reopened = storage.chunk_mount(input).await.unwrap();
        assert!(!client.same_storage(&storage));
        assert_eq!(reopened.key(), client.key());
        storage.close().await.unwrap();
    })
    .await
    .unwrap();
}

#[test]
fn namespace_ack_keeps_new_dirty_head_and_saved_replay_ignores_damaged_source_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut authority = State::open_managed(&tmp.path().join("authority")).unwrap();
    let volume = authority
        .namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
        .unwrap();
    let mut input = input();
    input.volume = volume.id.clone();
    let c = input.controls[0].clone();
    let mut seq = 0;
    for (op, mutation) in [
        (
            3,
            ns::Mutation::RegisterStore {
                route: c.store.route.clone(),
                root: c.store.root.clone(),
            },
        ),
        (
            10,
            ns::Mutation::CreateEmptyManifest {
                parent: ns::ROOT.into(),
                parent_revision: 0,
                name: "café.bin".into(),
                store: c.store.id.clone(),
                upload_operation: "b".repeat(64),
                etag: "\"model\"".into(),
                version: None,
            },
        ),
    ] {
        seq = authority
            .namespace_apply(&ns::Apply {
                volume: input.volume.clone(),
                operation: id(op),
                expected_sequence: seq,
                mutation,
            })
            .unwrap()
            .sequence;
    }
    let node = authority.namespace_node(&input.volume, &id(10)).unwrap();
    let payload = node.payload.as_ref().unwrap();
    let descriptor = s3drive_core::manifest::base::Descriptor::from_hex(
        &input.volume,
        &node.id,
        &payload.reservation.id,
        &ns::payload_reference(&node.payload).unwrap(),
    )
    .unwrap();
    let binding = sparse_views::Binding::new(
        descriptor,
        [3; 16],
        c.source,
        c.fingerprint.clone(),
        c.store.root.clone(),
        0,
    )
    .unwrap();
    let path = tmp.path().join("client");
    let mut state = State::open_managed(&path).unwrap();
    let key = admit(&mut state, input.clone()).unwrap();
    let initial = sparse_views::Journal::new(&mut state, sparse_views::Options::default())
        .unwrap()
        .begin([1; 32], binding, 0)
        .unwrap();
    let move_input = names::Input {
        request: ns::Apply {
            volume: input.volume.clone(),
            operation: id(20),
            expected_sequence: seq,
            mutation: ns::Mutation::MoveEntry {
                node: ns::NodeGuard {
                    id: node.id.clone(),
                    revision: node.revision,
                },
                source_parent: ns::NodeGuard {
                    id: ns::ROOT.into(),
                    revision: 0,
                },
                parent: ns::NodeGuard {
                    id: ns::ROOT.into(),
                    revision: 0,
                },
                name: "renamed 日本語.bin".into(),
                destination: None,
                destination_removal: None,
            },
        },
        store: c.store.clone(),
        source: c.source,
        fingerprint: c.fingerprint.clone(),
        entries: vec![names::Entry::from_node(&node).unwrap()],
        destination_control: None,
    };
    names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
        .unwrap()
        .begin(move_input.clone())
        .unwrap();
    let receipt = authority.namespace_apply(&move_input.request).unwrap();
    let dirty = sparse_views::Journal::new(&mut state, sparse_views::Options::default())
        .unwrap()
        .write([2; 32], initial.id(), 0, b"unpublished local bytes")
        .unwrap();
    // A source-body fault after server commit must not prevent saving its exact
    // metadata receipt, or replace the newer local head with the old snapshot.
    state
        .connection
        .execute(
            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
            [],
        )
        .unwrap();
    let before: Vec<u8> = state
        .connection
        .query_row("SELECT head FROM sparse_view_files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, dirty.id());
    let saved = names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
        .unwrap()
        .acknowledge(&id(20), Ok(receipt.clone()))
        .unwrap();
    assert_eq!(saved.outcome, Some(Ok(receipt.clone())));
    assert_eq!(
        names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
            .unwrap()
            .begin(move_input)
            .unwrap()
            .outcome,
        Some(Ok(receipt.clone()))
    );
    let after: Vec<u8> = state
        .connection
        .query_row("SELECT head FROM sparse_view_files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, before);
    let mut remove = directory(&input, 21);
    remove.request.expected_sequence = receipt.sequence;
    remove.request.mutation = ns::Mutation::RemoveEntry {
        node: ns::NodeGuard {
            id: node.id.clone(),
            revision: receipt.sequence,
        },
        parent: ns::NodeGuard {
            id: ns::ROOT.into(),
            revision: 0,
        },
    };
    remove.entries = vec![names::Entry::from_node(&receipt.node).unwrap()];
    assert!(
        names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
            .unwrap()
            .begin(remove)
            .is_err()
    );
    drop(state);
    assert!(State::open_managed(&path).is_err());
}

#[test]
fn admission_full_disk_and_control_capacity_never_leave_identity_or_partially_append_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&tmp.path().join("state")).unwrap();
    let before = rows(&state);
    let pages: i64 = state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let full = Input {
        volume: id(1),
        controls: (1..=64).map(control).collect(),
    };
    let error = admit(&mut state, full.clone()).unwrap_err();
    assert!(
        matches!(error.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(code,_)) if code.code==rusqlite::ErrorCode::DiskFull),
        "{error}"
    );
    assert_eq!(rows(&state), before);
    state
        .connection
        .pragma_update(None, "max_page_count", 1000000)
        .unwrap();
    let key = admit(&mut state, full.clone()).unwrap();
    let before = rows(&state);
    assert_eq!(
        admit(
            &mut state,
            Input {
                volume: id(1),
                controls: vec![control(1)]
            }
        )
        .unwrap(),
        key
    );
    assert_eq!(rows(&state), before);
    let overflow = Input {
        volume: id(1),
        controls: vec![control(65)],
    };
    assert!(admit(&mut state, overflow).is_err());
    assert_eq!(rows(&state), before);
    assert_eq!(load(&state.connection).unwrap().unwrap().controls.len(), 64);
}

#[test]
fn removed_role_or_downgraded_marker_never_launders_the_independent_chunk_identity() {
    for downgrade in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut state = State::open_managed(&path).unwrap();
        admit(&mut state, input()).unwrap();
        if downgrade {
            state
                .connection
                .execute_batch("DROP TABLE sparse_retention_anchors; DROP TABLE sparse_retention_role; DROP TABLE sparse_retry_mode; DROP TABLE chunk_mount_role; PRAGMA user_version=40")
                .unwrap();
        } else {
            state
                .connection
                .execute("DELETE FROM chunk_mount_role", [])
                .unwrap();
        }
        assert!(names::Journal::new(&mut state).is_err());
        assert!(opens::Journal::new(&mut state).is_err());
        assert!(admit(&mut state, input()).is_err());
        let owner: String = state
            .connection
            .query_row("SELECT owner_id FROM client_identity", [], |r| r.get(0))
            .unwrap();
        drop(state);
        assert!(State::open_managed(&path).is_err());
        let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
        assert_eq!(
            db.query_row("SELECT owner_id FROM client_identity", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            owner
        );
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                .unwrap(),
            if downgrade { 40 } else { crate::SCHEMA_VERSION }
        );
    }
}

#[test]
fn schema40_upgrade_preserves_creation_blobs_and_collision_rolls_back_without_marker_change() {
    for collision in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        crate::manifest_creation::Journal::new(&mut s, crate::manifest_creation::Limits::default())
            .unwrap()
            .begin(creation(&input(), 22))
            .unwrap();
        let blobs = |db: &Connection| -> Vec<(String, Vec<u8>, Vec<u8>)> {
            let mut q = db
                .prepare(
                    "SELECT operation,record,state_hash FROM manifest_creations ORDER BY operation",
                )
                .unwrap();
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        let before = blobs(&s.connection);
        crate::test_schema::without_chunk_mount(&s.connection);
        crate::test_schema::demote(&s.connection, 40).unwrap();
        if collision {
            s.connection
                .execute_batch("CREATE TABLE chunk_mount_role(wrong BLOB)")
                .unwrap();
        }
        drop(s);
        if collision {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                    .unwrap(),
                40
            );
            assert_eq!(blobs(&db), before);
        } else {
            let mut s = State::open_managed(&path).unwrap();
            assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            assert_eq!(blobs(&s.connection), before);
            let key = admit(&mut s, input()).unwrap();
            assert_eq!(blobs(&s.connection), before);
            assert_eq!(load(&s.connection).unwrap().unwrap().owner, key.owner());
        }
    }
}
#[test]
fn composition_crash_worker() {
    let Some(path) = std::env::var_os("S3DRIVE_CHUNK_MOUNT_PATH") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let mut declared = input();
    if std::env::var_os("S3DRIVE_CHUNK_MOUNT_APPEND").is_some() {
        declared.controls.push(control(4));
    }
    admit(&mut s, declared).unwrap();
}
#[test]
fn killed_owners_never_split_identity_sources_and_role_or_lose_existing_controls() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        time::Duration,
    };
    struct Reap(std::process::Child);
    impl Drop for Reap {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for append in [false, true] {
        for boundary in [
            "IdentityStored",
            "SourcesStored",
            "RoleStored",
            "RoleCommitted",
        ] {
            if append && boundary == "IdentityStored" {
                continue;
            }
            let t = tempfile::tempdir().unwrap();
            let path = t.path().join("state");
            let mut s = State::open_managed(&path).unwrap();
            let old = if append {
                Some(admit(&mut s, input()).unwrap())
            } else {
                None
            };
            let before = rows(&s);
            drop(s);
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "chunk_mount::tests::composition_crash_worker",
                    "--nocapture",
                ])
                .env("S3DRIVE_CHUNK_MOUNT_PATH", &path)
                .env("S3DRIVE_CHUNK_MOUNT_STOP", boundary)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            if append {
                command.env("S3DRIVE_CHUNK_MOUNT_APPEND", "1");
            }
            let mut child = Reap(command.spawn().unwrap());
            let stdout = child.0.stdout.take().unwrap();
            let (send, recv) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if line.contains("S3DRIVE_CHUNK_MOUNT_BOUNDARY") {
                        let _ = send.send(line);
                        return;
                    }
                }
            });
            let marker = recv
                .recv_timeout(Duration::from_secs(15))
                .unwrap_or_else(|e| panic!("append={append}/{boundary}: {e}"));
            assert!(marker.contains(boundary));
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            reader.join().unwrap();
            let mut s = State::open_managed(&path).unwrap();
            if boundary != "RoleCommitted" {
                assert_eq!(rows(&s), before);
            }
            if let Some(b) = load(&s.connection).unwrap() {
                assert_eq!(
                    b.controls.len(),
                    if append && boundary == "RoleCommitted" {
                        2
                    } else {
                        1
                    }
                );
                if let Some(old) = old {
                    assert_eq!(b.owner, old.owner());
                }
                names::Journal::for_chunks(
                    &mut s,
                    journal::Options::default(),
                    &Key {
                        volume: b.volume,
                        owner: b.owner,
                    },
                )
                .unwrap();
            } else {
                assert!(!append);
                assert_eq!(rows(&s), before);
            }
            let mut declared = input();
            if append {
                declared.controls.push(control(4));
            }
            admit(&mut s, declared).unwrap();
            assert_eq!(
                load(&s.connection).unwrap().unwrap().controls.len(),
                if append { 2 } else { 1 }
            );
        }
    }
}

fn open_input(declared: &Input, n: u8) -> opens::OpenInput {
    let c = &declared.controls[0];
    opens::OpenInput {
        id: id(n),
        session: id(80),
        volume: declared.volume.clone(),
        node: id(20),
        store: c.store.clone(),
        source: c.source,
        fingerprint: c.fingerprint.clone(),
        delete_on_close: false,
        controlled: true,
        session_bound: true,
        posix: false,
        admission: None,
        creation: None,
    }
}
#[test]
fn unfinished_and_finished_session_recovery_compose_across_restarts_without_synthetic_callbacks() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state");
    let declared = input();
    let mut s = State::open_managed(&path).unwrap();
    let key = admit(&mut s, declared.clone()).unwrap();
    let mut j = opens::Journal::for_chunks(&mut s, &key).unwrap();
    j.start_session(&id(80), &declared.volume).unwrap();
    j.prepare(open_input(&declared, 81)).unwrap();
    j.activate(&id(81), &id(82)).unwrap();
    let original = j.get(&id(81)).unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let mut j = opens::Journal::for_chunks(&mut s, &key).unwrap();
    j.start_session(&id(83), &declared.volume).unwrap();
    let pending = j
        .prepare_recovery(&id(83), &declared.volume, &id(20), None)
        .unwrap();
    assert!(!pending.finished);
    assert_eq!(j.get(&id(81)).unwrap(), original);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(admit(&mut s, declared.clone()).unwrap(), key);
    let mut j = opens::Journal::for_chunks(&mut s, &key).unwrap();
    assert!(
        j.pending_recovery(&declared.volume, &id(20))
            .unwrap()
            .unwrap()
            == pending
    );
    j.start_session(&id(84), &declared.volume).unwrap();
    assert!(
        j.finish_recovery(&id(84), &pending.input.id)
            .unwrap()
            .finished
    );
    assert!(j.recovered(&id(81)).unwrap());
    j.retire(&id(81)).unwrap();
    assert_eq!(j.get(&id(81)).unwrap(), original);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let j = opens::Journal::for_chunks(&mut s, &key).unwrap();
    assert!(j.recovered(&id(81)).unwrap());
    assert!(
        j.pending_recovery(&declared.volume, &id(20))
            .unwrap()
            .is_none()
    );
}
#[test]
fn late_triggers_and_oversized_role_metadata_are_refused_without_mutation() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state");
    let declared = input();
    let mut s = State::open_managed(&path).unwrap();
    let key = admit(&mut s, declared.clone()).unwrap();
    let mut j = opens::Journal::for_chunks(&mut s, &key).unwrap();
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER late_role AFTER INSERT ON windows_mount_sessions BEGIN SELECT 1; END",
    )
    .unwrap();
    assert!(j.start_session(&id(80), &declared.volume).is_err());
    assert!(j.prepare(open_input(&declared, 81)).is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM windows_mount_sessions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER late_role; PRAGMA ignore_check_constraints=ON; UPDATE chunk_mount_role SET record=zeroblob(65537)").unwrap();
    assert!(j.records().is_err());
    assert!(admit(&mut s, declared).is_err());
    assert_eq!(
        db.query_row("SELECT length(record) FROM chunk_mount_role", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        65537
    );
    drop((db, s));
    assert!(State::open_managed(&path).is_err());
}

#[test]
fn bounded_head_enumeration_retains_only_current_views_and_clean_hints_survive_restart() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let declared = input();
    admit(&mut s, declared.clone()).unwrap();
    let mut j = sparse_views::Journal::new(&mut s, sparse_views::Options::default()).unwrap();
    j.enable_scoped().unwrap();
    let a = sparse(&declared, 10);
    let b = sparse(&declared, 11);
    let initial = j
        .begin_scoped(
            sparse_views::ScopedOperation::new(&a, [1; 32]).unwrap(),
            a,
            4,
        )
        .unwrap();
    let other = j
        .begin_scoped(
            sparse_views::ScopedOperation::new(&b, [2; 32]).unwrap(),
            b,
            4,
        )
        .unwrap();
    assert!(initial.is_clean() && other.is_clean());
    let dirty = j
        .write_scoped(
            sparse_views::ScopedOperation::new(initial.binding(), [3; 32]).unwrap(),
            initial.id(),
            1,
            b"\0\xff",
        )
        .unwrap();
    assert!(!dirty.is_clean());
    let heads = j.heads().unwrap();
    assert_eq!(heads.len(), 2);
    assert!(heads.contains(&dirty) && heads.contains(&other) && !heads.contains(&initial));
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let mut j = sparse_views::Journal::new(&mut s, sparse_views::Options::default()).unwrap();
    assert_eq!(j.heads().unwrap(), heads);
    s.connection
        .execute_batch("UPDATE sparse_view_blocks SET data=zeroblob(length(data))")
        .unwrap();
    assert!(sparse_views::Journal::new(&mut s, sparse_views::Options::default()).is_err());
}

#[test]
fn retry_safety_refuses_native_owner_activation_without_changing_default_composition() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let declared = input();
    let key = admit(&mut s, declared.clone()).unwrap();
    let before = rows(&s);
    assert!(
        crate::sparse_views::preparation::Journal::new(
            &mut s,
            crate::sparse_views::preparation::Options::default()
        )
        .unwrap()
        .enable_retry_safety()
        .is_err()
    );
    assert!(
        crate::sparse_views::preparation::Journal::new(
            &mut s,
            crate::sparse_views::preparation::Options::default()
        )
        .unwrap()
        .migrate_retry_safety()
        .is_err()
    );
    assert!(!crate::sparse_views::preparation::retry::enabled(&s.connection).unwrap());
    assert!(!crate::sparse_views::scoped::enabled(&s.connection).unwrap());
    assert_eq!(rows(&s), before);
    assert_eq!(admit(&mut s, declared).unwrap(), key);
}

#[test]
fn native_retry_validation_work_cached_foreign_sql_rereads_rows_and_schema() {
    let mut db = Connection::open_in_memory().unwrap();
    for table in FOREIGN_TABLES {
        db.execute_batch(&format!("CREATE TABLE {table}(value INTEGER)"))
            .unwrap();
    }
    foreign(&db).unwrap();
    for table in FOREIGN_TABLES {
        db.execute(&format!("INSERT INTO {table} VALUES(1)"), [])
            .unwrap();
        assert!(foreign(&db).is_err(), "{table}");
        db.execute(&format!("DELETE FROM {table}"), []).unwrap();
        foreign(&db).unwrap();
        let tx = db.transaction().unwrap();
        tx.execute(&format!("INSERT INTO {table} VALUES(2)"), [])
            .unwrap();
        assert!(foreign(&tx).is_err());
        tx.rollback().unwrap();
        foreign(&db).unwrap();
        db.execute_batch(&format!("DROP TABLE {table}")).unwrap();
        assert!(foreign(&db).is_err(), "missing {table}");
        db.execute_batch(&format!(
            "CREATE TABLE {table}(changed TEXT); INSERT INTO {table} VALUES('present')"
        ))
        .unwrap();
        assert!(foreign(&db).is_err(), "recreated {table}");
        db.execute(&format!("DELETE FROM {table}"), []).unwrap();
        foreign(&db).unwrap();
    }
    assert!(db.is_autocommit());
}

#[test]
fn native_retry_validation_work_quiescent_contract_keeps_profiles_separate_and_admissions_monotonic()
 {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&tmp.path().join("quiescent admission")).unwrap();
    let declared = input();
    let key = admit_quiescent(&mut state, declared.clone()).unwrap();
    assert!(admit_retry(&mut state, declared.clone()).is_err());
    assert!(admit(&mut state, declared.clone()).is_err());
    assert_eq!(admit_quiescent(&mut state, declared.clone()).unwrap(), key);
    let mut create = creation(&declared, 30);
    create.owner = Some(key.owner().into());
    create.operation = admission::next(&state.connection, AdmissionDomain::Creation)
        .unwrap()
        .unwrap();
    let mut journal = crate::manifest_creation::Journal::for_chunks(
        &mut state,
        crate::manifest_creation::Limits::default(),
        &key,
    )
    .unwrap();
    journal.begin(create.clone()).unwrap();
    assert_eq!(journal.begin(create.clone()).unwrap().input(), &create);
    let mut name = directory(&declared, 31);
    name.request.operation = admission::next(&state.connection, AdmissionDomain::Namespace)
        .unwrap()
        .unwrap();
    assert_ne!(create.operation, name.request.operation);
    let mut journal =
        names::Journal::for_chunks(&mut state, journal::Options::default(), &key).unwrap();
    journal.begin(name.clone()).unwrap();
    assert_eq!(journal.begin(name.clone()).unwrap().input, name);
    validate_all(&state.connection).unwrap();
    let before = rows(&state);
    let mut invalid = directory(&declared, 32);
    invalid.request.operation = create.operation;
    assert!(
        names::Journal::for_chunks(&mut state, journal::Options::default(), &key)
            .unwrap()
            .begin(invalid)
            .is_err()
    );
    assert_eq!(rows(&state), before);
}

#[test]
fn native_retry_validation_work_cached_sql_rechecks_live_rows_schema_and_rollback() {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = State::open_managed(&tmp.path().join("fresh cached SQL")).unwrap();
    let db = &mut state.connection;
    for _ in 0..3 {
        check_schema(db).unwrap();
        preflight(db).unwrap();
        assert!(load(db).unwrap().is_none());
    }
    for sql in [
        "INSERT INTO chunk_mount_role(singleton,record,state_hash) VALUES(1,x'00',zeroblob(32))",
        "CREATE TRIGGER cached_sql_guard AFTER UPDATE ON source_bindings BEGIN SELECT 1; END",
        "ALTER TABLE chunk_mount_role RENAME COLUMN record TO wrong_record",
        "PRAGMA user_version=49",
    ] {
        let tx = db.transaction().unwrap();
        tx.execute_batch(sql).unwrap();
        if sql.starts_with("ALTER") {
            // Prepare-only schema probes must not reuse unstepped stale bytecode.
            assert!(check_schema(&tx).is_err());
        }
        if sql.starts_with("CREATE TRIGGER") || sql.starts_with("PRAGMA") {
            assert!(preflight(&tx).is_err());
        } else {
            assert!(load(&tx).is_err(), "{sql}");
        }
        tx.rollback().unwrap();
        check_schema(db).unwrap();
        preflight(db).unwrap();
        assert!(load(db).unwrap().is_none());
    }
    db.pragma_update(None, "foreign_keys", false).unwrap();
    assert!(preflight(db).is_err());
    db.pragma_update(None, "foreign_keys", true).unwrap();
    preflight(db).unwrap();
    assert!(db.is_autocommit());
    let busy: i64 = db
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
        .unwrap();
    assert_eq!(busy, 0);
    // Warm query execution cannot pin a read transaction beyond the call.
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
