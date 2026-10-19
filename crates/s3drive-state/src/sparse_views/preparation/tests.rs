use super::*;
#[path = "tests/fixture.rs"]
mod fixture;
use fixture::*;
#[test]
fn mixed_data_inheritance_exact_retry_and_old_views_survive_newer_heads_and_restart() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("mixed café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let b = base(
        2 * C + 17,
        BTreeMap::from([(0, pattern(0, C)), (1, pattern(1, C)), (2, pattern(2, 17))]),
    );
    let initial = view(&mut s, &b, b.root.length());
    let v = super::super::Journal::new(&mut s, super::super::Options::default())
        .unwrap()
        .write([2; 32], initial.id(), 5, &[255])
        .unwrap();
    let mut expected = pattern(0, C);
    expected[5] = 255;
    let first = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    let plan = first.id().unwrap();
    let second = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(plan, first.cursor(), Candidate::Bytes(expected.clone()))
        .unwrap();
    let retry = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(plan, first.cursor(), Candidate::Bytes(expected.clone()))
        .unwrap();
    assert_eq!(retry.cursor(), second.cursor());
    let mut wrong = expected.clone();
    wrong[6] ^= 1;
    fail(
        Journal::new(&mut s, Options::default()).unwrap().append(
            plan,
            first.cursor(),
            Candidate::Bytes(wrong),
        ),
        Failure::Conflict,
    );
    let third = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(
            plan,
            second.cursor(),
            Candidate::InheritedChunk(proof(&b, 1, None)),
        )
        .unwrap();
    let last = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(
            plan,
            third.cursor(),
            Candidate::InheritedChunk(proof(&b, 2, None)),
        )
        .unwrap();
    let newer = super::super::Journal::new(&mut s, super::super::Options::default())
        .unwrap()
        .resize([3; 32], v.id(), 0)
        .unwrap();
    let replay = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    assert_eq!(replay.cursor(), last.cursor());
    assert_eq!(replay.view(), &v);
    let done = finish(&mut s, last);
    let root = done.root().unwrap().unwrap();
    let Dependency::New { bytes, .. } = Journal::new(&mut s, Options::default())
        .unwrap()
        .dependency(plan, 0, 0)
        .unwrap()
    else {
        panic!()
    };
    let page = root.verify_page(&bytes).unwrap();
    assert_eq!(page.page().length(), 2 * C + 17);
    let entries = page.page().entries();
    assert_eq!(entries.len(), 3);
    for (i, e) in entries.iter().enumerate() {
        let Entry::Data { object, .. } = e else {
            panic!()
        };
        assert_eq!(
            object.generation(),
            if i == 0 { &[9; 16] } else { &[7; 16] }
        );
    }
    let original = ledger::inspect(&s.connection).unwrap().plans[&plan]
        .input_raw
        .clone();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let replay = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    assert_eq!(replay.root().unwrap(), Some(root));
    assert_eq!(replay.view(), &v);
    assert_eq!(
        ledger::inspect(&s.connection).unwrap().plans[&plan].input_raw,
        original
    );
    assert_eq!(
        super::super::Journal::new(&mut s, super::super::Options::default())
            .unwrap()
            .head(v.binding())
            .unwrap(),
        newer
    );
}
#[test]
fn partial_final_lengths_local_masks_and_explicit_proof_tags_cannot_be_substituted() {
    for size in [7, 33] {
        let t = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&t.path().join("state")).unwrap();
        let b = base(17, BTreeMap::from([(0, pattern(0, 17))]));
        let v = view(&mut s, &b, size);
        let info = Journal::new(&mut s, Options::default())
            .unwrap()
            .begin(request(&v, &b, 9))
            .unwrap();
        let plan = info.id().unwrap();
        let before = ledger::inspect(&s.connection).unwrap();
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .append(
                    plan,
                    info.cursor(),
                    Candidate::InheritedChunk(proof(&b, 0, None))
                )
                .is_err()
        );
        assert!(
            Journal::new(&mut s, Options::default())
                .unwrap()
                .append(
                    plan,
                    info.cursor(),
                    Candidate::InheritedPage(proof(&b, 0, None))
                )
                .is_err()
        );
        assert!(ledger::inspect(&s.connection).unwrap() == before);
        let mut bytes = pattern(0, 17);
        bytes.resize(size as usize, 0);
        if size == 33 {
            let mut bad = bytes.clone();
            bad[32] = 1;
            fail(
                Journal::new(&mut s, Options::default()).unwrap().append(
                    plan,
                    info.cursor(),
                    Candidate::Bytes(bad),
                ),
                Failure::Corrupt,
            );
        }
        let ready = Journal::new(&mut s, Options::default())
            .unwrap()
            .append(plan, info.cursor(), Candidate::Bytes(bytes))
            .unwrap();
        let done = finish(&mut s, ready);
        assert_eq!(done.root().unwrap().unwrap().length(), size);
    }
}
#[test]
fn old_roots_are_reused_as_exact_children_across_both_height_growth_boundaries() {
    for chunks in [128, 16384] {
        let t = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&t.path().join("state")).unwrap();
        let b = base(chunks * C, BTreeMap::new());
        let v = view(&mut s, &b, (chunks + 1) * C);
        let info = Journal::new(&mut s, Options::default())
            .unwrap()
            .begin(request(&v, &b, 9))
            .unwrap();
        let plan = info.id().unwrap();
        let next = Journal::new(&mut s, Options::default())
            .unwrap()
            .append(
                plan,
                info.cursor(),
                Candidate::InheritedPage(proof(&b, 0, Some(b.page.level()))),
            )
            .unwrap();
        assert_eq!(next.cursor().offset, chunks * C);
        let ready = Journal::new(&mut s, Options::default())
            .unwrap()
            .append(plan, next.cursor(), Candidate::Zero { length: C })
            .unwrap();
        let done = finish(&mut s, ready);
        let level = model::root_level(v.size());
        let Dependency::New { bytes, .. } = Journal::new(&mut s, Options::default())
            .unwrap()
            .dependency(plan, level, 0)
            .unwrap()
        else {
            panic!()
        };
        let root = done.root().unwrap().unwrap();
        let page = root.verify_page(&bytes).unwrap();
        let Entry::Child { object, .. } = &page.page().entries()[0] else {
            panic!()
        };
        assert_eq!(object, b.root.page());
        let snapshot = ledger::inspect(&s.connection).unwrap();
        assert!(
            snapshot.objects.len() <= 5,
            "unchanged subtree enumerated descendants"
        );
        assert!(snapshot.blobs.len() <= 4);
    }
}
#[test]
fn sixty_four_gib_zero_coverage_uses_one_input_record_and_bounded_page_metadata() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let b = base(0, BTreeMap::new());
    let v = view(&mut s, &b, MAX_FILE_BYTES);
    let info = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    let plan = info.id().unwrap();
    let ready = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(
            plan,
            info.cursor(),
            Candidate::Zero {
                length: MAX_FILE_BYTES,
            },
        )
        .unwrap();
    let done = finish(&mut s, ready);
    assert_eq!(done.root().unwrap().unwrap().length(), MAX_FILE_BYTES);
    let Dependency::New { bytes, .. } = Journal::new(&mut s, Options::default())
        .unwrap()
        .dependency(plan, 2, 0)
        .unwrap()
    else {
        panic!()
    };
    done.root().unwrap().unwrap().verify_page(&bytes).unwrap();

    let snapshot = ledger::inspect(&s.connection).unwrap();
    assert_eq!(snapshot.objects.len(), 518);
    assert!(snapshot.blobs.values().map(Vec::len).sum::<usize>() < 100_000);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .info(plan)
            .unwrap()
            .root()
            .unwrap()
            .is_some()
    );
}
#[test]
fn narrowed_quotas_preserve_old_progress_bytes_replay_and_durable_completion() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("state")).unwrap();
    let b = base(C, BTreeMap::from([(0, pattern(0, C))]));
    let v = view(&mut s, &b, C);
    let tiny = Options {
        metadata_bytes: 1,
        ..Options::default()
    };
    let empty = ledger::inspect(&s.connection).unwrap();
    fail(
        Journal::new(&mut s, tiny)
            .unwrap()
            .begin(request(&v, &b, 9)),
        Failure::Quota,
    );
    assert!(ledger::inspect(&s.connection).unwrap() == empty);
    let info = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    let plan = info.id().unwrap();
    let data = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(plan, info.cursor(), Candidate::Bytes(pattern(0, C)))
        .unwrap();
    let before = ledger::inspect(&s.connection).unwrap();
    let recovered = Journal::new(&mut s, tiny)
        .unwrap()
        .operation([9; 32])
        .unwrap()
        .unwrap();
    assert_eq!(recovered.id().unwrap(), plan);
    assert_eq!(recovered.cursor(), data.cursor());
    assert!(
        Journal::new(&mut s, tiny)
            .unwrap()
            .operation([55; 32])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        Journal::new(&mut s, tiny)
            .unwrap()
            .begin(request(&v, &b, 9))
            .unwrap()
            .cursor(),
        data.cursor()
    );
    fail(
        Journal::new(&mut s, tiny)
            .unwrap()
            .step(plan, data.cursor()),
        Failure::Quota,
    );
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    assert!(finish(&mut s, data).root().unwrap().is_some());
}

#[path = "tests/refusals.rs"]
mod refusals;

#[path = "tests/recovery.rs"]
mod recovery;
#[test]
fn repeated_chunk_witness_paths_share_one_leaf_blob_instead_of_copying_its_bytes() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("state")).unwrap();
    let b = base(
        129 * C,
        BTreeMap::from([(0, pattern(0, C)), (127, pattern(127, C))]),
    );
    let v = view(&mut s, &b, 129 * C);
    let mut info = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    let plan = info.id().unwrap();
    for chunk in 0..128 {
        info = Journal::new(&mut s, Options::default())
            .unwrap()
            .append(
                plan,
                info.cursor(),
                Candidate::InheritedChunk(proof(&b, chunk, None)),
            )
            .unwrap();
    }
    let snapshot = ledger::inspect(&s.connection).unwrap();
    assert_eq!(snapshot.objects.len(), 128);
    assert_eq!(snapshot.blobs.len(), 2);
    assert!(snapshot.blobs.values().map(Vec::len).sum::<usize>() < 1000);
    let ready = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(
            plan,
            info.cursor(),
            Candidate::InheritedPage(proof(&b, 128 * C, Some(0))),
        )
        .unwrap();
    let done = finish(&mut s, ready);
    let Dependency::New { bytes, .. } = Journal::new(&mut s, Options::default())
        .unwrap()
        .dependency(plan, 1, 0)
        .unwrap()
    else {
        panic!()
    };
    done.root().unwrap().unwrap().verify_page(&bytes).unwrap();
    assert_eq!(ledger::inspect(&s.connection).unwrap().blobs.len(), 4);
}
#[test]
fn two_level_chunk_paths_verify_full_partial_objects_without_adopting_short_or_extra_paths() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("state")).unwrap();
    let size = 16384 * C + 17;
    let b = base(size, BTreeMap::from([(16384, pattern(16384, 17))]));
    let v = view(&mut s, &b, size);
    let info = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    let plan = info.id().unwrap();
    let next = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(
            plan,
            info.cursor(),
            Candidate::InheritedPage(proof(&b, 0, Some(1))),
        )
        .unwrap();
    assert_eq!(next.cursor().offset, 16384 * C);
    let raw = proof(&b, 16384, None);
    assert_eq!(raw[48], 2);
    let before = ledger::inspect(&s.connection).unwrap();
    let mut short = raw.clone();
    short[48] = 1;
    assert!(
        Journal::new(&mut s, Options::default())
            .unwrap()
            .append(plan, next.cursor(), Candidate::InheritedChunk(short))
            .is_err()
    );
    assert!(ledger::inspect(&s.connection).unwrap() == before);
    let ready = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(plan, next.cursor(), Candidate::InheritedChunk(raw))
        .unwrap();
    let done = finish(&mut s, ready);
    let Dependency::New { bytes, .. } = Journal::new(&mut s, Options::default())
        .unwrap()
        .dependency(plan, 2, 0)
        .unwrap()
    else {
        panic!()
    };
    done.root().unwrap().unwrap().verify_page(&bytes).unwrap();
    let Dependency::Inherited { reference, .. } = Journal::new(&mut s, Options::default())
        .unwrap()
        .dependency(plan, -1, 16384)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(reference.sha256(), &digest(&pattern(16384, 17)));
}
