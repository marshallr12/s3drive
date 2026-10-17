use super::*;
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
struct World {
    state: State,
    tmp: tempfile::TempDir,
    volume: String,
    store: String,
    next: u64,
}
impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = State::open_managed(&tmp.path().join("authority")).unwrap();
        let volume = state
            .namespace_initialize_with_policy(names::PORTABLE)
            .unwrap()
            .id;
        let mut w = Self {
            state,
            tmp,
            volume,
            store: String::new(),
            next: 0,
        };
        let r = w
            .apply(Mutation::RegisterStore {
                route: "a".repeat(64),
                root: "internal/".into(),
            })
            .unwrap();
        let Some(Artifact::Store(store)) = r.artifact else {
            panic!()
        };
        w.store = store.id;
        w
    }
    fn request(&mut self, mutation: Mutation) -> Apply {
        self.next += 1;
        Apply {
            volume: self.volume.clone(),
            operation: id(self.next),
            expected_sequence: self.state.namespace_volume().unwrap().sequence,
            mutation,
        }
    }
    fn apply(&mut self, m: Mutation) -> Result<Receipt> {
        let r = self.request(m);
        self.state.namespace_apply(&r)
    }
    fn root(&self) -> Node {
        self.state.namespace_node(&self.volume, ROOT).unwrap()
    }
    fn file(&mut self, name: &str) -> Node {
        self.create(&self.root(), name, Kind::EmptyFile)
            .unwrap()
            .node
    }
    fn create(&mut self, p: &Node, name: &str, kind: Kind) -> Result<Receipt> {
        self.apply(Mutation::CreateEntry {
            parent: p.id.clone(),
            parent_revision: p.revision,
            store: (kind == Kind::EmptyFile).then(|| self.store.clone()),
            name: name.into(),
            node_kind: kind,
        })
    }
    fn arm_request(&mut self, n: &Node) -> Apply {
        self.request(Mutation::ArmRemoval {
            node: guard(n),
            parent: guard(&self.root()),
            owner: id(9000),
        })
    }
    fn arm(&mut self, n: &Node) -> PendingRemoval {
        let r = self.arm_request(n);
        let receipt = self.state.namespace_apply(&r).unwrap();
        let Some(Artifact::Removal(fence)) = receipt.artifact else {
            panic!()
        };
        PendingRemoval {
            node: receipt.node,
            fence,
        }
    }
    fn cancel(&mut self, p: &PendingRemoval) -> Apply {
        self.request(Mutation::CancelRemoval {
            node: p.node.id.clone(),
            fence: p.fence.clone(),
        })
    }
    fn commit(&mut self, p: &PendingRemoval) -> Apply {
        self.request(Mutation::CommitRemoval {
            node: guard(&p.node),
            parent: guard(&self.root()),
            fence: p.fence.clone(),
        })
    }
    fn inspect(&self, n: &Node) -> PendingRemoval {
        self.state
            .namespace_pending_removal(&self.volume, &n.id)
            .unwrap()
            .unwrap()
    }
    fn reserve(&mut self, n: &Node, fence: Option<&RemovalFence>) -> Apply {
        let m = match fence {
            Some(fence) => Mutation::ReservePendingContent {
                node: n.id.clone(),
                expected_revision: n.revision,
                store: self.store.clone(),
                size: 1,
                sha256: "b".repeat(64),
                expected_payload: payload_reference(&n.payload),
                fence: fence.clone(),
            },
            None => Mutation::ReserveContent {
                node: n.id.clone(),
                expected_revision: n.revision,
                store: self.store.clone(),
                size: 1,
                sha256: "b".repeat(64),
                expected_payload: payload_reference(&n.payload),
            },
        };
        self.request(m)
    }
    fn publication(&mut self, reservation: &str) -> Apply {
        self.request(Mutation::Publish {
            reservation: reservation.into(),
            upload_operation: "c".repeat(64),
            etag: "\"verified-fixture\"".into(),
            version: None,
        })
    }
    fn publish(&mut self, n: &Node, fence: Option<&RemovalFence>) -> Node {
        let reserve = self.reserve(n, fence);
        self.state.namespace_apply(&reserve).unwrap();
        let publish = self.publication(&reserve.operation);
        self.state.namespace_apply(&publish).unwrap().node
    }
    fn reopen(&mut self) {
        let placeholder = State::open_managed(&self.tmp.path().join("placeholder")).unwrap();
        drop(std::mem::replace(&mut self.state, placeholder));
        self.state = State::open_managed(&self.tmp.path().join("authority")).unwrap();
    }
}
#[test]
fn disposition_is_reversible_and_retains_exact_payload_and_store() {
    let mut w = World::new();
    let n = w.file("café 日本語 🦀");
    let n = w.publish(&n, None);
    let request = w.arm_request(&n);
    let armed = w.state.namespace_apply(&request).unwrap();
    let p = w.inspect(&n);
    assert_eq!(p.node, armed.node);
    assert_eq!(p.fence.intent, request.operation);
    assert_eq!(p.node.payload, n.payload);
    assert_eq!(p.node.placement, n.placement);
    assert_eq!(p.node.name, n.name);
    assert!(!p.node.deleted);
    assert_eq!(
        w.state.namespace_node(&w.volume, &n.id),
        Err(Error::DeletePending)
    );
    assert_eq!(
        w.state.namespace_list(&w.volume, ROOT).unwrap().children,
        vec![p.node.clone()]
    );
    let duplicate = w.create(&w.root(), &n.name, Kind::EmptyFile);
    assert_eq!(duplicate, Err(Error::AlreadyExists));
    w.reopen();
    assert_eq!(w.state.namespace_apply(&request), Ok(armed.clone()));
    let cancel = w.cancel(&p);
    let receipt = w.state.namespace_apply(&cancel).unwrap();
    assert!(!receipt.node.deleted);
    assert_eq!(receipt.node.payload, n.payload);
    assert_eq!(receipt.node.placement, n.placement);
    assert_eq!(
        w.state.namespace_pending_removal(&w.volume, &n.id),
        Ok(None)
    );
    let next = w.arm(&receipt.node);
    assert_ne!(next.fence, p.fence);
    // Old successful arm/cancel replays cannot change the current reservation.
    assert_eq!(w.state.namespace_apply(&request), Ok(armed));
    assert_eq!(w.state.namespace_apply(&cancel), Ok(receipt));
    assert_eq!(w.inspect(&n), next);
    let mut stale = w.cancel(&p);
    assert_eq!(w.state.namespace_apply(&stale), Err(Error::Conflict));
    stale.operation = id(8100);
    stale.mutation = Mutation::CancelRemoval {
        node: n.id.clone(),
        fence: RemovalFence {
            intent: next.fence.intent,
            owner: id(99),
        },
    };
    assert_eq!(w.state.namespace_apply(&stale), Err(Error::Conflict));
    let changed = Apply {
        mutation: Mutation::CancelRemoval {
            node: n.id.clone(),
            fence: p.fence,
        },
        ..request
    };
    assert_eq!(
        w.state.namespace_apply(&changed),
        Err(Error::OperationMismatch)
    );
}
#[test]
fn pending_nodes_block_legacy_and_guarded_namespace_bypasses() {
    let mut w = World::new();
    let n = w.file("reserved");
    let other = w.file("other");
    let p = w.arm(&n);
    let root = w.root();
    let attempts = [
        Mutation::Move {
            node: n.id.clone(),
            parent: ROOT.into(),
            name: "bypass".into(),
        },
        Mutation::Remove { node: n.id.clone() },
        Mutation::RemoveEntry {
            node: guard(&p.node),
            parent: guard(&root),
        },
        Mutation::MoveEntry {
            node: guard(&p.node),
            source_parent: guard(&root),
            parent: guard(&root),
            name: "bypass".into(),
            destination: None,
        },
        Mutation::MoveEntry {
            node: guard(&other),
            source_parent: guard(&root),
            parent: guard(&root),
            name: n.name.clone(),
            destination: Some(guard(&p.node)),
        },
        Mutation::ArmRemoval {
            node: guard(&p.node),
            parent: guard(&root),
            owner: id(9001),
        },
    ];
    for m in attempts {
        assert_eq!(w.apply(m), Err(Error::DeletePending));
        assert_eq!(w.inspect(&n), p);
    }
    let dir = w.create(&root, "empty 🦀", Kind::Directory).unwrap().node;
    let reserved_dir = w.arm(&dir);
    assert_eq!(
        w.create(&reserved_dir.node, "child", Kind::EmptyFile),
        Err(Error::DeletePending)
    );
    assert_eq!(
        w.apply(Mutation::Create {
            parent: dir.id.clone(),
            name: "legacy child".into(),
            node_kind: Kind::Directory
        }),
        Err(Error::DeletePending)
    );
    assert_eq!(
        w.state.namespace_list(&w.volume, &dir.id),
        Err(Error::DeletePending)
    );
    assert_eq!(
        w.apply(Mutation::MoveEntry {
            node: guard(&other),
            source_parent: guard(&root),
            parent: guard(&reserved_dir.node),
            name: "child".into(),
            destination: None
        }),
        Err(Error::DeletePending)
    );
    let nonempty = w.create(&root, "nonempty", Kind::Directory).unwrap().node;
    w.create(&nonempty, "child", Kind::EmptyFile).unwrap();
    let arm = w.arm_request(&nonempty);
    assert_eq!(w.state.namespace_apply(&arm), Err(Error::NotEmpty));
    let arm = w.arm_request(&root);
    assert_eq!(w.state.namespace_apply(&arm), Err(Error::Root));
}
#[test]
fn publication_requires_the_live_fence_and_complete_content_guard() {
    let mut w = World::new();
    let n = w.file("shared");
    let old_reserve = w.reserve(&n, None);
    w.state.namespace_apply(&old_reserve).unwrap();
    let p = w.arm(&n);
    let old_publish = w.publication(&old_reserve.operation);
    assert_eq!(
        w.state.namespace_apply(&old_publish),
        Err(Error::DeletePending)
    );
    let ordinary = w.reserve(&p.node, None);
    assert_eq!(
        w.state.namespace_apply(&ordinary),
        Err(Error::DeletePending)
    );
    let wrong = RemovalFence {
        owner: id(123),
        ..p.fence.clone()
    };
    let request = w.reserve(&p.node, Some(&wrong));
    assert_eq!(w.state.namespace_apply(&request), Err(Error::Conflict));
    let published = w.publish(&p.node, Some(&p.fence));
    let stale = w.commit(&p);
    assert_eq!(w.state.namespace_apply(&stale), Err(Error::Conflict));
    let fresh = w.inspect(&n);
    assert_eq!(fresh.node, published);
    let pending_reserve = w.reserve(&published, Some(&p.fence));
    w.state.namespace_apply(&pending_reserve).unwrap();
    let cancel = w.cancel(&fresh);
    let cancelled = w.state.namespace_apply(&cancel).unwrap();
    assert_eq!(cancelled.node.payload, published.payload);
    let late = w.publication(&pending_reserve.operation);
    assert_eq!(w.state.namespace_apply(&late), Err(Error::Conflict));
    let next = w.arm(&cancelled.node);
    let late = w.publication(&pending_reserve.operation);
    assert_eq!(w.state.namespace_apply(&late), Err(Error::Conflict));
    // A matching owner cannot bypass complete prior-payload comparison.
    let mut stale_payload = w.reserve(&next.node, Some(&next.fence));
    if let Mutation::ReservePendingContent {
        expected_payload, ..
    } = &mut stale_payload.mutation
    {
        *expected_payload = None;
    }
    assert_eq!(
        w.state.namespace_apply(&stale_payload),
        Err(Error::Conflict)
    );
    let rm = w.commit(&next);
    let deleted = w.state.namespace_apply(&rm).unwrap();
    assert!(deleted.node.deleted);
    assert_eq!(deleted.node.payload, published.payload);
    assert_eq!(deleted.node.placement, n.placement);
    let reused = w.file("shared");
    assert_ne!(reused.id, n.id);
    assert_eq!(w.state.namespace_apply(&rm), Ok(deleted.clone()));
    assert!(
        !w.state
            .namespace_node(&w.volume, &reused.id)
            .unwrap()
            .deleted
    );
    // Retained handles can subsequently publish only through explicit detached CAS.
    let reserve = w.request(Mutation::ReserveDetachedContent {
        node: n.id.clone(),
        expected_revision: deleted.node.revision,
        store: w.store.clone(),
        size: 1,
        sha256: "d".repeat(64),
        expected_payload: payload_reference(&deleted.node.payload),
    });
    w.state.namespace_apply(&reserve).unwrap();
    let publish = w.publication(&reserve.operation);
    assert!(w.state.namespace_apply(&publish).unwrap().node.deleted);
    assert!(
        w.state
            .namespace_node(&w.volume, &reused.id)
            .unwrap()
            .payload
            .is_none()
    );
}
#[test]
fn every_ignored_transition_rolls_back_reservation_node_sequence_and_result() {
    for trigger in [
        "CREATE TRIGGER fail BEFORE INSERT ON namespace_removals BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_nodes BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE UPDATE ON namespace_volume BEGIN SELECT RAISE(IGNORE); END;",
        "CREATE TRIGGER fail BEFORE INSERT ON namespace_operations BEGIN SELECT RAISE(IGNORE); END;",
    ] {
        let mut w = World::new();
        let n = w.file("rollback arm");
        let v = w.state.namespace_volume().unwrap();
        let r = w.arm_request(&n);
        w.state.connection.execute_batch(trigger).unwrap();
        assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
        assert_eq!(w.state.namespace_volume().unwrap(), v);
        assert_eq!(
            w.state.namespace_operation(&w.volume, &r.operation),
            Ok(None)
        );
        assert_eq!(w.state.namespace_node(&w.volume, &n.id), Ok(n.clone()));
        assert_eq!(
            w.state.namespace_pending_removal(&w.volume, &n.id),
            Ok(None)
        );
        w.state
            .connection
            .execute_batch("DROP TRIGGER fail")
            .unwrap();
        assert!(w.state.namespace_apply(&r).is_ok());
    }
    for cancel in [true, false] {
        for table in [
            "namespace_removals",
            "namespace_nodes",
            "namespace_operations",
            "namespace_volume",
        ] {
            let mut w = World::new();
            let n = w.file("rollback completion");
            let p = w.arm(&n);
            let v = w.state.namespace_volume().unwrap();
            let r = if cancel { w.cancel(&p) } else { w.commit(&p) };
            let action = match table {
                "namespace_removals" => "DELETE",
                "namespace_operations" => "INSERT",
                _ => "UPDATE",
            };
            w.state.connection.execute_batch(&format!("CREATE TRIGGER fail BEFORE {action} ON {table} BEGIN SELECT RAISE(IGNORE); END;")).unwrap();
            assert_eq!(w.state.namespace_apply(&r), Err(Error::Internal));
            assert_eq!(w.state.namespace_volume().unwrap(), v);
            assert_eq!(
                w.state.namespace_operation(&w.volume, &r.operation),
                Ok(None)
            );
            assert_eq!(w.inspect(&n), p);
            w.state
                .connection
                .execute_batch("DROP TRIGGER fail")
                .unwrap();
            assert!(w.state.namespace_apply(&r).is_ok());
        }
    }
}
#[test]
fn corrupted_valid_looking_reservations_are_not_capabilities() {
    for change in ["owner_id", "intent_id"] {
        let mut w = World::new();
        let n = w.file("corrupt");
        let p = w.arm(&n);
        w.state
            .connection
            .execute(
                &format!("UPDATE namespace_removals SET {change}=?"),
                [id(3333)],
            )
            .unwrap();
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
        assert_eq!(
            w.state.namespace_operation(&w.volume, &cancel.operation),
            Ok(None)
        );
    }
}
#[test]
fn schema_twelve_migration_preserves_old_serialized_nodes_and_guards() {
    let mut w = World::new();
    let n = w.file("migration 🦀");
    let n = w.publish(&n, None);
    let serialized = serde_json::to_vec(&n).unwrap();
    let volume = w.state.namespace_volume().unwrap();
    w.state
        .connection
        .execute_batch("DROP TABLE retained_client_identity; DROP TABLE client_identity; DROP TABLE namespace_removals; PRAGMA user_version=12")
        .unwrap();
    w.reopen();
    assert_eq!(
        w.state.info().unwrap().schema_version,
        crate::SCHEMA_VERSION
    );
    assert_eq!(w.state.namespace_volume().unwrap(), volume);
    assert_eq!(
        serde_json::to_vec(&w.state.namespace_node(&w.volume, &n.id).unwrap()).unwrap(),
        serialized
    );
    assert_eq!(
        serde_json::to_string(&ContentGuard {
            expected: None,
            detached: false,
            removal: None
        })
        .unwrap(),
        "{\"expected\":null}"
    );
    let old: ContentGuard = serde_json::from_str("{\"expected\":null,\"detached\":true}").unwrap();
    assert!(old.detached && old.removal.is_none());
    let invalid = ContentGuard {
        expected: None,
        detached: true,
        removal: Some(RemovalFence {
            intent: id(1),
            owner: id(2),
        }),
    };
    let mut r = n.payload.unwrap().reservation;
    r.guard = Some(invalid);
    assert!(!r.valid());
}
#[test]
fn real_full_disk_preserves_arm_uncertainty_and_exact_retry() {
    let mut w = World::new();
    let files = (0..128)
        .map(|i| w.file(&format!("full-{i}")))
        .collect::<Vec<_>>();
    w.state
        .connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
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
    let mut failed = None;
    for n in files {
        let before = w.state.namespace_volume().unwrap();
        let r = w.arm_request(&n);
        match w.state.namespace_apply(&r) {
            Ok(_) => {}
            Err(Error::Internal) => {
                assert_eq!(w.state.namespace_volume().unwrap(), before);
                assert_eq!(w.state.namespace_node(&w.volume, &n.id), Ok(n.clone()));
                assert_eq!(
                    w.state.namespace_pending_removal(&w.volume, &n.id),
                    Ok(None)
                );
                assert_eq!(
                    w.state.namespace_operation(&w.volume, &r.operation),
                    Ok(None)
                );
                failed = Some(r);
                break;
            }
            other => panic!("unexpected full-disk outcome {other:?}"),
        }
    }
    let failed = failed.expect("must exercise actual SQLITE_FULL");
    w.state
        .connection
        .pragma_update(None, "max_page_count", pages + 1000)
        .unwrap();
    let receipt = w.state.namespace_apply(&failed).unwrap();
    w.reopen();
    assert_eq!(w.state.namespace_apply(&failed), Ok(receipt.clone()));
    assert_eq!(w.inspect(&receipt.node).node, receipt.node);
}

#[test]
fn failed_schema_thirteen_migration_preserves_version_and_original_database() {
    let mut w = World::new();
    let n = w.file("preserved migration data");
    w.state.connection.execute_batch("DROP TABLE retained_client_identity; DROP TABLE client_identity; DROP TABLE namespace_removals; CREATE TABLE namespace_removals(keep TEXT); INSERT INTO namespace_removals VALUES('untouched'); PRAGMA user_version=12").unwrap();
    let path = w.tmp.path().join("authority");
    let placeholder = State::open_managed(&w.tmp.path().join("placeholder")).unwrap();
    drop(std::mem::replace(&mut w.state, placeholder));
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        12
    );
    assert_eq!(
        db.query_row::<String, _, _>("SELECT keep FROM namespace_removals", [], |r| r.get(0))
            .unwrap(),
        "untouched"
    );
    assert_eq!(node(&db, &n.id).unwrap(), n);
    db.execute_batch("DROP TABLE namespace_removals").unwrap();
    drop(db);
    w.state = State::open_managed(&path).unwrap();
    assert_eq!(
        w.state.info().unwrap().schema_version,
        crate::SCHEMA_VERSION
    );
    assert_eq!(w.state.namespace_node(&w.volume, &n.id), Ok(n));
}
#[test]
fn removal_owner_child() {
    let Some(path) = std::env::var_os("S3DRIVE_REMOVAL_CHILD") else {
        return;
    };
    let request: Apply = serde_json::from_slice(
        &std::fs::read(std::env::var_os("S3DRIVE_REMOVAL_INPUT").unwrap()).unwrap(),
    )
    .unwrap();
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    s.namespace_apply(&request).unwrap();
    use std::io::Write;
    println!("removal-committed");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    panic!("parent must terminate owner without graceful SQLite close");
}
#[test]
fn killed_owner_preserves_every_committed_disposition_boundary_and_exact_result() {
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
    let mut w = World::new();
    let n = w.file("process death 日本語 🦀");
    let path = w.tmp.path().join("authority");
    let input = w.tmp.path().join("request.json");
    let mut history = Vec::new();
    for step in 0..4 {
        let request = match step {
            0 | 2 => {
                let live = w.state.namespace_node(&w.volume, &n.id).unwrap();
                w.arm_request(&live)
            }
            1 => w.cancel(&w.inspect(&n)),
            _ => w.commit(&w.inspect(&n)),
        };
        std::fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
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
                .env("S3DRIVE_REMOVAL_INPUT", &input)
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
            assert!(Instant::now() < deadline, "owner termination timed out");
            std::thread::yield_now();
        }
        reader.join().unwrap();
        signaled.unwrap();
        w.state = State::open_managed(&path).unwrap();
        let expected = w
            .state
            .namespace_operation(&w.volume, &request.operation)
            .unwrap()
            .unwrap()
            .unwrap();
        let sequence = w.state.namespace_volume().unwrap().sequence;
        assert_eq!(w.state.namespace_apply(&request), Ok(expected.clone()));
        assert_eq!(w.state.namespace_volume().unwrap().sequence, sequence);
        history.push((request, expected));
    }
    let tombstone = w.state.namespace_node(&w.volume, &n.id).unwrap();
    assert!(tombstone.deleted);
    let reused = w.file(&n.name);
    for (r, expected) in history {
        assert_eq!(w.state.namespace_apply(&r), Ok(expected));
    }
    assert_eq!(w.state.namespace_node(&w.volume, &n.id), Ok(tombstone));
    assert_eq!(w.state.namespace_node(&w.volume, &reused.id), Ok(reused));
}

#[test]
fn temporary_pending_response_does_not_poison_an_exact_content_retry_after_cancel() {
    let mut w = World::new();
    let n = w.file("retry retained bytes");
    let reserve = w.reserve(&n, None);
    let p = w.arm(&n);
    assert_eq!(w.state.namespace_apply(&reserve), Err(Error::DeletePending));
    assert_eq!(
        w.state.namespace_operation(&w.volume, &reserve.operation),
        Ok(None)
    );
    let cancelled = w.cancel(&p);
    w.state.namespace_apply(&cancelled).unwrap();
    let r = w.state.namespace_apply(&reserve).unwrap();
    assert_eq!(w.state.namespace_apply(&reserve), Ok(r));
    let publish = w.publication(&reserve.operation);
    let p = w.arm(&w.state.namespace_node(&w.volume, &n.id).unwrap());
    assert_eq!(w.state.namespace_apply(&publish), Err(Error::DeletePending));
    assert_eq!(
        w.state.namespace_operation(&w.volume, &publish.operation),
        Ok(None)
    );
    let cancelled = w.cancel(&p);
    w.state.namespace_apply(&cancelled).unwrap();
    let receipt = w.state.namespace_apply(&publish).unwrap();
    assert!(!receipt.node.deleted);
    assert_eq!(
        receipt.node.payload.unwrap().reservation.id,
        reserve.operation
    );
}

#[test]
fn operation_capacity_reserves_a_terminal_slot_for_each_pending_node() {
    let mut w = World::new();
    let n = w.file("capacity must allow cancel");
    let count: i64 = w
        .state
        .connection
        .query_row("SELECT count(*) FROM namespace_operations", [], |r| {
            r.get(0)
        })
        .unwrap();
    {
        let tx = w.state.connection.transaction().unwrap();
        for i in count..MAX_OPERATIONS - 1 {
            tx.execute(
                "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
                [id(100_000 + i as u64)],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let before = w.state.namespace_volume().unwrap();
    let arm = w.arm_request(&n);
    assert_eq!(w.state.namespace_apply(&arm), Err(Error::Limit));
    assert_eq!(
        w.state.namespace_operation(&w.volume, &arm.operation),
        Ok(None)
    );
    assert_eq!(
        w.state.namespace_pending_removal(&w.volume, &n.id),
        Ok(None)
    );
    assert_eq!(w.state.namespace_node(&w.volume, &n.id), Ok(n.clone()));
    assert_eq!(w.state.namespace_volume().unwrap(), before);
    // Release one fixture slot: there is now capacity for arm AND its close.
    w.state
        .connection
        .execute(
            "DELETE FROM namespace_operations WHERE operation_id=?",
            [id(100_000 + count as u64)],
        )
        .unwrap();
    let armed = w.state.namespace_apply(&arm).unwrap();
    let p = w.inspect(&n);
    assert_eq!(p.node, armed.node);
    assert_eq!(
        w.create(
            &w.root(),
            "must not consume close capacity",
            Kind::EmptyFile
        ),
        Err(Error::Limit)
    );
    let mut wrong = w.cancel(&p);
    if let Mutation::CancelRemoval { fence, .. } = &mut wrong.mutation {
        fence.owner = id(99);
    }
    assert_eq!(w.state.namespace_apply(&wrong), Err(Error::Limit));
    assert_eq!(
        w.state.namespace_operation(&w.volume, &wrong.operation),
        Ok(None)
    );
    assert_eq!(w.inspect(&n), p);
    let cancel = w.cancel(&p);
    assert!(w.state.namespace_apply(&cancel).is_ok());
    assert_eq!(
        w.state.namespace_pending_removal(&w.volume, &n.id),
        Ok(None)
    );
    assert!(!w.state.namespace_node(&w.volume, &n.id).unwrap().deleted);
    assert_eq!(
        w.state
            .connection
            .query_row::<i64, _, _>("SELECT count(*) FROM namespace_operations", [], |r| r
                .get(0))
            .unwrap(),
        MAX_OPERATIONS
    );
}
