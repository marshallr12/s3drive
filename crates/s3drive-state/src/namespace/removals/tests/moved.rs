use super::*;
fn prepared(w: &mut World, kind: Kind, atomic: bool) -> (Apply, PendingRemoval) {
    let r = if atomic {
        w.request(Mutation::CreateDeferredEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: (kind == Kind::EmptyFile).then(|| w.store.clone()),
            name: "original café 日本語 🦀".into(),
            node_kind: kind,
            owner: id(9000),
        })
    } else {
        let n = w
            .create(&w.root(), "original café 日本語 🦀", kind)
            .unwrap()
            .node;
        w.request(Mutation::PrepareRemoval {
            node: guard(&n),
            parent: guard(&w.root()),
            owner: id(9000),
        })
    };
    let receipt = w.state.namespace_apply(&r).unwrap();
    let p = w.inspect(&receipt.node);
    (r, p)
}
fn movement(
    w: &mut World,
    p: &PendingRemoval,
    previous: &str,
    parent: &Node,
    name: &str,
    target: Option<&Node>,
) -> Apply {
    let source = w
        .state
        .namespace_node(&w.volume, p.node.parent.as_ref().unwrap())
        .unwrap();
    w.request(Mutation::MoveDeferredEntry {
        node: guard(&p.node),
        source_parent: guard(&source),
        parent: guard(parent),
        name: name.into(),
        destination: target.map(guard),
        fence: Box::new(p.fence.clone()),
        previous: previous.into(),
    })
}
#[test]
fn owner_move_retains_origin_and_fence_across_unicode_parents_publication_and_cleanup() {
    for kind in [Kind::EmptyFile, Kind::Directory] {
        for atomic in [false, true] {
            let mut w = World::new();
            let parent = w
                .create(&w.root(), "destination 資料", Kind::Directory)
                .unwrap()
                .node;
            let (origin, mut p) = prepared(&mut w, kind, atomic);
            let initial = w
                .state
                .namespace_operation(&w.volume, &origin.operation)
                .unwrap()
                .unwrap()
                .unwrap();
            let child = if kind == Kind::Directory {
                Some(
                    w.create(&p.node, "retained child 🦀", Kind::EmptyFile)
                        .unwrap()
                        .node,
                )
            } else {
                w.publish(&p.node, Some(&p.fence));
                p = w.inspect(&p.node);
                None
            };
            let r = movement(
                &mut w,
                &p,
                &origin.operation,
                &parent,
                "renamed café 🦀",
                None,
            );
            let moved = w.state.namespace_apply(&r).unwrap();
            assert_eq!(moved.node.parent.as_ref(), Some(&parent.id));
            assert_eq!(moved.node.payload, p.node.payload);
            assert_eq!(w.inspect(&p.node).fence, p.fence);
            assert!(
                w.state
                    .namespace_list(&w.volume, ROOT)
                    .unwrap()
                    .children
                    .iter()
                    .all(|n| n.id != p.node.id)
            );
            if kind == Kind::EmptyFile {
                w.publish(&moved.node, Some(&p.fence));
            }
            let current = w.inspect(&p.node);
            let case = movement(
                &mut w,
                &current,
                &r.operation,
                &parent,
                "RENAMED café 🦀",
                Some(&current.node),
            );
            let upper = w.state.namespace_apply(&case).unwrap();
            w.reopen();
            assert_eq!(w.state.namespace_apply(&origin), Ok(initial));
            assert_eq!(w.state.namespace_apply(&r), Ok(moved));
            assert_eq!(w.inspect(&p.node).node, upper.node);
            if let Some(child) = child {
                assert_eq!(
                    w.state
                        .namespace_list(&w.volume, &p.node.id)
                        .unwrap()
                        .children,
                    vec![child]
                );
                let cancel = w.cancel(&w.inspect(&p.node));
                let kept = w.state.namespace_apply(&cancel).unwrap();
                assert_eq!(kept.node.name, upper.node.name);
            } else {
                let promote = w.request(Mutation::TransitionRemoval {
                    node: guard(&upper.node),
                    parent: guard(&parent),
                    fence: p.fence.clone(),
                    expected: RemovalMode::Deferred,
                    mode: RemovalMode::Pending,
                });
                let pending = w.state.namespace_apply(&promote).unwrap();
                let commit = w.request(Mutation::CommitRemoval {
                    node: guard(&pending.node),
                    parent: guard(&parent),
                    fence: p.fence.clone(),
                });
                assert!(w.state.namespace_apply(&commit).unwrap().node.deleted);
                let fresh = w
                    .create(&parent, &upper.node.name, Kind::EmptyFile)
                    .unwrap()
                    .node;
                w.state.namespace_apply(&case).unwrap();
                w.state.namespace_apply(&commit).unwrap();
                assert_eq!(w.state.namespace_node(&w.volume, &fresh.id), Ok(fresh));
            }
        }
    }
}
#[test]
fn owner_move_replaces_only_guarded_clear_target_and_retains_immutable_bytes() {
    let mut w = World::new();
    let target = w.file("target 🦀");
    let target = w.publish(&target, None);
    let (origin, p) = prepared(&mut w, Kind::EmptyFile, true);
    let root = w.root();
    let request = movement(
        &mut w,
        &p,
        &origin.operation,
        &root,
        &target.name,
        Some(&target),
    );
    let result = w.state.namespace_apply(&request).unwrap();
    let Some(Artifact::RemovalMove {
        fence,
        displaced: Some(old),
    }) = result.artifact
    else {
        panic!()
    };
    assert_eq!(fence, p.fence);
    assert!(old.deleted);
    assert_eq!(old.payload, target.payload);
    assert_eq!(old.placement, target.placement);
    assert_eq!(w.state.namespace_node(&w.volume, &target.id), Ok(old));
    assert_eq!(w.inspect(&p.node).node, result.node);
}
#[test]
fn owner_move_refuses_foreign_or_stale_fences_pending_sources_reserved_targets_and_cycles() {
    let mut w = World::new();
    let (origin, p) = prepared(&mut w, Kind::EmptyFile, true);
    let root = w.root();
    let template = movement(&mut w, &p, &origin.operation, &root, "new", None);
    for case in 0..4 {
        let mut r = template.clone();
        r.operation = id(1000 + case);
        let Mutation::MoveDeferredEntry {
            fence,
            previous,
            node,
            source_parent,
            ..
        } = &mut r.mutation
        else {
            unreachable!()
        };
        match case {
            0 => fence.owner = id(999),
            1 => *previous = id(888).into(),
            2 => node.revision -= 1,
            _ => source_parent.revision += 1,
        }
        assert_eq!(w.state.namespace_apply(&r), Err(Error::Conflict));
        assert_eq!(w.inspect(&p.node), p);
    }
    let other = w.file("reserved");
    let other = w.arm(&other);
    let r = movement(
        &mut w,
        &p,
        &origin.operation,
        &root,
        "reserved",
        Some(&other.node),
    );
    assert_eq!(w.state.namespace_apply(&r), Err(Error::DeletePending));
    let promote = w.request(Mutation::TransitionRemoval {
        node: guard(&p.node),
        parent: guard(&root),
        fence: p.fence.clone(),
        expected: RemovalMode::Deferred,
        mode: RemovalMode::Pending,
    });
    w.state.namespace_apply(&promote).unwrap();
    let pending = w.inspect(&p.node);
    let r = movement(
        &mut w,
        &pending,
        &promote.operation,
        &root,
        "pending move",
        None,
    );
    assert_eq!(w.state.namespace_apply(&r), Err(Error::DeletePending));
    let current = w.inspect(&p.node);
    let defer = w.request(Mutation::TransitionRemoval {
        node: guard(&current.node),
        parent: guard(&root),
        fence: p.fence.clone(),
        expected: RemovalMode::Pending,
        mode: RemovalMode::Deferred,
    });
    w.state.namespace_apply(&defer).unwrap();
    let current = w.inspect(&p.node);
    let stale = movement(
        &mut w,
        &current,
        &origin.operation,
        &root,
        "stale event",
        None,
    );
    assert_eq!(w.state.namespace_apply(&stale), Err(Error::Conflict));
    let fresh = movement(
        &mut w,
        &current,
        &defer.operation,
        &root,
        "fresh event",
        None,
    );
    w.state.namespace_apply(&fresh).unwrap();
    let (dir_origin, dir) = {
        let n = w.create(&root, "directory", Kind::Directory).unwrap().node;
        let r = w.request(Mutation::PrepareRemoval {
            node: guard(&n),
            parent: guard(&root),
            owner: id(9000),
        });
        w.state.namespace_apply(&r).unwrap();
        (r, w.inspect(&n))
    };
    let child = w.create(&dir.node, "child", Kind::Directory).unwrap().node;
    let r = movement(&mut w, &dir, &dir_origin.operation, &child, "cycle", None);
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Cycle));
}
#[test]
fn ignored_owner_move_effects_roll_back_both_nodes_event_pointer_sequence_and_receipt() {
    for (table, action) in [
        ("namespace_nodes", "UPDATE"),
        ("namespace_removals", "UPDATE"),
        ("namespace_removal_events", "INSERT"),
        ("namespace_volume", "UPDATE"),
        ("namespace_operations", "INSERT"),
    ] {
        let mut w = World::new();
        let target = w.file("target");
        let (origin, p) = prepared(&mut w, Kind::EmptyFile, true);
        let root = w.root();
        let r = movement(
            &mut w,
            &p,
            &origin.operation,
            &root,
            &target.name,
            Some(&target),
        );
        let volume = w.state.namespace_volume().unwrap();
        w.state
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER fail BEFORE {action} ON {table} BEGIN SELECT RAISE(IGNORE); END"
            ))
            .unwrap();
        assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
        assert_eq!(w.state.namespace_volume().unwrap(), volume);
        assert_eq!(w.inspect(&p.node), p);
        assert_eq!(w.state.namespace_node(&w.volume, &target.id), Ok(target));
        assert_eq!(
            w.state.namespace_operation(&w.volume, &r.operation),
            Ok(None)
        );
        w.state
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        let result = w.state.namespace_apply(&r).unwrap();
        w.reopen();
        assert_eq!(w.state.namespace_apply(&r), Ok(result));
    }
}
#[test]
fn actual_full_disk_during_owner_move_leaves_source_and_replacement_unchanged() {
    let mut w = World::new();
    let target = w.file("target");
    let (origin, p) = prepared(&mut w, Kind::EmptyFile, true);
    let root = w.root();
    let r = movement(
        &mut w,
        &p,
        &origin.operation,
        &root,
        &target.name,
        Some(&target),
    );
    let volume = w.state.namespace_volume().unwrap();
    w.state
        .connection
        .execute_batch("CREATE TABLE move_full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
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
    w.state.connection.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON namespace_removal_events BEGIN INSERT INTO move_full VALUES(zeroblob(1048576)); END").unwrap();
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
    assert_eq!(w.state.namespace_volume().unwrap(), volume);
    assert_eq!(w.inspect(&p.node), p);
    assert_eq!(w.state.namespace_node(&w.volume, &target.id), Ok(target));
    assert_eq!(
        w.state.namespace_operation(&w.volume, &r.operation),
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
    let receipt = w.state.namespace_apply(&r).unwrap();
    w.reopen();
    assert_eq!(w.inspect(&p.node).node, receipt.node);
}
#[test]
fn corrupted_move_lineage_cannot_authorize_cancellation_or_publication() {
    for case in 0..5 {
        let mut w = World::new();
        let (origin, p) = prepared(&mut w, Kind::EmptyFile, true);
        let root = w.root();
        let mut r = movement(&mut w, &p, &origin.operation, &root, "moved", None);
        let mut receipt = w.state.namespace_apply(&r).unwrap();
        match case {
            0 => {
                let Mutation::MoveDeferredEntry { previous, .. } = &mut r.mutation else {
                    unreachable!()
                };
                *previous = r.operation.clone().into();
                w.state
                    .connection
                    .execute(
                        "UPDATE namespace_operations SET request=?1 WHERE operation_id=?2",
                        params![serde_json::to_vec(&r).unwrap(), r.operation],
                    )
                    .unwrap();
            }
            1 => {
                receipt.node.name = "forged".into();
                let outcome: Result<Receipt> = Ok(receipt);
                w.state
                    .connection
                    .execute(
                        "UPDATE namespace_operations SET outcome=?1 WHERE operation_id=?2",
                        params![serde_json::to_vec(&outcome).unwrap(), r.operation],
                    )
                    .unwrap();
            }
            2 => {
                w.state
                    .connection
                    .execute(
                        "DELETE FROM namespace_removal_events WHERE operation_id=?",
                        [&origin.operation],
                    )
                    .unwrap();
            }
            3 => {
                w.state
                    .connection
                    .execute(
                        "UPDATE namespace_removal_events SET owner_id=?1 WHERE operation_id=?2",
                        params![id(888), r.operation],
                    )
                    .unwrap();
            }
            _ => {
                w.state.connection.execute("UPDATE namespace_nodes SET name='forged',name_key='forged' WHERE node_id=?",[&p.node.id]).unwrap();
            }
        }
        assert_eq!(
            w.state.namespace_pending_removal(&w.volume, &p.node.id),
            Err(Error::Internal)
        );
        let cancel = w.cancel(&p);
        assert_eq!(w.state.namespace_apply(&cancel), Err(Error::Internal));
    }
}
#[test]
fn owner_move_cannot_spend_reserved_authority_closing_capacity() {
    let mut w = World::new();
    let (origin, p) = prepared(&mut w, Kind::EmptyFile, true);
    let root = w.root();
    let r = movement(&mut w, &p, &origin.operation, &root, "quota moved", None);
    let count: i64 = w
        .state
        .connection
        .query_row("SELECT count(*) FROM namespace_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    let count = MAX_OPERATIONS - count - 2;
    let tx = w.state.connection.transaction().unwrap();
    for op in 0..count {
        tx.execute(
            "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
            [id(100_000 + op as u64)],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Limit));
    assert_eq!(w.inspect(&p.node), p);
    assert_eq!(
        w.state.namespace_operation(&w.volume, &r.operation),
        Ok(None)
    );
    w.state
        .connection
        .execute(
            "DELETE FROM namespace_operations WHERE operation_id=?",
            [id(100_000)],
        )
        .unwrap();
    let moved = w.state.namespace_apply(&r).unwrap();
    let promote = w.request(Mutation::TransitionRemoval {
        node: guard(&moved.node),
        parent: guard(&root),
        fence: p.fence.clone(),
        expected: RemovalMode::Deferred,
        mode: RemovalMode::Pending,
    });
    let pending = w.state.namespace_apply(&promote).unwrap();
    let commit = w.commit(&w.inspect(&pending.node));
    assert!(w.state.namespace_apply(&commit).unwrap().node.deleted);
}
#[test]
fn killed_authority_move_recovers_exact_destination_fence_and_displaced_target() {
    let mut w = World::new();
    let target = w.file("target 🦀");
    let (origin, p) = prepared(&mut w, Kind::EmptyFile, true);
    let root = w.root();
    let r = movement(
        &mut w,
        &p,
        &origin.operation,
        &root,
        &target.name,
        Some(&target),
    );
    let receipt = created::killed_apply(&mut w, &r);
    assert_eq!(w.state.namespace_apply(&r), Ok(receipt.clone()));
    assert_eq!(w.inspect(&p.node).node, receipt.node);
    assert_eq!(w.inspect(&p.node).fence, p.fence);
    assert!(
        w.state
            .namespace_node(&w.volume, &target.id)
            .unwrap()
            .deleted
    );
    w.state.namespace_apply(&origin).unwrap();
    assert_eq!(w.inspect(&p.node).node, receipt.node);
}
