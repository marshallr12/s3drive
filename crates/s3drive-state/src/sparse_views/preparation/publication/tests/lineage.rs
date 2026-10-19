use super::*;
pub(super) fn first(s: &mut State, f: &Fixture) -> Record {
    let i = prepare(s, f, false);
    let p = i.id().unwrap();
    j(s).check(p, 0, Some(&proof(f))).unwrap();
    physical(s, p);
    j(s).begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap()
}
pub(super) fn commit(s: &mut State, r: Record) -> Record {
    let p = r.input.plan;
    let sequence = r.input.reserve.expected_sequence + 1;
    let reserve = ns::Receipt {
        sequence,
        node: r.input.base.clone(),
        artifact: Some(ns::Artifact::Reservation(r.input.reservation().unwrap())),
    };
    j(s).reserved(p, reserve).unwrap();
    let r = j(s).prepare_publish(p, sequence).unwrap();
    let mut receipt = published(&r);
    receipt.sequence = sequence + 1;
    receipt.node.revision = sequence + 1;
    j(s).published(p, receipt).unwrap()
}
pub(super) fn next(
    s: &mut State,
    f: &Fixture,
    parent: Id,
    n: u8,
    offset: u64,
    bytes: Vec<u8>,
) -> Info {
    let v = crate::sparse_views::Journal::new(s, crate::sparse_views::Options::default())
        .unwrap()
        .write([n; 32], parent, offset, &[n])
        .unwrap();
    let i = pj(s)
        .begin(Request {
            operation: [n + 1; 32],
            view: v.id(),
            generation: [n + 1; 16],
            base_root: f.base.root.encode().to_vec(),
            base_page: f.base.page.encode(),
        })
        .unwrap();
    let i = pj(s)
        .append(i.id().unwrap(), i.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    let i = super::super::super::tests::fixture::finish(s, i);
    j(s).check(i.id().unwrap(), 0, Some(&proof(f))).unwrap();
    physical(s, i.id().unwrap());
    i
}
#[test]
fn consecutive_local_descendants_use_exact_prior_payload_and_preserve_newer_heads_and_old_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lineage café");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let a = first(&mut s, &f);
    let old_raw = serde_json::to_vec(&a).unwrap();
    assert!(
        !String::from_utf8(old_raw.clone())
            .unwrap()
            .contains("predecessor")
    );
    let a = commit(&mut s, a);
    let old_done = serde_json::to_vec(&a).unwrap();
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    bytes[6] = 20;
    let b = next(&mut s, &f, a.input.view, 20, 6, bytes.clone());
    let mut base = a.receipt().unwrap().node.clone();
    base.name = "renamed 日本語 🦀".into();
    base.revision += 1;
    let b = j(&mut s)
        .begin(
            b.id().unwrap(),
            base.clone(),
            f.store.clone(),
            base.revision,
            hex(&[22; 16]),
        )
        .unwrap();
    assert_eq!(b.format, 2);
    assert_eq!(b.input.predecessor(), Some(a.input.plan));
    assert_eq!(
        b.input.reservation().unwrap().guard.unwrap().expected,
        ns::payload_reference(&a.receipt().unwrap().node.payload)
    );
    bytes[7] = 30;
    let c = next(&mut s, &f, b.input.view, 30, 7, bytes);
    let b = commit(&mut s, b);
    let head = c.view().clone();
    assert_eq!(
        crate::sparse_views::Journal::new(&mut s, crate::sparse_views::Options::default())
            .unwrap()
            .head(&f.binding)
            .unwrap(),
        head
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        serde_json::to_vec(&j(&mut s).get(a.input.plan).unwrap()).unwrap(),
        old_done
    );
    assert_eq!(j(&mut s).get(b.input.plan).unwrap(), b);
    let c = j(&mut s)
        .begin(
            c.id().unwrap(),
            b.receipt().unwrap().node.clone(),
            f.store.clone(),
            b.receipt().unwrap().sequence,
            hex(&[32; 16]),
        )
        .unwrap();
    assert_eq!(c.input.predecessor, Some(b.input.plan));
    let c = commit(&mut s, c);
    assert_eq!(c.input.view, head.id());
    assert!(
        j(&mut s)
            .check_base(a.input.plan, &c.receipt().unwrap().node)
            .is_err(),
        "a newer local publication is not an ancestor of an older view"
    );
    assert_eq!(j(&mut s).records().unwrap().len(), 3);
    assert_eq!(
        crate::sparse_views::Journal::new(&mut s, crate::sparse_views::Options::default())
            .unwrap()
            .head(&f.binding)
            .unwrap(),
        head
    );
}
#[test]
fn unknown_foreign_nonancestor_and_changed_identity_never_authorize_a_new_request() {
    let temp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&temp.path().join("refusals")).unwrap();
    let f = fixture();
    let a = first(&mut s, &f);
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    bytes[6] = 20;
    let b = next(&mut s, &f, a.input.view, 20, 6, bytes);
    let p = b.id().unwrap();
    let simulated = published(&a).node;
    assert!(
        j(&mut s).check_base(p, &simulated).is_err(),
        "unknown result adopted"
    );
    let a = commit(&mut s, a);
    let node = a.receipt().unwrap().node.clone();
    j(&mut s).check_base(p, &node).unwrap();
    assert!(
        j(&mut s).check_base(a.input.plan, &node).is_err(),
        "same view is not strict descendant"
    );
    for n in 0..5 {
        let mut wrong = node.clone();
        match n {
            0 => wrong.payload.as_mut().unwrap().etag = "\"foreign\"".into(),
            1 => wrong.id = hex(&[99; 16]),
            2 => wrong.deleted = true,
            3 => wrong.revision = a.receipt().unwrap().sequence - 1,
            _ => wrong.payload.as_mut().unwrap().reservation.store.route = "c".repeat(64),
        }
        let before = ledger::inspect(&s.connection).unwrap();
        assert!(
            j(&mut s)
                .begin(p, wrong, f.store.clone(), 10, hex(&[22; 16]))
                .is_err()
        );
        assert!(ledger::inspect(&s.connection).unwrap() == before);
    }
    // Saved exact requests do not become permission to rebase the same plan.
    let before = ledger::inspect(&s.connection).unwrap();
    s.connection.execute_batch("CREATE TRIGGER skip_lineage BEFORE INSERT ON sparse_publications BEGIN SELECT RAISE(IGNORE); END;").unwrap();
    assert!(
        j(&mut s)
            .begin(p, node.clone(), f.store.clone(), 7, hex(&[22; 16]))
            .is_err()
    );
    s.connection
        .execute_batch("DROP TRIGGER skip_lineage")
        .unwrap();
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    let limits = Limits {
        plans: 1,
        ..Limits::default()
    };
    assert!(
        Journal::new(&mut s, limits)
            .unwrap()
            .begin(p, node.clone(), f.store.clone(), 7, hex(&[22; 16]))
            .is_err()
    );
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    let b = j(&mut s)
        .begin(p, node.clone(), f.store.clone(), 7, hex(&[22; 16]))
        .unwrap();
    let mut other = node;
    other.revision += 1;
    assert!(
        j(&mut s)
            .begin(p, other, f.store.clone(), 8, hex(&[22; 16]))
            .is_err()
    );
    assert_eq!(j(&mut s).get(p).unwrap(), b);
}
#[test]
fn schema_thirty_four_keeps_literal_old_publication_blobs_hashes_and_requests() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    let capture = |db: &Connection| -> Vec<(Vec<u8>, Vec<u8>)> {
        db.prepare("SELECT record,state_hash FROM sparse_publications ORDER BY plan_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    let before = capture(&s.connection);
    assert!(
        !String::from_utf8(before[0].0.clone())
            .unwrap()
            .contains("predecessor")
    );
    s.connection
        .execute_batch("DROP TABLE sparse_scoped_mode")
        .unwrap();
    s.connection
        .pragma_update(None, "user_version", 33)
        .unwrap();
    drop(s);
    // Recover old exact metadata without migrating or opening source BLOBs.
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.get(r.input.plan).unwrap(), r);
    drop(recovery);
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        33
    );
    drop(db);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
    assert_eq!(capture(&s.connection), before);
    assert_eq!(j(&mut s).get(r.input.plan).unwrap(), r);
}
#[test]
fn retained_predecessor_metadata_is_required_for_recovery_even_when_source_is_damaged() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("recover");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let a = first(&mut s, &f);
    let a = commit(&mut s, a);
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    bytes[6] = 20;
    let b = next(&mut s, &f, a.input.view, 20, 6, bytes);
    let b = j(&mut s)
        .begin(
            b.id().unwrap(),
            a.receipt().unwrap().node.clone(),
            f.store.clone(),
            7,
            hex(&[22; 16]),
        )
        .unwrap();
    // A self-consistently hashed bad predecessor edge is still rejected.
    let before = ledger::inspect(&s.connection).unwrap();
    let mut bad = before.clone();
    bad.records
        .get_mut(&b.input.plan)
        .unwrap()
        .input
        .predecessor = Some([99; 32]);
    assert!(bad.validate().is_err());
    let mut bad = before.clone();
    bad.records
        .get_mut(&b.input.plan)
        .unwrap()
        .input
        .predecessor = Some(b.input.plan);
    assert!(bad.validate().is_err());
    let mut bad = before.clone();
    bad.records.get_mut(&a.input.plan).unwrap().receipt = None;
    assert!(bad.validate().is_err());
    s.connection
        .execute(
            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
            [],
        )
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.get(b.input.plan).unwrap(), b);
    let sequence = 8;
    let reserve = ns::Receipt {
        sequence,
        node: b.input.base.clone(),
        artifact: Some(ns::Artifact::Reservation(b.input.reservation().unwrap())),
    };
    assert!(
        recovery
            .reserved(b.input.plan, reserve)
            .unwrap()
            .reservation()
            .is_some()
    );
    assert!(recovery.get(a.input.plan).unwrap().receipt().is_some());
}

#[test]
fn schema_thirty_three_pending_publish_recovers_without_source_or_migration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("old recovery");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let p = r.input.plan;
    let r = j(&mut s).reserved(p, reserved(&r)).unwrap();
    let r = j(&mut s)
        .prepare_publish(p, r.reservation().unwrap().sequence)
        .unwrap();
    s.connection
        .execute(
            "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
            [],
        )
        .unwrap();
    s.connection
        .execute_batch("DROP TABLE sparse_scoped_mode")
        .unwrap();
    s.connection
        .pragma_update(None, "user_version", 33)
        .unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.get(p).unwrap(), r);
    let done = recovery.published(p, published(&r)).unwrap();
    assert_eq!(done.publication(), r.publication());
    assert_eq!(recovery.published(p, published(&r)).unwrap(), done);
    drop(recovery);
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        33
    );
    let bad: i64 = db
        .query_row(
            "SELECT count(*) FROM sparse_view_blocks WHERE data=zeroblob(length(data))",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(bad > 0);
}
