use super::*;
fn prepare(w: &mut World, n: &Node) -> PendingRemoval {
    let mut request = w.arm_request(n);
    let Mutation::ArmRemoval {
        node,
        parent,
        owner,
    } = request.mutation
    else {
        unreachable!()
    };
    request.mutation = Mutation::PrepareRemoval {
        node,
        parent,
        owner,
    };
    let r = w.state.namespace_apply(&request).unwrap();
    assert_eq!(w.state.namespace_apply(&request), Ok(r));
    w.inspect(n)
}
fn transition(w: &mut World, p: &PendingRemoval, mode: RemovalMode) -> Apply {
    w.request(Mutation::TransitionRemoval {
        node: guard(&p.node),
        parent: guard(&w.root()),
        fence: p.fence.clone(),
        expected: p.mode,
        mode,
    })
}
#[test]
fn deferred_nonempty_directory_allows_children_but_pending_promotion_requires_empty() {
    let mut w = World::new();
    let dir = w
        .create(&w.root(), "deferred 日本語 🦀", Kind::Directory)
        .unwrap()
        .node;
    let first = w.create(&dir, "existing", Kind::EmptyFile).unwrap().node;
    let p = prepare(&mut w, &dir);
    assert_eq!(p.mode, RemovalMode::Deferred);
    assert_eq!(
        w.state.namespace_node(&w.volume, &dir.id),
        Ok(p.node.clone())
    );
    let child = w.create(&p.node, "café 🦀", Kind::EmptyFile).unwrap().node;
    assert_eq!(
        w.state
            .namespace_list(&w.volume, &dir.id)
            .unwrap()
            .children
            .len(),
        2
    );
    let promote = transition(&mut w, &p, RemovalMode::Pending);
    assert_eq!(w.state.namespace_apply(&promote), Err(Error::NotEmpty));
    assert_eq!(w.inspect(&dir), p);
    let commit = w.commit(&p);
    assert_eq!(w.state.namespace_apply(&commit), Err(Error::Conflict));
    for node in [&first, &child] {
        w.apply(Mutation::RemoveEntry {
            node: guard(node),
            parent: guard(&p.node),
        })
        .unwrap();
    }
    let promote = transition(&mut w, &p, RemovalMode::Pending);
    let r = w.state.namespace_apply(&promote).unwrap();
    let pending = w.inspect(&dir);
    assert_eq!(pending.fence, p.fence);
    assert_eq!(pending.mode, RemovalMode::Pending);
    assert_eq!(pending.node, r.node);
    assert_eq!(
        w.state.namespace_node(&w.volume, &dir.id),
        Err(Error::DeletePending)
    );
    assert_eq!(
        w.create(&pending.node, "denied child", Kind::EmptyFile),
        Err(Error::DeletePending)
    );
    let defer = transition(&mut w, &pending, RemovalMode::Deferred);
    w.state.namespace_apply(&defer).unwrap();
    let p = w.inspect(&dir);
    let child = w
        .create(&p.node, "preserved child", Kind::EmptyFile)
        .unwrap()
        .node;
    let cancel = w.cancel(&p);
    w.state.namespace_apply(&cancel).unwrap();
    assert_eq!(
        w.state.namespace_list(&w.volume, &dir.id).unwrap().children,
        vec![child]
    );
    w.reopen();
    assert_eq!(w.state.namespace_apply(&promote), Ok(r));
    assert_eq!(
        w.state.namespace_pending_removal(&w.volume, &dir.id),
        Ok(None)
    );
}
#[test]
fn owner_publication_survives_same_fence_modes_and_expired_claims_cannot_write() {
    let mut w = World::new();
    let file = w.file("payload café 🦀");
    let mut p = prepare(&mut w, &file);
    for mode in [
        RemovalMode::Deferred,
        RemovalMode::Pending,
        RemovalMode::Deferred,
    ] {
        if p.mode != mode {
            let request = transition(&mut w, &p, mode);
            w.state.namespace_apply(&request).unwrap();
            p = w.inspect(&file);
        }
        let ordinary = w.reserve(&p.node, None);
        assert_eq!(
            w.state.namespace_apply(&ordinary),
            Err(Error::DeletePending)
        );
        let published = w.publish(&p.node, Some(&p.fence));
        let observed = w.inspect(&file);
        assert_eq!(observed.fence, p.fence);
        assert_eq!(observed.mode, mode);
        assert_eq!(observed.node, published);
        assert_eq!(published.placement, file.placement);
        p = observed;
        w.reopen();
    }
    let reserved = w.reserve(&p.node, Some(&p.fence));
    w.state.namespace_apply(&reserved).unwrap();
    let cancel = w.cancel(&p);
    w.state.namespace_apply(&cancel).unwrap();
    let live = w.state.namespace_node(&w.volume, &file.id).unwrap();
    let successor = prepare(&mut w, &live);
    assert_ne!(successor.fence, p.fence);
    let stale = w.publication(&reserved.operation);
    assert_eq!(w.state.namespace_apply(&stale), Err(Error::Conflict));
    assert_eq!(w.inspect(&file), successor);
}
#[test]
fn mutable_mode_anchor_or_missing_evidence_never_grants_a_capability() {
    for corrupt in [
        "UPDATE namespace_removals SET mode='pending'",
        "UPDATE namespace_removals SET event_id='11111111111111111111111111111111'",
        "DELETE FROM namespace_removals",
        "DELETE FROM namespace_removal_events",
        "UPDATE namespace_removal_events SET mode='pending'",
    ] {
        let mut w = World::new();
        let n = w.file("corruption");
        let p = prepare(&mut w, &n);
        w.state.connection.execute_batch(corrupt).unwrap();
        assert_eq!(
            w.state.namespace_node(&w.volume, &n.id),
            Err(Error::Internal)
        );
        assert_eq!(
            w.state.namespace_pending_removal(&w.volume, &n.id),
            Err(Error::Internal)
        );
        let cancel = w.cancel(&p);
        assert_eq!(w.state.namespace_apply(&cancel), Err(Error::Internal));
    }
    let mut w = World::new();
    let n = w.file("rollback valid mode and anchor");
    let p = prepare(&mut w, &n);
    let promote = transition(&mut w, &p, RemovalMode::Pending);
    w.state.namespace_apply(&promote).unwrap();
    w.state
        .connection
        .execute(
            "UPDATE namespace_removals SET mode='deferred',event_id=?",
            [&p.fence.intent],
        )
        .unwrap();
    assert_eq!(
        w.state.namespace_node(&w.volume, &n.id),
        Err(Error::Internal)
    );
}
#[test]
fn ignored_event_or_mode_update_rolls_back_every_effect_and_result() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE INSERT ON namespace_removal_events BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_removals BEGIN SELECT RAISE(IGNORE); END",
    ] {
        let mut w = World::new();
        let n = w.file("atomic transition");
        let p = prepare(&mut w, &n);
        let before = w.state.namespace_volume().unwrap();
        let request = transition(&mut w, &p, RemovalMode::Pending);
        w.state.connection.execute_batch(trigger).unwrap();
        assert_eq!(w.state.namespace_apply(&request), Err(Error::Internal));
        assert_eq!(w.state.namespace_volume().unwrap(), before);
        assert_eq!(w.inspect(&n), p);
        assert_eq!(
            w.state.namespace_operation(&w.volume, &request.operation),
            Ok(None)
        );
        w.state
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        w.state.namespace_apply(&request).unwrap();
    }
}
#[test]
fn schema_fifteen_preserves_exact_old_pending_receipts_and_payloads() {
    for corrupt in [false, true] {
        let mut w = World::new();
        let n = w.file("legacy café 🦀");
        let p = w.arm(&n);
        let published = w.publish(&p.node, Some(&p.fence));
        let bytes: Vec<u8> = w
            .state
            .connection
            .query_row(
                "SELECT outcome FROM namespace_operations WHERE operation_id=?",
                [&p.fence.intent],
                |r| r.get(0),
            )
            .unwrap();
        w.state.connection.execute_batch("DROP TABLE windows_open_events; DROP TABLE retained_windows_opens; DROP TABLE windows_open_journal; DROP TABLE namespace_removal_events; ALTER TABLE namespace_removals DROP COLUMN mode; ALTER TABLE namespace_removals DROP COLUMN event_id; PRAGMA user_version=15").unwrap();
        if corrupt {
            w.state
                .connection
                .execute("UPDATE namespace_removals SET owner_id=?", [id(33)])
                .unwrap();
        }
        let path = w.tmp.path().join("authority");
        let placeholder = State::open_managed(&w.tmp.path().join("placeholder")).unwrap();
        drop(std::mem::replace(&mut w.state, placeholder));
        if corrupt {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                15
            );
            assert!(db.prepare("SELECT mode FROM namespace_removals").is_err());
            assert_eq!(node(&db, &n.id).unwrap(), published);
        } else {
            w.state = State::open_managed(&path).unwrap();
            let migrated = w.inspect(&n);
            assert_eq!(migrated.node, published);
            assert_eq!(migrated.mode, RemovalMode::Pending);
            assert_eq!(migrated.fence, p.fence);
            assert_eq!(
                w.state
                    .connection
                    .query_row::<Vec<u8>, _, _>(
                        "SELECT outcome FROM namespace_operations WHERE operation_id=?",
                        [&p.fence.intent],
                        |r| r.get(0)
                    )
                    .unwrap(),
                bytes
            );
            assert!(
                !serde_json::to_string(&migrated)
                    .unwrap()
                    .contains("\"mode\"")
            );
        }
    }
}

#[test]
fn deferred_headroom_allows_last_promotion_and_commit_but_never_ordinary_work() {
    let mut w = World::new();
    let n = w.file("quota");
    let p = prepare(&mut w, &n);
    let count: i64 = w
        .state
        .connection
        .query_row("SELECT count(*) FROM namespace_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    let tx = w.state.connection.transaction().unwrap();
    for i in count..MAX_OPERATIONS - 2 {
        tx.execute(
            "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
            [id(100_000 + i as u64)],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(
        w.create(&w.root(), "must preserve headroom", Kind::EmptyFile),
        Err(Error::Limit)
    );
    let mut wrong = transition(&mut w, &p, RemovalMode::Pending);
    if let Mutation::TransitionRemoval { fence, .. } = &mut wrong.mutation {
        fence.owner = id(99);
    }
    assert_eq!(w.state.namespace_apply(&wrong), Err(Error::Limit));
    let promote = transition(&mut w, &p, RemovalMode::Pending);
    w.state.namespace_apply(&promote).unwrap();
    let p = w.inspect(&n);
    let defer = transition(&mut w, &p, RemovalMode::Deferred);
    assert_eq!(w.state.namespace_apply(&defer), Err(Error::Limit));
    assert_eq!(w.inspect(&n), p);
    let commit = w.commit(&p);
    assert!(w.state.namespace_apply(&commit).unwrap().node.deleted);
}

#[test]
fn deferred_source_and_replacement_target_remain_fenced_in_legacy_and_guarded_routes() {
    let mut w = World::new();
    let n = w.file("protected");
    let source = w.file("source");
    let p = prepare(&mut w, &n);
    for mutation in [
        Mutation::Remove { node: n.id.clone() },
        Mutation::RemoveEntry {
            node: guard(&p.node),
            parent: guard(&w.root()),
        },
        Mutation::Move {
            node: n.id.clone(),
            parent: ROOT.into(),
            name: "renamed".into(),
        },
        Mutation::MoveEntry {
            node: guard(&p.node),
            source_parent: guard(&w.root()),
            parent: guard(&w.root()),
            name: "renamed".into(),
            destination: None,
        },
        Mutation::MoveEntry {
            node: guard(&source),
            source_parent: guard(&w.root()),
            parent: guard(&w.root()),
            name: n.name.clone(),
            destination: Some(guard(&p.node)),
        },
    ] {
        assert_eq!(w.apply(mutation), Err(Error::DeletePending));
        assert_eq!(w.inspect(&n), p);
    }
}
#[test]
fn actual_sqlite_full_transition_rolls_back_mode_evidence_sequence_and_result() {
    let mut w = World::new();
    let n = w.file("full disk mode");
    let p = prepare(&mut w, &n);
    let request = transition(&mut w, &p, RemovalMode::Pending);
    let before = w.state.namespace_volume().unwrap();
    w.state
        .connection
        .execute_batch("CREATE TABLE full_disk_filler(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
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
    w.state.connection.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON namespace_removal_events BEGIN INSERT INTO full_disk_filler VALUES(zeroblob(1048576)); END").unwrap();
    // The trigger allocates real SQLite pages inside the transition transaction.
    assert_eq!(w.state.namespace_apply(&request), Err(Error::Internal));
    assert_eq!(w.inspect(&n), p);
    assert_eq!(w.state.namespace_volume().unwrap(), before);
    assert_eq!(
        w.state.namespace_operation(&w.volume, &request.operation),
        Ok(None)
    );
    w.state
        .connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    w.state
        .connection
        .execute_batch("DROP TRIGGER fail")
        .unwrap();
    let expected = w.state.namespace_apply(&request).unwrap();
    w.reopen();
    assert_eq!(w.state.namespace_apply(&request), Ok(expected.clone()));
    assert_eq!(w.inspect(&n).node, expected.node);
}
