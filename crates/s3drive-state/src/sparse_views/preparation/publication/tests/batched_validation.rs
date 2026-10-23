use super::*;
fn two_roots(s: &mut State) -> (Record, Record) {
    let f = fixture();
    let a = lineage::first(s, &f);
    let a = lineage::commit(s, a);
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    bytes[6] = 20;
    let b = lineage::next(s, &f, a.input.view, 20, 6, bytes);
    let node = a.receipt().unwrap().node.clone();
    let b = j(s)
        .begin(
            b.id().unwrap(),
            node.clone(),
            f.store,
            node.revision,
            hex(&[22; 16]),
        )
        .unwrap();
    (a, b)
}
#[test]
fn publication_batch_validates_two_roots_with_one_source_snapshot() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("batch")).unwrap();
    let (a, b) = two_roots(&mut s);
    let before = super::super::super::ledger::inspection_count();
    let rows = j(&mut s).records().unwrap();
    assert_eq!(super::super::super::ledger::inspection_count() - before, 1);
    assert_eq!(rows.len(), 2);
    assert!(rows.contains(&a) && rows.contains(&b));
}
#[test]
fn publication_batch_rejects_unselected_root_physical_corruption_without_repair() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("batch-corrupt")).unwrap();
    let (a, b) = two_roots(&mut s);
    assert_eq!(j(&mut s).get(a.input.plan).unwrap(), a);
    let generation = pj(&mut s)
        .transfers(b.input.plan, None, 128)
        .unwrap()
        .last()
        .unwrap()
        .generation()
        .unwrap();
    assert_eq!(
        s.connection
            .execute(
                "UPDATE remote_flushes SET receipt_hash=zeroblob(32) WHERE generation_id=?",
                [generation.value()]
            )
            .unwrap(),
        1
    );
    assert!(j(&mut s).get(a.input.plan).is_err());
    let hash: Vec<u8> = s
        .connection
        .query_row(
            "SELECT receipt_hash FROM remote_flushes WHERE generation_id=?",
            [generation.value()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hash, vec![0; 32]);
}
#[test]
fn publication_batch_never_reuses_a_proof_across_command_or_late_trigger() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("batch-trigger")).unwrap();
    let _ = two_roots(&mut s);
    let before = j(&mut s).records().unwrap();
    s.connection
        .execute_batch(
            "CREATE TRIGGER batch_probe AFTER UPDATE ON remote_flushes BEGIN SELECT 1; END;",
        )
        .unwrap();
    assert!(j(&mut s).records().is_err());
    s.connection
        .execute_batch("DROP TRIGGER batch_probe")
        .unwrap();
    assert_eq!(j(&mut s).records().unwrap(), before);
}
