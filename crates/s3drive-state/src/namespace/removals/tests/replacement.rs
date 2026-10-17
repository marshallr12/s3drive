use super::*;

fn fixture(deferred_source: bool, mode: RemovalMode) -> (World, Apply, Node, PendingRemoval) {
    let mut w = World::new();
    let n = w.file("source café 日本語 🦀");
    let n = w.publish(&n, None);
    let n = if deferred_source {
        deferred::prepare(&mut w, &n).node
    } else {
        n
    };
    let t = w.file("target café 日本語 🦀");
    let t = w.publish(&t, None);
    let p = if mode == RemovalMode::Deferred {
        deferred::prepare(&mut w, &t)
    } else {
        w.arm(&t)
    };
    let claim = Some(Box::new(RemovalClaim {
        fence: p.fence.clone(),
        previous: p.fence.intent.clone(),
        mode,
    }));
    let r = w.request(if deferred_source {
        let source = w.inspect(&n);
        Mutation::MoveDeferredEntry {
            node: guard(&n),
            source_parent: guard(&w.root()),
            parent: guard(&w.root()),
            name: t.name.clone(),
            destination: Some(guard(&p.node)),
            fence: Box::new(source.fence.clone()),
            previous: source.fence.intent.into(),
            destination_removal: claim,
        }
    } else {
        Mutation::MoveEntry {
            node: guard(&n),
            source_parent: guard(&w.root()),
            parent: guard(&w.root()),
            name: t.name.clone(),
            destination: Some(guard(&p.node)),
            destination_removal: claim,
        }
    });
    (w, r, n, p)
}
#[test]
fn owned_replacement_atomically_closes_both_target_modes_and_preserves_source_epoch_and_bytes() {
    for source in [false, true] {
        for mode in [RemovalMode::Deferred, RemovalMode::Pending] {
            let (mut w, r, n, p) = fixture(source, mode);
            let receipt = w.state.namespace_apply(&r).unwrap();
            let old = w.state.namespace_node(&w.volume, &p.node.id).unwrap();
            assert!(old.deleted);
            assert_eq!(old.payload, p.node.payload);
            assert_eq!(receipt.node.payload, n.payload);
            assert_eq!(receipt.node.id, n.id);
            assert_eq!(
                w.state.namespace_pending_removal(&w.volume, &old.id),
                Ok(None)
            );
            if source {
                assert_eq!(w.inspect(&n).node, receipt.node);
            }
            let close = w.cancel(&p);
            assert_eq!(w.state.namespace_apply(&close), Err(Error::Conflict));
            w.reopen();
            assert_eq!(w.state.namespace_apply(&r), Ok(receipt.clone()));
            assert_eq!(
                w.state.namespace_pending_removal(&w.volume, &old.id),
                Ok(None)
            );
            assert_eq!(w.state.namespace_node(&w.volume, &n.id), Ok(receipt.node));
        }
    }
}
#[test]
fn owned_replacement_refuses_stale_foreign_or_missing_claims_without_partial_changes() {
    for case in 0..7 {
        let (mut w, mut r, n, p) = fixture(true, RemovalMode::Deferred);
        let Mutation::MoveDeferredEntry {
            destination_removal,
            destination,
            ..
        } = &mut r.mutation
        else {
            unreachable!()
        };
        match case {
            0 => destination_removal.as_mut().unwrap().fence.owner = id(888),
            1 => destination_removal.as_mut().unwrap().fence.intent = id(888),
            2 => destination_removal.as_mut().unwrap().previous = id(888),
            3 => destination_removal.as_mut().unwrap().mode = RemovalMode::Pending,
            4 => destination.as_mut().unwrap().revision -= 1,
            5 => *destination_removal = None,
            _ => *destination = None,
        }
        assert!(w.state.namespace_apply(&r).is_err());
        assert_eq!(w.inspect(&n).node, n);
        assert_eq!(w.inspect(&p.node), p);
    }
}
#[test]
fn owned_replacement_ignored_writes_roll_back_both_event_chains_and_exact_receipt() {
    for case in 0..7 {
        let (mut w, r, n, p) = fixture(true, RemovalMode::Deferred);
        let trigger = match case {
            0 => "BEFORE DELETE ON namespace_removals".into(),
            1 => format!(
                "BEFORE INSERT ON namespace_removal_events WHEN NEW.node_id='{}'",
                n.id
            ),
            2 => format!(
                "BEFORE INSERT ON namespace_removal_events WHEN NEW.node_id='{}'",
                p.node.id
            ),
            3 => format!(
                "BEFORE UPDATE ON namespace_nodes WHEN NEW.node_id='{}'",
                n.id
            ),
            4 => format!(
                "BEFORE UPDATE ON namespace_nodes WHEN NEW.node_id='{}'",
                p.node.id
            ),
            5 => "BEFORE INSERT ON namespace_operations".into(),
            _ => "BEFORE UPDATE ON namespace_volume".into(),
        };
        let v = w.state.namespace_volume().unwrap();
        w.state
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER fail {trigger} BEGIN SELECT RAISE(IGNORE); END;"
            ))
            .unwrap();
        assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
        assert_eq!(w.state.namespace_volume().unwrap(), v);
        assert_eq!(
            w.state.namespace_operation(&w.volume, &r.operation),
            Ok(None)
        );
        assert_eq!(w.inspect(&n).node, n);
        assert_eq!(w.inspect(&p.node), p);
        w.state
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        assert!(w.state.namespace_apply(&r).is_ok());
    }
}
#[test]
fn owned_replacement_tampered_closed_target_proof_is_not_adopted() {
    for field in ["owner_id", "intent_id", "mode"] {
        let (mut w, r, _, p) = fixture(false, RemovalMode::Pending);
        w.state.namespace_apply(&r).unwrap();
        let value = if field == "mode" {
            "cancelled".into()
        } else {
            id(777)
        };
        w.state.connection.execute(&format!("UPDATE namespace_removal_events SET {field}=? WHERE node_id=? AND operation_id=?"), params![value,p.node.id,r.operation]).unwrap();
        assert_eq!(
            w.state.namespace_pending_removal(&w.volume, &p.node.id),
            Err(Error::Internal)
        );
    }
}

#[test]
fn owned_replacement_keeps_reserved_authority_closing_capacity() {
    let (mut w, r, n, p) = fixture(true, RemovalMode::Deferred);
    let count: i64 = w
        .state
        .connection
        .query_row("SELECT count(*) FROM namespace_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    let tx = w.state.connection.transaction().unwrap();
    for op in 0..MAX_OPERATIONS - count - 4 {
        tx.execute(
            "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
            [id(100000 + op as u64)],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Limit));
    assert_eq!(w.inspect(&n).node, n);
    assert_eq!(w.inspect(&p.node), p);
    w.state
        .connection
        .execute(
            "DELETE FROM namespace_operations WHERE operation_id=?",
            [id(100000)],
        )
        .unwrap();
    assert!(w.state.namespace_apply(&r).is_ok());
}

#[test]
fn owned_replacement_requires_latest_target_transition_and_keeps_old_requests_exact() {
    let (mut w, mut r, n, p) = fixture(true, RemovalMode::Deferred);
    let transition = w.request(Mutation::TransitionRemoval {
        node: guard(&p.node),
        parent: guard(&w.root()),
        fence: p.fence.clone(),
        expected: RemovalMode::Deferred,
        mode: RemovalMode::Pending,
    });
    w.state.namespace_apply(&transition).unwrap();
    let p = w.inspect(&p.node);
    let Mutation::MoveDeferredEntry { destination, .. } = &mut r.mutation else {
        unreachable!()
    };
    *destination = Some(guard(&p.node));
    r.expected_sequence = w.state.namespace_volume().unwrap().sequence;
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Conflict));
    assert_eq!(w.inspect(&n).node, n);
    let mut next = r.clone();
    next.operation = id(50000);
    let Mutation::MoveDeferredEntry {
        destination_removal: Some(c),
        ..
    } = &mut next.mutation
    else {
        unreachable!()
    };
    c.previous = transition.operation;
    c.mode = RemovalMode::Pending;
    assert!(w.state.namespace_apply(&next).is_ok());
    w.reopen();
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Conflict));
    assert_eq!(
        w.state.namespace_pending_removal(&w.volume, &p.node.id),
        Ok(None)
    );
}

#[test]
fn owned_replacement_sqlite_full_is_unknown_and_retry_uses_original_request() {
    let (mut w, r, n, p) = fixture(true, RemovalMode::Pending);
    w.state
        .connection
        .execute_batch("CREATE TABLE full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = w
        .state
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    w.state
        .connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    w.state.connection.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON namespace_removal_events BEGIN INSERT INTO full VALUES(zeroblob(1048576)); END").unwrap();
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
    assert_eq!(
        w.state.namespace_operation(&w.volume, &r.operation),
        Ok(None)
    );
    assert_eq!(w.inspect(&n).node, n);
    assert_eq!(w.inspect(&p.node), p);
    w.state
        .connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    w.state
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    w.reopen();
    assert!(w.state.namespace_apply(&r).is_ok());
}
