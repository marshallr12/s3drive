use super::*;
fn input(f: &mut Fixture, kind: Kind) -> Input {
    let owner = owner(f);
    f.input(
        Mutation::CreateDeferredEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: (kind == Kind::EmptyFile).then(|| f.store.id.clone()),
            name: "created café 日本語 🦀".into(),
            node_kind: kind,
            owner,
        },
        &[],
    )
}
#[test]
fn predicted_created_node_and_parent_are_blocked_until_exact_origin_is_acknowledged() {
    for kind in [Kind::EmptyFile, Kind::Directory] {
        let mut f = Fixture::new();
        let i = input(&mut f, kind);
        Journal::new(&mut f.client)
            .unwrap()
            .begin(i.clone())
            .unwrap();
        let receipt = f.authority.namespace_apply(&i.request).unwrap();
        assert!(owned(&mut f, &receipt.node).is_none());
        assert!(
            crate::namespace_client::pending_for_node(
                &f.client.connection,
                &f.volume,
                &receipt.node.id
            )
            .unwrap()
        );
        let Some(Artifact::Removal(fence)) = &receipt.artifact else {
            unreachable!()
        };
        let cancel = close(&mut f, &receipt.node, fence, false);
        assert!(
            Journal::new(&mut f.client).unwrap().begin(cancel).is_err(),
            "unacknowledged origin cannot be cancelled"
        );
        if kind == Kind::Directory {
            let child = f.input(
                Mutation::CreateEntry {
                    parent: receipt.node.id.clone(),
                    parent_revision: receipt.node.revision,
                    store: Some(f.store.id.clone()),
                    name: "child".into(),
                    node_kind: Kind::EmptyFile,
                },
                &[],
            );
            assert!(Journal::new(&mut f.client).unwrap().begin(child).is_err());
        } else {
            let edit = f.stage(&receipt.node, 500, b"dirty before acknowledgment");
            for fenced in [false, true] {
                let mutation = if fenced {
                    Mutation::ReservePendingContent {
                        node: receipt.node.id.clone(),
                        expected_revision: receipt.node.revision,
                        store: f.store.id.clone(),
                        size: edit.input.size,
                        sha256: edit.input.sha256.clone(),
                        expected_payload: None,
                        fence: fence.clone(),
                    }
                } else {
                    Mutation::ReserveContent {
                        node: receipt.node.id.clone(),
                        expected_revision: receipt.node.revision,
                        store: f.store.id.clone(),
                        size: edit.input.size,
                        sha256: edit.input.sha256.clone(),
                        expected_payload: None,
                    }
                };
                let reserve = f.request(mutation);
                let publication = crate::managed::Input {
                    reserve,
                    publish_operation: id(f.op + 10_000),
                    store: f.store.clone(),
                    source: [1; 32],
                    fingerprint: "b".repeat(64),
                };
                assert!(
                    logical::Journal::new(&mut f.client, journal::Options::default())
                        .unwrap()
                        .prepare(&edit.input.id, publication)
                        .is_err()
                );
            }
            assert_eq!(f.file(&receipt.node).head, edit.generation);
        }
        let mut wrong = receipt.clone();
        wrong.artifact = Some(Artifact::Removal(RemovalFence {
            intent: i.request.operation.clone(),
            owner: id(888),
        }));
        assert!(f.ack(&i, wrong).is_err());
        assert!(owned(&mut f, &receipt.node).is_none());
        f.reopen_client();
        f.ack(&i, receipt.clone()).unwrap();
        let initial = owned(&mut f, &receipt.node).unwrap();
        assert!(initial.input.entries.is_empty());
        assert_eq!(initial.fence, fence.clone());
        assert_eq!(initial.mode, namespace::RemovalMode::Deferred);
        let cancel = close(&mut f, &receipt.node, fence, false);
        let restored = apply(&mut f, &cancel);
        let next = arm(&mut f, &restored.node);
        apply(&mut f, &next);
        let current = owned(&mut f, &receipt.node).unwrap();
        f.ack(&i, receipt.clone()).unwrap();
        assert_eq!(owned(&mut f, &receipt.node).unwrap(), current);
    }
}
#[test]
fn foreign_creation_and_ignored_ack_never_adopt_a_created_fence() {
    let mut f = Fixture::new();
    let i = input(&mut f, Kind::EmptyFile);
    let mut wrong = i.clone();
    let Mutation::CreateDeferredEntry { owner, .. } = &mut wrong.request.mutation else {
        unreachable!()
    };
    *owner = id(999);
    assert!(Journal::new(&mut f.client).unwrap().begin(wrong).is_err());
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
    let r = f.authority.namespace_apply(&i.request).unwrap();
    f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert!(f.ack(&i, r.clone()).is_err());
    assert!(owned(&mut f, &r.node).is_none());
    f.client
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    f.reopen_client();
    f.ack(&i, r.clone()).unwrap();
    assert_eq!(owned(&mut f, &r.node).unwrap().receipt, r);
}
#[test]
fn unknown_atomic_create_reserves_local_promotion_and_final_close_slots() {
    let mut f = Fixture::new();
    let i = input(&mut f, Kind::EmptyFile);
    let mut template = i.clone();
    template.request.mutation = Mutation::CreateEntry {
        parent: ROOT.into(),
        parent_revision: 0,
        store: Some(f.store.id.clone()),
        name: "fixture".into(),
        node_kind: Kind::EmptyFile,
    };
    let tx = f.client.connection.transaction().unwrap();
    for op in 10_000..14_094 {
        let mut input = template.clone();
        input.request.operation = id(op);
        let r = Record {
            input,
            outcome: None,
        };
        tx.execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,NULL,?3)",
            params![
                r.input.request.operation,
                serde_json::to_vec(&r.input).unwrap(),
                hash(&r).unwrap()
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
    let receipt = apply(&mut f, &i);
    let initial = owned(&mut f, &receipt.node).unwrap();
    let promote = f.input(
        Mutation::TransitionRemoval {
            node: guard(&receipt.node),
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: initial.fence.clone(),
            expected: namespace::RemovalMode::Deferred,
            mode: namespace::RemovalMode::Pending,
        },
        std::slice::from_ref(&receipt.node),
    );
    let promoted = apply(&mut f, &promote);
    let commit = close(&mut f, &promoted.node, &initial.fence, true);
    apply(&mut f, &commit);
    assert!(owned(&mut f, &receipt.node).is_none());
}
