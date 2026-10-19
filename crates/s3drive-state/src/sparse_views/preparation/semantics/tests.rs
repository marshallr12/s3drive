use super::*;
use crate::sparse_views::preparation::tests::fixture::*;

fn prepared(s: &mut State, b: &Base, view: &View, slot: u64, bytes: Vec<u8>) -> Info {
    let mut info = Journal::new(s, Options::default())
        .unwrap()
        .begin(request(view, b, 9))
        .unwrap();
    while info.cursor().level == -1 {
        let at = info.cursor().offset / C;
        let candidate = if at == slot {
            Candidate::Bytes(bytes.clone())
        } else if at == 0 && slot >= 16384 {
            Candidate::InheritedPage(proof(b, 0, Some(1)))
        } else if at.is_multiple_of(128) && slot.saturating_sub(at) >= 128 {
            Candidate::InheritedPage(proof(b, at * C, Some(0)))
        } else if info.cursor().offset < b.root.length() {
            Candidate::InheritedChunk(proof(b, at, None))
        } else {
            Candidate::Zero {
                length: view.size() - info.cursor().offset,
            }
        };
        info = Journal::new(s, Options::default())
            .unwrap()
            .append(info.id().unwrap(), info.cursor(), candidate)
            .unwrap();
    }
    finish(s, info)
}
fn original(b: &Base, slot: u64, bytes: Option<Vec<u8>>) -> OriginalChunk {
    OriginalChunk {
        witness: proof(b, slot, None),
        bytes,
    }
}
fn check(s: &mut State, p: &Info, slot: u64, old: Option<&OriginalChunk>) -> Result<()> {
    Journal::new(s, Options::default())?.check_data(p.id()?, slot, old)
}
#[test]
fn partial_retained_chunks_require_complete_original_bytes_including_discarded_tail() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("semantic café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let bytes = pattern(0, 17);
    let b = base(17, BTreeMap::from([(0, bytes.clone())]));
    let initial = view(&mut s, &b, 17);
    let edited = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
        .unwrap()
        .write([2; 32], initial.id(), 5, &[255])
        .unwrap();
    let edited = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
        .unwrap()
        .resize([3; 32], edited.id(), 7)
        .unwrap();
    let mut expected = bytes[..7].to_vec();
    expected[5] = 255;
    let p = prepared(&mut s, &b, &edited, 0, expected);
    let before = ledger::inspect(&s.connection).unwrap();
    let valid = original(&b, 0, Some(bytes.clone()));
    check(&mut s, &p, 0, Some(&valid)).unwrap();
    fail(check(&mut s, &p, 0, None), Failure::Invalid);
    let short = original(&b, 0, Some(bytes[..7].to_vec()));
    fail(check(&mut s, &p, 0, Some(&short)), Failure::Corrupt);
    let mut corrupt_tail = bytes.clone();
    corrupt_tail[16] ^= 1;
    fail(
        check(&mut s, &p, 0, Some(&original(&b, 0, Some(corrupt_tail)))),
        Failure::Corrupt,
    );
    let mut extra = bytes;
    extra.push(0);
    fail(
        check(&mut s, &p, 0, Some(&original(&b, 0, Some(extra)))),
        Failure::Corrupt,
    );
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    let newest = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
        .unwrap()
        .resize([4; 32], edited.id(), 0)
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    check(&mut s, &p, 0, Some(&valid)).unwrap();
    assert_eq!(
        super::super::super::Journal::new(&mut s, super::super::super::Options::default())
            .unwrap()
            .head(edited.binding())
            .unwrap(),
        newest
    );
}
#[test]
fn forged_base_bytes_and_false_zero_data_never_pass_despite_valid_offline_geometry() {
    for zeros in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&temp.path().join("state")).unwrap();
        let bytes = pattern(0, 17);
        let b = base(17, BTreeMap::from([(0, bytes.clone())]));
        let v = view(&mut s, &b, 17);
        let wrong = if zeros { vec![0; 17] } else { vec![255; 17] };
        // Preparation stores offline intent; only the new check certifies
        // correspondence with original bytes under the frozen root.
        let p = prepared(&mut s, &b, &v, 0, wrong);
        let before = ledger::inspect(&s.connection).unwrap();
        fail(
            check(&mut s, &p, 0, Some(&original(&b, 0, Some(bytes)))),
            Failure::Conflict,
        );
        assert!(ledger::inspect(&s.connection).unwrap() == before);
    }
}
#[test]
fn holes_fully_local_chunks_and_zero_growth_have_exact_distinct_material_requirements() {
    for mode in 0..4 {
        let temp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&temp.path().join("state")).unwrap();
        let b = base(
            17,
            if mode == 1 {
                BTreeMap::from([(0, pattern(0, 17))])
            } else {
                BTreeMap::new()
            },
        );
        let initial = view(&mut s, &b, 17);
        let (v, candidate, old) = match mode {
            0 => (initial.clone(), vec![0; 17], Some(original(&b, 0, None))),
            1 => {
                let v = super::super::super::Journal::new(
                    &mut s,
                    super::super::super::Options::default(),
                )
                .unwrap()
                .write([2; 32], initial.id(), 0, &[42; 17])
                .unwrap();
                (v, vec![42; 17], None)
            }
            2 => {
                let v = super::super::super::Journal::new(
                    &mut s,
                    super::super::super::Options::default(),
                )
                .unwrap()
                .resize([2; 32], initial.id(), 0)
                .unwrap();
                let v = super::super::super::Journal::new(
                    &mut s,
                    super::super::super::Options::default(),
                )
                .unwrap()
                .resize([3; 32], v.id(), 33)
                .unwrap();
                (v, vec![0; 33], None)
            }
            _ => {
                let v = super::super::super::Journal::new(
                    &mut s,
                    super::super::super::Options::default(),
                )
                .unwrap()
                .resize([2; 32], initial.id(), 33)
                .unwrap();
                (v, vec![0; 33], Some(original(&b, 0, None)))
            }
        };
        let p = prepared(&mut s, &b, &v, 0, candidate);
        let before = ledger::inspect(&s.connection).unwrap();
        check(&mut s, &p, 0, old.as_ref()).unwrap();
        let unexpected = original(&b, 0, Some(vec![0; 17]));
        fail(
            check(&mut s, &p, 0, Some(&unexpected)),
            if old.is_some() {
                Failure::Corrupt
            } else {
                Failure::Invalid
            },
        );
        assert!(ledger::inspect(&s.connection).unwrap() == before);
        if old.is_some() {
            fail(check(&mut s, &p, 0, None), Failure::Invalid);
        }
    }
}
#[test]
fn shared_ancestry_is_bounded_and_wrong_positions_paths_tags_and_incomplete_plans_fail() {
    let temp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&temp.path().join("state")).unwrap();
    let bytes = pattern(16384, 17);
    let b = base(16384 * C + 17, BTreeMap::from([(16384, bytes.clone())]));
    let v = view(&mut s, &b, b.root.length());
    let p = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    fail(check(&mut s, &p, 16384, None), Failure::Invalid);
    let p = prepared(&mut s, &b, &v, 16384, bytes.clone());
    let valid = original(&b, 16384, Some(bytes.clone()));
    check(&mut s, &p, 16384, Some(&valid)).unwrap();
    let mut wrong = original(&b, 0, None);
    fail(check(&mut s, &p, 16384, Some(&wrong)), Failure::Conflict);
    wrong.witness = proof(&b, 16384 * C, Some(0));
    fail(check(&mut s, &p, 16384, Some(&wrong)), Failure::Bounds);
    let mut wrong = original(&b, 16384, Some(bytes));
    wrong.witness.push(0);
    fail(check(&mut s, &p, 16384, Some(&wrong)), Failure::Corrupt);
    fail(
        check(&mut s, &p, MAX_FILE_BYTES / C, Some(&valid)),
        Failure::Bounds,
    );
    // Position0 is inherited, not a new Data candidate.
    fail(check(&mut s, &p, 0, None), Failure::Invalid);
}

#[tokio::test]
async fn private_worker_checks_grown_partial_data_after_restart_and_releases_owner_on_refusal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("worker café 🦀");
    let bytes = pattern(0, 17);
    let b = base(17, BTreeMap::from([(0, bytes.clone())]));
    let mut s = State::open_managed(&path).unwrap();
    let initial = view(&mut s, &b, 17);
    let grown = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
        .unwrap()
        .resize([2; 32], initial.id(), 33)
        .unwrap();
    let mut candidate = bytes.clone();
    candidate.resize(33, 0);
    let p = prepared(&mut s, &b, &grown, 0, candidate);
    let before = ledger::inspect(&s.connection).unwrap();
    drop(s);
    let storage = crate::worker::Storage::open(
        &path,
        crate::cache::Options::default(),
        crate::journal::Options::default(),
    )
    .await
    .unwrap();
    let views = storage
        .sparse_views(super::super::super::Options::default())
        .unwrap();
    let plans = views.preparations(Options::default()).unwrap();
    let plan = p.id().unwrap();
    plans
        .check_data(plan, 0, Some(original(&b, 0, Some(bytes))))
        .await
        .unwrap();
    fail(plans.check_data(plan, 0, None).await, Failure::Invalid);
    let huge = OriginalChunk {
        witness: vec![0; MAX_RECORD],
        bytes: None,
    };
    fail(plans.check_data(plan, 0, Some(huge)).await, Failure::Bounds);
    assert_eq!(plans.info(plan).await.unwrap().view(), &grown);
    storage.close().await.unwrap();
    assert!(plans.check_data(plan, 0, None).await.is_err());
    let mut s = State::open_managed(&path).unwrap();
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    check(&mut s, &p, 0, Some(&original(&b, 0, Some(pattern(0, 17))))).unwrap();
}
