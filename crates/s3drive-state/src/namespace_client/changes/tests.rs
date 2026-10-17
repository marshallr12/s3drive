use super::*;
use crate::{journal, logical};
use namespace::{Artifact, NodeGuard, ROOT};
mod pending_publication;
mod removals;
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
struct Fixture {
    authority: State,
    client: State,
    tmp: tempfile::TempDir,
    volume: String,
    store: Store,
    op: u64,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let mut authority = State::open_managed(&tmp.path().join("authority")).unwrap();
        let volume = authority
            .namespace_initialize_with_policy(s3drive_core::names::PORTABLE)
            .unwrap()
            .id;
        let r = authority
            .namespace_apply(&Apply {
                volume: volume.clone(),
                operation: id(200),
                expected_sequence: 0,
                mutation: Mutation::RegisterStore {
                    route: "a".repeat(64),
                    root: "internal/".into(),
                },
            })
            .unwrap();
        let Some(Artifact::Store(store)) = r.artifact else {
            panic!()
        };
        let client = State::open_managed(&tmp.path().join("client")).unwrap();
        Self {
            authority,
            client,
            tmp,
            volume,
            store,
            op: 1000,
        }
    }
    fn request(&mut self, mutation: Mutation) -> Apply {
        self.op += 1;
        Apply {
            volume: self.volume.clone(),
            operation: id(self.op),
            expected_sequence: self.authority.namespace_volume().unwrap().sequence,
            mutation,
        }
    }
    fn create(&mut self, name: &str) -> Node {
        let r = self.request(Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: Some(self.store.id.clone()),
            name: name.into(),
            node_kind: Kind::EmptyFile,
        });
        self.authority.namespace_apply(&r).unwrap().node
    }
    fn bind(&mut self, n: &Node) -> logical::File {
        logical::Journal::new(&mut self.client, journal::Options::default())
            .unwrap()
            .bind(logical::Binding {
                volume: self.volume.clone(),
                node: n.clone(),
                store: self.store.clone(),
                source: [1; 32],
                fingerprint: "b".repeat(64),
            })
            .unwrap()
    }
    fn stage(&mut self, n: &Node, op: u64, bytes: &[u8]) -> logical::Record {
        let f = self.bind(n);
        let r = logical::Journal::new(&mut self.client, journal::Options::default())
            .unwrap()
            .begin(logical::Input::from_bytes(
                id(op),
                &f.binding,
                f.head,
                bytes,
            ))
            .unwrap();
        journal::Journal::new(&mut self.client, journal::Options::default())
            .unwrap()
            .append(r.generation.unwrap(), bytes)
            .unwrap();
        logical::Journal::new(&mut self.client, journal::Options::default())
            .unwrap()
            .seal(&id(op))
            .unwrap()
    }
    fn file(&mut self, n: &Node) -> logical::File {
        logical::Journal::new(&mut self.client, journal::Options::default())
            .unwrap()
            .file(&self.volume, &n.id)
            .unwrap()
    }
    fn input(&mut self, m: Mutation, nodes: &[Node]) -> Input {
        Input {
            request: self.request(m),
            store: self.store.clone(),
            source: [1; 32],
            fingerprint: "b".repeat(64),
            entries: nodes
                .iter()
                .map(Entry::from_node)
                .collect::<Result<_>>()
                .unwrap(),
        }
    }
    fn remove(&mut self, n: &Node) -> Input {
        self.input(
            Mutation::RemoveEntry {
                node: guard(n),
                parent: NodeGuard {
                    id: ROOT.into(),
                    revision: 0,
                },
            },
            std::slice::from_ref(n),
        )
    }
    fn ack(&mut self, i: &Input, r: Receipt) -> Result<Record> {
        Journal::new(&mut self.client)?.acknowledge(&i.request.operation, Ok(r))
    }
    fn reopen_client(&mut self) {
        let placeholder = State::open_managed(&self.tmp.path().join("placeholder")).unwrap();
        drop(std::mem::replace(&mut self.client, placeholder));
        self.client = State::open_managed(&self.tmp.path().join("client")).unwrap();
    }
}
#[test]
fn unlink_observation_and_receipt_are_atomic_and_keep_dirty_bytes_after_restart() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE UPDATE ON logical_files BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN SELECT RAISE(IGNORE); END;",
    ] {
        let mut f = Fixture::new();
        let n = f.create("資料 🦀");
        let edit = f.stage(&n, 500, b"retained dirty bytes");
        let target = f.file(&n).binding.target().unwrap();
        let i = f.remove(&n);
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        let r = f.authority.namespace_apply(&i.request).unwrap();
        f.client.connection.execute_batch(trigger).unwrap();
        assert!(f.ack(&i, r.clone()).is_err());
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .get(&i.request.operation)
                .unwrap()
                .outcome
                .is_none()
        );
        assert_eq!(f.file(&n).base, n);
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        f.reopen_client();
        let saved = Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        assert!(saved.outcome.is_none());
        f.ack(&i, r.clone()).unwrap();
        let after = f.file(&n);
        assert_eq!(after.base, r.node);
        assert!(after.base.deleted);
        assert_eq!(after.head, edit.generation);
        assert_eq!(after.settled, None);
        assert_eq!(after.binding.target().unwrap(), target);
        let j = journal::Journal::new(&mut f.client, journal::Options::default()).unwrap();
        assert_eq!(
            j.read(edit.generation.unwrap(), 0, 100).unwrap(),
            b"retained dirty bytes"
        );
        f.reopen_client();
        assert!(f.file(&n).base.deleted);
        assert_eq!(f.ack(&i, r.clone()).unwrap().outcome, Some(Ok(r)));
    }
}
#[test]
fn replacement_observes_both_files_together_and_refuses_dirty_source() {
    let mut f = Fixture::new();
    let a = f.create("temporary");
    let b = f.create("destination");
    f.bind(&a);
    let dirty = f.stage(&b, 500, b"open destination data");
    let make = |a: &Node, b: &Node| Mutation::MoveEntry {
        destination_removal: None,
        node: guard(a),
        source_parent: NodeGuard {
            id: ROOT.into(),
            revision: 0,
        },
        parent: NodeGuard {
            id: ROOT.into(),
            revision: 0,
        },
        name: b.name.clone(),
        destination: Some(guard(b)),
    };
    let i = f.input(make(&b, &a), &[b.clone(), a.clone()]);
    assert!(Journal::new(&mut f.client).unwrap().begin(i).is_err());
    let i = f.input(make(&a, &b), &[a.clone(), b.clone()]);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let other = f.remove(&b);
    assert!(
        Journal::new(&mut f.client).unwrap().begin(other).is_err(),
        "pending effects on the same file must recover first"
    );
    let r = f.authority.namespace_apply(&i.request).unwrap();
    f.client.connection.execute_batch(&format!("CREATE TRIGGER fail BEFORE UPDATE ON logical_files WHEN NEW.node='{}' BEGIN SELECT RAISE(IGNORE); END;",b.id)).unwrap();
    assert!(f.ack(&i, r.clone()).is_err());
    assert_eq!(f.file(&a).base, a);
    assert_eq!(f.file(&b).base, b);
    f.client
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    let mut corrupt = r.clone();
    corrupt.node.placement = None;
    assert!(f.ack(&i, corrupt).is_err());
    f.ack(&i, r.clone()).unwrap();
    assert_eq!(f.file(&a).base, r.node);
    assert!(f.file(&b).base.deleted);
    assert_eq!(f.file(&b).head, dirty.generation);
}
#[test]
fn detached_content_guard_is_additive_and_never_resurrects_a_name() {
    assert_eq!(
        serde_json::to_string(&namespace::ContentGuard {
            expected: None,
            detached: false,
            removal: None
        })
        .unwrap(),
        "{\"expected\":null}"
    );
    let old: namespace::ContentGuard = serde_json::from_str("{\"expected\":null}").unwrap();
    assert!(!old.detached);
    let mut f = Fixture::new();
    let n = f.create("removed");
    let reserve = |n: &Node| Mutation::ReserveDetachedContent {
        node: n.id.clone(),
        expected_revision: n.revision,
        store: f.store.id.clone(),
        size: 1,
        sha256: "c".repeat(64),
        expected_payload: namespace::payload_reference(&n.payload),
    };
    let live = f.request(reserve(&n));
    assert_eq!(
        f.authority.namespace_apply(&live),
        Err(namespace::Error::Conflict)
    );
    let i = f.remove(&n);
    let tomb = f.authority.namespace_apply(&i.request).unwrap().node;
    let reserve = |n: &Node| Mutation::ReserveDetachedContent {
        node: n.id.clone(),
        expected_revision: n.revision,
        store: f.store.id.clone(),
        size: 1,
        sha256: "c".repeat(64),
        expected_payload: namespace::payload_reference(&n.payload),
    };
    let a = f.request(reserve(&tomb));
    let b = f.request(a.mutation.clone());
    f.authority.namespace_apply(&a).unwrap();
    f.authority.namespace_apply(&b).unwrap();
    let new = f.create("removed");
    let publish = |reservation: String| Mutation::Publish {
        reservation,
        upload_operation: "d".repeat(64),
        etag: "\"tag\"".into(),
        version: None,
    };
    let r = f.request(publish(a.operation));
    let winner = f.authority.namespace_apply(&r).unwrap();
    assert!(winner.node.deleted);
    assert_eq!(winner.node.id, n.id);
    assert_eq!(winner.node.name, n.name);
    let r = f.request(publish(b.operation));
    assert_eq!(
        f.authority.namespace_apply(&r),
        Err(namespace::Error::Conflict)
    );
    let list = f.authority.namespace_list(&f.volume, ROOT).unwrap();
    assert_eq!(list.children, vec![new]);
}

#[test]
fn namespace_pending_blocks_new_publication_and_input_tampering_is_refused() {
    let mut f = Fixture::new();
    let n = f.create("pending file");
    let edit = f.stage(&n, 500, b"bytes");
    let i = f.remove(&n);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let request = f.request(Mutation::ReserveContent {
        node: n.id.clone(),
        expected_revision: n.revision,
        store: f.store.id.clone(),
        size: edit.input.size,
        sha256: edit.input.sha256.clone(),
        expected_payload: None,
    });
    let publication = crate::managed::Input {
        reserve: request,
        publish_operation: id(600),
        store: f.store.clone(),
        source: [1; 32],
        fingerprint: "b".repeat(64),
    };
    assert!(
        logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .prepare(&id(500), publication.clone())
            .is_err()
    );
    let mut changed = i.clone();
    changed.entries[0].name = "another name".into();
    assert!(Journal::new(&mut f.client).unwrap().begin(changed).is_err());
    let r = f.authority.namespace_apply(&i.request).unwrap();
    f.ack(&i, r.clone()).unwrap();
    // The old live-file reservation cannot be attached to a deleted base.
    assert!(
        logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .prepare(&id(500), publication)
            .is_err()
    );
    let reserve = f.request(Mutation::ReserveDetachedContent {
        node: n.id.clone(),
        expected_revision: r.node.revision,
        store: f.store.id.clone(),
        size: edit.input.size,
        sha256: edit.input.sha256,
        expected_payload: None,
    });
    let publication = crate::managed::Input {
        reserve,
        publish_operation: id(601),
        store: f.store.clone(),
        source: [1; 32],
        fingerprint: "b".repeat(64),
    };
    assert!(
        logical::Journal::new(&mut f.client, journal::Options::default())
            .unwrap()
            .prepare(&id(500), publication)
            .is_ok()
    );
}
#[test]
fn full_disk_acknowledgment_preserves_pending_intent_and_dirty_generation() {
    let mut f = Fixture::new();
    let n = f.create("large proof");
    let reserve = f.request(Mutation::ReserveContent {
        node: n.id.clone(),
        expected_revision: n.revision,
        store: f.store.id.clone(),
        size: 0,
        sha256: "c".repeat(64),
        expected_payload: None,
    });
    f.authority.namespace_apply(&reserve).unwrap();
    let publish = f.request(Mutation::Publish {
        reservation: reserve.operation,
        upload_operation: "d".repeat(64),
        etag: format!("\"{}\"", "e".repeat(1000)),
        version: Some("v".repeat(1000)),
    });
    let n = f.authority.namespace_apply(&publish).unwrap().node;
    let edit = f.stage(&n, 500, b"dirty survives full disk");
    let i = f.remove(&n);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let r = f.authority.namespace_apply(&i.request).unwrap();
    f.client
        .connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
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
    assert!(
        f.ack(&i, r.clone()).is_err(),
        "test must reach real SQLite full while acknowledging"
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .get(&i.request.operation)
            .unwrap()
            .outcome
            .is_none()
    );
    assert_eq!(f.file(&n).base, n);
    assert_eq!(f.file(&n).head, edit.generation);
    f.client
        .connection
        .pragma_update(None, "max_page_count", pages + 64)
        .unwrap();
    f.ack(&i, r).unwrap();
    assert!(f.file(&n).base.deleted);
    assert_eq!(f.file(&n).head, edit.generation);
}

#[test]
fn ignored_or_deleted_begin_row_never_admits_namespace_rpc() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE INSERT ON namespace_intents BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail AFTER INSERT ON namespace_intents BEGIN DELETE FROM namespace_intents WHERE operation_id=NEW.operation_id; END;",
    ] {
        let mut f = Fixture::new();
        let n = f.create("no durable intent");
        f.stage(&n, 500, b"retain me");
        let input = f.remove(&n);
        f.client.connection.execute_batch(trigger).unwrap();
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .begin(input.clone())
                .is_err()
        );
        assert!(
            Journal::new(&mut f.client)
                .unwrap()
                .pending(None, 100)
                .unwrap()
                .is_empty()
        );
        assert!(!f.file(&n).base.deleted);
        assert_eq!(
            f.authority
                .namespace_operation(&f.volume, &input.request.operation),
            Ok(None)
        );
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        assert!(Journal::new(&mut f.client).unwrap().begin(input).is_ok());
    }
}
