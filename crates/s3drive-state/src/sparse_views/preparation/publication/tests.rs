use super::super::tests::fixture::{Base, base, pattern};
use super::super::{Candidate, Options, Request};
use super::*;
use s3drive_core::manifest::{base::Descriptor, objects::Namespace};
fn pj(s: &mut State) -> super::super::Journal<'_> {
    super::super::Journal::new(s, Options::default()).unwrap()
}
fn j(s: &mut State) -> Journal<'_> {
    Journal::new(s, Limits::default()).unwrap()
}
struct Fixture {
    base: Base,
    node: ns::Node,
    store: ns::Store,
    binding: Binding,
    bytes: Vec<u8>,
}
fn fixture() -> Fixture {
    let bytes = pattern(0, 17);
    let b = base(17, BTreeMap::from([(0, bytes.clone())]));
    let store = ns::Store {
        id: hex(&[5; 16]),
        route: "b".repeat(64),
        root: "managed/".into(),
    };
    let r = ns::Reservation {
        id: hex(&[7; 16]),
        volume: hex(&[1; 16]),
        node: hex(&[2; 16]),
        expected_revision: 3,
        store: store.clone(),
        key: Namespace::new(store.root.clone(), [1; 16], [2; 16], [5; 16])
            .unwrap()
            .root_key(&b.root)
            .unwrap()
            .as_str()
            .into(),
        size: b.page.encode().len() as u64,
        sha256: hex(&digest(&b.page.encode())),
        manifest: Some(ns::ManifestRoot::new(b.root.clone()).unwrap()),
        guard: Some(ns::ContentGuard {
            removal_claim: None,
            expected: None,
            detached: false,
            removal: None,
        }),
    };
    let payload = ns::Payload {
        reservation: r,
        upload_operation: hex(&[8; 32]),
        etag: "\"old-model\"".into(),
        version: None,
    };
    assert!(payload.valid());
    let node = ns::Node {
        id: hex(&[2; 16]),
        parent: Some(ns::ROOT.into()),
        name: "café 🦀.bin".into(),
        kind: ns::Kind::File,
        revision: 5,
        deleted: false,
        payload: Some(payload),
        placement: Some(store.clone()),
    };
    let reference = ns::payload_reference(&node.payload).unwrap();
    let binding = Binding::new(
        Descriptor::from_hex(&hex(&[1; 16]), &hex(&[2; 16]), &hex(&[7; 16]), &reference).unwrap(),
        [5; 16],
        [6; 32],
        "a".repeat(64),
        store.root.clone(),
        17,
    )
    .unwrap();
    Fixture {
        base: b,
        node,
        store,
        binding,
        bytes,
    }
}
fn proof(f: &Fixture) -> OriginalChunk {
    let mut raw = super::super::tests::fixture::proof(&f.base, 0, None);
    let mut h = Sha256::new();
    h.update(b"s3drive-authenticated-chunk-reference-v1\0");
    h.update(f.binding.base().unwrap().payload_reference().as_bytes());
    raw[8..40].copy_from_slice(&<[u8; 32]>::from(h.finalize()));
    OriginalChunk {
        witness: raw,
        bytes: Some(f.bytes.clone()),
    }
}
fn prepare(s: &mut State, f: &Fixture, wrong: bool) -> Info {
    let v = crate::sparse_views::Journal::new(s, crate::sparse_views::Options::default())
        .unwrap()
        .begin([1; 32], f.binding.clone(), 17)
        .unwrap();
    let v = crate::sparse_views::Journal::new(s, crate::sparse_views::Options::default())
        .unwrap()
        .write([2; 32], v.id(), 5, &[255])
        .unwrap();
    let mut bytes = f.bytes.clone();
    bytes[5] = 255;
    if wrong {
        bytes[6] ^= 1;
    }
    let i = pj(s)
        .begin(Request {
            operation: [9; 32],
            view: v.id(),
            generation: [9; 16],
            base_root: f.base.root.encode().to_vec(),
            base_page: f.base.page.encode(),
        })
        .unwrap();
    let i = pj(s)
        .append(i.id().unwrap(), i.cursor(), Candidate::Bytes(bytes))
        .unwrap();
    super::super::tests::fixture::finish(s, i)
}
fn physical(s: &mut State, p: Id) {
    let mut after = None;
    loop {
        let transfers = pj(s).transfers(p, after, 128).unwrap();
        if transfers.is_empty() {
            break;
        }
        for t in transfers {
            let pos = t.position();
            let t = pj(s)
                .materialize(p, pos, crate::journal::Options::default())
                .unwrap()
                .unwrap();
            let mut r = crate::remote::Journal::new(s, crate::remote::Options::default()).unwrap();
            let i = r.prepare(t.generation().unwrap()).unwrap();
            let _permit = r.claim(i.operation, &i.target).unwrap();
            r.verified(crate::remote::Verification {
                operation: i.operation,
                target: i.target,
                payload_sha256: i.payload_sha256,
                size: i.size,
                revision: crate::remote::Revision::new("\"owned-model\"", None).unwrap(),
            })
            .unwrap();
            r.acknowledge(i.operation).unwrap();
            after = Some(pos);
        }
    }
}
fn reserved(r: &Record) -> ns::Receipt {
    ns::Receipt {
        sequence: 6,
        node: r.input.base.clone(),
        artifact: Some(ns::Artifact::Reservation(r.input.reservation().unwrap())),
    }
}
fn published(r: &Record) -> ns::Receipt {
    let payload = ns::Payload {
        reservation: r.input.reservation().unwrap(),
        upload_operation: hex(&r.physical.operation),
        etag: r.physical.etag.clone(),
        version: r.physical.version.clone(),
    };
    let mut node = r.input.base.clone();
    node.revision = 7;
    node.payload = Some(payload.clone());
    ns::Receipt {
        sequence: 7,
        node,
        artifact: Some(ns::Artifact::Published(payload)),
    }
}
#[test]
fn exact_semantic_coverage_physical_proof_requests_and_newer_heads_survive_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("café root 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let info = prepare(&mut s, &f, false);
    let p = info.id().unwrap();
    assert_eq!(
        j(&mut s).unchecked(p, None, 128).unwrap(),
        vec![DataNeed {
            slot: 0,
            uses_base: true
        }]
    );
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
            .is_err()
    );
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    assert!(j(&mut s).unchecked(p, None, 128).unwrap().is_empty());
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
            .is_err()
    );
    physical(&mut s, p);
    let r = j(&mut s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    assert_eq!(
        r.input.reserve.mutation,
        ns::Mutation::ReserveManifest {
            node: f.node.id.clone(),
            expected_revision: 5,
            store: f.store.id.clone(),
            size: r.physical.size,
            sha256: hex(&r.physical.sha256),
            manifest: Box::new(ns::ManifestRoot::new(info.root().unwrap().unwrap()).unwrap()),
            guard: ns::ContentGuard {
                removal_claim: None,
                expected: ns::payload_reference(&f.node.payload),
                detached: false,
                removal: None
            }
        }
    );
    let same = j(&mut s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    assert_eq!(r, same);
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 6, hex(&[10; 16]))
            .is_err()
    );
    let a = reserved(&r);
    let r = j(&mut s).reserved(p, a.clone()).unwrap();
    assert_eq!(j(&mut s).reserved(p, a).unwrap(), r);
    let r = j(&mut s).prepare_publish(p, 6).unwrap();
    assert!(j(&mut s).prepare_publish(p, 7).is_err());
    let next = crate::sparse_views::Journal::new(&mut s, crate::sparse_views::Options::default())
        .unwrap()
        .resize([3; 32], info.view().id(), 0)
        .unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(j(&mut s).get(p).unwrap(), r);
    let receipt = published(&r);
    let done = j(&mut s).published(p, receipt.clone()).unwrap();
    assert_eq!(j(&mut s).published(p, receipt).unwrap(), done);
    assert_eq!(
        crate::sparse_views::Journal::new(&mut s, crate::sparse_views::Options::default())
            .unwrap()
            .head(&f.binding)
            .unwrap(),
        next
    );
    assert_eq!(j(&mut s).get(p).unwrap().input.view(), info.view().id());
}
#[test]
fn false_candidates_wrong_bindings_and_corrupt_metadata_never_admit_requests() {
    let temp = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&temp.path().join("state")).unwrap();
    let f = fixture();
    let info = prepare(&mut s, &f, true);
    let p = info.id().unwrap();
    assert!(j(&mut s).check(p, 0, Some(&proof(&f))).is_err());
    assert!(ledger::inspect(&s.connection).unwrap().empty());
    assert!(
        j(&mut s)
            .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
            .is_err()
    );
    assert_eq!(
        s.connection
            .query_row("SELECT count(*) FROM sparse_publications", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn additive_schema_thirty_three_preserves_dirty_ids_and_rolls_back_collisions() {
    for collision in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("migration");
        let mut s = State::open_managed(&path).unwrap();
        let f = fixture();
        let info = prepare(&mut s, &f, false);
        let before = super::super::ledger::inspect(&s.connection).unwrap();
        s.connection
            .execute_batch(&crate::test_schema::before_publication(
                "PRAGMA user_version=32;",
            ))
            .unwrap();
        if collision {
            s.connection
                .execute_batch("CREATE TABLE sparse_publications(wrong TEXT)")
                .unwrap();
        }
        drop(s);
        let opened = State::open_managed(&path);
        if collision {
            assert!(opened.is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
                    .unwrap(),
                32
            );
            assert!(db.prepare("SELECT * FROM sparse_publication_role").is_err());
            assert!(
                db.prepare("SELECT * FROM sparse_publication_checks")
                    .is_err()
            );
        } else {
            let mut s = opened.unwrap();
            assert_eq!(s.info().unwrap().schema_version, crate::SCHEMA_VERSION);
            assert!(super::super::ledger::inspect(&s.connection).unwrap() == before);
            assert_eq!(
                pj(&mut s).info(info.id().unwrap()).unwrap().view(),
                info.view()
            );
        }
    }
}
#[test]
fn restricted_recovery_replays_saved_metadata_despite_source_corruption_without_repair() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("recovery");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let info = prepare(&mut s, &f, false);
    let p = info.id().unwrap();
    j(&mut s).check(p, 0, Some(&proof(&f))).unwrap();
    physical(&mut s, p);
    let r = j(&mut s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    let r = j(&mut s).reserved(p, reserved(&r)).unwrap();
    let r = j(&mut s)
        .prepare_publish(p, r.reservation().unwrap().sequence)
        .unwrap();
    assert!(RecoveryStore::open(&path).is_err());
    drop(s);
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    db.execute(
        "UPDATE sparse_preparation_blobs SET data=zeroblob(length(data))",
        [],
    )
    .unwrap();
    drop(db);
    assert!(State::open_managed(&path).is_err());
    let mut recovery = RecoveryStore::open(&path).unwrap();
    assert_eq!(recovery.get(p).unwrap(), r);
    let done = recovery.published(p, published(&r)).unwrap();
    assert!(done.receipt().is_some());
    drop(recovery);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    db.execute("UPDATE sparse_publications SET state_hash=zeroblob(32)", [])
        .unwrap();
    drop(db);
    assert!(RecoveryStore::open(&path).is_err());
}
mod crash;
mod faults;

mod lineage;

mod settlement;

#[test]
fn chunk_namespace_barrier_retains_unknown_and_remotely_committed_roots_until_local_source_ack() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let r = lineage::first(&mut s, &f);
    let volume = f.binding.base().unwrap().volume().to_owned();
    let node = f.binding.base().unwrap().node().to_owned();
    let key = crate::chunk_mount::admit(
        &mut s,
        crate::chunk_mount::Input {
            volume: volume.clone(),
            controls: vec![crate::namespace_client::Control {
                store: f.store.clone(),
                source: *f.binding.source(),
                fingerprint: f.binding.fingerprint().into(),
            }],
        },
    )
    .unwrap();
    assert!(namespace_pending(&s.connection, &volume, &node).is_err());
    assert!(namespace_pending(&s.connection, &volume, &hex(&[99; 16])).is_ok());
    assert!(
        crate::sparse_views::namespace_guard(
            &s.connection,
            &volume,
            &crate::namespace_client::Entry::from_node(&f.node).unwrap()
        )
        .is_err()
    );
    let r = lineage::commit(&mut s, r);
    assert!(
        namespace_pending(&s.connection, &volume, &node).is_err(),
        "remote commit is not local source acknowledgment"
    );
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert!(namespace_pending(&s.connection, &volume, &node).is_err());
    let source = j(&mut s)
        .acknowledge(
            [44; 32],
            r.input.plan,
            crate::sparse_views::Options::default(),
        )
        .unwrap();
    assert!(namespace_pending(&s.connection, &volume, &node).is_ok());
    let after = &source.record().receipt().unwrap().node;
    crate::sparse_views::namespace_guard(
        &s.connection,
        &volume,
        &crate::namespace_client::Entry::from_node(after).unwrap(),
    )
    .unwrap();
    crate::namespace_client::Journal::for_chunks(&mut s, crate::journal::Options::default(), &key)
        .unwrap();
}
