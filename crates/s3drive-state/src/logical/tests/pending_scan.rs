//! Read-only scan study: retained data and publication receipts are never pruned.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

fn settled(s: &mut State, n: u64, parent: Option<GenerationId>, base: u64) -> GenerationId {
    let r = stage(s, n, parent, &[n as u8; 32]);
    let mut input = publication(&r, base);
    input.reserve.operation = id(10_000 + n);
    input.publish_operation = id(20_000 + n);
    let prepared = Journal::new(s, journal::Options::default())
        .unwrap()
        .prepare(&r.input.id, input)
        .unwrap();
    physical_commit_at(s, &prepared, false, base + 3);
    Journal::new(s, journal::Options::default())
        .unwrap()
        .acknowledge(&r.input.id)
        .unwrap();
    r.generation.unwrap()
}

// Preserve the old enumeration with current public validators as an independent
// result/cost oracle. Public file() itself now reuses proofs within its call;
// the exact pre-change cost is separately retained in the before-study log.
fn reference(s: &mut State, after: Option<&str>, limit: u16) -> Result<Vec<Record>> {
    if !(1..=1000).contains(&limit) {
        return Err("invalid limit".into());
    }
    let mut j = Journal::new(s, journal::Options::default())?;
    let ids = j
        .state
        .connection
        .prepare("SELECT operation_id FROM logical_edits ORDER BY operation_id LIMIT 4097")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > LIMIT {
        return Err("too many edits".into());
    }
    let mut out = Vec::new();
    for id in ids {
        let r = j.get(&id)?;
        let file = j.file(&r.input.volume, &r.input.node)?;
        let covered = match (r.generation, file.settled) {
            (Some(g), Some(s)) => ancestor(&j.state.connection, &file.binding, g, s)?,
            _ => false,
        };
        if !covered && after.is_none_or(|a| id.as_str() > a) && out.len() < usize::from(limit) {
            out.push(r);
        }
    }
    Ok(out)
}

fn measured(
    s: &mut State,
    scan: impl FnOnce(&mut State) -> Result<Vec<Record>>,
) -> (Vec<Record>, u64, u128) {
    let count = Arc::new(AtomicU64::new(0));
    let observed = count.clone();
    s.connection
        .progress_handler(
            1,
            Some(move || {
                observed.fetch_add(1, Ordering::Relaxed);
                false
            }),
        )
        .unwrap();
    let start = std::time::Instant::now();
    let result = scan(s);
    let elapsed = start.elapsed().as_micros();
    s.connection
        .progress_handler(0, None::<fn() -> bool>)
        .unwrap();
    (result.unwrap(), count.load(Ordering::Relaxed), elapsed)
}

#[test]
#[ignore = "diagnostic retained-history cost study; no wall-clock acceptance threshold"]
fn pending_scan_scaling_study() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let sqlite: String = s
        .connection
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    eprintln!(
        "PENDING_SCAN_ENV os={} arch={} sqlite={sqlite}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let mut parent = None;
    let mut base = 1;
    for n in 1..=32 {
        parent = Some(settled(&mut s, n, parent, base));
        base += 3;
        if [4, 8, 16, 32].contains(&n) {
            let (old, old_ops, old_us) = measured(&mut s, |s| reference(s, None, 1000));
            let (new, new_ops, new_us) = measured(&mut s, |s| {
                Journal::new(s, journal::Options::default())?.pending(None, 1000)
            });
            assert!(old.is_empty() && new.is_empty());
            eprintln!(
                "PENDING_SCAN rows={n} reference_vm_callbacks={old_ops} current_vm_callbacks={new_ops} reference_us={old_us} current_us={new_us}"
            );
        }
    }
}

fn assert_records(a: &[Record], b: &[Record]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.input, b.input);
        assert_eq!(a.generation, b.generation);
        assert_eq!(a.publication, b.publication);
        assert_eq!(a.receipt, b.receipt);
    }
}
fn other_settled(s: &mut State) -> Record {
    let mut b = binding();
    // A second volume with the SAME node ID exercises the complete cache key.
    b.volume = id(998);
    let mut j = Journal::new(s, journal::Options::default()).unwrap();
    j.bind(b.clone()).unwrap();
    let r = j
        .begin(Input::from_bytes(id(100), &b, None, b"other volume"))
        .unwrap();
    journal::Journal::new(s, journal::Options::default())
        .unwrap()
        .append(r.generation.unwrap(), b"other volume")
        .unwrap();
    let r = Journal::new(s, journal::Options::default())
        .unwrap()
        .seal(&r.input.id)
        .unwrap();
    let mut input = publication(&r, 1);
    input.reserve.volume = b.volume;
    input.reserve.operation = id(10_100);
    input.publish_operation = id(20_100);
    let prepared = Journal::new(s, journal::Options::default())
        .unwrap()
        .prepare(&r.input.id, input)
        .unwrap();
    physical_commit_at(s, &prepared, false, 4);
    Journal::new(s, journal::Options::default())
        .unwrap()
        .acknowledge(&r.input.id)
        .unwrap()
}

fn unrelated_physical_flush(s: &mut State) -> GenerationId {
    let target = Target::new([9; 32], &"c".repeat(64), "unrelated/physical").unwrap();
    let mut j = journal::Journal::new(s, journal::Options::default()).unwrap();
    let g = j.begin(&target, journal::Base::Missing, None, 3).unwrap();
    j.append(g, b"raw").unwrap();
    j.seal(g).unwrap();
    let mut remote = remote::Journal::new(s, remote::Options::default()).unwrap();
    let info = remote.prepare(g).unwrap();
    let _permit = remote.claim(info.operation, &target).unwrap();
    remote
        .verified(remote::Verification {
            operation: info.operation,
            target,
            payload_sha256: info.payload_sha256,
            size: 3,
            revision: remote::Revision::new("\"unrelated\"", None).unwrap(),
        })
        .unwrap();
    remote.acknowledge(info.operation).unwrap();
    g
}

#[test]
fn pending_scan_preserves_pages_drafts_other_volumes_and_reduces_validation_work() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let mut parent = None;
    let mut base = 1;
    for n in 1..=8 {
        parent = Some(settled(&mut s, n, parent, base));
        base += 3;
    }
    let dirty = stage(&mut s, 9, parent, b"new dirty head");
    let draft = Journal::new(&mut s, journal::Options::default())
        .unwrap()
        .begin(Input::from_bytes(
            id(10),
            &binding(),
            dirty.generation,
            b"incomplete draft",
        ))
        .unwrap();
    let other = other_settled(&mut s);
    let before_changes = s.connection.total_changes();
    let (expected, old_ops, _) = measured(&mut s, |s| reference(s, None, 1000));
    let (actual, new_ops, _) = measured(&mut s, |s| {
        Journal::new(s, journal::Options::default())?.pending(None, 1000)
    });
    assert_records(&actual, &expected);
    assert_eq!(
        actual
            .iter()
            .map(|r| r.input.id.as_str())
            .collect::<Vec<_>>(),
        vec![dirty.input.id.as_str(), draft.input.id.as_str()]
    );
    assert!(other.receipt.is_some());
    assert!(
        new_ops < old_ops,
        "proof reuse did not reduce SQLite work: {new_ops} >= {old_ops}"
    );
    for after in [
        None,
        Some(id(8)),
        Some(id(9)),
        Some(id(10)),
        Some(id(100)),
        Some(id(101)),
    ] {
        for limit in [1, 2, 1000] {
            let expected = reference(&mut s, after.as_deref(), limit).unwrap();
            let actual = Journal::new(&mut s, journal::Options::default())
                .unwrap()
                .pending(after.as_deref(), limit)
                .unwrap();
            assert_records(&actual, &expected);
        }
    }
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .pending(None, 0)
            .is_err()
    );
    assert!(
        Journal::new(&mut s, journal::Options::default())
            .unwrap()
            .pending(None, 1001)
            .is_err()
    );
    assert_eq!(
        s.connection.total_changes(),
        before_changes,
        "read proofs must not perform data mutations"
    );
    drop(s);
    let mut reopened = open(&tmp.path().join("state"));
    let actual = Journal::new(&mut reopened, journal::Options::default())
        .unwrap()
        .pending(None, 1000)
        .unwrap();
    assert_records(&actual, &expected);
}

#[test]
fn pending_scan_rechecks_corrupt_covered_and_offpage_evidence_between_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let mut s = open(&tmp.path().join("state"));
    let first = settled(&mut s, 1, None, 1);
    settled(&mut s, 2, Some(first), 4);
    let unrelated = unrelated_physical_flush(&mut s);
    let after = id(1000);
    // Reuse the SAME Journal, but every call must construct fresh read proofs.
    let mut j = Journal::new(&mut s, journal::Options::default()).unwrap();
    for sql in [
        "UPDATE logical_edits SET state_hash=zeroblob(32) WHERE operation_id='00000000000000000000000000000001'",
        "UPDATE remote_flushes SET receipt_hash=zeroblob(32) WHERE rowid=(SELECT min(rowid) FROM remote_flushes)",
        "UPDATE write_settlements SET settlement_hash=zeroblob(32) WHERE generation_id=(SELECT min(generation_id) FROM write_settlements)",
        "UPDATE managed_publications SET state_hash=zeroblob(32) WHERE operation_id=(SELECT min(operation_id) FROM managed_publications)",
        "UPDATE staged_chunks SET data=zeroblob(length(data))",
        "UPDATE staged_generations SET parent_id=NULL WHERE parent_id IS NOT NULL",
        "UPDATE logical_files SET state_hash=zeroblob(32)",
    ] {
        assert!(j.pending(Some(&after), 1).unwrap().is_empty());
        j.state
            .connection
            .execute_batch("SAVEPOINT damaged_read_evidence")
            .unwrap();
        assert!(j.state.connection.execute(sql, []).unwrap() > 0);
        assert!(
            j.pending(Some(&after), 1).is_err(),
            "corrupt evidence was hidden: {sql}"
        );
        assert!(
            reference(j.state, Some(&after), 1).is_err(),
            "reference accepted corrupt fixture: {sql}"
        );
        j.state
            .connection
            .execute_batch("ROLLBACK TO damaged_read_evidence; RELEASE damaged_read_evidence")
            .unwrap();
    }
    // This root/settlement has no logical row at all. Filtering the remote
    // validator down to requested logical publications would miss it.
    for table in ["remote_flushes", "write_settlements"] {
        assert!(j.pending(Some(&after), 1).unwrap().is_empty());
        j.state
            .connection
            .execute_batch("SAVEPOINT unrelated_corruption")
            .unwrap();
        let hash_column = if table == "remote_flushes" {
            "receipt_hash"
        } else {
            "settlement_hash"
        };
        assert_eq!(
            j.state
                .connection
                .execute(
                    &format!(
                        "UPDATE {table} SET {hash_column}=zeroblob(32) WHERE generation_id=?1"
                    ),
                    [unrelated.value()]
                )
                .unwrap(),
            1
        );
        assert!(
            j.pending(Some(&after), 1).is_err(),
            "unrelated forest corruption was hidden"
        );
        assert!(reference(j.state, Some(&after), 1).is_err());
        j.state
            .connection
            .execute_batch("ROLLBACK TO unrelated_corruption; RELEASE unrelated_corruption")
            .unwrap();
    }
    // Validly rehash a foreign settlement pointer while leaving every staged
    // manifest/byte and remote receipt intact. This reaches logical ancestry
    // verification rather than failing a row checksum or manifest first.
    j.state
        .connection
        .execute_batch("SAVEPOINT foreign_settlement")
        .unwrap();
    let mut row = file_row(&j.state.connection, &binding().volume, &binding().node.id).unwrap();
    row.settled = Some(unrelated);
    j.state
        .connection
        .execute(
            "UPDATE logical_files SET settled_generation=?1,state_hash=?2",
            params![unrelated.value(), hash(&row).unwrap()],
        )
        .unwrap();
    assert!(file_row(&j.state.connection, &binding().volume, &binding().node.id).is_ok());
    let foreign = journal::load(&j.state.connection, unrelated).unwrap();
    assert!(journal::validate_manifest(&j.state.connection, &foreign, true).is_ok());
    let err = j.pending(Some(&after), 1).unwrap_err();
    assert!(
        err.to_string().contains("logical ancestry binding failure"),
        "expected logical ancestor rejection, got {err}"
    );
    assert!(reference(j.state, Some(&after), 1).is_err());
    j.state
        .connection
        .execute_batch("ROLLBACK TO foreign_settlement; RELEASE foreign_settlement")
        .unwrap();
    assert!(j.pending(Some(&after), 1).unwrap().is_empty());
    // A valid logical row hash cannot bless a forged logical receipt. This
    // checks exact physical/metadata equality, not merely checksum refusal.
    let mut r = load(&j.state.connection, &id(1)).unwrap();
    r.receipt.as_mut().unwrap().sequence += 1;
    j.state
        .connection
        .execute(
            "UPDATE logical_edits SET receipt=?1,state_hash=?2 WHERE operation_id=?3",
            params![
                serde_json::to_vec(r.receipt.as_ref().unwrap()).unwrap(),
                record_hash(&r).unwrap(),
                r.input.id
            ],
        )
        .unwrap();
    assert!(
        load(&j.state.connection, &id(1)).is_ok(),
        "fixture must have valid logical encoding/hash"
    );
    assert!(
        j.pending(Some(&after), 1).is_err(),
        "forged covered receipt suppressed work"
    );
    assert!(reference(j.state, Some(&after), 1).is_err());
}
