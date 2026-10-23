use super::*;
fn id(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}
fn setup(path: &std::path::Path) -> State {
    let mut s = State::open_managed(path).unwrap();
    let v = s.namespace_initialize_with_policy(names::PORTABLE).unwrap();
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: id(3),
        expected_sequence: 0,
        mutation: Mutation::RegisterStore {
            route: "a".repeat(64),
            root: "managed/".into(),
        },
    })
    .unwrap();
    s
}
fn request(s: &State, n: u8) -> Apply {
    let v = s.namespace_volume().unwrap();
    Apply {
        volume: v.id,
        operation: id(n),
        expected_sequence: v.sequence,
        mutation: Mutation::CreateDeferredEmptyManifest {
            parent: ROOT.into(),
            parent_revision: 0,
            name: format!("café 日本語 🦀 {n}.bin"),
            store: id(3),
            upload_operation: "a".repeat(64),
            etag: "\"verified-model\"".into(),
            version: None,
            owner: id(9),
        },
    }
}
fn apply(s: &mut State, n: u8, mutation: Mutation) -> Result<Receipt> {
    let v = s.namespace_volume()?;
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: id(n),
        expected_sequence: v.sequence,
        mutation,
    })
}
fn guard(n: &Node) -> NodeGuard {
    NodeGuard {
        id: n.id.clone(),
        revision: n.revision,
    }
}
fn root() -> NodeGuard {
    NodeGuard {
        id: ROOT.into(),
        revision: 0,
    }
}
fn artifact(r: &Receipt) -> (&Payload, &RemovalFence) {
    let Some(Artifact::PublishedRemoval { payload, fence }) = &r.artifact else {
        panic!("combined canonical ownership receipt")
    };
    (payload, fence)
}
fn rows(db: &Connection) -> Vec<Vec<String>> {
    ["SELECT quote(volume_id)||quote(sequence)||quote(name_policy) FROM namespace_volume",
    "SELECT quote(node_id)||quote(parent_id)||quote(name)||quote(kind)||quote(revision)||quote(deleted)||quote(name_key)||quote(payload)||quote(placement) FROM namespace_nodes ORDER BY node_id",
    "SELECT quote(reservation_id)||quote(descriptor)||quote(publication) FROM namespace_uploads ORDER BY reservation_id",
    "SELECT quote(operation_id)||quote(request)||quote(outcome) FROM namespace_operations ORDER BY operation_id",
    "SELECT quote(node_id)||quote(intent_id)||quote(owner_id)||quote(mode)||quote(event_id) FROM namespace_removals ORDER BY node_id",
    "SELECT quote(node_id)||quote(operation_id)||quote(intent_id)||quote(owner_id)||quote(mode)||quote(sequence) FROM namespace_removal_events ORDER BY node_id,sequence"]
    .iter().map(|q|db.prepare(q).unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap()).collect()
}
fn history(s: &mut State, p: &Payload) -> Result<Payload> {
    s.namespace_manifest_payload(
        &p.reservation.volume,
        &p.reservation.node,
        &p.reservation.id,
        &payload_reference(&Some(p.clone())).unwrap(),
    )
}
#[test]
fn deferred_canonical_original_fence_history_and_replay_survive_owned_move_transition_commit_and_restart()
 {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = setup(&path);
    let q = request(&s, 4);
    let r = s.namespace_apply(&q).unwrap();
    let (p, fence) = artifact(&r);
    let p = p.clone();
    let fence = fence.clone();
    assert_eq!(r.node.kind, Kind::File);
    assert_eq!(r.node.payload.as_ref(), Some(&p));
    assert_eq!(p.reservation.size, 64);
    assert_eq!(p.reservation.id, q.operation);
    assert_eq!(
        fence,
        RemovalFence {
            intent: q.operation.clone(),
            owner: id(9)
        }
    );
    assert_eq!(history(&mut s, &p).unwrap(), p);
    let pending = s
        .namespace_pending_removal(&q.volume, &q.operation)
        .unwrap()
        .unwrap();
    assert_eq!(pending.mode, RemovalMode::Deferred);
    assert_eq!(pending.fence, fence);
    assert_eq!(s.namespace_node(&q.volume, &q.operation).unwrap(), r.node);
    assert_eq!(
        apply(
            &mut s,
            5,
            Mutation::Reserve {
                node: q.operation.clone(),
                expected_revision: r.sequence,
                store: id(3),
                size: 1,
                sha256: "a".repeat(64)
            }
        ),
        Err(Error::DeletePending)
    );
    let moved = apply(
        &mut s,
        6,
        Mutation::MoveDeferredEntry {
            node: guard(&r.node),
            source_parent: root(),
            parent: root(),
            name: "moved 日本語 🦀".into(),
            destination: None,
            destination_removal: None,
            fence: Box::new(fence.clone()),
            previous: q.operation.clone().into(),
        },
    )
    .unwrap();
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
    assert_eq!(
        s.namespace_pending_removal(&q.volume, &q.operation)
            .unwrap()
            .unwrap()
            .node,
        moved.node
    );
    let pending = apply(
        &mut s,
        7,
        Mutation::TransitionRemoval {
            node: guard(&moved.node),
            parent: root(),
            fence: fence.clone(),
            expected: RemovalMode::Deferred,
            mode: RemovalMode::Pending,
        },
    )
    .unwrap();
    assert_eq!(
        s.namespace_node(&q.volume, &q.operation),
        Err(Error::DeletePending)
    );
    let mut wrong = fence.clone();
    wrong.owner = id(99);
    assert_eq!(
        apply(
            &mut s,
            8,
            Mutation::CommitRemoval {
                node: guard(&pending.node),
                parent: root(),
                fence: wrong
            }
        ),
        Err(Error::Conflict)
    );
    let done = apply(
        &mut s,
        10,
        Mutation::CommitRemoval {
            node: guard(&pending.node),
            parent: root(),
            fence: fence.clone(),
        },
    )
    .unwrap();
    assert!(done.node.deleted);
    assert_eq!(
        s.namespace_pending_removal(&q.volume, &q.operation)
            .unwrap(),
        None
    );
    assert_eq!(
        s.namespace_operation(&q.volume, &q.operation).unwrap(),
        Some(Ok(r.clone()))
    );
    assert_eq!(history(&mut s, &p).unwrap(), p);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
    assert_eq!(history(&mut s, &p).unwrap(), p);
}
#[test]
fn deferred_canonical_cancellation_retains_initial_receipt_and_releases_ordinary_namespace_work() {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("s"));
    let q = request(&s, 4);
    let r = s.namespace_apply(&q).unwrap();
    let (p, fence) = artifact(&r);
    let p = p.clone();
    let fence = fence.clone();
    let cancel = apply(
        &mut s,
        5,
        Mutation::CancelRemoval {
            node: q.operation.clone(),
            fence,
        },
    )
    .unwrap();
    let moved = apply(
        &mut s,
        6,
        Mutation::MoveEntry {
            node: guard(&cancel.node),
            source_parent: root(),
            parent: root(),
            name: "cancelled café".into(),
            destination: None,
            destination_removal: None,
        },
    )
    .unwrap();
    let removed = apply(
        &mut s,
        7,
        Mutation::RemoveEntry {
            node: guard(&moved.node),
            parent: root(),
        },
    )
    .unwrap();
    assert!(removed.node.deleted);
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
    assert_eq!(history(&mut s, &p).unwrap(), p);
}
#[test]
fn deferred_canonical_migration_preserves_old_bytes_and_rejects_downgraded_new_records() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = setup(&path);
    let old = ordinary(&request(&s, 4)).unwrap();
    s.namespace_apply(&old).unwrap();
    let before = rows(&s.connection);
    let tables: i64 = s
        .connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(tables <= 65);
    crate::test_schema::without_retry(&s.connection);
    crate::test_schema::demote(&s.connection, 41).unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(rows(&s.connection), before);
    let after_tables: i64 = s
        .connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, after_tables);
    let q = request(&s, 5);
    s.namespace_apply(&q).unwrap();
    let before = rows(&s.connection);
    crate::test_schema::demote(&s.connection, 41).unwrap();
    assert_eq!(s.namespace_apply(&q), Err(Error::Internal));
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        41
    );
    assert_eq!(rows(&db), before);
}

#[test]
fn deferred_canonical_validation_collisions_and_unrelated_sequences_keep_exact_original_outcomes() {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("s"));
    let q = request(&s, 4);
    apply(
        &mut s,
        14,
        Mutation::RegisterStore {
            route: "b".repeat(64),
            root: "other/".into(),
        },
    )
    .unwrap();
    let r = s.namespace_apply(&q).unwrap();
    assert!(r.sequence > q.expected_sequence + 1);
    let mut changed = q.clone();
    if let Mutation::CreateDeferredEmptyManifest { owner, .. } = &mut changed.mutation {
        *owner = id(10)
    }
    assert_eq!(s.namespace_apply(&changed), Err(Error::OperationMismatch));
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
    for case in 0..7 {
        let mut bad = request(&s, 20 + case);
        let Mutation::CreateDeferredEmptyManifest {
            owner,
            parent_revision,
            store,
            upload_operation,
            etag,
            version,
            name,
            ..
        } = &mut bad.mutation
        else {
            panic!()
        };
        let expected = match case {
            0 => {
                *owner = ROOT.into();
                Error::Invalid
            }
            1 => {
                *parent_revision = 1;
                Error::Conflict
            }
            2 => {
                *store = id(99);
                Error::NotFound
            }
            3 => {
                *upload_operation = "a".repeat(63);
                Error::Invalid
            }
            4 => {
                *etag = "".into();
                Error::Invalid
            }
            5 => {
                *version = Some("\0".into());
                Error::Invalid
            }
            _ => {
                *store = id(14);
                *name = r.node.name.clone();
                Error::AlreadyExists
            }
        };
        let before = rows(&s.connection);
        assert_eq!(s.namespace_apply(&bad), Err(expected), "case{case}");
        let after = rows(&s.connection);
        for index in [0, 1, 2, 4, 5] {
            assert_eq!(before[index], after[index], "partial creation case{case}");
        }
    }
}
#[test]
fn deferred_canonical_missing_or_forged_initial_history_fence_and_event_fail_without_repair() {
    for case in 0..7 {
        let t = tempfile::tempdir().unwrap();
        let mut s = setup(&t.path().join("s"));
        let q = request(&s, 4);
        s.namespace_apply(&q).unwrap();
        match case {
            0 => {
                s.connection
                    .execute(
                        "DELETE FROM namespace_removal_events WHERE node_id=?1",
                        [&q.operation],
                    )
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute(
                        "UPDATE namespace_removal_events SET owner_id=?2 WHERE node_id=?1",
                        params![q.operation, id(99)],
                    )
                    .unwrap();
            }
            2 => {
                s.connection
                    .execute(
                        "UPDATE namespace_removal_events SET mode='pending' WHERE node_id=?1",
                        [&q.operation],
                    )
                    .unwrap();
            }
            3 => {
                s.connection
                    .execute(
                        "UPDATE namespace_removals SET owner_id=?2 WHERE node_id=?1",
                        params![q.operation, id(99)],
                    )
                    .unwrap();
            }
            4 => {
                s.connection
                    .execute(
                        "DELETE FROM namespace_uploads WHERE reservation_id=?1",
                        [&q.operation],
                    )
                    .unwrap();
            }
            5 => {
                s.connection
                    .execute(
                        "UPDATE namespace_uploads SET publication=x'7b7d' WHERE reservation_id=?1",
                        [&q.operation],
                    )
                    .unwrap();
            }
            _ => {
                s.connection.execute_batch("CREATE TRIGGER ignored_creation BEFORE INSERT ON namespace_nodes BEGIN SELECT RAISE(IGNORE); END").unwrap();
            }
        }
        let before = rows(&s.connection);
        if case == 3 {
            assert_eq!(
                s.namespace_pending_removal(&q.volume, &q.operation),
                Err(Error::Internal)
            );
        } else {
            assert_eq!(s.namespace_apply(&q), Err(Error::Internal), "case{case}");
            assert!(s.namespace_operation(&q.volume, &q.operation).is_err());
        }
        assert_eq!(rows(&s.connection), before);
    }
}
#[test]
fn deferred_canonical_sqlite_full_rolls_back_payload_owner_event_sequence_and_result_then_retries_exactly()
 {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("s"));
    s.connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); CREATE TABLE pressure(data BLOB)")
        .unwrap();
    let maximum: i64 = s
        .connection
        .pragma_query_value(None, "max_page_count", |r| r.get(0))
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let e = s
        .connection
        .execute("INSERT INTO pressure VALUES(zeroblob(1048576))", [])
        .unwrap_err();
    assert!(crate::cache::is_disk_full(&e));
    let mut failed = None;
    for n in 4..64 {
        let q = request(&s, n);
        let before = rows(&s.connection);
        match s.namespace_apply(&q) {
            Ok(_) => (),
            Err(e) => {
                assert_eq!(e, Error::Internal);
                assert_eq!(rows(&s.connection), before);
                failed = Some(q);
                break;
            }
        }
    }
    let q = failed.expect("creation must encounter SQLITE_FULL");
    s.connection
        .pragma_update(None, "max_page_count", maximum)
        .unwrap();
    let r = s.namespace_apply(&q).unwrap();
    let (p, fence) = artifact(&r);
    let p = p.clone();
    let fence = fence.clone();
    assert_eq!(history(&mut s, &p).unwrap(), p);
    assert_eq!(
        s.namespace_pending_removal(&q.volume, &q.operation)
            .unwrap()
            .unwrap()
            .fence,
        fence
    );
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
}
mod crash;

#[test]
fn deferred_canonical_combined_artifact_cannot_be_attached_to_old_operations_or_laundered_by_old_marker()
 {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = setup(&path);
    let q = ordinary(&request(&s, 4)).unwrap();
    let mut r = s.namespace_apply(&q).unwrap();
    r.artifact = Some(Artifact::PublishedRemoval {
        payload: r.node.payload.clone().unwrap(),
        fence: RemovalFence {
            intent: q.operation.clone(),
            owner: id(9),
        },
    });
    let outcome = serde_json::to_vec(&Ok::<_, Error>(r)).unwrap();
    s.connection
        .execute(
            "UPDATE namespace_operations SET outcome=?2 WHERE operation_id=?1",
            params![q.operation, outcome],
        )
        .unwrap();
    assert_eq!(
        s.namespace_operation(&q.volume, &q.operation),
        Err(Error::Internal)
    );
    let before = rows(&s.connection);
    crate::test_schema::demote(&s.connection, 41).unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(rows(&db), before);
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        41
    );
}
#[test]
fn deferred_canonical_quotas_reserve_final_owner_closing_capacity_and_rollback_partial_effects() {
    for case in 0..3 {
        let t = tempfile::tempdir().unwrap();
        let mut s = setup(&t.path().join("s"));
        s.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        let count = match case {
            0 => MAX_NODES - 1,
            1 => 4096,
            _ => MAX_OPERATIONS - 3,
        };
        for n in 0..count {
            let key = format!("{:032x}", 10000 + n);
            match case {
                0 => {
                    let name = format!("retired {n}");
                    s.connection.execute("INSERT INTO namespace_nodes(node_id,parent_id,name,kind,revision,deleted,name_key) VALUES(?1,?2,?3,'empty-file',0,1,?4)",params![key,ROOT,name,names::collision_key(&name).unwrap()]).unwrap();
                }
                1 => {
                    s.connection
                        .execute("INSERT INTO namespace_uploads VALUES(?1,x'00',NULL)", [key])
                        .unwrap();
                }
                _ => {
                    s.connection
                        .execute(
                            "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
                            [key],
                        )
                        .unwrap();
                }
            }
        }
        s.connection.execute_batch("COMMIT").unwrap();
        let q = request(&s, 4);
        let before = rows(&s.connection);
        assert_eq!(s.namespace_apply(&q), Err(Error::Limit));
        let after = rows(&s.connection);
        for i in [0, 1, 2, 4, 5] {
            assert_eq!(after[i], before[i], "case{case}");
        }
        if case == 2 {
            assert_eq!(after, before);
        } else {
            assert_eq!(after[3].len(), before[3].len() + 1);
            assert_eq!(
                s.namespace_operation(&q.volume, &q.operation).unwrap(),
                Some(Err(Error::Limit))
            );
        }
    }
}

#[test]
fn deferred_canonical_offline_snapshot_preserves_initial_owner_and_exact_operation_bytes_without_restore_rights()
 {
    use crate::authority_snapshot::{Limits, create, inspect};
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("authority");
    let target = t.path().join("snapshot");
    let mut s = setup(&path);
    let q = request(&s, 4);
    s.namespace_apply(&q).unwrap();
    let before = rows(&s.connection);
    drop(s);
    let manifest = create(
        &path,
        &q.volume,
        names::PORTABLE,
        &target,
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(manifest.schema_version, crate::SCHEMA_VERSION);
    let inspection = inspect(&target, &q.volume, names::PORTABLE, &Limits::default()).unwrap();
    assert_eq!(inspection.manifest, manifest);
    assert!(!inspection.restore_authorized && !inspection.history_continuity_proven);
    let copied = Connection::open(target.join("snapshot.sqlite3")).unwrap();
    assert_eq!(rows(&copied), before);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(rows(&s.connection), before);
    assert!(s.namespace_apply(&q).is_ok());
}

#[test]
fn deferred_canonical_initial_receipt_survives_later_owned_manifest_publication_without_adopting_new_root()
 {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("s"));
    let q = request(&s, 4);
    let initial = s.namespace_apply(&q).unwrap();
    let (original, fence) = artifact(&initial);
    let original = original.clone();
    let fence = fence.clone();
    let before = original
        .reservation
        .manifest
        .as_ref()
        .unwrap()
        .root()
        .unwrap();
    let (root, body) = s3drive_core::manifest::objects::RootDescriptor::empty(
        *before.volume(),
        *before.node(),
        *before.page().store(),
        [11; 16],
    )
    .unwrap();
    let reserved = apply(
        &mut s,
        11,
        Mutation::ReserveManifest {
            node: q.operation.clone(),
            expected_revision: initial.node.revision,
            store: id(3),
            size: body.len() as u64,
            sha256: Sha256::digest(&body)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            manifest: Box::new(ManifestRoot::new(root).unwrap()),
            guard: ContentGuard {
                removal_claim: None,
                expected: payload_reference(&initial.node.payload),
                detached: false,
                removal: Some(fence.clone()),
            },
        },
    )
    .unwrap();
    let Some(Artifact::Reservation(reservation)) = reserved.artifact else {
        panic!()
    };
    let published = apply(
        &mut s,
        12,
        Mutation::Publish {
            reservation: reservation.id,
            upload_operation: "b".repeat(64),
            etag: r#""later-owned-root""#.into(),
            version: None,
        },
    )
    .unwrap();
    assert_ne!(published.node.payload, initial.node.payload);
    let pending = s
        .namespace_pending_removal(&q.volume, &q.operation)
        .unwrap()
        .unwrap();
    assert_eq!(pending.fence, fence);
    assert_eq!(pending.node, published.node);
    assert_eq!(s.namespace_apply(&q).unwrap(), initial);
    assert_eq!(history(&mut s, &original).unwrap(), original);
    let later = published.node.payload.unwrap();
    assert_eq!(history(&mut s, &later).unwrap(), later);
}
#[test]
fn deferred_canonical_replay_never_rebinds_original_owner_to_a_new_epoch() {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("s"));
    let q = request(&s, 4);
    let initial = s.namespace_apply(&q).unwrap();
    let (_, fence) = artifact(&initial);
    let fence = fence.clone();
    let kept = apply(
        &mut s,
        5,
        Mutation::CancelRemoval {
            node: q.operation.clone(),
            fence: fence.clone(),
        },
    )
    .unwrap();
    let next = apply(
        &mut s,
        6,
        Mutation::PrepareRemoval {
            node: guard(&kept.node),
            parent: root(),
            owner: id(99),
        },
    )
    .unwrap();
    let Some(Artifact::Removal(new_fence)) = next.artifact else {
        panic!()
    };
    assert_ne!(fence, new_fence);
    assert_eq!(s.namespace_apply(&q).unwrap(), initial);
    assert_eq!(
        s.namespace_pending_removal(&q.volume, &q.operation)
            .unwrap()
            .unwrap()
            .fence,
        new_fence
    );
    assert_eq!(
        apply(
            &mut s,
            7,
            Mutation::CancelRemoval {
                node: q.operation.clone(),
                fence
            }
        ),
        Err(Error::Conflict)
    );
    assert_eq!(
        s.namespace_pending_removal(&q.volume, &q.operation)
            .unwrap()
            .unwrap()
            .fence,
        new_fence
    );
}
