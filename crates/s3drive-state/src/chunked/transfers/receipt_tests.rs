//! Synthetic historical receipts only; no provider capability or file flush.
use super::{tests::*, *};

fn prepare(s: &mut State, size: u64, tail: &[u8]) -> (Input, GenerationId) {
    let i = Input {
        id: [7; 32],
        volume: [1; 16],
        node: [2; 16],
        store: [3; 16],
        generation: [4; 16],
        root: "managed/".into(),
        source: [8; 32],
        fingerprint: "a".repeat(64),
        parent: None,
        size,
        expected_content: Some([9; 32]),
    };
    Journal::new(s, Options::default())
        .unwrap()
        .freeze(i.clone())
        .unwrap();
    let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
    let g = j
        .begin_operation(
            &i.staging_operation().unwrap(),
            &i.local_target().unwrap(),
            journal::Base::Missing,
            None,
            size,
        )
        .unwrap();
    let zeros = size.checked_sub(tail.len() as u64).unwrap();
    if zeros != 0 {
        j.append_zero_at(g, 0, zeros).unwrap();
    }
    if !tail.is_empty() {
        j.append_at(g, zeros, tail).unwrap();
    }
    j.seal(g).unwrap();
    let mut p = Journal::new(s, Options::default()).unwrap();
    let mut info = p.attach(i.id, g).unwrap();
    while info.root().is_none() {
        info = p.step(i.id, info.cursor()).unwrap();
    }
    (i, g)
}
fn complete(s: &mut State, i: &Input) -> Vec<Transfer> {
    let info = Journal::new(s, Options::default())
        .unwrap()
        .info(i.id)
        .unwrap();
    let mut objects = Vec::new();
    for level in -1..=info.root_level() {
        for slot in 0..info.object_slots(level).unwrap() {
            let transfer = Journal::new(s, Options::default())
                .unwrap()
                .materialize(i.id, level, slot, journal::Options::default())
                .unwrap();
            if let Some(t) = transfer {
                acknowledge(s, &t);
                objects.push(t);
            }
        }
    }
    objects
}
fn changes(s: &State) -> i64 {
    s.connection
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap()
}
fn remove_ack(s: &State, generation: GenerationId) {
    s.connection
        .execute(
            "DELETE FROM write_settlements WHERE generation_id=?1",
            [generation.value()],
        )
        .unwrap();
    s.connection
        .execute(
            "DELETE FROM remote_flushes WHERE generation_id=?1",
            [generation.value()],
        )
        .unwrap();
}
#[test]
fn complete_tree_report_binds_empty_holes_binary_snapshot_and_all_receipts_without_writes() {
    for (size, tail, objects, holes) in [
        (0, b"".as_slice(), 1, 0),
        (17, b"".as_slice(), 1, 1),
        (
            manifest::CHUNK_BYTES + 64,
            "binary\0 café 日本語 🦀".as_bytes(),
            2,
            1,
        ),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let (i, g) = prepare(&mut s, size, tail);
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .tree_receipt(i.id)
                .is_err()
        );
        let all = complete(&mut s, &i);
        assert_eq!(all.len(), objects);
        let manifest = journal::Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .info(g)
            .unwrap()
            .manifest_hash
            .unwrap();
        let before = changes(&s);
        let r = Journal::new(&mut s, Options::default())
            .unwrap()
            .tree_receipt(i.id)
            .unwrap();
        assert_eq!(changes(&s), before);
        assert_eq!(r.plan().input(), &i);
        assert_eq!(r.snapshot(), g);
        assert_eq!(*r.snapshot_manifest(), manifest);
        assert_eq!(r.root(), r.plan().root().unwrap());
        assert_eq!(r.verified_objects(), objects as u32);
        assert_eq!(r.planned_hole_chunks(), holes);
        assert_eq!(
            r.root_receipt().info().generation,
            all.last().unwrap().generation.unwrap()
        );
        assert!(
            remote::Journal::new(&mut s, remote::Options::default())
                .unwrap()
                .flush_receipt(g)
                .unwrap()
                .is_none()
        );
    }
}
#[test]
fn deleted_grandchild_ack_is_refused_while_direct_root_dependencies_and_root_receipt_remain_valid()
{
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let (i, original) = prepare(&mut s, 128 * manifest::CHUNK_BYTES + 1, b"z");
    let objects = complete(&mut s, &i);
    assert_eq!(objects.len(), 4);
    let report = Journal::new(&mut s, Options::default())
        .unwrap()
        .tree_receipt(i.id)
        .unwrap();
    assert_eq!(report.verified_objects(), 4);
    assert_eq!(report.planned_hole_chunks(), 128);
    let root = report.root_receipt().info().generation;
    let child = objects
        .iter()
        .find(|t| t.role() == Role::Data)
        .unwrap()
        .generation
        .unwrap();
    remove_ack(&s, child);
    let before = changes(&s);
    // The remaining forest is valid; this is not an orphan-detection test.
    assert!(
        remote::Journal::new(&mut s, remote::Options::default())
            .unwrap()
            .flush_receipt(root)
            .unwrap()
            .is_some()
    );
    Journal::new(&mut s, Options::default())
        .unwrap()
        .require_dependencies(i.id, 1, 0)
        .unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .tree_receipt(i.id)
            .is_err()
    );
    assert_eq!(changes(&s), before);
    assert!(
        remote::Journal::new(&mut s, remote::Options::default())
            .unwrap()
            .flush_receipt(original)
            .unwrap()
            .is_none()
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .tree_receipt(i.id)
            .is_err()
    );
    assert!(
        remote::Journal::new(&mut s, remote::Options::default())
            .unwrap()
            .flush_receipt(root)
            .unwrap()
            .is_some()
    );
    // Exact historical verification can be acknowledged again, without a PUT.
    let op = remote::Journal::new(&mut s, remote::Options::default())
        .unwrap()
        .for_generation(child)
        .unwrap()
        .unwrap()
        .operation;
    remote::Journal::new(&mut s, remote::Options::default())
        .unwrap()
        .acknowledge(op)
        .unwrap();
    let r = Journal::new(&mut s, Options::default())
        .unwrap()
        .tree_receipt(i.id)
        .unwrap();
    assert_eq!(r.snapshot(), original);
    assert_eq!(r.root_receipt().info().generation, root);
}
#[test]
fn missing_corrupt_or_redirected_historical_evidence_is_retained_and_refused() {
    for fault in 0..6 {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let (i, _) = setup(&mut s);
        let objects = complete(&mut s, &i);
        let data = &objects[0];
        match fault {
            0 => remove_ack(&s, data.generation.unwrap()),
            1 => {
                s.connection
                    .execute("UPDATE remote_flushes SET receipt_hash=zeroblob(32)", [])
                    .unwrap();
            }
            2 => {
                s.connection
                    .execute("UPDATE remote_intents SET payload_sha256=zeroblob(32)", [])
                    .unwrap();
            }
            3 => {
                s.connection
                    .execute(
                        "DELETE FROM staged_operations WHERE operation_key=?1",
                        [data.operation.as_slice()],
                    )
                    .unwrap();
            }
            4 => {
                let foreign = journal::Journal::new(&mut s, journal::Options::default())
                    .unwrap()
                    .begin(
                        &Target::new(i.source, &i.fingerprint, "foreign").unwrap(),
                        journal::Base::Missing,
                        None,
                        5,
                    )
                    .unwrap();
                s.connection
                    .execute(
                        "UPDATE staged_operations SET generation_id=?1 WHERE operation_key=?2",
                        params![foreign.value(), data.operation.as_slice()],
                    )
                    .unwrap();
            }
            _ => {
                s.connection
                    .execute(
                        "UPDATE chunk_objects SET sha256=zeroblob(32) WHERE level=-1",
                        [],
                    )
                    .unwrap();
            }
        }
        let before = changes(&s);
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .tree_receipt(i.id)
                .is_err(),
            "fault {fault}"
        );
        assert_eq!(changes(&s), before);
        assert_eq!(
            s.connection
                .query_row("SELECT count(*) FROM staged_generations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            if fault == 4 { 4 } else { 3 }
        );
    }
}
#[test]
fn historical_report_preserves_newer_dirty_heads_and_does_not_verify_current_payload_media() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let (i, original) = setup(&mut s);
    complete(&mut s, &i);
    let target = i.local_target().unwrap();
    let mut staged = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
    let newer = staged
        .begin(&target, journal::Base::Missing, Some(original), 5)
        .unwrap();
    staged.append(newer, b"newer").unwrap();
    staged.seal(newer).unwrap();
    // Historical receipt inspection must not read or claim current payload integrity.
    s.connection
        .execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    let before = changes(&s);
    let r = Journal::new(&mut s, Options::default())
        .unwrap()
        .tree_receipt(i.id)
        .unwrap();
    assert_eq!(changes(&s), before);
    assert_eq!(r.snapshot(), original);
    let staged = journal::Journal::new(&mut s, journal::Options::default()).unwrap();
    assert!(staged.read(original, 0, 5).is_err());
    assert!(staged.read(newer, 0, 5).is_err());
    assert_eq!(staged.head(&target).unwrap().map(|g| g.id), Some(newer));
    let remote = remote::Journal::new(&mut s, remote::Options::default()).unwrap();
    assert!(remote.flush_receipt(original).unwrap().is_none());
    assert!(remote.flush_receipt(newer).unwrap().is_none());
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .tree_receipt(i.id)
            .unwrap()
            .snapshot(),
        original
    );
}
#[test]
fn metadata_only_report_still_works_at_actual_sqlite_full_without_acknowledging_the_source() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
    let (i, original) = setup(&mut s);
    complete(&mut s, &i);
    s.connection
        .execute_batch("CREATE TABLE pressure(data BLOB)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
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
            Err(e) => {
                assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                break;
            }
        }
    }
    let before = changes(&s);
    let r = Journal::new(&mut s, Options::default())
        .unwrap()
        .tree_receipt(i.id)
        .unwrap();
    assert_eq!(changes(&s), before);
    assert_eq!(r.snapshot(), original);
    assert!(
        remote::Journal::new(&mut s, remote::Options::default())
            .unwrap()
            .flush_receipt(original)
            .unwrap()
            .is_none()
    );
}
