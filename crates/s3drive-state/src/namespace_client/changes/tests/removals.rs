use super::*;
use namespace::RemovalFence;
mod cancellation;
mod created;
mod deferred;

fn owner(f: &mut Fixture) -> String {
    Journal::new(&mut f.client)
        .unwrap()
        .client_identity()
        .unwrap()
}
pub(super) fn owned(f: &mut Fixture, n: &Node) -> Option<crate::namespace_client::OwnedRemoval> {
    Journal::new(&mut f.client)
        .unwrap()
        .owned_removal(&f.volume, &n.id)
        .unwrap()
}
pub(super) fn arm(f: &mut Fixture, n: &Node) -> Input {
    let owner = owner(f);
    f.input(
        Mutation::ArmRemoval {
            node: guard(n),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner,
        },
        std::slice::from_ref(n),
    )
}
pub(super) fn close(f: &mut Fixture, n: &Node, fence: &RemovalFence, commit: bool) -> Input {
    let mutation = if commit {
        Mutation::CommitRemoval {
            node: guard(n),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: fence.clone(),
        }
    } else {
        Mutation::CancelRemoval {
            node: n.id.clone(),
            fence: fence.clone(),
        }
    };
    f.input(mutation, std::slice::from_ref(n))
}
pub(super) fn apply(f: &mut Fixture, input: &Input) -> Receipt {
    Journal::new(&mut f.client)
        .unwrap()
        .begin(input.clone())
        .unwrap();
    let r = f.authority.namespace_apply(&input.request).unwrap();
    f.ack(input, r.clone()).unwrap();
    r
}
#[test]
fn identity_is_stable_separate_from_credentials_and_pins_client_role() {
    let mut f = Fixture::new();
    let id = owner(&mut f);
    assert_ne!(id, owner(&mut Fixture::new()));
    f.client.register_source(&[8; 32], &"a".repeat(64)).unwrap();
    f.reopen_client();
    assert_eq!(owner(&mut f), id);
    assert_eq!(
        f.client
            .namespace_initialize_with_policy(s3drive_core::names::PORTABLE),
        Err(namespace::Error::Invalid)
    );
    assert!(Journal::new(&mut f.authority).is_err());
    let mut pristine = Fixture::new();
    owner(&mut pristine);
    assert_eq!(
        pristine
            .client
            .namespace_initialize_with_policy(s3drive_core::names::PORTABLE),
        Err(namespace::Error::Invalid)
    );
}
#[test]
fn identity_faults_never_rotate_or_leave_half_an_identity() {
    for table in ["client_identity", "retained_client_identity"] {
        let mut f = Fixture::new();
        f.client
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER fail BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END;"
            ))
            .unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .client_identity()
                .is_err()
        );
        assert!(identity::read(&f.client.connection).unwrap().is_none());
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        owner(&mut f);
        f.client
            .connection
            .execute(&format!("DELETE FROM {table}"), [])
            .unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .client_identity()
                .is_err()
        );
    }
    let mut f = Fixture::new();
    let n = f.create("identity retained");
    let i = arm(&mut f, &n);
    apply(&mut f, &i);
    f.client
        .connection
        .execute_batch("DELETE FROM client_identity; DELETE FROM retained_client_identity")
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .client_identity()
            .is_err(),
        "history forbids regenerating both missing rows"
    );
}
#[test]
fn every_disposition_ack_is_atomic_and_old_replay_cannot_release_a_new_arm() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE UPDATE ON logical_files BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN SELECT RAISE(IGNORE); END;",
    ] {
        let mut f = Fixture::new();
        let n = f.create("資料 café 🦀");
        let dirty = f.stage(&n, 500, b"all local bytes survive every disposition");
        let mut before = n.clone();
        let mut old: Vec<(Input, Receipt)> = Vec::new();
        for step in 0..4 {
            let active = owned(&mut f, &n);
            let i = if step % 2 == 0 {
                arm(&mut f, &before)
            } else {
                close(&mut f, &before, &active.as_ref().unwrap().fence, step == 3)
            };
            Journal::new(&mut f.client)
                .unwrap()
                .begin(i.clone())
                .unwrap();
            let r = f.authority.namespace_apply(&i.request).unwrap();
            f.client.connection.execute_batch(trigger).unwrap();
            assert!(f.ack(&i, r.clone()).is_err());
            assert_eq!(f.file(&n).base, before);
            assert_eq!(owned(&mut f, &n), active);
            assert!(
                Journal::new(&mut f.client)
                    .unwrap()
                    .get(&i.request.operation)
                    .unwrap()
                    .outcome
                    .is_none()
            );
            f.client
                .connection
                .execute_batch("DROP TRIGGER fail")
                .unwrap();
            f.reopen_client();
            f.ack(&i, r.clone()).unwrap();
            before = r.node.clone();
            let expected_owner = owned(&mut f, &n);
            for (i, r) in &old {
                f.ack(i, r.clone()).unwrap();
            }
            assert_eq!(owned(&mut f, &n), expected_owner);
            assert_eq!(f.file(&n).base, before);
            old.push((i, r));
        }
        assert!(f.file(&n).base.deleted);
        assert_eq!(f.file(&n).head, dirty.generation);
        assert!(f.file(&n).settled.is_none());
        assert_eq!(
            journal::Journal::new(&mut f.client, journal::Options::default())
                .unwrap()
                .read(dirty.generation.unwrap(), 0, 100)
                .unwrap(),
            b"all local bytes survive every disposition"
        );
    }
}
#[test]
fn owned_and_unknown_dispositions_block_ordinary_work_before_it_is_journaled() {
    let mut f = Fixture::new();
    let n = f.create("fenced dirty data");
    let dirty = f.stage(&n, 500, b"pending bytes");
    let i = arm(&mut f, &n);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let remove = f.remove(&n);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(remove.clone())
            .is_err()
    );
    assert!(
        load(&f.client.connection, &remove.request.operation)
            .unwrap()
            .is_none()
    );
    let r = f.authority.namespace_apply(&i.request).unwrap();
    f.ack(&i, r.clone()).unwrap();
    let remove = f.remove(&r.node);
    assert!(Journal::new(&mut f.client).unwrap().begin(remove).is_err());
    let reserve = f.request(Mutation::ReserveContent {
        node: n.id.clone(),
        expected_revision: r.sequence,
        store: f.store.id.clone(),
        size: dirty.input.size,
        sha256: dirty.input.sha256.clone(),
        expected_payload: None,
    });
    let publication = crate::managed::Input {
        reserve,
        publish_operation: id(600),
        store: f.store.clone(),
        source: [1; 32],
        fingerprint: "b".repeat(64),
    };
    assert!(
        logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .prepare(&id(500), publication)
            .is_err()
    );
    let pending = owned(&mut f, &n).unwrap();
    let cancel = close(&mut f, &r.node, &pending.fence, false);
    let restored = apply(&mut f, &cancel);
    let reserve = f.request(Mutation::ReserveContent {
        node: n.id.clone(),
        expected_revision: restored.sequence,
        store: f.store.id.clone(),
        size: dirty.input.size,
        sha256: dirty.input.sha256.clone(),
        expected_payload: None,
    });
    let publication = crate::managed::Input {
        reserve,
        publish_operation: id(601),
        store: f.store.clone(),
        source: [1; 32],
        fingerprint: "b".repeat(64),
    };
    logical::Journal::new(&mut f.client, journal::Options::default())
        .unwrap()
        .prepare(&id(500), publication)
        .unwrap();
    let i = arm(&mut f, &restored.node);
    assert!(
        Journal::new(&mut f.client).unwrap().begin(i).is_err(),
        "resolve prepared publication before arming"
    );
}
#[test]
fn foreign_fences_changed_bindings_and_wrong_receipts_never_adopt_ownership() {
    let mut f = Fixture::new();
    let n = f.create("protected");
    let i = arm(&mut f, &n);
    let mut foreign = i.clone();
    if let Mutation::ArmRemoval { owner, .. } = &mut foreign.request.mutation {
        *owner = id(987);
    }
    assert!(Journal::new(&mut f.client).unwrap().begin(foreign).is_err());
    let r = apply(&mut f, &i);
    let pending = owned(&mut f, &n).unwrap();
    let i = close(&mut f, &r.node, &pending.fence, false);
    let mut changed = i.clone();
    changed.source = [3; 32];
    assert!(Journal::new(&mut f.client).unwrap().begin(changed).is_err());
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let r = f.authority.namespace_apply(&i.request).unwrap();
    let mut changed = r.clone();
    changed.artifact = Some(Artifact::Removal(RemovalFence {
        intent: id(999),
        owner: pending.fence.owner.clone(),
    }));
    assert!(f.ack(&i, changed).is_err());
    f.ack(&i, r).unwrap();
    assert!(owned(&mut f, &n).is_none());
}
#[test]
fn closing_headroom_survives_ordinary_quota_and_failed_closing_receipts() {
    let mut f = Fixture::new();
    let n = f.create("last closing slot");
    let i = arm(&mut f, &n);
    let r = apply(&mut f, &i);
    let pending = owned(&mut f, &n).unwrap();
    let template = f.input(
        Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            name: "unrelated".into(),
            node_kind: Kind::EmptyFile,
            store: Some(f.store.id.clone()),
        },
        &[],
    );
    let tx = f.client.connection.transaction().unwrap();
    for operation in 10_000..14_093 {
        let mut input = template.clone();
        input.request.operation = id(operation);
        let record = Record {
            input,
            outcome: None,
        };
        tx.execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,NULL,?3)",
            params![
                record.input.request.operation,
                serde_json::to_vec(&record.input).unwrap(),
                hash(&record).unwrap()
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    Journal::new(&mut f.client)
        .unwrap()
        .begin(template.clone())
        .unwrap();
    let mut denied = template.clone();
    denied.request.operation = id(20_000);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(denied.clone())
            .is_err()
    );
    assert!(
        load(&f.client.connection, &denied.request.operation)
            .unwrap()
            .is_none()
    );
    let close = close(&mut f, &r.node, &pending.fence, false);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(close.clone())
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .acknowledge(&close.request.operation, Err(namespace::Error::Conflict))
            .is_err(),
        "failed closure cannot consume its own successful close slot"
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .get(&close.request.operation)
            .unwrap()
            .outcome
            .is_none()
    );
    let restored = f.authority.namespace_apply(&close.request).unwrap();
    f.ack(&close, restored).unwrap();
    assert!(owned(&mut f, &n).is_none());
}
#[test]
fn pending_directory_blocks_local_child_admission_and_foreign_adoption() {
    let mut f = Fixture::new();
    let request = f.request(Mutation::CreateEntry {
        parent: ROOT.into(),
        parent_revision: 0,
        name: "directory".into(),
        node_kind: Kind::Directory,
        store: None,
    });
    let dir = f.authority.namespace_apply(&request).unwrap().node;
    let i = arm(&mut f, &dir);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let child = f.input(
        Mutation::CreateEntry {
            parent: dir.id.clone(),
            parent_revision: dir.revision,
            name: "child".into(),
            node_kind: Kind::EmptyFile,
            store: Some(f.store.id.clone()),
        },
        &[],
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(child.clone())
            .is_err()
    );
    let r = f.authority.namespace_apply(&i.request).unwrap();
    f.ack(&i, r.clone()).unwrap();
    assert!(Journal::new(&mut f.client).unwrap().begin(child).is_err());
    let fence = owned(&mut f, &dir).unwrap().fence;
    let cancel = close(&mut f, &r.node, &fence, false);
    let mut foreign = State::open_managed(&f.tmp.path().join("foreign")).unwrap();
    Journal::new(&mut foreign)
        .unwrap()
        .client_identity()
        .unwrap();
    assert!(Journal::new(&mut foreign).unwrap().begin(cancel).is_err());
}
#[test]
fn schema_thirteen_migration_preserves_existing_journal_and_conflicts_roll_back() {
    for conflict in [false, true] {
        let mut f = Fixture::new();
        let n = f.create("legacy journal bytes");
        let i = f.remove(&n);
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        let original = f
            .client
            .connection
            .query_row("SELECT input FROM namespace_intents", [], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .unwrap();
        f.client.connection.execute_batch("DROP TABLE namespace_removal_events; ALTER TABLE namespace_removals DROP COLUMN mode; ALTER TABLE namespace_removals DROP COLUMN event_id; DROP TABLE logical_conflicts; DROP TABLE managed_rejections; DROP TABLE retained_client_identity; DROP TABLE client_identity; PRAGMA user_version=13").unwrap();
        if conflict {
            f.client.connection.execute_batch("CREATE TABLE client_identity(keep TEXT); INSERT INTO client_identity VALUES('preserved')").unwrap();
        }
        let path = f.tmp.path().join("client");
        let placeholder = State::open_managed(&f.tmp.path().join("placeholder")).unwrap();
        drop(std::mem::replace(&mut f.client, placeholder));
        if conflict {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                13
            );
            assert_eq!(
                db.query_row::<String, _, _>("SELECT keep FROM client_identity", [], |r| r.get(0))
                    .unwrap(),
                "preserved"
            );
        } else {
            f.client = State::open_managed(&path).unwrap();
            assert_eq!(
                f.client.info().unwrap().schema_version,
                crate::SCHEMA_VERSION
            );
            assert_eq!(
                f.client
                    .connection
                    .query_row("SELECT input FROM namespace_intents", [], |r| r
                        .get::<_, Vec<u8>>(0))
                    .unwrap(),
                original
            );
            assert_eq!(
                Journal::new(&mut f.client)
                    .unwrap()
                    .get(&i.request.operation)
                    .unwrap()
                    .input,
                i
            );
            owner(&mut f);
        }
    }
}
#[test]
fn full_disk_disposition_ack_preserves_owned_arm_and_dirty_generation() {
    let mut f = Fixture::new();
    let n = f.create("disk full disposition");
    let edit = f.stage(&n, 500, b"preserved dirty contents");
    let i = arm(&mut f, &n);
    let r = apply(&mut f, &i);
    let before = owned(&mut f, &n).unwrap();
    let i = close(&mut f, &r.node, &before.fence, true);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let receipt = f.authority.namespace_apply(&i.request).unwrap();
    f.client
        .connection
        .execute_batch("CREATE TABLE full_disk_filler(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = f
        .client
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    // An actual disk-full error inside the acknowledgment transaction, rather
    // than an in-memory failure standing in for SQLite's rollback behavior.
    f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN INSERT INTO full_disk_filler VALUES(zeroblob(1048576)); END").unwrap();
    let error = f.ack(&i, receipt.clone()).unwrap_err().to_string();
    assert!(error.contains("full"), "expected SQLite full, got {error}");
    assert_eq!(owned(&mut f, &n).unwrap(), before);
    assert_eq!(f.file(&n).base, r.node);
    assert_eq!(f.file(&n).head, edit.generation);
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    f.client
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    f.ack(&i, receipt).unwrap();
    assert!(owned(&mut f, &n).is_none());
    assert!(f.file(&n).base.deleted);
}
