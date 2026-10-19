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
        mutation: Mutation::CreateEmptyManifest {
            parent: ROOT.into(),
            parent_revision: 0,
            name: "café 日本語 🦀.bin".into(),
            store: id(3),
            upload_operation: "a".repeat(64),
            etag: "\"verified-model\"".into(),
            version: None,
        },
    }
}
fn rows(s: &State) -> Vec<Vec<String>> {
    ["SELECT quote(volume_id)||quote(sequence)||quote(name_policy) FROM namespace_volume",
     "SELECT quote(node_id)||quote(parent_id)||quote(name)||quote(kind)||quote(revision)||quote(deleted)||quote(name_key)||quote(payload)||quote(placement) FROM namespace_nodes ORDER BY node_id",
     "SELECT quote(reservation_id)||quote(descriptor)||quote(publication) FROM namespace_uploads ORDER BY reservation_id",
     "SELECT quote(operation_id)||quote(request)||quote(outcome) FROM namespace_operations ORDER BY operation_id"]
        .iter().map(|q|s.connection.prepare(q).unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap()).collect()
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
fn atomic_empty_creation_derives_real_canonical_history_and_replays_after_rename_removal_restart() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("authority café 🦀");
    let mut s = setup(&path);
    let q = request(&s, 4);
    let r = s.namespace_apply(&q).unwrap();
    assert_eq!(r.node.kind, Kind::File);
    assert_eq!(r.node.id, q.operation);
    let p = r.node.payload.clone().unwrap();
    assert_eq!(r.artifact, Some(Artifact::Published(p.clone())));
    assert_eq!(p.reservation.size, 64);
    assert_eq!(p.reservation.expected_revision, r.sequence);
    let m = p.reservation.manifest.as_ref().unwrap().root().unwrap();
    assert_eq!(m.length(), 0);
    assert_eq!(m.page().generation(), m.node());
    let (canonical, body) =
        RootDescriptor::empty(*m.volume(), *m.node(), *m.page().store(), *m.node()).unwrap();
    assert_eq!(m, canonical);
    assert_ne!(hex(&Sha256::digest(&body)), hex(m.page().sha256()));
    assert_eq!(p.reservation.sha256, hex(&Sha256::digest(&body)));
    assert_eq!(history(&mut s, &p).unwrap(), p);
    let v = s.namespace_volume().unwrap();
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: id(5),
        expected_sequence: v.sequence,
        mutation: Mutation::Move {
            node: q.operation.clone(),
            parent: ROOT.into(),
            name: "renamed café 🦀".into(),
        },
    })
    .unwrap();
    let v = s.namespace_volume().unwrap();
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: id(6),
        expected_sequence: v.sequence,
        mutation: Mutation::Remove {
            node: q.operation.clone(),
        },
    })
    .unwrap();
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
    assert_eq!(
        s.namespace_operation(&q.volume, &q.operation).unwrap(),
        Some(Ok(r.clone()))
    );
    assert_eq!(history(&mut s, &p).unwrap(), p);
    let before = rows(&s);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
    assert_eq!(history(&mut s, &p).unwrap(), p);
    assert_eq!(rows(&s), before);
}
fn hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}
#[test]
fn atomic_empty_collision_rejections_and_request_reuse_never_leave_partial_history() {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("s"));
    let q = request(&s, 4);
    let r = s.namespace_apply(&q).unwrap();
    let mut rival = q.clone();
    rival.operation = id(5);
    assert_eq!(s.namespace_apply(&rival), Err(Error::AlreadyExists));
    assert_eq!(
        s.namespace_reservation(&q.volume, &id(5)),
        Err(Error::NotFound)
    );
    assert_eq!(s.namespace_node(&q.volume, &id(5)), Err(Error::NotFound));
    let before = rows(&s);
    let mut wrong = q.clone();
    if let Mutation::CreateEmptyManifest { name, .. } = &mut wrong.mutation {
        *name = "different".into();
    }
    assert_eq!(s.namespace_apply(&wrong), Err(Error::OperationMismatch));
    assert_eq!(rows(&s), before);
    assert_eq!(s.namespace_apply(&q).unwrap(), r);
    for case in 0..7 {
        let t = tempfile::tempdir().unwrap();
        let mut s = setup(&t.path().join("s"));
        let mut q = request(&s, 4);
        let v = s.namespace_volume().unwrap();
        if let Mutation::CreateEmptyManifest {
            parent,
            parent_revision,
            name,
            store,
            upload_operation,
            etag,
            version,
        } = &mut q.mutation
        {
            match case {
                0 => *parent_revision = 1,
                1 => *parent = id(99),
                2 => *name = String::new(),
                3 => *store = id(99),
                4 => *upload_operation = "invalid".into(),
                5 => *etag = String::new(),
                6 => *version = Some("null".into()),
                _ => unreachable!(),
            }
        }
        assert!(s.namespace_apply(&q).is_err(), "case {case}");
        assert_eq!(s.namespace_volume().unwrap(), v);
        assert_eq!(
            s.namespace_node(&q.volume, &q.operation),
            Err(Error::NotFound)
        );
        assert_eq!(
            s.namespace_reservation(&q.volume, &q.operation),
            Err(Error::NotFound)
        );
    }
}
#[test]
fn atomic_empty_guarded_parent_allows_unrelated_sequence_but_refuses_future_sequence() {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("s"));
    let q = request(&s, 4);
    let v = s.namespace_volume().unwrap();
    s.namespace_apply(&Apply {
        volume: v.id,
        operation: id(8),
        expected_sequence: v.sequence,
        mutation: Mutation::CreateEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: None,
            name: "unrelated".into(),
            node_kind: Kind::Directory,
        },
    })
    .unwrap();
    assert_eq!(s.namespace_apply(&q).unwrap().sequence, 3);
    let mut future = request(&s, 9);
    future.expected_sequence += 1;
    assert_eq!(s.namespace_apply(&future), Err(Error::Conflict));
    assert_eq!(
        s.namespace_node(&future.volume, &future.operation),
        Err(Error::NotFound)
    );
}
#[test]
fn atomic_empty_pre39_marker_preserves_literal_rows_and_refuses_new_record_laundering() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let s = setup(&path);
    let before = rows(&s);
    s.connection
        .pragma_update(None, "user_version", 38)
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, 39);
    assert_eq!(rows(&s), before);
    let q = request(&s, 4);
    s.namespace_apply(&q).unwrap();
    let before = rows(&s);
    s.connection
        .pragma_update(None, "user_version", 38)
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        38
    );
    let state = State {
        connection: db,
        _directory: None,
        incarnation: None,
        source_pin_witness: None,
    };
    assert_eq!(rows(&state), before);
}
#[test]
fn atomic_empty_triggers_corrupt_history_and_bad_name_key_refuse_without_repair() {
    for case in 0..5 {
        let t = tempfile::tempdir().unwrap();
        let mut s = setup(&t.path().join("s"));
        let q = request(&s, 4);
        let r = s.namespace_apply(&q).unwrap();
        match case {
            0 => {
                s.connection.execute_batch("CREATE TRIGGER drop_effect AFTER INSERT ON namespace_operations BEGIN DELETE FROM namespace_uploads; END").unwrap();
            }
            1 => {
                s.connection
                    .execute(
                        "UPDATE namespace_uploads SET publication=x'7b7d' WHERE reservation_id=?1",
                        [&q.operation],
                    )
                    .unwrap();
            }
            2 => {
                s.connection
                    .execute(
                        "UPDATE namespace_uploads SET descriptor=x'7b7d' WHERE reservation_id=?1",
                        [&q.operation],
                    )
                    .unwrap();
            }
            3 => {
                let mut bad = r.clone();
                bad.node.name = "changed".into();
                s.connection
                    .execute(
                        "UPDATE namespace_operations SET outcome=?1 WHERE operation_id=?2",
                        params![
                            serde_json::to_vec(&Ok::<_, Error>(bad)).unwrap(),
                            q.operation
                        ],
                    )
                    .unwrap();
            }
            4 => {
                s.connection
                    .execute(
                        "UPDATE namespace_nodes SET name_key='wrong' WHERE node_id=?1",
                        [&q.operation],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let before = rows(&s);
        if case == 4 {
            let mut next = request(&s, 5);
            if let Mutation::CreateEmptyManifest { name, .. } = &mut next.mutation {
                *name = "new".into();
            }
            assert_eq!(s.namespace_apply(&next), Err(Error::Internal));
        } else {
            assert!(s.namespace_apply(&q).is_err());
        }
        assert_eq!(rows(&s), before);
    }
}
#[test]
fn atomic_empty_real_sqlite_full_rolls_back_node_history_and_outcome_then_retry_succeeds() {
    let t = tempfile::tempdir().unwrap();
    let mut s = setup(&t.path().join("full"));
    s.connection.execute_batch("CREATE TABLE pressure(bytes BLOB); PRAGMA journal_mode=DELETE; PRAGMA page_size=512; VACUUM; PRAGMA journal_mode=WAL").unwrap();
    let original: i64 = s
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
        let mut q = request(&s, n);
        if let Mutation::CreateEmptyManifest { name, .. } = &mut q.mutation {
            *name = format!("full {n}");
        }
        let before = rows(&s);
        match s.namespace_apply(&q) {
            Ok(_) => (),
            Err(e) => {
                assert_eq!(e, Error::Internal);
                assert_eq!(rows(&s), before);
                failed = Some(q);
                break;
            }
        }
    }
    let q = failed.expect("empty creation never reached SQL full");
    s.connection
        .pragma_update(None, "max_page_count", original)
        .unwrap();
    let r = s.namespace_apply(&q).unwrap();
    assert_eq!(
        history(&mut s, r.node.payload.as_ref().unwrap()).unwrap(),
        r.node.payload.unwrap()
    );
}
mod crash;

#[test]
fn atomic_empty_node_upload_and_operation_limits_preserve_all_prior_data() {
    for case in 0..3 {
        let t = tempfile::tempdir().unwrap();
        let mut s = setup(&t.path().join("limited"));
        s.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        let count = match case {
            0 => MAX_NODES - 1,
            1 => 4096,
            _ => MAX_OPERATIONS - 1,
        };
        for n in 0..count {
            let id = format!("{:032x}", 10000 + n);
            match case {
                0 => {
                    let name = format!("retired {n}");
                    s.connection.execute("INSERT INTO namespace_nodes(node_id,parent_id,name,kind,revision,deleted,name_key) VALUES(?1,?2,?3,'empty-file',0,1,?4)",params![id,ROOT,name,names::collision_key(&name).unwrap()]).unwrap();
                }
                1 => {
                    s.connection
                        .execute("INSERT INTO namespace_uploads VALUES(?1,x'00',NULL)", [id])
                        .unwrap();
                }
                _ => {
                    s.connection
                        .execute(
                            "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
                            [id],
                        )
                        .unwrap();
                }
            }
        }
        s.connection.execute_batch("COMMIT").unwrap();
        let q = request(&s, 4);
        let before = rows(&s);
        assert_eq!(s.namespace_apply(&q), Err(Error::Limit));
        let after = rows(&s);
        assert_eq!(after[..3], before[..3]);
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
