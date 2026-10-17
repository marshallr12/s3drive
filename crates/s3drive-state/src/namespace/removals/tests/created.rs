use super::*;
fn input(w: &mut World, name: &str, kind: Kind) -> Apply {
    w.request(Mutation::CreateDeferredEntry {
        parent: ROOT.into(),
        parent_revision: 0,
        store: (kind == Kind::EmptyFile).then(|| w.store.clone()),
        name: name.into(),
        node_kind: kind,
        owner: id(9000),
    })
}
#[test]
fn atomic_deferred_create_proves_origin_and_replays_without_reacquiring_expired_epoch() {
    for kind in [Kind::EmptyFile, Kind::Directory] {
        let mut w = World::new();
        let r = input(&mut w, "created café 日本語 🦀", kind);
        let receipt = w.state.namespace_apply(&r).unwrap();
        let p = w.inspect(&receipt.node);
        assert_eq!(p.mode, RemovalMode::Deferred);
        assert_eq!(p.fence.intent, r.operation);
        assert_eq!(p.node.id, r.operation);
        assert_eq!(receipt.artifact, Some(Artifact::Removal(p.fence.clone())));
        assert_eq!(
            w.state.namespace_node(&w.volume, &p.node.id),
            Ok(p.node.clone())
        );
        if kind == Kind::EmptyFile {
            let ordinary = w.reserve(&p.node, None);
            assert_eq!(
                w.state.namespace_apply(&ordinary),
                Err(Error::DeletePending)
            );
            w.publish(&p.node, Some(&p.fence));
        } else {
            w.create(&p.node, "child 🦀", Kind::EmptyFile).unwrap();
        }
        let current = w.inspect(&p.node);
        let cancel = w.cancel(&current);
        let restored = w.state.namespace_apply(&cancel).unwrap();
        let next = w.arm_request(&restored.node);
        if kind == Kind::EmptyFile {
            w.state.namespace_apply(&next).unwrap();
            let latest = w.inspect(&p.node);
            let stale = w.reserve(&latest.node, Some(&p.fence));
            assert_eq!(w.state.namespace_apply(&stale), Err(Error::Conflict));
        }
        w.reopen();
        assert_eq!(w.state.namespace_apply(&r), Ok(receipt));
        if kind == Kind::EmptyFile {
            assert_ne!(w.inspect(&p.node).fence, p.fence);
        } else {
            assert!(
                w.state
                    .namespace_pending_removal(&w.volume, &p.node.id)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                w.state
                    .namespace_list(&w.volume, &p.node.id)
                    .unwrap()
                    .children
                    .len(),
                1
            );
        }
    }
}
#[test]
fn ignored_create_effects_never_leave_an_unprotected_node_or_partial_receipt() {
    for (table, action) in [
        ("namespace_nodes", "INSERT"),
        ("namespace_removals", "INSERT"),
        ("namespace_removal_events", "INSERT"),
        ("namespace_volume", "UPDATE"),
        ("namespace_operations", "INSERT"),
    ] {
        let mut w = World::new();
        let r = input(&mut w, "atomic 🦀", Kind::EmptyFile);
        let before = w.state.namespace_volume().unwrap();
        w.state
            .connection
            .execute_batch(&format!(
                "CREATE TRIGGER fail BEFORE {action} ON {table} BEGIN SELECT RAISE(IGNORE); END"
            ))
            .unwrap();
        assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
        assert_eq!(w.state.namespace_volume().unwrap(), before);
        assert_eq!(
            w.state.namespace_node(&w.volume, &r.operation),
            Err(Error::NotFound)
        );
        assert_eq!(
            w.state.namespace_operation(&w.volume, &r.operation),
            Ok(None)
        );
        assert_eq!(
            w.state
                .connection
                .query_row::<i64, _, _>("SELECT count(*) FROM namespace_removals", [], |r| r.get(0))
                .unwrap(),
            0
        );
        assert_eq!(
            w.state
                .connection
                .query_row::<i64, _, _>("SELECT count(*) FROM namespace_removal_events", [], |r| r
                    .get(0))
                .unwrap(),
            0
        );
        w.state
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        assert!(w.state.namespace_apply(&r).is_ok());
    }
}
#[test]
fn actual_full_disk_rolls_back_created_node_reservation_event_and_sequence() {
    let mut w = World::new();
    let r = input(&mut w, "full create", Kind::EmptyFile);
    let before = w.state.namespace_volume().unwrap();
    w.state
        .connection
        .execute_batch("CREATE TABLE create_full(data BLOB); PRAGMA wal_checkpoint(TRUNCATE)")
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
    w.state.connection.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON namespace_removal_events BEGIN INSERT INTO create_full VALUES(zeroblob(1048576)); END").unwrap();
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
    assert_eq!(w.state.namespace_volume().unwrap(), before);
    assert_eq!(
        w.state.namespace_node(&w.volume, &r.operation),
        Err(Error::NotFound)
    );
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
    let expected = w.state.namespace_apply(&r).unwrap();
    w.reopen();
    assert_eq!(w.state.namespace_apply(&r), Ok(expected.clone()));
    assert_eq!(w.inspect(&expected.node).node, expected.node);
}
#[test]
fn created_origin_receipt_or_node_corruption_is_not_an_ownership_capability() {
    for receipt in [false, true] {
        let mut w = World::new();
        let r = input(&mut w, "original", Kind::EmptyFile);
        let mut expected = w.state.namespace_apply(&r).unwrap();
        if receipt {
            expected.node.name = "different".into();
            let result: Result<Receipt> = Ok(expected);
            w.state
                .connection
                .execute(
                    "UPDATE namespace_operations SET outcome=?1 WHERE operation_id=?2",
                    params![serde_json::to_vec(&result).unwrap(), r.operation],
                )
                .unwrap();
        } else {
            w.state.connection.execute_batch("UPDATE namespace_nodes SET name='different',name_key='different' WHERE parent_id IS NOT NULL").unwrap();
        }
        assert_eq!(
            w.state.namespace_node(&w.volume, &r.operation),
            Err(Error::Internal)
        );
    }
}
#[test]
fn invalid_create_guards_and_normalized_collision_do_not_create_reservations() {
    let mut w = World::new();
    let first = input(&mut w, "café", Kind::EmptyFile);
    w.state.namespace_apply(&first).unwrap();
    let duplicate = input(&mut w, "cafe\u{301}", Kind::EmptyFile);
    assert_eq!(
        w.state.namespace_apply(&duplicate),
        Err(Error::AlreadyExists)
    );
    for case in 0..4 {
        let mut r = input(&mut w, "different", Kind::EmptyFile);
        let Mutation::CreateDeferredEntry {
            parent_revision,
            store,
            owner,
            node_kind,
            ..
        } = &mut r.mutation
        else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                *parent_revision = 1;
                Error::Conflict
            }
            1 => {
                *store = Some(id(888));
                Error::NotFound
            }
            2 => {
                *owner = ROOT.into();
                Error::Invalid
            }
            _ => {
                *node_kind = Kind::Directory;
                Error::Invalid
            }
        };
        assert_eq!(w.state.namespace_apply(&r), Err(expected));
        assert_eq!(
            w.state.namespace_node(&w.volume, &r.operation),
            Err(Error::NotFound)
        );
    }
    assert_eq!(
        w.state
            .connection
            .query_row::<i64, _, _>("SELECT count(*) FROM namespace_removals", [], |r| r.get(0))
            .unwrap(),
        1
    );
}
#[test]
fn create_capacity_reserves_promotion_and_commit_without_leaving_partial_nodes() {
    let mut w = World::new();
    let r = input(&mut w, "quota create", Kind::EmptyFile);
    let count: i64 = w
        .state
        .connection
        .query_row("SELECT count(*) FROM namespace_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    let tx = w.state.connection.transaction().unwrap();
    for op in count..MAX_OPERATIONS - 2 {
        tx.execute(
            "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
            [id(100_000 + op as u64)],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(w.state.namespace_apply(&r), Err(Error::Limit));
    assert_eq!(
        w.state.namespace_node(&w.volume, &r.operation),
        Err(Error::NotFound)
    );
    w.state
        .connection
        .execute(
            "DELETE FROM namespace_operations WHERE operation_id=?",
            [id(100_000 + count as u64)],
        )
        .unwrap();
    let receipt = w.state.namespace_apply(&r).unwrap();
    let p = w.inspect(&receipt.node);
    let promote = w.request(Mutation::TransitionRemoval {
        node: guard(&p.node),
        parent: guard(&w.root()),
        fence: p.fence,
        expected: RemovalMode::Deferred,
        mode: RemovalMode::Pending,
    });
    w.state.namespace_apply(&promote).unwrap();
    let commit = w.commit(&w.inspect(&receipt.node));
    assert!(w.state.namespace_apply(&commit).unwrap().node.deleted);
}

#[test]
fn killed_authority_owner_recovers_exact_created_node_and_reservation() {
    let mut w = World::new();
    let request = input(&mut w, "killed create 資料 🦀", Kind::EmptyFile);
    let original = killed_apply(&mut w, &request);
    let before = w.state.namespace_volume().unwrap();
    assert_eq!(w.state.namespace_apply(&request), Ok(original.clone()));
    assert_eq!(w.state.namespace_volume().unwrap(), before);
    let p = w.inspect(&original.node);
    assert_eq!(p.fence.intent, request.operation);
    assert_eq!(p.mode, RemovalMode::Deferred);
    assert_eq!(p.node, original.node);
}

pub(super) fn killed_apply(w: &mut World, request: &Apply) -> Receipt {
    use std::{
        io::BufRead,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            self.0.stdin.take();
        }
    }
    let path = w.tmp.path().join("authority");
    let request_path = w.tmp.path().join("create.json");
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    let placeholder = State::open_managed(&w.tmp.path().join("placeholder")).unwrap();
    drop(std::mem::replace(&mut w.state, placeholder));
    let mut child = Child(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "namespace::removals::tests::removal_owner_child",
                "--nocapture",
            ])
            .env("S3DRIVE_REMOVAL_CHILD", &path)
            .env("S3DRIVE_REMOVAL_INPUT", &request_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let output = child.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            if line.unwrap() == "removal-committed" {
                let _ = send.send(());
                return;
            }
        }
    });
    let signaled = receive.recv_timeout(Duration::from_secs(10));
    child.0.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.0.try_wait().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "created owner termination timed out"
        );
        std::thread::yield_now();
    }
    reader.join().unwrap();
    signaled.unwrap();
    w.state = State::open_managed(&path).unwrap();
    w.state
        .namespace_operation(&w.volume, &request.operation)
        .unwrap()
        .unwrap()
        .unwrap()
}
