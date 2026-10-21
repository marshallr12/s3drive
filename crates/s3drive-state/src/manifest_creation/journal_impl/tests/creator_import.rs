//! Same-owner import tests. Physical verification here is modeled; SDK fixtures
//! separately use the real HTTP/S3 client with owned loopback bytes.
use super::*;
use crate::{chunk_mount, namespace_client as names, namespace_client::opens};
use std::{collections::BTreeMap, path::Path};
mod prospective;
fn setup(root: &Path) -> (State, State, chunk_mount::Key, Input) {
    std::fs::create_dir_all(root).unwrap();
    let mut a = State::open_managed(&root.join("authority")).unwrap();
    let v = a
        .namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
        .unwrap();
    a.namespace_apply(&Apply {
        volume: v.id.clone(),
        operation: id(3),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "a".repeat(64),
            root: "managed/".into(),
        },
    })
    .unwrap();
    let mut s = State::open_managed(&root.join("client")).unwrap();
    let mut i = input(4);
    i.volume = v.id;
    let key = chunk_mount::admit(
        &mut s,
        chunk_mount::Input {
            volume: i.volume.clone(),
            controls: vec![names::Control {
                store: i.store.clone(),
                source: i.source,
                fingerprint: i.fingerprint.clone(),
            }],
        },
    )
    .unwrap();
    i.owner = Some(key.owner().into());
    (s, a, key, i)
}
fn ack(s: &mut State, a: &mut State, i: &Input) -> Record {
    let r = prepared(s, i);
    let q = r.request().unwrap().clone();
    let result = a.namespace_apply(&q).unwrap();
    j(s).acknowledge(&i.operation, &q, result).unwrap()
}
fn ns<'a>(s: &'a mut State, key: &chunk_mount::Key) -> names::Journal<'a> {
    names::Journal::for_chunks(s, journal::Options::default(), key).unwrap()
}
fn import(s: &mut State, key: &chunk_mount::Key, op: &str) -> crate::Result<names::Record> {
    names::Journal::for_chunks(s, journal::Options::default(), key)?.import_creation(op)
}
fn ns_input(i: &Input, q: &Apply) -> names::Input {
    names::Input {
        request: q.clone(),
        store: i.store.clone(),
        source: i.source,
        fingerprint: i.fingerprint.clone(),
        entries: Vec::new(),
        destination_control: None,
    }
}
fn open(r: &names::Record) -> opens::OpenInput {
    opens::OpenInput {
        id: id(71),
        session: id(70),
        volume: r.input.request.volume.clone(),
        node: r.input.request.operation.clone(),
        store: r.input.store.clone(),
        source: r.input.source,
        fingerprint: r.input.fingerprint.clone(),
        delete_on_close: true,
        controlled: true,
        session_bound: true,
        posix: false,
        admission: Some(Box::new(r.input.clone())),
        creation: None,
    }
}
fn catalog(s: &State) -> BTreeMap<String, Vec<String>> {
    catalog_db(&s.connection)
}
fn catalog_db(db: &rusqlite::Connection) -> BTreeMap<String, Vec<String>> {
    let tables:Vec<(String,String)>=db.prepare("SELECT name,sql FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    tables
        .into_iter()
        .map(|(table, definition)| {
            let quoted = format!("\"{}\"", table.replace('"', "\"\""));
            let columns: Vec<String> = db
                .prepare(&format!("PRAGMA table_info({quoted})"))
                .unwrap()
                .query_map([], |r| r.get(1))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            let expression = columns
                .iter()
                .map(|c| format!("quote(\"{}\")", c.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join("||'|'||");
            let mut hashes: Vec<String> = db
                .prepare(&format!("SELECT {expression} FROM {quoted}"))
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(|r| hex(&Sha256::digest(r.unwrap().as_bytes())))
                .collect();
            hashes.sort();
            hashes.insert(0, hex(&Sha256::digest(definition.as_bytes())));
            (table, hashes)
        })
        .collect()
}
fn namespace_hash(r: &names::Record) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(b"s3drive-namespace-intent-v1\0");
    hash.update(serde_json::to_vec(&(&r.input, &r.outcome)).unwrap());
    hash.finalize().to_vec()
}
fn write_namespace(s: &State, r: &names::Record) {
    s.connection
        .execute(
            "UPDATE namespace_intents SET input=?1,outcome=?2,state_hash=?3 WHERE operation_id=?4",
            params![
                serde_json::to_vec(&r.input).unwrap(),
                serde_json::to_vec(r.outcome.as_ref().unwrap()).unwrap(),
                namespace_hash(r),
                r.input.request.operation
            ],
        )
        .unwrap();
}
#[test]
fn creator_import_exact_receipt_admission_activation_and_restart_never_use_generic_begin() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup(t.path());
    let done = ack(&mut s, &mut a, &i);
    let expected = ns_input(&i, done.request().unwrap());
    let before = catalog(&s);
    assert!(ns(&mut s, &key).begin(expected.clone()).is_err());
    assert_eq!(catalog(&s), before);
    let record = names::Record {
        input: expected.clone(),
        outcome: Some(Ok(done.receipt().unwrap().clone())),
    };
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .start_session(&id(70), &i.volume)
        .unwrap();
    let before = catalog(&s);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare(open(&record))
            .is_err()
    );
    assert_eq!(catalog(&s), before);
    let imported = import(&mut s, &key, &i.operation).unwrap();
    assert_eq!(imported, record);
    let before = catalog(&s);
    assert_eq!(import(&mut s, &key, &i.operation).unwrap(), imported);
    assert!(ns(&mut s, &key).begin(expected).is_err());
    assert_eq!(catalog(&s), before);
    assert!(ns(&mut s, &key).pending(None, 64).unwrap().is_empty());
    let owned = ns(&mut s, &key)
        .owned_removal(&i.volume, &i.operation)
        .unwrap()
        .unwrap();
    assert_eq!(owned.fence.owner, key.owner());
    assert_eq!(owned.fence.intent, i.operation);
    assert_eq!(owned.mode, ns::RemovalMode::Deferred);
    let mut journal = opens::Journal::for_chunks(&mut s, &key).unwrap();
    journal.prepare(open(&imported)).unwrap();
    let active = journal.activate(&id(71), &id(72)).unwrap();
    assert!(active.active());
    drop(s);
    let mut s = State::open_managed(&t.path().join("client")).unwrap();
    assert_eq!(import(&mut s, &key, &i.operation).unwrap(), imported);
    let journal = opens::Journal::for_chunks(&mut s, &key).unwrap();
    assert_eq!(journal.get(&id(71)).unwrap(), active);
    assert!(journal.obsolete_session(&id(70), &i.volume).unwrap());
}
#[test]
fn creator_import_unknown_rejected_ordinary_and_foreign_owner_never_create_namespace_facts() {
    for mode in 0..4 {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, mut i) = setup(t.path());
        if mode == 0 {
            j(&mut s).begin(i.clone()).unwrap();
        }
        if mode == 1 {
            let r = prepared(&mut s, &i);
            let q = r.request().unwrap().clone();
            j(&mut s)
                .reject(&i.operation, &q, ns::Error::AlreadyExists)
                .unwrap();
        }
        if mode == 2 {
            i.owner = None;
            ack(&mut s, &mut a, &i);
        }
        if mode == 3 {
            i.owner = Some(id(99));
            ack(&mut s, &mut a, &i);
        }
        let before = catalog(&s);
        assert!(import(&mut s, &key, &i.operation).is_err());
        assert!(import(&mut s, &key, &id(55)).is_err());
        assert_eq!(catalog(&s), before);
        assert_eq!(
            s.connection
                .query_row("SELECT count(*) FROM namespace_intents", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("plain")).unwrap();
    let before = catalog(&s);
    assert!(
        names::Journal::new(&mut s)
            .unwrap()
            .import_creation(&id(4))
            .is_err()
    );
    assert_eq!(catalog(&s), before);
    let (mut s, _a, key, _i) = setup(&t.path().join("one"));
    let (_other, _authority, foreign, _) = setup(&t.path().join("two"));
    assert_ne!(key.owner(), foreign.owner());
    let before = catalog(&s);
    assert!(names::Journal::for_chunks(&mut s, journal::Options::default(), &foreign).is_err());
    assert_eq!(catalog(&s), before);
}
#[test]
fn creator_import_saved_metadata_does_not_read_or_repair_damaged_body_bytes() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup(t.path());
    let done = ack(&mut s, &mut a, &i);
    s.connection
        .execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    let before = catalog(&s);
    let imported = import(&mut s, &key, &i.operation).unwrap();
    assert_eq!(imported.outcome, Some(Ok(done.receipt().unwrap().clone())));
    let after = catalog(&s);
    for (table, rows) in before {
        if table != "namespace_intents" {
            assert_eq!(after[&table], rows, "{table}");
        }
    }
}
#[test]
fn creator_import_self_consistent_namespace_or_creator_rewrite_cannot_replace_independent_witness()
{
    for joint in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup(t.path());
        let mut done = ack(&mut s, &mut a, &i);
        let mut record = import(&mut s, &key, &i.operation).unwrap();
        let old: Vec<u8> = s
            .connection
            .query_row("SELECT state_hash FROM namespace_intents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(namespace_hash(&record), old);
        let Mutation::CreateDeferredEmptyManifest { name, .. } = &mut record.input.request.mutation
        else {
            panic!()
        };
        *name = "forged 日本語.bin".into();
        record.outcome.as_mut().unwrap().as_mut().unwrap().node.name = "forged 日本語.bin".into();
        let mut changed = i.clone();
        changed.name = "forged 日本語.bin".into();
        changed
            .check_receipt(
                &record.input.request,
                record.outcome.as_ref().unwrap().as_ref().unwrap(),
            )
            .unwrap();
        if joint {
            done.input = changed;
            done.request = Some(record.input.request.clone());
            done.receipt = Some(record.outcome.as_ref().unwrap().as_ref().unwrap().clone());
            done.validate().unwrap();
            let map = BTreeMap::from([(i.operation.clone(), done.clone())]);
            s.connection
                .execute(
                    "UPDATE manifest_creations SET record=?1,state_hash=?2",
                    params![
                        serde_json::to_vec(&done).unwrap(),
                        record_hash(&done).unwrap().as_slice()
                    ],
                )
                .unwrap();
            s.connection
                .execute(
                    "UPDATE manifest_creation_role SET state_hash=?1",
                    [projection(&map).unwrap().as_slice()],
                )
                .unwrap();
        }
        write_namespace(&s, &record);
        let before = catalog(&s);
        assert!(import(&mut s, &key, &i.operation).is_err());
        assert_eq!(catalog(&s), before);
        drop(s);
        assert!(State::open_managed(&t.path().join("client")).is_err());
        let connection =
            rusqlite::Connection::open(t.path().join("client/metadata.sqlite3")).unwrap();
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                .unwrap(),
            crate::SCHEMA_VERSION
        );
    }
}
#[test]
fn creator_import_missing_creator_or_changed_upload_witness_cannot_be_repaired_on_load() {
    for mode in 0..3 {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup(t.path());
        ack(&mut s, &mut a, &i);
        let imported = import(&mut s, &key, &i.operation).unwrap();
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .start_session(&id(70), &i.volume)
            .unwrap();
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare(open(&imported))
            .unwrap();
        match mode {
            0 => {
                s.connection
                    .execute_batch(
                        "DELETE FROM manifest_creations;DELETE FROM manifest_creation_role",
                    )
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute("DELETE FROM staged_operations", [])
                    .unwrap();
            }
            _ => {
                s.connection
                    .execute("UPDATE remote_intents SET state_hash=zeroblob(32)", [])
                    .unwrap();
            }
        }
        let before = catalog(&s);
        assert!(import(&mut s, &key, &i.operation).is_err());
        assert!(opens::Journal::for_chunks(&mut s, &key).is_err());
        assert_eq!(catalog(&s), before);
        drop(s);
        assert!(State::open_managed(&t.path().join("client")).is_err());
    }
}
fn change(
    s: &mut State,
    a: &mut State,
    key: &chunk_mount::Key,
    i: &Input,
    operation: u8,
    mutation: Mutation,
) -> names::Record {
    let old = match a.namespace_node(&i.volume, &i.operation) {
        Ok(node) => node,
        Err(ns::Error::DeletePending) => {
            a.namespace_pending_removal(&i.volume, &i.operation)
                .unwrap()
                .unwrap()
                .node
        }
        Err(e) => panic!("unexpected owned fixture read: {e:?}"),
    };
    let request = Apply {
        volume: i.volume.clone(),
        operation: id(operation),
        expected_sequence: a.namespace_volume().unwrap().sequence,
        mutation,
    };
    let mut input = ns_input(i, &request);
    input.entries = vec![names::Entry::from_node(&old).unwrap()];
    ns(s, key).begin(input).unwrap();
    let result = a.namespace_apply(&request).unwrap();
    ns(s, key).acknowledge(&id(operation), Ok(result)).unwrap()
}
#[test]
fn creator_import_historical_repeat_never_resurrects_cancellation_or_adopts_successor_epoch() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup(t.path());
    ack(&mut s, &mut a, &i);
    let original = import(&mut s, &key, &i.operation).unwrap();
    let fence = ns(&mut s, &key)
        .owned_removal(&i.volume, &i.operation)
        .unwrap()
        .unwrap()
        .fence;
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .start_session(&id(70), &i.volume)
        .unwrap();
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .prepare(open(&original))
        .unwrap();
    change(
        &mut s,
        &mut a,
        &key,
        &i,
        5,
        Mutation::CancelRemoval {
            node: i.operation.clone(),
            fence,
        },
    );
    assert_eq!(import(&mut s, &key, &i.operation).unwrap(), original);
    assert!(
        ns(&mut s, &key)
            .owned_removal(&i.volume, &i.operation)
            .unwrap()
            .is_none()
    );
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(&id(71), &id(72))
            .is_err()
    );
    let live = a.namespace_node(&i.volume, &i.operation).unwrap();
    let parent = a.namespace_node(&i.volume, ns::ROOT).unwrap();
    let successor = change(
        &mut s,
        &mut a,
        &key,
        &i,
        6,
        Mutation::PrepareRemoval {
            node: ns::NodeGuard {
                id: live.id,
                revision: live.revision,
            },
            parent: ns::NodeGuard {
                id: parent.id,
                revision: parent.revision,
            },
            owner: key.owner().into(),
        },
    );
    let mut next = open(&successor);
    next.node = i.operation.clone();
    next.id = id(73);
    assert_eq!(import(&mut s, &key, &i.operation).unwrap(), original);
    assert_eq!(
        ns(&mut s, &key)
            .owned_removal(&i.volume, &i.operation)
            .unwrap()
            .unwrap()
            .fence
            .intent,
        id(6)
    );
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(&id(71), &id(72))
            .is_err()
    );
    let mut journal = opens::Journal::for_chunks(&mut s, &key).unwrap();
    journal.prepare(next).unwrap();
    assert!(journal.activate(&id(73), &id(74)).unwrap().active());
}
#[test]
fn creator_import_old43_role_upgrades_without_rewriting_rows_and_cannot_launder_new_shape() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup(t.path());
    ack(&mut s, &mut a, &i);
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .start_session(&id(70), &i.volume)
        .unwrap();
    let plain = opens::OpenInput {
        admission: None,
        creation: None,
        delete_on_close: false,
        ..open(&names::Record {
            input: ns_input(&i, j(&mut s).get(&i.operation).unwrap().request().unwrap()),
            outcome: None,
        })
    };
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .prepare(plain)
        .unwrap();
    s.connection
        .pragma_update(None, "user_version", 43)
        .unwrap();
    let before = catalog(&s);
    drop(s);
    let mut s = State::open_managed(&t.path().join("client")).unwrap();
    assert_eq!(catalog(&s), before);
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    import(&mut s, &key, &i.operation).unwrap();
    s.connection
        .pragma_update(None, "user_version", 43)
        .unwrap();
    let before = catalog(&s);
    drop(s);
    assert!(RecoveryStore::open(&t.path().join("client")).is_err());
    assert!(State::open_managed(&t.path().join("client")).is_err());
    let db = rusqlite::Connection::open(t.path().join("client/metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        43
    );
    assert_eq!(catalog_db(&db), before);
}
#[test]
fn creator_import_failed43_upgrade_rolls_back_the_marker_without_repairing_damaged_rows() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, _key, i) = setup(t.path());
    ack(&mut s, &mut a, &i);
    s.connection
        .execute(
            "UPDATE source_bindings SET config_fingerprint=?1",
            ["b".repeat(64)],
        )
        .unwrap();
    s.connection
        .pragma_update(None, "user_version", 43)
        .unwrap();
    let before = catalog(&s);
    drop(s);
    assert!(State::open_managed(&t.path().join("client")).is_err());
    let db = rusqlite::Connection::open(t.path().join("client/metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        43
    );
    assert_eq!(catalog_db(&db), before);
}
fn model_directories(s: &mut State, i: &Input, count: usize) {
    let tx = s.connection.transaction().unwrap();
    for n in 0..count {
        let q = Apply {
            volume: i.volume.clone(),
            operation: format!("{:032x}", 1000 + n),
            expected_sequence: 0,
            mutation: Mutation::CreateEntry {
                parent: ns::ROOT.into(),
                parent_revision: 0,
                name: format!("quota-{n}"),
                node_kind: ns::Kind::Directory,
                store: None,
            },
        };
        let r = names::Record {
            input: ns_input(i, &q),
            outcome: Some(Ok(ns::Receipt {
                sequence: (1000 + n) as u64,
                node: ns::creation::expected(&q, None, (1000 + n) as u64).unwrap(),
                artifact: ns::creation::artifact(&q),
            })),
        };
        tx.execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,?3,?4)",
            params![
                q.operation,
                serde_json::to_vec(&r.input).unwrap(),
                serde_json::to_vec(r.outcome.as_ref().unwrap()).unwrap(),
                namespace_hash(&r)
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}
#[test]
fn creator_import_reserves_real_namespace_closing_capacity_and_rejects_one_row_too_many() {
    for count in [4093, 4094] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup(t.path());
        ack(&mut s, &mut a, &i);
        model_directories(&mut s, &i, count);
        let before = catalog(&s);
        if count == 4094 {
            assert!(import(&mut s, &key, &i.operation).is_err());
            assert_eq!(catalog(&s), before);
            continue;
        }
        import(&mut s, &key, &i.operation).unwrap();
        let owned = ns(&mut s, &key)
            .owned_removal(&i.volume, &i.operation)
            .unwrap()
            .unwrap();
        let live = a.namespace_node(&i.volume, &i.operation).unwrap();
        let parent = a.namespace_node(&i.volume, ns::ROOT).unwrap();
        let node = ns::NodeGuard {
            id: live.id.clone(),
            revision: live.revision,
        };
        let parent_guard = ns::NodeGuard {
            id: parent.id.clone(),
            revision: parent.revision,
        };
        change(
            &mut s,
            &mut a,
            &key,
            &i,
            5,
            Mutation::TransitionRemoval {
                node,
                parent: parent_guard,
                fence: owned.fence.clone(),
                expected: ns::RemovalMode::Deferred,
                mode: ns::RemovalMode::Pending,
            },
        );
        let live = a
            .namespace_pending_removal(&i.volume, &i.operation)
            .unwrap()
            .unwrap()
            .node;
        let parent = a.namespace_node(&i.volume, ns::ROOT).unwrap();
        change(
            &mut s,
            &mut a,
            &key,
            &i,
            6,
            Mutation::CommitRemoval {
                node: ns::NodeGuard {
                    id: live.id,
                    revision: live.revision,
                },
                parent: ns::NodeGuard {
                    id: parent.id,
                    revision: parent.revision,
                },
                fence: owned.fence,
            },
        );
        assert_eq!(
            s.connection
                .query_row("SELECT count(*) FROM namespace_intents", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            4096
        );
        assert!(
            ns(&mut s, &key)
                .owned_removal(&i.volume, &i.operation)
                .unwrap()
                .is_none()
        );
    }
}
fn pressure(s: &mut State) {
    s.connection
        .execute_batch("CREATE TABLE IF NOT EXISTS creator_pressure(data BLOB)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages + 16)
        .unwrap();
    let mut full = false;
    for _ in 0..1024 {
        match s
            .connection
            .execute("INSERT INTO creator_pressure VALUES(zeroblob(4096))", [])
        {
            Ok(_) => {}
            Err(e) => {
                assert!(crate::cache::is_disk_full(&e), "{e}");
                full = true;
                break;
            }
        }
    }
    assert!(full, "real SQLite page cap was not reached");
}
fn grow(s: &mut State) {
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages + 256)
        .unwrap();
}
#[test]
fn creator_import_actual_sqlite_full_preserves_exact_import_open_and_activation_retry() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, mut i) = setup(t.path());
    let mut operations = Vec::new();
    for n in 4..20 {
        i.operation = id(n);
        i.name = format!("café 日本語 {n}.bin");
        i.expected_sequence = a.namespace_volume().unwrap().sequence;
        i.parent.revision = a.namespace_node(&i.volume, ns::ROOT).unwrap().revision;
        ack(&mut s, &mut a, &i);
        operations.push(i.operation.clone());
    }
    pressure(&mut s);
    let mut failed = None;
    for op in operations {
        let before = catalog(&s);
        match import(&mut s, &key, &op) {
            Ok(_) => {}
            Err(e) => {
                assert!(crate::cache::is_disk_full(e.as_ref()), "{e}");
                assert_eq!(catalog(&s), before);
                failed = Some(op);
                break;
            }
        }
    }
    let op = failed.expect("owned imports must require a page by sixteen records");
    grow(&mut s);
    let original = import(&mut s, &key, &op).unwrap();
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .start_session(&id(70), &i.volume)
        .unwrap();
    pressure(&mut s);
    let mut failed = None;
    for n in 0..32 {
        let mut input = open(&original);
        input.id = format!("{:032x}", 10000 + n);
        let before = catalog(&s);
        match opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare(input.clone())
        {
            Ok(_) => {}
            Err(e) => {
                assert!(crate::cache::is_disk_full(e.as_ref()), "{e}");
                assert_eq!(catalog(&s), before);
                failed = Some(input);
                break;
            }
        }
    }
    let input = failed.expect("open rows must eventually allocate a page");
    grow(&mut s);
    opens::Journal::for_chunks(&mut s, &key)
        .unwrap()
        .prepare(input.clone())
        .unwrap();
    let mut prepared = vec![input.id];
    for n in 0..32 {
        let mut input = open(&original);
        input.id = format!("{:032x}", 20000 + n);
        prepared.push(input.id.clone());
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .prepare(input)
            .unwrap();
    }
    pressure(&mut s);
    let mut failed = None;
    for (n, open) in prepared.iter().enumerate() {
        let event = format!("{:032x}", 30000 + n);
        let before = catalog(&s);
        match opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(open, &event)
        {
            Ok(_) => {}
            Err(e) => {
                assert!(crate::cache::is_disk_full(e.as_ref()), "{e}");
                assert_eq!(catalog(&s), before);
                failed = Some((open.clone(), event));
                break;
            }
        }
    }
    let (open, event) = failed.expect("activation events must eventually allocate a page");
    grow(&mut s);
    assert!(
        opens::Journal::for_chunks(&mut s, &key)
            .unwrap()
            .activate(&open, &event)
            .unwrap()
            .active()
    );
    assert_eq!(import(&mut s, &key, &op).unwrap(), original);
}
#[tokio::test]
async fn creator_import_crash_worker() {
    use std::io::{Read, Write};
    let Ok(path) = std::env::var("S3DRIVE_CREATOR_IMPORT_PATH") else {
        return;
    };
    let mut state = State::open_managed(Path::new(&path)).unwrap();
    let i = j(&mut state).get(&id(4)).unwrap().input().clone();
    drop(state);
    let storage = crate::worker::Storage::open(
        Path::new(&path),
        crate::cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let client = storage
        .chunk_mount(chunk_mount::Input {
            volume: i.volume.clone(),
            controls: vec![names::Control {
                store: i.store.clone(),
                source: i.source,
                fingerprint: i.fingerprint.clone(),
            }],
        })
        .await
        .unwrap();
    assert!(client.same_storage(&storage));
    let kind = std::env::var("S3DRIVE_CREATOR_IMPORT_ACTION").unwrap();
    if kind != "import" {
        client
            .opens()
            .start_session(id(70), i.volume.clone())
            .await
            .unwrap();
        if kind == "activate" {
            let r = client.namespace().get(i.operation.clone()).await.unwrap();
            client.opens().prepare(open(&r)).await.unwrap();
        }
    }
    println!("S3DRIVE_CREATOR_IMPORT_READY");
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
    match kind.as_str() {
        "import" => {
            client
                .namespace()
                .import_creation(i.operation)
                .await
                .unwrap();
        }
        "open" => {
            let r = client.namespace().get(i.operation).await.unwrap();
            client.opens().prepare(open(&r)).await.unwrap();
        }
        "activate" => {
            client.opens().activate(id(71), id(72)).await.unwrap();
        }
        _ => panic!("unknown owned crash action"),
    }
    panic!("expected process-stop checkpoint was not reached");
}
#[test]
fn creator_import_killed_actual_worker_owners_preserve_old_or_exact_commit_at_seven_boundaries() {
    use std::{
        io::{BufRead, BufReader, Write},
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
    for (action, boundary, committed) in [
        ("import", "ImportBeforeStored", false),
        ("import", "ImportStored", false),
        ("import", "ImportCommitted", true),
        ("open", "CreatorOpenStored", false),
        ("open", "CreatorOpenCommitted", true),
        ("activate", "CreatorActivationStored", false),
        ("activate", "CreatorActivationCommitted", true),
    ] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup(t.path());
        ack(&mut s, &mut a, &i);
        if action != "import" {
            import(&mut s, &key, &i.operation).unwrap();
        }
        drop(s);
        let mut child=Reap(Command::new(std::env::current_exe().unwrap()).args(["--exact","manifest_creation::journal_impl::tests::creator_import::creator_import_crash_worker","--nocapture"]).env("S3DRIVE_CREATOR_IMPORT_PATH",t.path().join("client")).env("S3DRIVE_CREATOR_IMPORT_ACTION",action).env("S3DRIVE_CREATOR_IMPORT_STOP",boundary).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap());
        let stdout = child.0.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.contains("S3DRIVE_CREATOR_IMPORT_READY") {
                    let _ = send.send(line);
                } else if line.contains("S3DRIVE_CREATOR_IMPORT_BOUNDARY") {
                    let _ = send.send(line);
                    return;
                }
            }
        });
        let ready = receive
            .recv_timeout(Duration::from_secs(15))
            .unwrap_or_else(|e| panic!("{action}/{boundary}: no setup marker: {e}"));
        assert!(ready.contains("READY"));
        let db = rusqlite::Connection::open_with_flags(
            t.path().join("client/metadata.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let before = catalog_db(&db);
        drop(db);
        child.0.stdin.as_mut().unwrap().write_all(b"1").unwrap();
        let marker = receive
            .recv_timeout(Duration::from_secs(15))
            .unwrap_or_else(|e| panic!("{action}/{boundary}: no transaction marker: {e}"));
        assert!(marker.contains(boundary));
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        reader.join().unwrap();
        let mut s = State::open_managed(&t.path().join("client")).unwrap();
        if !committed {
            assert_eq!(catalog(&s), before, "{boundary}");
        }
        let imported = import(&mut s, &key, &i.operation).unwrap();
        assert_eq!(
            imported.input.request.mutation,
            ns_input(&i, j(&mut s).get(&i.operation).unwrap().request().unwrap())
                .request
                .mutation
        );
        if action != "import" {
            let mut journal = opens::Journal::for_chunks(&mut s, &key).unwrap();
            assert!(journal.obsolete_session(&id(70), &i.volume).unwrap());
            if action == "open" {
                assert_eq!(journal.get(&id(71)).is_ok(), committed);
                if committed {
                    assert!(!journal.get(&id(71)).unwrap().active());
                }
                assert!(
                    journal.prepare(open(&imported)).is_err(),
                    "obsolete session must not admit a fresh handle"
                );
            } else {
                assert_eq!(journal.get(&id(71)).unwrap().active(), committed);
                let active = journal.activate(&id(71), &id(72)).unwrap();
                assert!(active.active());
                assert_eq!(active.events.len(), 1);
            }
        }
    }
}
#[test]
fn creator_import_unknown_local_receipt_blocks_a_real_remote_commit_until_exact_ack() {
    let t = tempfile::tempdir().unwrap();
    let (mut s, mut a, key, i) = setup(t.path());
    let waiting = prepared(&mut s, &i);
    let request = waiting.request().unwrap().clone();
    let receipt = a.namespace_apply(&request).unwrap();
    assert_eq!(
        a.namespace_node(&i.volume, &i.operation).unwrap().name,
        i.name
    );
    let before = catalog(&s);
    assert!(import(&mut s, &key, &i.operation).is_err());
    assert_eq!(catalog(&s), before);
    j(&mut s)
        .acknowledge(&i.operation, &request, receipt.clone())
        .unwrap();
    let r = import(&mut s, &key, &i.operation).unwrap();
    assert_eq!(r.outcome, Some(Ok(receipt)));
}
#[test]
fn creator_import_targeted_provenance_still_checks_the_complete_role_metadata_projection() {
    for missing in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let (mut s, mut a, key, i) = setup(t.path());
        ack(&mut s, &mut a, &i);
        let mut other = i.clone();
        other.operation = id(5);
        other.name = "other 日本語.bin".into();
        other.expected_sequence = a.namespace_volume().unwrap().sequence;
        other.parent.revision = a.namespace_node(&i.volume, ns::ROOT).unwrap().revision;
        ack(&mut s, &mut a, &other);
        assert!(super::super::super::verified_creation(&s.connection, &i.operation).is_ok());
        if missing {
            s.connection
                .execute(
                    "DELETE FROM manifest_creations WHERE operation=?1",
                    [&other.operation],
                )
                .unwrap();
        } else {
            s.connection
                .execute(
                    "UPDATE manifest_creations SET state_hash=zeroblob(32) WHERE operation=?1",
                    [&other.operation],
                )
                .unwrap();
        }
        let before = catalog(&s);
        assert!(super::super::super::verified_creation(&s.connection, &i.operation).is_err());
        assert!(import(&mut s, &key, &i.operation).is_err());
        assert_eq!(catalog(&s), before);
    }
}
