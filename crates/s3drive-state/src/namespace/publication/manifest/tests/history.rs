use super::*;

fn published(s: &mut State, n: &Node, reserve: u8, publication: u8) -> Payload {
    let req = request(s, n, reserve, normal(n));
    let r = reservation(s, &req);
    let p = publish(s, &r, publication);
    s.namespace_apply(&p).unwrap().node.payload.unwrap()
}
fn get(s: &mut State, p: &Payload) -> Result<Payload> {
    s.namespace_manifest_payload(
        &p.reservation.volume,
        &p.reservation.node,
        &p.reservation.id,
        &payload_reference(&Some(p.clone())).unwrap(),
    )
}

#[test]
fn manifest_history_retains_exact_base_after_replacement_rename_removal_and_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("authority");
    let mut s = State::open_managed(&path).unwrap();
    let n = setup(&mut s);
    let p = published(&mut s, &n, 10, 11);
    assert_eq!(get(&mut s, &p).unwrap(), p);
    let live = s.namespace_node(&p.reservation.volume, &n.id).unwrap();
    let newer = published(&mut s, &live, 12, 13);
    assert_ne!(newer, p);
    apply(
        &mut s,
        14,
        Mutation::Move {
            node: n.id.clone(),
            parent: ROOT.into(),
            name: "renamed history 日本語 🦀".into(),
        },
    );
    apply(&mut s, 15, Mutation::Remove { node: n.id });
    let volume = s.namespace_volume().unwrap();
    assert_eq!(get(&mut s, &p).unwrap(), p);
    assert_eq!(get(&mut s, &newer).unwrap(), newer);
    assert_eq!(s.namespace_volume().unwrap(), volume);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(get(&mut s, &p).unwrap(), p);
    assert_eq!(get(&mut s, &newer).unwrap(), newer);
    assert_eq!(s.namespace_volume().unwrap(), volume);
}

#[test]
fn manifest_history_refuses_unknown_unpublished_wrong_bindings_and_legacy_without_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
    let n = setup(&mut s);
    let req = request(&s, &n, 10, normal(&n));
    let r = reservation(&mut s, &req);
    let arbitrary = "0".repeat(64);
    assert_eq!(
        s.namespace_manifest_payload(&r.volume, &r.node, &r.id, &arbitrary),
        Err(Error::NotFound)
    );
    let pubreq = publish(&mut s, &r, 11);
    let p = s.namespace_apply(&pubreq).unwrap().node.payload.unwrap();
    let reference = payload_reference(&Some(p.clone())).unwrap();
    let volume = s.namespace_volume().unwrap();
    assert_eq!(
        s.namespace_manifest_payload(&id(90), &r.node, &r.id, &reference),
        Err(Error::WrongVolume)
    );
    assert_eq!(
        s.namespace_manifest_payload(&r.volume, &id(90), &r.id, &reference),
        Err(Error::NotFound)
    );
    assert_eq!(
        s.namespace_manifest_payload(&r.volume, &r.node, &id(90), &reference),
        Err(Error::NotFound)
    );
    assert_eq!(
        s.namespace_manifest_payload(&r.volume, &r.node, &r.id, &arbitrary),
        Err(Error::Conflict)
    );
    for value in ["", ROOT, "../../other", &"x".repeat(8192)] {
        assert_eq!(
            s.namespace_manifest_payload(&r.volume, value, &r.id, &reference),
            Err(Error::Invalid)
        );
        assert_eq!(
            s.namespace_manifest_payload(&r.volume, &r.node, value, &reference),
            Err(Error::Invalid)
        );
    }
    for value in ["", "00", &"A".repeat(64), &"a".repeat(65)] {
        assert_eq!(
            s.namespace_manifest_payload(&r.volume, &r.node, &r.id, value),
            Err(Error::Invalid)
        );
    }
    assert_eq!(s.namespace_volume().unwrap(), volume);
    let live = s.namespace_node(&r.volume, &r.node).unwrap();
    let Some(Artifact::Reservation(legacy)) = apply(
        &mut s,
        20,
        Mutation::ReserveContent {
            node: live.id,
            expected_revision: live.revision,
            store: id(3),
            size: 3,
            sha256: "a".repeat(64),
            expected_payload: payload_reference(&live.payload),
        },
    )
    .artifact
    else {
        panic!("legacy reservation")
    };
    let req = publish(&mut s, &legacy, 21);
    let legacy = s.namespace_apply(&req).unwrap().node.payload.unwrap();
    assert_eq!(get(&mut s, &legacy), Err(Error::Invalid));
    assert_eq!(get(&mut s, &p).unwrap(), p);
}

fn records(s: &State) -> [Option<Vec<u8>>; 5] {
    s.connection
        .query_row(
            "SELECT (SELECT descriptor FROM namespace_uploads WHERE reservation_id=?1),
        (SELECT publication FROM namespace_uploads WHERE reservation_id=?1),
        (SELECT request FROM namespace_operations WHERE operation_id=?1),
        (SELECT outcome FROM namespace_operations WHERE operation_id=?1),
        (SELECT descriptor FROM namespace_stores WHERE store_id=?2)",
            params![id(10), id(3)],
            |row| {
                Ok([
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ])
            },
        )
        .unwrap()
}

#[test]
fn manifest_history_never_repairs_corrupt_or_substituted_retained_evidence() {
    for fault in 0..17 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("authority")).unwrap();
        let n = setup(&mut s);
        let p = published(&mut s, &n, 10, 11);
        let mut expected = Error::Internal;
        match fault {
            0 => {
                s.connection
                    .execute("UPDATE namespace_uploads SET publication=x'00'", [])
                    .unwrap();
            }
            1 => {
                s.connection
                    .execute("UPDATE namespace_uploads SET publication=NULL", [])
                    .unwrap();
                expected = Error::NotFound;
            }
            2 => {
                let mut bad = p.reservation.clone();
                bad.key.push('x');
                s.connection
                    .execute(
                        "UPDATE namespace_uploads SET descriptor=?1",
                        [encode(&bad).unwrap()],
                    )
                    .unwrap();
            }
            3 | 9 => {
                let mut bad = p.clone();
                if fault == 3 {
                    bad.etag = "\"another-valid-etag\"".into();
                } else {
                    bad.upload_operation = "f".repeat(64);
                }
                assert!(bad.valid());
                s.connection
                    .execute(
                        "UPDATE namespace_uploads SET publication=?1",
                        [encode(&bad).unwrap()],
                    )
                    .unwrap();
                expected = Error::Conflict;
            }
            4 => {
                let mut bad = p.clone();
                bad.reservation.node = id(90);
                s.connection
                    .execute(
                        "UPDATE namespace_uploads SET publication=?1",
                        [encode(&bad).unwrap()],
                    )
                    .unwrap();
            }
            5 => {
                s.connection
                    .execute(
                        "DELETE FROM namespace_operations WHERE operation_id=?1",
                        [id(10)],
                    )
                    .unwrap();
            }
            6 => {
                let mut bad = s
                    .namespace_operation(&p.reservation.volume, &id(10))
                    .unwrap()
                    .unwrap()
                    .unwrap();
                if let Some(Artifact::Reservation(r)) = &mut bad.artifact {
                    r.expected_revision += 1;
                }
                let encoded = encode(&Ok::<_, Error>(bad)).unwrap();
                s.connection
                    .execute(
                        "UPDATE namespace_operations SET outcome=?1 WHERE operation_id=?2",
                        params![encoded, id(10)],
                    )
                    .unwrap();
            }
            7 | 10 => {
                let raw: Vec<u8> = s
                    .connection
                    .query_row(
                        "SELECT request FROM namespace_operations WHERE operation_id=?1",
                        [id(10)],
                        |r| r.get(0),
                    )
                    .unwrap();
                let mut bad: Apply = decode(raw).unwrap();
                if fault == 7 {
                    bad.expected_sequence = u64::MAX;
                } else {
                    bad.mutation = Mutation::Remove { node: n.id };
                }
                s.connection
                    .execute(
                        "UPDATE namespace_operations SET request=?1 WHERE operation_id=?2",
                        params![encode(&bad).unwrap(), id(10)],
                    )
                    .unwrap();
            }
            8 => {
                let mut bad = p.reservation.store.clone();
                bad.route = "b".repeat(64);
                s.connection
                    .execute(
                        "UPDATE namespace_stores SET descriptor=?1",
                        [encode(&bad).unwrap()],
                    )
                    .unwrap();
            }
            11 => {
                s.connection
                    .execute("UPDATE namespace_volume SET sequence=1", [])
                    .unwrap();
            }
            _ => {
                let (table, column, key, value) = match fault {
                    12 => ("namespace_uploads", "publication", "reservation_id", id(10)),
                    13 => ("namespace_uploads", "descriptor", "reservation_id", id(10)),
                    14 => ("namespace_operations", "outcome", "operation_id", id(10)),
                    15 => ("namespace_operations", "request", "operation_id", id(10)),
                    _ => ("namespace_stores", "descriptor", "store_id", id(3)),
                };
                let mut raw: Vec<u8> = s
                    .connection
                    .query_row(
                        &format!("SELECT {column} FROM {table} WHERE {key}=?1"),
                        [&value],
                        |row| row.get(0),
                    )
                    .unwrap();
                raw.resize(16 * 1024 + 1, b' ');
                s.connection
                    .execute(
                        &format!("UPDATE {table} SET {column}=?1 WHERE {key}=?2"),
                        params![raw, value],
                    )
                    .unwrap();
            }
        }
        let before = records(&s);
        let volume = s.namespace_volume().unwrap();
        assert_eq!(get(&mut s, &p), Err(expected), "fault{fault}");
        assert_eq!(
            records(&s),
            before,
            "history was silently repaired: {fault}"
        );
        assert_eq!(s.namespace_volume().unwrap(), volume);
    }
}
