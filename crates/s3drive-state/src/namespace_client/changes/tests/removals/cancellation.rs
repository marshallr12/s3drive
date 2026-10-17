use super::deferred::{prepare, transition};
use super::*;
use namespace::RemovalMode;

fn begin(f: &mut Fixture, i: &Input) {
    Journal::new(&mut f.client)
        .unwrap()
        .begin(i.clone())
        .unwrap();
}
fn proof(f: &mut Fixture, i: &Input) -> Option<crate::namespace_client::TransitionCancellation> {
    Journal::new(&mut f.client)
        .unwrap()
        .transition_cancellation(&i.request.operation)
        .unwrap()
}
#[test]
fn cancellation_closes_unknown_modes_in_both_orderings_without_fabricating_or_rewinding() {
    for deferred in [false, true] {
        for committed_first in [false, true] {
            let mut f = Fixture::new();
            let n = f.create("unknown café 日本語 🦀");
            let dirty = f.stage(&n, 500, b"binary\0\xff preserved");
            let i = if deferred {
                prepare(&mut f, &n)
            } else {
                arm(&mut f, &n)
            };
            let initial = apply(&mut f, &i);
            let first = owned(&mut f, &n).unwrap();
            let mode = if deferred {
                RemovalMode::Pending
            } else {
                RemovalMode::Deferred
            };
            let unknown = transition(&mut f, &initial.node, mode);
            begin(&mut f, &unknown);
            let late =
                committed_first.then(|| f.authority.namespace_apply(&unknown.request).unwrap());
            assert!(proof(&mut f, &unknown).is_none());
            let cancel = close(&mut f, &initial.node, &first.fence, false);
            let restored = apply(&mut f, &cancel);
            f.reopen_client();
            let p = proof(&mut f, &unknown).unwrap();
            assert_eq!(p.transition.input, unknown);
            assert!(p.transition.outcome.is_none());
            assert_eq!(p.cancellation.input, cancel);
            assert_eq!(p.cancellation.outcome, Some(Ok(restored.clone())));
            assert!(
                !crate::namespace_client::pending_for_node(&f.client.connection, &f.volume, &n.id)
                    .unwrap()
            );
            assert!(
                Journal::new(&mut f.client)
                    .unwrap()
                    .pending(None, 100)
                    .unwrap()
                    .iter()
                    .any(|r| r.input == unknown && r.outcome.is_none())
            );
            let successor = prepare(&mut f, &restored.node);
            let fresh = apply(&mut f, &successor);
            let current = owned(&mut f, &n).unwrap();
            assert_ne!(current.fence, first.fence);
            let actual = f.authority.namespace_apply(&unknown.request);
            assert_eq!(actual, late.ok_or(namespace::Error::Conflict));
            Journal::new(&mut f.client)
                .unwrap()
                .acknowledge(&unknown.request.operation, actual)
                .unwrap();
            assert_eq!(owned(&mut f, &n).unwrap(), current);
            assert_eq!(f.file(&n).base, fresh.node);
            assert_eq!(f.file(&n).head, dirty.generation);
            assert_eq!(
                journal::Journal::new(&mut f.client, journal::Options::default())
                    .unwrap()
                    .read(dirty.generation.unwrap(), 0, 100)
                    .unwrap(),
                b"binary\0\xff preserved"
            );
            assert!(
                proof(&mut f, &unknown).is_none(),
                "known late outcome no longer needs unknown-outcome proof"
            );
        }
    }
}

#[test]
fn real_authority_and_client_quota_keep_unknown_rejection_and_allow_final_cancellation() {
    let mut f = Fixture::new();
    let req = f.request(Mutation::CreateEntry {
        parent: ROOT.into(),
        parent_revision: 0,
        name: "nonempty 🦀".into(),
        node_kind: Kind::Directory,
        store: None,
    });
    let dir = f.authority.namespace_apply(&req).unwrap().node;
    let child_req = f.request(Mutation::CreateEntry {
        parent: dir.id.clone(),
        parent_revision: dir.revision,
        name: "child".into(),
        node_kind: Kind::EmptyFile,
        store: Some(f.store.id.clone()),
    });
    let child = f.authority.namespace_apply(&child_req).unwrap().node;
    let i = prepare(&mut f, &dir);
    let prepared = apply(&mut f, &i);
    let fence = owned(&mut f, &dir).unwrap().fence;
    let template = f.input(
        Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            name: "fixture".into(),
            node_kind: Kind::EmptyFile,
            store: Some(f.store.id.clone()),
        },
        &[],
    );
    let tx = f.client.connection.transaction().unwrap();
    for op in 10_000..14_093 {
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
    let count: i64 = f
        .authority
        .connection
        .query_row("SELECT count(*) FROM namespace_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    let tx = f.authority.connection.transaction().unwrap();
    for op in count..namespace::MAX_OPERATIONS - 2 {
        tx.execute(
            "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
            [id(100_000 + op as u64)],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let unknown = transition(&mut f, &prepared.node, RemovalMode::Pending);
    begin(&mut f, &unknown);
    assert_eq!(
        f.authority.namespace_apply(&unknown.request),
        Err(namespace::Error::Limit)
    );
    assert_eq!(
        f.authority
            .namespace_operation(&f.volume, &unknown.request.operation),
        Ok(None)
    );
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .acknowledge(&unknown.request.operation, Err(namespace::Error::Limit))
            .is_err()
    );
    let cancel = close(&mut f, &prepared.node, &fence, false);
    let restored = apply(&mut f, &cancel);
    assert_eq!(restored.node.payload, dir.payload);
    assert!(!restored.node.deleted);
    assert_eq!(
        f.authority
            .namespace_list(&f.volume, &dir.id)
            .unwrap()
            .children,
        vec![child]
    );
    f.reopen_client();
    let p = proof(&mut f, &unknown).unwrap();
    assert!(p.transition.outcome.is_none());
    assert_eq!(
        f.authority
            .namespace_operation(&f.volume, &unknown.request.operation),
        Ok(None)
    );
    assert!(
        !crate::namespace_client::pending_for_node(&f.client.connection, &f.volume, &dir.id)
            .unwrap()
    );
    assert_eq!(
        f.client
            .connection
            .query_row::<i64, _, _>("SELECT count(*) FROM namespace_intents", [], |r| r.get(0))
            .unwrap(),
        LIMIT
    );
}

#[test]
fn cancellation_acknowledgment_rollback_and_full_disk_keep_epoch_and_unknown_transition() {
    for full in [false, true] {
        let mut f = Fixture::new();
        let n = f.create("atomic cancel");
        let dirty = f.stage(&n, 500, b"preserved");
        let i = prepare(&mut f, &n);
        let initial = apply(&mut f, &i);
        let unknown = transition(&mut f, &initial.node, RemovalMode::Pending);
        begin(&mut f, &unknown);
        f.authority.namespace_apply(&unknown.request).unwrap();
        let old = owned(&mut f, &n).unwrap();
        let cancel = close(&mut f, &initial.node, &old.fence, false);
        begin(&mut f, &cancel);
        let receipt = f.authority.namespace_apply(&cancel.request).unwrap();
        if full {
            f.client
                .connection
                .execute_batch(
                    "CREATE TABLE cancel_full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)",
                )
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
            f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON namespace_intents BEGIN INSERT INTO cancel_full VALUES(zeroblob(1048576)); END").unwrap();
        } else {
            f.client.connection.execute_batch("CREATE TRIGGER fail BEFORE UPDATE ON logical_files BEGIN SELECT RAISE(IGNORE); END").unwrap();
        }
        let error = f.ack(&cancel, receipt.clone()).unwrap_err().to_string();
        if full {
            assert!(error.contains("full"), "{error}");
        }
        assert_eq!(owned(&mut f, &n).unwrap(), old);
        assert_eq!(f.file(&n).base, initial.node);
        assert!(proof(&mut f, &unknown).is_none());
        assert_eq!(f.file(&n).head, dirty.generation);
        f.client
            .connection
            .pragma_update(None, "max_page_count", 1_000_000)
            .unwrap();
        f.client
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        f.reopen_client();
        f.ack(&cancel, receipt).unwrap();
        assert!(proof(&mut f, &unknown).is_some());
    }
}

#[test]
fn foreign_bindings_unrelated_unknown_effects_and_missing_history_never_authorize_recovery() {
    let mut f = Fixture::new();
    let n = f.create("guarded");
    let i = prepare(&mut f, &n);
    let r = apply(&mut f, &i);
    let unknown = transition(&mut f, &r.node, RemovalMode::Pending);
    begin(&mut f, &unknown);
    let old = owned(&mut f, &n).unwrap();
    let cancel = close(&mut f, &r.node, &old.fence, false);
    for change in 0..4 {
        let mut wrong = cancel.clone();
        wrong.request.operation = id(8000 + change);
        match change {
            0 => wrong.source = [9; 32],
            1 => wrong.fingerprint = "c".repeat(64),
            2 => {
                let Mutation::CancelRemoval { fence, .. } = &mut wrong.request.mutation else {
                    unreachable!()
                };
                fence.owner = id(77);
            }
            _ => {
                let Mutation::CancelRemoval { fence, .. } = &mut wrong.request.mutation else {
                    unreachable!()
                };
                fence.intent = id(77);
            }
        }
        assert!(Journal::new(&mut f.client).unwrap().begin(wrong).is_err());
    }
    let restored = apply(&mut f, &cancel);
    assert!(proof(&mut f, &unknown).is_some());
    let remove = f.remove(&restored.node);
    begin(&mut f, &remove);
    assert!(
        crate::namespace_client::pending_for_node(&f.client.connection, &f.volume, &n.id).unwrap(),
        "unrelated unknown remove is still a barrier"
    );
    f.client
        .connection
        .execute(
            "DELETE FROM namespace_intents WHERE operation_id=?",
            [&i.request.operation],
        )
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .transition_cancellation(&unknown.request.operation)
            .is_err(),
        "closing receipt without acknowledged origin is not proof"
    );
}

#[test]
fn cancellation_exception_preserves_prepared_publication_barrier_and_rejects_corrupt_proof() {
    let mut f = Fixture::new();
    let n = f.create("prepared");
    let edit = f.stage(&n, 500, b"dirty");
    let i = prepare(&mut f, &n);
    let initial = apply(&mut f, &i);
    let old = owned(&mut f, &n).unwrap();
    let reserve = f.request(Mutation::ReservePendingContent {
        node: n.id.clone(),
        expected_revision: initial.node.revision,
        store: f.store.id.clone(),
        size: edit.input.size,
        sha256: edit.input.sha256.clone(),
        expected_payload: None,
        fence: old.fence.clone(),
    });
    let publication = crate::managed::Input {
        reserve,
        publish_operation: id(999),
        store: f.store.clone(),
        source: [1; 32],
        fingerprint: "b".repeat(64),
    };
    logical::Journal::new(&mut f.client, journal::Options::default())
        .unwrap()
        .prepare(&edit.input.id, publication)
        .unwrap();
    let unknown = transition(&mut f, &initial.node, RemovalMode::Pending);
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .begin(unknown.clone())
            .is_err()
    );
    // Inject a hash-valid concurrent unknown row to ensure the cancellation
    // exception never skips the independent logical publication barrier.
    let record = Record {
        input: unknown.clone(),
        outcome: None,
    };
    f.client
        .connection
        .execute(
            "INSERT INTO namespace_intents VALUES(?1,?2,NULL,?3)",
            params![
                unknown.request.operation,
                serde_json::to_vec(&unknown).unwrap(),
                hash(&record).unwrap()
            ],
        )
        .unwrap();
    let cancel = close(&mut f, &initial.node, &old.fence, false);
    assert!(Journal::new(&mut f.client).unwrap().begin(cancel).is_err());
    let mut f = Fixture::new();
    let n = f.create("corrupt proof");
    let i = prepare(&mut f, &n);
    let r = apply(&mut f, &i);
    let unknown = transition(&mut f, &r.node, RemovalMode::Pending);
    begin(&mut f, &unknown);
    let old = owned(&mut f, &n).unwrap();
    let cancel = close(&mut f, &r.node, &old.fence, false);
    apply(&mut f, &cancel);
    f.client
        .connection
        .execute(
            "UPDATE namespace_intents SET state_hash=zeroblob(32) WHERE operation_id=?",
            [&cancel.request.operation],
        )
        .unwrap();
    assert!(
        Journal::new(&mut f.client)
            .unwrap()
            .transition_cancellation(&unknown.request.operation)
            .is_err()
    );
    assert!(
        crate::namespace_client::pending_for_node(&f.client.connection, &f.volume, &n.id).is_err()
    );
}

#[test]
fn unknown_commit_and_unknown_origin_still_require_exact_recovery() {
    let mut f = Fixture::new();
    let n = f.create("unknown terminal operation");
    let i = arm(&mut f, &n);
    let armed = apply(&mut f, &i);
    let fence = owned(&mut f, &n).unwrap().fence;
    let commit = close(&mut f, &armed.node, &fence, true);
    begin(&mut f, &commit);
    let cancel = close(&mut f, &armed.node, &fence, false);
    assert!(Journal::new(&mut f.client).unwrap().begin(cancel).is_err());
    assert!(proof(&mut f, &commit).is_none());
    let mut f = Fixture::new();
    let n = f.create("unknown initial reservation");
    let initial = prepare(&mut f, &n);
    begin(&mut f, &initial);
    let receipt = f.authority.namespace_apply(&initial.request).unwrap();
    let Some(Artifact::Removal(fence)) = receipt.artifact else {
        unreachable!()
    };
    let cancel = close(&mut f, &receipt.node, &fence, false);
    assert!(Journal::new(&mut f.client).unwrap().begin(cancel).is_err());
    assert!(proof(&mut f, &initial).is_none());
}
