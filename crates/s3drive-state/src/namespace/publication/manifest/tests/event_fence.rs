use super::*;

fn owned(s: &mut State, mode: RemovalMode) -> RemovalEpoch {
    let n = setup(s);
    let before = s.namespace_volume().unwrap();
    let r = apply(
        s,
        4,
        if mode == RemovalMode::Deferred {
            Mutation::PrepareRemoval {
                node: NodeGuard {
                    id: n.id.clone(),
                    revision: n.revision,
                },
                parent: NodeGuard {
                    id: ROOT.into(),
                    revision: 0,
                },
                owner: id(90),
            }
        } else {
            Mutation::ArmRemoval {
                node: NodeGuard {
                    id: n.id.clone(),
                    revision: n.revision,
                },
                parent: NodeGuard {
                    id: ROOT.into(),
                    revision: 0,
                },
                owner: id(90),
            }
        },
    );
    let epoch = s
        .namespace_removal_epoch(&before.id, &n.id)
        .unwrap()
        .unwrap();
    assert_eq!(epoch.node, r.node);
    assert_eq!(epoch.claim.previous, id(4));
    epoch
}
fn guard(e: &RemovalEpoch) -> ContentGuard {
    ContentGuard {
        expected: payload_reference(&e.node.payload),
        detached: e.claim.mode.detached(),
        removal: Some(e.claim.fence.clone()),
        removal_claim: Some(Box::new(e.claim.clone())),
    }
}
fn epoch(s: &mut State, id: &str) -> RemovalEpoch {
    let v = s.namespace_volume().unwrap();
    s.namespace_removal_epoch(&v.id, id).unwrap().unwrap()
}
fn transition(s: &mut State, e: &RemovalEpoch, op: u8, mode: RemovalMode) -> RemovalEpoch {
    let expected = match e.claim.mode {
        RemovalPublicationMode::Deferred => RemovalMode::Deferred,
        RemovalPublicationMode::Pending => RemovalMode::Pending,
        _ => panic!("active only"),
    };
    apply(
        s,
        op,
        Mutation::TransitionRemoval {
            node: NodeGuard {
                id: e.node.id.clone(),
                revision: e.node.revision,
            },
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            fence: e.claim.fence.clone(),
            expected,
            mode,
        },
    );
    epoch(s, &e.node.id)
}
fn cancel(s: &mut State, e: &RemovalEpoch, op: u8) {
    apply(
        s,
        op,
        Mutation::CancelRemoval {
            node: e.node.id.clone(),
            fence: e.claim.fence.clone(),
        },
    );
}
fn outputs(s: &State) -> Vec<(String, Vec<u8>, Vec<u8>)> {
    s.connection
        .prepare(
            "SELECT operation_id,request,outcome FROM namespace_operations ORDER BY operation_id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[test]
fn event_manifest_both_active_modes_replay_original_receipts_after_cancel_and_restart() {
    for mode in [RemovalMode::Deferred, RemovalMode::Pending] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        let e = owned(&mut s, mode);
        let q = request(&s, &e.node, 5, guard(&e));
        let reserved = s.namespace_apply(&q).unwrap();
        let Some(Artifact::Reservation(r)) = &reserved.artifact else {
            panic!()
        };
        let p = publish(&mut s, r, 6);
        let done = s.namespace_apply(&p).unwrap();
        let e = epoch(&mut s, &e.node.id);
        cancel(&mut s, &e, 7);
        assert_eq!(
            s.namespace_removal_epoch(&q.volume, &e.node.id).unwrap(),
            None
        );
        let rows = outputs(&s);
        drop(s);
        let mut s = State::open_managed(&path).unwrap();
        assert_eq!(outputs(&s), rows);
        assert_eq!(s.namespace_apply(&q).unwrap(), reserved);
        assert_eq!(s.namespace_apply(&p).unwrap(), done);
        let Some(Artifact::Published(payload)) = done.artifact else {
            panic!()
        };
        let reference = payload_reference(&Some(payload.clone())).unwrap();
        assert_eq!(
            s.namespace_manifest_payload(&q.volume, &e.node.id, &q.operation, &reference)
                .unwrap(),
            payload
        );
        let mut changed = q.clone();
        if let Mutation::ReserveManifest { guard, .. } = &mut changed.mutation {
            guard.removal_claim.as_mut().unwrap().previous = id(9);
        }
        assert_eq!(s.namespace_apply(&changed), Err(Error::OperationMismatch));
        assert_eq!(outputs(&s), rows);
    }
}

#[test]
fn event_manifest_stale_event_cannot_be_paired_with_current_revision_after_mode_roundtrip() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let original = owned(&mut s, RemovalMode::Deferred);
    let pending = transition(&mut s, &original, 5, RemovalMode::Pending);
    let current = transition(&mut s, &pending, 6, RemovalMode::Deferred);
    assert_eq!(current.node.payload, original.node.payload);
    assert_ne!(current.node.revision, original.node.revision);
    assert_eq!(current.claim.fence, original.claim.fence);
    let stale = request(&s, &current.node, 7, guard(&original));
    assert_eq!(s.namespace_apply(&stale), Err(Error::Conflict));
    assert!(
        s.namespace_reservation(&stale.volume, &stale.operation)
            .is_err()
    );
    let good = request(&s, &current.node, 8, guard(&current));
    assert!(s.namespace_apply(&good).is_ok());
}

#[test]
fn event_manifest_publish_rechecks_event_even_when_payload_is_unchanged() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let original = owned(&mut s, RemovalMode::Deferred);
    let q = request(&s, &original.node, 5, guard(&original));
    let r = reservation(&mut s, &q);
    let p = publish(&mut s, &r, 6);
    let current = transition(&mut s, &original, 7, RemovalMode::Pending);
    assert_eq!(current.node.payload, original.node.payload);
    assert_eq!(s.namespace_apply(&p), Err(Error::Conflict));
    assert_eq!(epoch(&mut s, &original.node.id), current);
    assert_eq!(s.namespace_apply(&p), Err(Error::Conflict));
}

#[test]
fn event_manifest_committed_identity_survives_unlink_and_owned_replacement_without_rebinding_name()
{
    for replace in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&t.path().join("s")).unwrap();
        let e = owned(&mut s, RemovalMode::Pending);
        let replacement = if replace {
            let src = apply(
                &mut s,
                5,
                Mutation::CreateEntry {
                    parent: ROOT.into(),
                    parent_revision: 0,
                    store: Some(id(3)),
                    name: "replacement source".into(),
                    node_kind: Kind::EmptyFile,
                },
            )
            .node;
            let r = apply(
                &mut s,
                6,
                Mutation::MoveEntry {
                    node: NodeGuard {
                        id: src.id.clone(),
                        revision: src.revision,
                    },
                    source_parent: NodeGuard {
                        id: ROOT.into(),
                        revision: 0,
                    },
                    parent: NodeGuard {
                        id: ROOT.into(),
                        revision: 0,
                    },
                    name: e.node.name.clone(),
                    destination: Some(NodeGuard {
                        id: e.node.id.clone(),
                        revision: e.node.revision,
                    }),
                    destination_removal: Some(Box::new(RemovalClaim {
                        fence: e.claim.fence.clone(),
                        previous: e.claim.previous.clone(),
                        mode: RemovalMode::Pending,
                    })),
                },
            );
            Some(r.node)
        } else {
            apply(
                &mut s,
                6,
                Mutation::CommitRemoval {
                    node: NodeGuard {
                        id: e.node.id.clone(),
                        revision: e.node.revision,
                    },
                    parent: NodeGuard {
                        id: ROOT.into(),
                        revision: 0,
                    },
                    fence: e.claim.fence.clone(),
                },
            );
            None
        };
        let closed = epoch(&mut s, &e.node.id);
        assert!(closed.node.deleted);
        assert_eq!(closed.claim.mode, RemovalPublicationMode::Committed);
        assert_eq!(closed.claim.previous, id(6));
        assert_eq!(closed.claim.fence, e.claim.fence);
        let q = request(&s, &closed.node, 7, guard(&closed));
        let r = reservation(&mut s, &q);
        let p = publish(&mut s, &r, 8);
        let done = s.namespace_apply(&p).unwrap();
        assert!(done.node.deleted);
        assert!(s.namespace_node(&q.volume, &e.node.id).unwrap().deleted);
        assert!(
            s.namespace_list(&q.volume, ROOT)
                .unwrap()
                .children
                .iter()
                .all(|n| n.id != e.node.id)
        );
        if let Some(n) = replacement {
            assert_eq!(s.namespace_node(&q.volume, &n.id).unwrap(), n);
        }
        let current = epoch(&mut s, &e.node.id);
        assert_eq!(current.claim, closed.claim);
        assert!(current.node.deleted);
        assert_eq!(
            s.namespace_apply(&q).unwrap().artifact,
            Some(Artifact::Reservation(r))
        );
        assert_eq!(s.namespace_apply(&p).unwrap(), done);
    }
}

#[test]
fn event_manifest_cancel_and_successor_cannot_grant_old_owner_detached_permission() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let original = owned(&mut s, RemovalMode::Deferred);
    cancel(&mut s, &original, 5);
    let v = s.namespace_volume().unwrap();
    assert_eq!(
        s.namespace_removal_epoch(&v.id, &original.node.id).unwrap(),
        None
    );
    let n = s.namespace_node(&v.id, &original.node.id).unwrap();
    apply(
        &mut s,
        6,
        Mutation::PrepareRemoval {
            node: NodeGuard {
                id: n.id.clone(),
                revision: n.revision,
            },
            parent: NodeGuard {
                id: ROOT.into(),
                revision: 0,
            },
            owner: id(91),
        },
    );
    let current = epoch(&mut s, &n.id);
    assert_ne!(current.claim.fence, original.claim.fence);
    let q = request(&s, &current.node, 7, guard(&original));
    assert_eq!(s.namespace_apply(&q), Err(Error::Conflict));
    let mut g = guard(&current);
    g.detached = true;
    let q = request(&s, &current.node, 8, g);
    assert_eq!(s.namespace_apply(&q), Err(Error::Invalid));
    let mut g = guard(&current);
    g.removal_claim.as_mut().unwrap().mode = RemovalPublicationMode::Committed;
    g.detached = true;
    let q = request(&s, &current.node, 9, g);
    assert_eq!(s.namespace_apply(&q), Err(Error::Conflict));
}

#[test]
fn event_manifest_statement_success_cannot_hide_deleted_or_forged_event_witness() {
    for fragment in [
        "DELETE FROM namespace_removal_events WHERE operation_id=",
        "UPDATE namespace_removal_events SET mode='pending' WHERE operation_id=",
    ] {
        let t = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&t.path().join("s")).unwrap();
        let e = owned(&mut s, RemovalMode::Deferred);
        let before = outputs(&s);
        let volume = s.namespace_volume().unwrap();
        let q = request(&s, &e.node, 5, guard(&e));
        s.connection.execute_batch(&format!("CREATE TRIGGER corrupt_event AFTER INSERT ON namespace_operations WHEN NEW.operation_id='{}' BEGIN {fragment}'{}'; END",id(5),id(4))).unwrap();
        assert_eq!(s.namespace_apply(&q), Err(Error::Internal));
        assert_eq!(outputs(&s), before);
        assert_eq!(s.namespace_volume().unwrap(), volume);
        assert_eq!(epoch(&mut s, &e.node.id), e);
        s.connection
            .execute_batch("DROP TRIGGER corrupt_event")
            .unwrap();
        assert!(s.namespace_apply(&q).is_ok());
    }
}

#[test]
fn event_manifest_actual_sqlite_full_rolls_back_exact_reserve_and_publish() {
    for publishing in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&t.path().join("s")).unwrap();
        let e = owned(&mut s, RemovalMode::Deferred);
        let q = request(&s, &e.node, 5, guard(&e));
        let q = if publishing {
            let r = reservation(&mut s, &q);
            publish(&mut s, &r, 6)
        } else {
            q
        };
        let before = outputs(&s);
        let volume = s.namespace_volume().unwrap();
        s.connection
            .execute_batch("CREATE TABLE pressure(data BLOB)")
            .unwrap();
        let event = if publishing {
            "BEFORE UPDATE OF publication"
        } else {
            "BEFORE INSERT"
        };
        s.connection.execute_batch(&format!("CREATE TRIGGER pressure_event {event} ON namespace_uploads BEGIN INSERT INTO pressure VALUES(zeroblob(65536)); END")).unwrap();
        let pages: i64 = s
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        let max: i64 = s
            .connection
            .pragma_query_value(None, "max_page_count", |r| r.get(0))
            .unwrap();
        s.connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        loop {
            match s
                .connection
                .execute("INSERT INTO pressure VALUES(zeroblob(4096))", [])
            {
                Ok(_) => (),
                Err(err) => {
                    assert_eq!(err.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                    break;
                }
            }
        }
        assert_eq!(s.namespace_apply(&q), Err(Error::Internal));
        assert_eq!(outputs(&s), before);
        assert_eq!(s.namespace_volume().unwrap(), volume);
        assert_eq!(epoch(&mut s, &e.node.id), e);
        s.connection
            .pragma_update(None, "max_page_count", max)
            .unwrap();
        s.connection
            .execute_batch("DROP TRIGGER pressure_event")
            .unwrap();
        assert!(s.namespace_apply(&q).is_ok());
    }
}

#[test]
fn event_manifest_shared_closing_quota_rolls_back_new_reserve_and_publish() {
    for publishing in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&t.path().join("s")).unwrap();
        let e = owned(&mut s, RemovalMode::Deferred);
        let q = request(&s, &e.node, 5, guard(&e));
        let q = if publishing {
            let r = reservation(&mut s, &q);
            publish(&mut s, &r, 6)
        } else {
            q
        };
        let current: i64 = s
            .connection
            .query_row("SELECT count(*) FROM namespace_operations", [], |r| {
                r.get(0)
            })
            .unwrap();
        {
            let tx = s.connection.transaction().unwrap();
            for n in current..(MAX_OPERATIONS - 2) {
                tx.execute(
                    "INSERT INTO namespace_operations VALUES(?1,?2,?3)",
                    params![
                        format!("{:032x}", 10000 + n),
                        b"legacy opaque request".as_slice(),
                        b"legacy opaque outcome".as_slice()
                    ],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let before = outputs(&s);
        let volume = s.namespace_volume().unwrap();
        assert_eq!(s.namespace_apply(&q), Err(Error::Limit));
        assert_eq!(outputs(&s), before);
        assert_eq!(s.namespace_volume().unwrap(), volume);
        assert_eq!(epoch(&mut s, &e.node.id), e);
    }
}

#[test]
fn event_manifest_legacy_guard_bytes_and_fence_only_behavior_are_unchanged() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let old = owned(&mut s, RemovalMode::Deferred);
    let current = transition(&mut s, &old, 5, RemovalMode::Pending);
    let mut g = guard(&old);
    g.removal_claim = None;
    assert!(
        !String::from_utf8(serde_json::to_vec(&g).unwrap())
            .unwrap()
            .contains("removal_claim")
    );
    let q = request(&s, &current.node, 6, g);
    assert!(s.namespace_apply(&q).is_ok());
}

#[test]
fn event_manifest_raw_null_presence_is_rejected_before_any_old_marker_jump() {
    for version in [43, 44, 45] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        let n = setup(&mut s);
        let q = request(&s, &n, 4, normal(&n));
        s.namespace_apply(&q).unwrap();
        let mut raw = serde_json::to_value(&q).unwrap();
        raw["mutation"]["guard"]["removal_claim"] = serde_json::Value::Null;
        let bytes = serde_json::to_vec(&raw).unwrap();
        s.connection
            .execute(
                "UPDATE namespace_operations SET request=?1 WHERE operation_id=?2",
                params![bytes, q.operation],
            )
            .unwrap();
        s.connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        drop(s);
        let before = std::fs::read(path.join("metadata.sqlite3")).unwrap();
        let err = State::open_managed(&path)
            .err()
            .expect("new shape cannot be relabeled");
        assert!(err.to_string().contains("schema46"), "{err}");
        assert_eq!(
            std::fs::read(path.join("metadata.sqlite3")).unwrap(),
            before
        );
    }
}

#[test]
fn event_manifest_genuine_legacy45_upgrade_preserves_every_original_request_and_result() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let n = setup(&mut s);
    let q = request(&s, &n, 4, normal(&n));
    let r = reservation(&mut s, &q);
    let p = publish(&mut s, &r, 5);
    let done = s.namespace_apply(&p).unwrap();
    let before = outputs(&s);
    s.connection.execute_batch("CREATE TABLE user_bytes(data BLOB);INSERT INTO user_bytes VALUES(CAST('{\"removal_claim\":null}' AS BLOB));PRAGMA user_version=45").unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(outputs(&s), before);
    assert_eq!(s.namespace_apply(&p).unwrap(), done);
}

#[test]
fn event_manifest_crash_child() {
    let Some(root) = std::env::var_os("S3DRIVE_EVENT_MANIFEST_WORKER") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let request: Apply =
        serde_json::from_slice(&std::fs::read(root.join("request.json")).unwrap()).unwrap();
    let mut s = State::open_managed(&root.join("state")).unwrap();
    s.namespace_apply(&request).unwrap();
}

#[test]
fn event_manifest_killed_workers_preserve_atomic_reserve_publish_and_exact_committed_identity() {
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};
    struct Worker(Child);
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for publishing in [false, true] {
        for point in [
            "EffectStored",
            "SequenceUpdated",
            "OutcomeStored",
            "Committed",
        ] {
            let t = tempfile::tempdir().unwrap();
            let path = t.path().join("state");
            let mut s = State::open_managed(&path).unwrap();
            let e = owned(&mut s, RemovalMode::Deferred);
            let q = request(&s, &e.node, 5, guard(&e));
            let q = if publishing {
                let r = reservation(&mut s, &q);
                publish(&mut s, &r, 6)
            } else {
                q
            };
            let before = outputs(&s);
            let volume = s.namespace_volume().unwrap();
            drop(s);
            std::fs::write(
                t.path().join("request.json"),
                serde_json::to_vec(&q).unwrap(),
            )
            .unwrap();
            let ready = t.path().join("ready");
            let mut child=Worker(Command::new(std::env::current_exe().unwrap()).args(["namespace::publication::manifest::tests::event_fence::event_manifest_crash_child","--exact","--nocapture"]).env("S3DRIVE_EVENT_MANIFEST_WORKER",t.path()).env("S3DRIVE_EVENT_MANIFEST_CRASH_POINT",point).env("S3DRIVE_EVENT_MANIFEST_CRASH_OPERATION",&q.operation).env("S3DRIVE_EVENT_MANIFEST_CRASH_READY",&ready).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn().unwrap());
            let deadline = Instant::now() + Duration::from_secs(45);
            while !ready.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "worker exited before {point}"
                );
                assert!(Instant::now() < deadline, "no {point} readiness");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(std::fs::read(&ready).unwrap(), b"ready");
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            drop(child);
            let mut s = State::open_managed(&path).unwrap();
            let saved = s.namespace_operation(&q.volume, &q.operation).unwrap();
            if point == "Committed" {
                assert!(matches!(saved, Some(Ok(_))));
                assert_eq!(s.namespace_volume().unwrap().sequence, volume.sequence + 1);
            } else {
                assert_eq!(saved, None);
                assert_eq!(outputs(&s), before);
                assert_eq!(epoch(&mut s, &e.node.id), e);
            }
            let receipt = s.namespace_apply(&q).unwrap();
            if let Some(old) = saved {
                assert_eq!(old, Ok(receipt.clone()));
            }
            assert_eq!(s.namespace_apply(&q).unwrap(), receipt);
            assert_eq!(s.namespace_volume().unwrap().sequence, volume.sequence + 1);
        }
    }
}

#[test]
fn event_manifest_real_new_records_cannot_be_relabelled_as43_44_or45() {
    for version in [43, 44, 45] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        let e = owned(&mut s, RemovalMode::Deferred);
        let q = request(&s, &e.node, 5, guard(&e));
        s.namespace_apply(&q).unwrap();
        s.connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        drop(s);
        let before = std::fs::read(path.join("metadata.sqlite3")).unwrap();
        let err = State::open_managed(&path)
            .err()
            .expect("new record shape under old header");
        assert!(err.to_string().contains("schema46"), "{err}");
        assert_eq!(
            std::fs::read(path.join("metadata.sqlite3")).unwrap(),
            before
        );
    }
}

#[test]
fn event_manifest_live_old_marker_cannot_admit_the_new_guard_or_epoch_read() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let e = owned(&mut s, RemovalMode::Deferred);
    let q = request(&s, &e.node, 5, guard(&e));
    let before = outputs(&s);
    s.connection
        .pragma_update(None, "user_version", 45)
        .unwrap();
    assert_eq!(s.namespace_apply(&q), Err(Error::Internal));
    assert_eq!(
        s.namespace_removal_epoch(&q.volume, &e.node.id),
        Err(Error::Internal)
    );
    assert_eq!(outputs(&s), before);
    s.connection
        .pragma_update(None, "user_version", 46)
        .unwrap();
    assert!(s.namespace_apply(&q).is_ok());
}

#[test]
fn event_manifest_ready_marker_is_invisible_until_its_complete_atomic_publication() {
    let temp = tempfile::tempdir().unwrap();
    let ready = temp.path().join("ready");
    super::super::super::event::publish_ready(&ready, |temporary| {
        assert!(!ready.exists(), "parent must not observe partial readiness");
        assert_eq!(std::fs::read(temporary).unwrap(), b"ready");
    })
    .unwrap();
    assert_eq!(std::fs::read(&ready).unwrap(), b"ready");
    assert!(!ready.with_extension("tmp").exists());
}
