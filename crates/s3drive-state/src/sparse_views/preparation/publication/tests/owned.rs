//! Durable state tests use modeled remote receipts, not SDK/native acceptance.
use super::*;
use crate::sparse_views::preparation::owned::Settlement;

fn setup(
    s: &mut State,
    committed: bool,
) -> (
    Fixture,
    crate::chunk_mount::Key,
    opens::OpenInput,
    View,
    ns::RemovalPublicationClaim,
    ns::Node,
) {
    let f = fixture();
    let key = chunk(s, &f);
    let open = open(s, &f, &key);
    let v = clean(s, &f);
    let mut input = removal(&f, &f.node, 80);
    let ns::Mutation::RemoveEntry { node, parent } = input.request.mutation.clone() else {
        panic!()
    };
    input.request.mutation = ns::Mutation::ArmRemoval {
        node,
        parent,
        owner: key.owner().into(),
    };
    names(s, &key).begin(input.clone()).unwrap();
    let fence = ns::RemovalFence {
        intent: input.request.operation.clone(),
        owner: key.owner().into(),
    };
    let mut node = f.node.clone();
    node.revision = 6;
    names(s, &key)
        .acknowledge(
            &input.request.operation,
            Ok(ns::Receipt {
                sequence: 6,
                node: node.clone(),
                artifact: Some(ns::Artifact::Removal(fence.clone())),
            }),
        )
        .unwrap();
    let mut claim = ns::RemovalPublicationClaim {
        fence: fence.clone(),
        previous: input.request.operation,
        mode: ns::RemovalPublicationMode::Pending,
    };
    if committed {
        let mut input = removal(&f, &node, 81);
        let ns::Mutation::RemoveEntry {
            node: guard,
            parent,
        } = input.request.mutation.clone()
        else {
            panic!()
        };
        input.request.mutation = ns::Mutation::CommitRemoval {
            node: guard,
            parent,
            fence: fence.clone(),
        };
        names(s, &key).begin(input.clone()).unwrap();
        node.revision = 7;
        node.deleted = true;
        names(s, &key)
            .acknowledge(
                &input.request.operation,
                Ok(ns::Receipt {
                    sequence: 7,
                    node: node.clone(),
                    artifact: Some(ns::Artifact::Removal(fence)),
                }),
            )
            .unwrap();
        claim.previous = input.request.operation;
        claim.mode = ns::RemovalPublicationMode::Committed;
    }
    // Namespace admission requires clean data; model writes after ownership.
    let v = views::Journal::new(s, views::Options::default())
        .unwrap()
        .write([2; 32], v.id(), 5, &[255])
        .unwrap();
    (f, key, open, v, claim, node)
}
fn owned_begin(
    s: &mut State,
    f: &Fixture,
    key: &crate::chunk_mount::Key,
    open: &opens::OpenInput,
    v: &View,
    claim: &ns::RemovalPublicationClaim,
    n: u8,
) -> Info {
    pj(s)
        .begin_owned(request(f, v, n), key, &open.id, claim.clone())
        .unwrap()
}
fn finish(s: &mut State, f: &Fixture, info: Info) -> Info {
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    let info = pj(s)
        .append(info.id().unwrap(), info.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    crate::sparse_views::preparation::tests::fixture::finish(s, info)
}
fn root(s: &mut State, f: &Fixture, info: &Info, node: &ns::Node) -> Record {
    let plan = info.id().unwrap();
    j(s).check(plan, 0, Some(&proof(f))).unwrap();
    physical(s, plan);
    j(s).begin(
        plan,
        node.clone(),
        f.store.clone(),
        node.revision,
        hex(&[10; 16]),
    )
    .unwrap()
}
fn commit(s: &mut State, record: Record) -> Record {
    let p = record.input.plan;
    let mut reserved = reserved(&record);
    reserved.sequence = record.input.reserve.expected_sequence + 1;
    j(s).reserved(p, reserved.clone()).unwrap();
    let r = j(s).prepare_publish(p, reserved.sequence).unwrap();
    let mut receipt = published(&r);
    receipt.sequence = reserved.sequence + 1;
    receipt.node.revision = receipt.sequence;
    j(s).published(p, receipt).unwrap()
}
#[test]
fn owned_sparse_original_open_and_committed_event_are_persisted_before_objects() {
    for committed in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("owned café 🦀");
        let mut s = State::open_managed(&path).unwrap();
        let (f, key, open, v, claim, _) = setup(&mut s, committed);
        assert!(pj(&mut s).begin(request(&f, &v, 20)).is_err());
        let info = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
        assert_eq!(info.ownership().unwrap().original_open(), &open);
        assert_eq!(info.ownership().unwrap().claim(), &claim);
        assert!(info.root().unwrap().is_none());
        assert!(pending(&s, &f).is_err());
        let raw: Vec<u8> = s
            .connection
            .query_row("SELECT input FROM sparse_preparations", [], |r| r.get(0))
            .unwrap();
        drop(s);
        let mut s = State::open_managed(&path).unwrap();
        assert_eq!(
            owned_begin(&mut s, &f, &key, &open, &v, &claim, 9)
                .id()
                .unwrap(),
            info.id().unwrap()
        );
        assert_eq!(
            s.connection
                .query_row("SELECT input FROM sparse_preparations", [], |r| r
                    .get::<_, Vec<u8>>(0))
                .unwrap(),
            raw
        );
        assert!(
            pj(&mut s)
                .begin_owned(request(&f, &v, 20), &key, &open.id, claim.clone())
                .is_err()
        );
    }
}
#[test]
fn owned_sparse_fresh_admission_rejects_wrong_event_open_owner_and_closed_open() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    for kind in 0..4 {
        let mut c = claim.clone();
        let mut id = open.id.clone();
        match kind {
            0 => c.previous = hex(&[90; 16]),
            1 => c.fence.owner = hex(&[90; 16]),
            2 => c.mode = ns::RemovalPublicationMode::Committed,
            _ => id = hex(&[90; 16]),
        };
        assert!(
            pj(&mut s)
                .begin_owned(request(&f, &v, 9), &key, &id, c)
                .is_err()
        );
    }
    assert!(pj(&mut s).operation([9; 32]).unwrap().is_none());
    close(&mut s, &key, &open);
    assert!(
        pj(&mut s)
            .begin_owned(request(&f, &v, 9), &key, &open.id, claim)
            .is_err()
    );
}
#[test]
fn owned_sparse_published_root_binds_event_and_preserves_newer_dirty_head() {
    for committed in [false, true] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("s");
        let mut s = State::open_managed(&path).unwrap();
        let (f, key, open, v, claim, node) = setup(&mut s, committed);
        let info = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
        let info = finish(&mut s, &f, info);
        let r = root(&mut s, &f, &info, &node);
        let ns::Mutation::ReserveManifest { guard, .. } = &r.input.reserve.mutation else {
            panic!()
        };
        assert_eq!(guard.removal_claim.as_deref(), Some(&claim));
        assert_eq!(guard.detached, committed);
        let r = commit(&mut s, r);
        let newer = views::Journal::new(&mut s, views::Options::default())
            .unwrap()
            .write([3; 32], v.id(), 6, &[99])
            .unwrap();
        let before = views::ledger::inspect(&s.connection).unwrap();
        let receipt = ack(&mut s, &r);
        assert!(receipt.acknowledgment().clean_view().is_none());
        assert_eq!(receipt.acknowledgment().observed_head(), newer.id());
        assert!(views::ledger::inspect(&s.connection).unwrap() == before);
        pending(&s, &f).unwrap();
        drop(s);
        let mut recovery = RecoveryStore::open(&path).unwrap();
        assert_eq!(
            recovery.acknowledged(info.id().unwrap()).unwrap(),
            Some(receipt)
        );
        assert_eq!(recovery.get(info.id().unwrap()).unwrap().input, r.input);
    }
}
#[test]
fn owned_sparse_no_effect_is_explicit_replayable_and_never_cleans_dirty_data() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    let info = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let p = info.id().unwrap();
    let before = views::ledger::inspect(&s.connection).unwrap();
    let settlement = Settlement::NoEffect {
        operation: [40; 32],
    };
    pj(&mut s).settle(p, settlement.clone()).unwrap();
    pending(&s, &f).unwrap();
    assert!(views::ledger::inspect(&s.connection).unwrap() == before);
    assert!(pj(&mut s).step(p, info.cursor()).is_err());
    assert!(
        pj(&mut s)
            .append(p, info.cursor(), Candidate::Bytes(f.bytes.clone()))
            .is_err()
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(
        pj(&mut s)
            .settle(p, settlement.clone())
            .unwrap()
            .settlement(),
        Some(&settlement)
    );
    assert!(
        pj(&mut s)
            .settle(
                p,
                Settlement::NoEffect {
                    operation: [41; 32]
                }
            )
            .is_err()
    );
    assert_eq!(
        owned_begin(&mut s, &f, &key, &open, &v, &claim, 9).settlement(),
        Some(&settlement)
    );
}
#[test]
fn owned_sparse_no_effect_cannot_hide_staged_or_consumed_objects() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    let info = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let info = finish(&mut s, &f, info);
    let p = info.id().unwrap();
    let pos = pj(&mut s).transfers(p, None, 128).unwrap()[0].position();
    let transfer = pj(&mut s)
        .materialize(p, pos, crate::journal::Options::default())
        .unwrap()
        .unwrap();
    assert!(
        pj(&mut s)
            .settle(
                p,
                Settlement::NoEffect {
                    operation: [40; 32]
                }
            )
            .is_err()
    );
    let mut remote =
        crate::remote::Journal::new(&mut s, crate::remote::Options::default()).unwrap();
    let attempt = remote.prepare(transfer.generation().unwrap()).unwrap();
    let _permit = remote.claim(attempt.operation, &attempt.target).unwrap();
    assert!(
        pj(&mut s)
            .settle(
                p,
                Settlement::NoEffect {
                    operation: [40; 32]
                }
            )
            .is_err()
    );
    assert!(pending(&s, &f).is_err());
}
#[test]
fn owned_sparse_rejected_settlement_requires_exact_terminal_request_and_retains_dirty_bytes() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let (f, key, open, v, claim, node) = setup(&mut s, true);
    let info = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let info = finish(&mut s, &f, info);
    let p = info.id().unwrap();
    let r = root(&mut s, &f, &info, &node);
    let before = views::ledger::inspect(&s.connection).unwrap();
    let settlement = Settlement::Rejected {
        operation: [40; 32],
    };
    assert!(pj(&mut s).settle(p, settlement.clone()).is_err());
    j(&mut s)
        .reject(p, r.input.reserve.clone(), ns::Error::Conflict)
        .unwrap();
    assert!(pending(&s, &f).is_err());
    pj(&mut s).settle(p, settlement.clone()).unwrap();
    pending(&s, &f).unwrap();
    assert!(views::ledger::inspect(&s.connection).unwrap() == before);
    assert!(
        j(&mut s)
            .acknowledge([41; 32], p, views::Options::default())
            .is_err()
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(pj(&mut s).info(p).unwrap().settlement(), Some(&settlement));
    pending(&s, &f).unwrap();
}
#[test]
fn owned_sparse_old_marker_refusal_preserves_new_format_bytes() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    s.connection
        .pragma_update(None, "user_version", 46)
        .unwrap();
    drop(s);
    let bytes = std::fs::read(path.join("metadata.sqlite3")).unwrap();
    assert!(State::open_managed(&path).is_err());
    assert_eq!(std::fs::read(path.join("metadata.sqlite3")).unwrap(), bytes);
}

#[test]
fn owned_sparse_successor_requires_same_original_open_and_event() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let (f, key, open, v, claim, node) = setup(&mut s, true);
    let a = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let a = finish(&mut s, &f, a);
    let ar = root(&mut s, &f, &a, &node);
    let ar = commit(&mut s, ar);
    let v2 = views::Journal::new(&mut s, views::Options::default())
        .unwrap()
        .write([20; 32], v.id(), 6, &[20])
        .unwrap();
    let b = owned_begin(&mut s, &f, &key, &open, &v2, &claim, 21);
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    bytes[6] = 20;
    let b = pj(&mut s)
        .append(b.id().unwrap(), b.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    let b = crate::sparse_views::preparation::tests::fixture::finish(&mut s, b);
    let p = b.id().unwrap();
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    physical(&mut s, p);
    let base = ar.receipt().unwrap().node.clone();
    let br = j(&mut s)
        .begin(
            p,
            base.clone(),
            f.store.clone(),
            base.revision,
            hex(&[22; 16]),
        )
        .unwrap();
    assert_eq!(br.input.predecessor, Some(ar.input.plan));
    ack(&mut s, &ar);
    assert!(pending(&s, &f).is_err());
    let br = commit(&mut s, br);
    j(&mut s)
        .acknowledge([41; 32], br.input.plan, views::Options::default())
        .unwrap();
    pending(&s, &f).unwrap();
}
#[test]
fn owned_sparse_quota_refusal_preserves_original_open_source_and_preparation() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let before = crate::sparse_views::preparation::ledger::inspect(&s.connection).unwrap();
    let mut limited = crate::sparse_views::preparation::Journal::new(
        &mut s,
        Options {
            plans: 1,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        limited
            .begin_owned(request(&f, &v, 20), &key, &open.id, claim)
            .is_err()
    );
    assert!(crate::sparse_views::preparation::ledger::inspect(&s.connection).unwrap() == before);
}
#[test]
fn owned_sparse_actual_sqlite_full_rolls_back_admission_and_retries_exact_identity() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    s.connection
        .execute_batch("CREATE TABLE owned_full_filler(data BLOB)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    loop {
        match s.connection.execute(
            "INSERT INTO owned_full_filler VALUES(?1)",
            [vec![99u8; 3500]],
        ) {
            Ok(_) => (),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::DiskFull =>
            {
                break;
            }
            Err(e) => panic!("{e}"),
        }
    }
    let mut failed = None;
    for n in 9..25 {
        let before = crate::sparse_views::preparation::ledger::inspect(&s.connection).unwrap();
        match pj(&mut s).begin_owned(request(&f, &v, n), &key, &open.id, claim.clone()) {
            Ok(_) => (),
            Err(e) => {
                assert!(
                    matches!(e.downcast_ref::<rusqlite::Error>(),Some(rusqlite::Error::SqliteFailure(e,_)) if e.code==rusqlite::ErrorCode::DiskFull),
                    "{e}"
                );
                assert!(
                    crate::sparse_views::preparation::ledger::inspect(&s.connection).unwrap()
                        == before
                );
                failed = Some(n);
                break;
            }
        }
    }
    let n = failed.expect("real full database must refuse an owned admission");
    s.connection
        .pragma_update(None, "max_page_count", pages + 1024)
        .unwrap();
    let i = owned_begin(&mut s, &f, &key, &open, &v, &claim, n);
    assert_eq!(i.operation(), [n; 32]);
}
#[test]
fn owned_sparse_crash_child() {
    let Ok(kind) = std::env::var("S3DRIVE_OWNED_SPARSE_CHILD") else {
        return;
    };
    let path = std::path::PathBuf::from(std::env::var_os("S3DRIVE_OWNED_SPARSE_DIR").unwrap());
    let mut s = State::open_managed(&path).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    let i = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    if kind == "settle" {
        pj(&mut s)
            .settle(
                i.id().unwrap(),
                Settlement::NoEffect {
                    operation: [40; 32],
                },
            )
            .unwrap();
    }
    panic!("expected checkpoint was not reached");
}
#[test]
fn owned_sparse_killed_owners_recover_atomic_original_provenance_and_settlement() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    for (kind, points) in [
        (
            "begin",
            &["BlobStored", "PlanStored", "Validated", "Committed"][..],
        ),
        ("settle", &["StateUpdated", "Validated", "Committed"][..]),
    ] {
        for point in points {
            let t = tempfile::tempdir().unwrap();
            let path = t.path().join("crash café 🦀");
            let ready = t.path().join("ready");
            let mut child=Command::new(std::env::current_exe().unwrap()).args(["--exact","sparse_views::preparation::publication::tests::barriers::owned::owned_sparse_crash_child","--nocapture"])
                .env("S3DRIVE_OWNED_SPARSE_CHILD",kind).env("S3DRIVE_OWNED_SPARSE_DIR",&path)
                .env("S3DRIVE_MIXED_CRASH_KIND",kind).env("S3DRIVE_MIXED_CRASH_POINT",point).env("S3DRIVE_MIXED_CRASH_READY",&ready)
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
            let start = Instant::now();
            while !ready.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    use std::io::Read;
                    let mut error = String::new();
                    child
                        .stderr
                        .take()
                        .unwrap()
                        .read_to_string(&mut error)
                        .unwrap();
                    panic!("{kind}/{point}: {status}: {error}");
                }
                if start.elapsed() > Duration::from_secs(45) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("{kind}/{point} timeout");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(State::open_managed(&path).is_err());
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut s = State::open_managed(&path).unwrap();
            let info = pj(&mut s).operation([9; 32]).unwrap();
            if kind == "begin" && *point != "Committed" {
                assert!(info.is_none());
            } else {
                let info = info.unwrap();
                assert!(info.ownership().is_some());
                if kind == "settle" {
                    assert_eq!(info.settlement().is_some(), *point == "Committed");
                    pj(&mut s)
                        .settle(
                            info.id().unwrap(),
                            Settlement::NoEffect {
                                operation: [40; 32],
                            },
                        )
                        .unwrap();
                }
            }
        }
    }
}

#[test]
fn owned_sparse_no_effect_of_one_plan_never_settles_another_plan() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let (f, key, open, v, claim, _) = setup(&mut s, false);
    let first = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let other = owned_begin(&mut s, &f, &key, &open, &v, &claim, 20);
    let other = finish(&mut s, &f, other);
    pj(&mut s)
        .settle(
            first.id().unwrap(),
            Settlement::NoEffect {
                operation: [40; 32],
            },
        )
        .unwrap();
    assert!(pending(&s, &f).is_err());
    pj(&mut s)
        .settle(
            other.id().unwrap(),
            Settlement::NoEffect {
                operation: [41; 32],
            },
        )
        .unwrap();
    pending(&s, &f).unwrap();
    let pos = pj(&mut s)
        .transfers(other.id().unwrap(), None, 128)
        .unwrap()[0]
        .position();
    assert!(
        pj(&mut s)
            .materialize(other.id().unwrap(), pos, crate::journal::Options::default())
            .is_err()
    );
}
#[test]
fn owned_sparse_legacy_preparation_bytes_and_hashes_survive_schema46_upgrade() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let info = prepare(&mut s, &f, false);
    let raw: Vec<u8> = s
        .connection
        .query_row("SELECT input FROM sparse_preparations", [], |r| r.get(0))
        .unwrap();
    assert!(!String::from_utf8(raw.clone()).unwrap().contains("owned"));
    let before = crate::sparse_views::preparation::ledger::inspect(&s.connection).unwrap();
    s.connection
        .pragma_update(None, "user_version", 46)
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, 47);
    assert!(crate::sparse_views::preparation::ledger::inspect(&s.connection).unwrap() == before);
    assert_eq!(
        pj(&mut s).info(info.id().unwrap()).unwrap().ownership(),
        None
    );
    assert_eq!(
        s.connection
            .query_row("SELECT input FROM sparse_preparations", [], |r| r
                .get::<_, Vec<u8>>(0))
            .unwrap(),
        raw
    );
}
#[test]
fn owned_sparse_corrupt_independent_open_blocks_whole_graph_and_metadata_recovery() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("s");
    let mut s = State::open_managed(&path).unwrap();
    let (f, key, open, v, claim, node) = setup(&mut s, true);
    let info = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let info = finish(&mut s, &f, info);
    root(&mut s, &f, &info, &node);
    s.connection
        .execute(
            "UPDATE windows_open_journal SET state_hash=zeroblob(32)",
            [],
        )
        .unwrap();
    assert!(pj(&mut s).info(info.id().unwrap()).is_err());
    assert!(j(&mut s).records().is_err());
    drop(s);
    assert!(RecoveryStore::open(&path).is_err());
    assert!(State::open_managed(&path).is_err());
}

#[test]
fn owned_sparse_successor_binds_advanced_event_without_replacing_original_open() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let (f, key, open, v, claim, node) = setup(&mut s, false);
    let a = owned_begin(&mut s, &f, &key, &open, &v, &claim, 9);
    let a = finish(&mut s, &f, a);
    let ar = root(&mut s, &f, &a, &node);
    let ar = commit(&mut s, ar);
    let newer = views::Journal::new(&mut s, views::Options::default())
        .unwrap()
        .write([20; 32], v.id(), 6, &[20])
        .unwrap();
    ack(&mut s, &ar);
    let base = ar.receipt().unwrap().node.clone();
    let mut input = removal(&f, &base, 83);
    let ns::Mutation::RemoveEntry { node, parent } = input.request.mutation.clone() else {
        panic!()
    };
    input.request.mutation = ns::Mutation::CommitRemoval {
        node,
        parent,
        fence: claim.fence.clone(),
    };
    names(&mut s, &key).begin(input.clone()).unwrap();
    let mut base = base;
    base.revision += 1;
    base.deleted = true;
    names(&mut s, &key)
        .acknowledge(
            &input.request.operation,
            Ok(ns::Receipt {
                sequence: base.revision,
                node: base.clone(),
                artifact: Some(ns::Artifact::Removal(claim.fence.clone())),
            }),
        )
        .unwrap();
    let current = ns::RemovalPublicationClaim {
        fence: claim.fence.clone(),
        previous: input.request.operation,
        mode: ns::RemovalPublicationMode::Committed,
    };
    assert!(
        pj(&mut s)
            .begin_owned(request(&f, &newer, 21), &key, &open.id, claim)
            .is_err()
    );
    let b = owned_begin(&mut s, &f, &key, &open, &newer, &current, 21);
    assert_eq!(
        b.ownership().unwrap().original_open(),
        a.ownership().unwrap().original_open()
    );
    assert!(b.ownership().unwrap().event_sequence() > a.ownership().unwrap().event_sequence());
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    bytes[6] = 20;
    let b = pj(&mut s)
        .append(b.id().unwrap(), b.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    let b = crate::sparse_views::preparation::tests::fixture::finish(&mut s, b);
    j(&mut s)
        .check(b.id().unwrap(), 0, Some(&proof(&f)))
        .unwrap();
    physical(&mut s, b.id().unwrap());
    let br = j(&mut s)
        .begin(
            b.id().unwrap(),
            base.clone(),
            f.store.clone(),
            base.revision,
            hex(&[22; 16]),
        )
        .unwrap();
    assert_eq!(br.input.predecessor, Some(ar.input.plan));
    let br = commit(&mut s, br);
    j(&mut s)
        .acknowledge([41; 32], br.input.plan, views::Options::default())
        .unwrap();
    pending(&s, &f).unwrap();
}
