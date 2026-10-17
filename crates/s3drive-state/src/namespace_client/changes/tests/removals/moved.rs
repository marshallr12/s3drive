use super::*;
use namespace::RemovalMode;
fn movement(f: &mut Fixture, n: &Node, name: &str, target: Option<&Node>) -> Input {
    let old = owned(f, n).unwrap();
    let mut entries = vec![n.clone()];
    if let Some(t) = target
        && t.id != n.id
    {
        entries.push(t.clone());
    }
    f.input(
        Mutation::MoveDeferredEntry {
            destination_removal: None,
            node: guard(n),
            source_parent: NodeGuard {
                id: n.parent.clone().unwrap(),
                revision: 0,
            },
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            name: name.into(),
            destination: target.map(guard),
            fence: Box::new(old.fence),
            previous: old.event.into(),
        },
        &entries,
    )
}
#[test]
fn owner_move_ack_is_atomic_preserves_original_binding_and_latest_event() {
    let mut f = Fixture::new();
    let n = f.create("source café 🦀");
    let target = f.create("target 日本語");
    let origin = deferred::prepare(&mut f, &n);
    let prepared = apply(&mut f, &origin);
    let bound = f.bind(&prepared.node);
    let i = movement(&mut f, &prepared.node, &target.name, Some(&target));
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let receipt = f.authority.namespace_apply(&i.request).unwrap();
    let initial = owned(&mut f, &n).unwrap();
    assert_eq!(initial.event, origin.request.operation);
    f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert!(f.ack(&i, receipt.clone()).is_err());
    assert_eq!(owned(&mut f, &n).unwrap(), initial);
    assert_eq!(f.file(&n).base, prepared.node);
    assert_eq!(f.file(&n).head, bound.head);
    f.client
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    f.reopen_client();
    f.ack(&i, receipt.clone()).unwrap();
    let current = owned(&mut f, &n).unwrap();
    assert_eq!(current.input, origin);
    assert_eq!(current.event, i.request.operation);
    assert_eq!(current.receipt, receipt);
    assert_eq!(current.fence, initial.fence);
    assert_eq!(f.file(&n).base, receipt.node);
    assert_eq!(f.file(&n).head, bound.head);
    let next = movement(&mut f, &receipt.node, "TARGET 日本語", Some(&receipt.node));
    let next_receipt = apply(&mut f, &next);
    f.ack(&i, receipt).unwrap();
    let newer = owned(&mut f, &n).unwrap();
    assert_eq!(newer.event, next.request.operation);
    assert_eq!(newer.receipt, next_receipt);
    let cancel = close(&mut f, &next_receipt.node, &newer.fence, false);
    let kept = apply(&mut f, &cancel);
    assert_eq!(kept.node.name, "TARGET 日本語");
    let successor = deferred::prepare(&mut f, &kept.node);
    apply(&mut f, &successor);
    let newest = owned(&mut f, &n).unwrap();
    f.ack(
        &i,
        f.authority
            .namespace_operation(&f.volume, &i.request.operation)
            .unwrap()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(owned(&mut f, &n).unwrap(), newest);
}
#[test]
fn unknown_owner_move_blocks_cancellation_publication_target_and_new_namespace_effects() {
    let mut f = Fixture::new();
    let n = f.create("source");
    let target = f.create("target");
    let origin = deferred::prepare(&mut f, &n);
    let prepared = apply(&mut f, &origin);
    let i = movement(&mut f, &prepared.node, &target.name, Some(&target));
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let receipt = f.authority.namespace_apply(&i.request).unwrap();
    let old = owned(&mut f, &n).unwrap();
    let cancel = close(&mut f, &prepared.node, &old.fence, false);
    assert!(Journal::new(&mut f.client).unwrap().begin(cancel).is_err());
    for node in [&prepared.node, &target] {
        assert!(
            crate::namespace_client::pending_for_node(&f.client.connection, &f.volume, &node.id)
                .unwrap()
        );
    }
    let remove = f.remove(&target);
    assert!(Journal::new(&mut f.client).unwrap().begin(remove).is_err());
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .transition_cancellation(&i.request.operation)
            .unwrap()
            .is_none()
    );
    let mut forged = receipt.clone();
    if let Some(Artifact::RemovalMove { fence, .. }) = &mut forged.artifact {
        fence.owner = id(999);
    }
    assert!(f.ack(&i, forged).is_err());
    let mut forged = receipt.clone();
    if let Some(Artifact::RemovalMove {
        displaced: Some(n), ..
    }) = &mut forged.artifact
    {
        n.name = "wrong".into();
    }
    assert!(f.ack(&i, forged).is_err());
    assert_eq!(owned(&mut f, &n).unwrap(), old);
    f.reopen_client();
    f.ack(&i, receipt.clone()).unwrap();
    let cancel = close(&mut f, &receipt.node, &old.fence, false);
    apply(&mut f, &cancel);
}
#[test]
fn owner_move_admission_refuses_foreign_fences_stale_events_and_pending_mode() {
    let mut f = Fixture::new();
    let n = f.create("source");
    let origin = deferred::prepare(&mut f, &n);
    let prepared = apply(&mut f, &origin);
    let i = movement(&mut f, &prepared.node, "new", None);
    for case in 0..4 {
        let mut bad = i.clone();
        bad.request.operation = id(9000 + case);
        let Mutation::MoveDeferredEntry {
            fence, previous, ..
        } = &mut bad.request.mutation
        else {
            unreachable!()
        };
        match case {
            0 => fence.owner = id(999),
            1 => *previous = id(888).into(),
            2 => bad.fingerprint = "c".repeat(64),
            _ => bad.entries[0].name = "forged".into(),
        }
        assert!(Journal::new(&mut f.client).unwrap().begin(bad).is_err());
    }
    let promote = deferred::transition(&mut f, &prepared.node, RemovalMode::Pending);
    let p = apply(&mut f, &promote);
    let i = movement(&mut f, &p.node, "pending", None);
    assert!(Journal::new(&mut f.client).unwrap().begin(i).is_err());
}
#[test]
fn dirty_owner_move_is_refused_before_admission_and_preserves_binary_generation() {
    let mut f = Fixture::new();
    let n = f.create("dirty source 🦀");
    let origin = deferred::prepare(&mut f, &n);
    let prepared = apply(&mut f, &origin);
    let dirty = f.stage(&prepared.node, 500, b"binary\0\xff");
    let i = movement(&mut f, &prepared.node, "destination", None);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .is_err()
    );
    assert!(
        load(&f.client.connection, &i.request.operation)
            .unwrap()
            .is_none()
    );
    f.reopen_client();
    assert_eq!(f.file(&n).head, dirty.generation);
    assert_eq!(owned(&mut f, &n).unwrap().event, origin.request.operation);
}
#[test]
fn full_disk_owner_move_ack_does_not_advance_receipt_or_logical_identity() {
    let mut f = Fixture::new();
    let n = f.create("full source");
    let origin = deferred::prepare(&mut f, &n);
    let prepared = apply(&mut f, &origin);
    f.bind(&prepared.node);
    let i = movement(&mut f, &prepared.node, "new name", None);
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let receipt = f.authority.namespace_apply(&i.request).unwrap();
    let old = owned(&mut f, &n).unwrap();
    f.client
        .connection
        .execute_batch("CREATE TABLE move_full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
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
    f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN INSERT INTO move_full VALUES(zeroblob(1048576)); END").unwrap();
    assert!(f.ack(&i, receipt.clone()).is_err());
    assert_eq!(owned(&mut f, &n).unwrap(), old);
    assert_eq!(f.file(&n).base, prepared.node);
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
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    f.client
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    f.reopen_client();
    f.ack(&i, receipt.clone()).unwrap();
    assert_eq!(f.file(&n).base, receipt.node);
}
#[test]
fn owner_move_uses_ordinary_local_capacity_and_preserves_both_closing_slots() {
    let mut f = Fixture::new();
    let n = f.create("quota source");
    let origin = deferred::prepare(&mut f, &n);
    let prepared = apply(&mut f, &origin);
    let i = movement(&mut f, &prepared.node, "quota moved", None);
    let template = Input {
        request: f.request(Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: Some(f.store.id.clone()),
            name: "fixture".into(),
            node_kind: Kind::EmptyFile,
        }),
        entries: vec![],
        ..i.clone()
    };
    let tx = f.client.connection.transaction().unwrap();
    for op in 10_000..14_093 {
        let mut input = template.clone();
        input.request.operation = id(op);
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
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .is_err()
    );
    assert!(
        load(&f.client.connection, &i.request.operation)
            .unwrap()
            .is_none()
    );
    f.client
        .connection
        .execute(
            "DELETE FROM namespace_intents WHERE operation_id=?",
            [id(10_000)],
        )
        .unwrap();
    let moved = apply(&mut f, &i);
    let promote = deferred::transition(&mut f, &moved.node, RemovalMode::Pending);
    let p = apply(&mut f, &promote);
    let old = owned(&mut f, &n).unwrap();
    let commit = close(&mut f, &p.node, &old.fence, true);
    assert!(apply(&mut f, &commit).node.deleted);
}
