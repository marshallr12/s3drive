//! Node-wide barriers use real durable journals; remote receipts in this unit
//! fixture are modeled. This is not provider or native lifecycle acceptance.
use super::*;
use crate::namespace_client::{self as names, opens};
use crate::sparse_views::{self as views, View};

fn chunk(s: &mut State, f: &Fixture) -> crate::chunk_mount::Key {
    crate::chunk_mount::admit(
        s,
        crate::chunk_mount::Input {
            volume: f.binding.base().unwrap().volume().into(),
            controls: vec![names::Control {
                store: f.store.clone(),
                source: *f.binding.source(),
                fingerprint: f.binding.fingerprint().into(),
            }],
        },
    )
    .unwrap()
}
fn clean(s: &mut State, f: &Fixture) -> View {
    views::Journal::new(s, views::Options::default())
        .unwrap()
        .begin([1; 32], f.binding.clone(), 17)
        .unwrap()
}
fn request(f: &Fixture, view: &View, n: u8) -> Request {
    Request {
        operation: [n; 32],
        view: view.id(),
        generation: [n; 16],
        base_root: f.base.root.encode().to_vec(),
        base_page: f.base.page.encode(),
    }
}
fn pending(s: &State, f: &Fixture) -> Result<()> {
    let b = f.binding.base().unwrap();
    namespace_pending(&s.connection, b.volume(), b.node())
}
fn removal(f: &Fixture, node: &ns::Node, op: u8) -> names::Input {
    names::Input {
        request: ns::Apply {
            volume: f.binding.base().unwrap().volume().into(),
            operation: hex(&[op; 16]),
            expected_sequence: node.revision,
            mutation: ns::Mutation::RemoveEntry {
                node: ns::NodeGuard {
                    id: node.id.clone(),
                    revision: node.revision,
                },
                parent: ns::NodeGuard {
                    id: ns::ROOT.into(),
                    revision: 0,
                },
            },
        },
        store: f.store.clone(),
        source: *f.binding.source(),
        fingerprint: f.binding.fingerprint().into(),
        entries: vec![names::Entry::from_node(node).unwrap()],
        destination_control: None,
    }
}
fn names<'a>(s: &'a mut State, key: &crate::chunk_mount::Key) -> names::Journal<'a> {
    names::Journal::for_chunks(s, crate::journal::Options::default(), key).unwrap()
}
fn opens<'a>(s: &'a mut State, key: &crate::chunk_mount::Key) -> opens::Journal<'a> {
    opens::Journal::for_chunks(s, key).unwrap()
}
fn open(s: &mut State, f: &Fixture, key: &crate::chunk_mount::Key) -> opens::OpenInput {
    let input = opens::OpenInput {
        id: hex(&[60; 16]),
        session: hex(&[61; 16]),
        volume: key.volume().into(),
        node: f.node.id.clone(),
        store: f.store.clone(),
        source: *f.binding.source(),
        fingerprint: f.binding.fingerprint().into(),
        delete_on_close: false,
        controlled: true,
        session_bound: true,
        posix: false,
        admission: None,
        creation: None,
    };
    let mut j = opens(s, key);
    j.start_session(&input.session, &input.volume).unwrap();
    j.prepare(input.clone()).unwrap();
    j.activate(&input.id, &hex(&[62; 16])).unwrap();
    input
}
fn close(s: &mut State, key: &crate::chunk_mount::Key, input: &opens::OpenInput) {
    let mut j = opens(s, key);
    j.callback(
        &input.id,
        &hex(&[63; 16]),
        opens::Callback::Cleanup { flags: 0 },
    )
    .unwrap();
    j.complete(&input.id, &hex(&[64; 16]), &hex(&[63; 16]))
        .unwrap();
    j.callback(&input.id, &hex(&[65; 16]), opens::Callback::Close)
        .unwrap();
    j.complete(&input.id, &hex(&[66; 16]), &hex(&[65; 16]))
        .unwrap();
}
fn ack(s: &mut State, r: &Record) -> SourceReceipt {
    j(s).acknowledge([40; 32], r.input.plan, views::Options::default())
        .unwrap()
}

#[test]
fn sparse_barrier_clean_preparation_without_root_blocks_namespace_and_survives_restart() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("clean café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let v = clean(&mut s, &f);
    pending(&s, &f).unwrap();
    let i = pj(&mut s).begin(request(&f, &v, 9)).unwrap();
    assert!(i.root().unwrap().is_none());
    assert!(ledger::inspect(&s.connection).unwrap().records.is_empty());
    let before = super::super::super::ledger::inspect(&s.connection).unwrap();
    assert!(pending(&s, &f).is_err());
    assert!(names(&mut s, &key).begin(removal(&f, &f.node, 80)).is_err());
    assert!(names(&mut s, &key).pending(None, 100).unwrap().is_empty());
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == before);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert!(pending(&s, &f).is_err());
    assert_eq!(
        pj(&mut s).begin(request(&f, &v, 9)).unwrap().id().unwrap(),
        i.id().unwrap()
    );
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == before);
}

#[test]
fn sparse_barrier_consumed_physical_claim_and_complete_tree_need_source_settlement() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("claimed café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let i = prepare(&mut s, &f, false);
    let p = i.id().unwrap();
    let pos = pj(&mut s).transfers(p, None, 128).unwrap()[0].position();
    let transfer = pj(&mut s)
        .materialize(p, pos, crate::journal::Options::default())
        .unwrap()
        .unwrap();
    let generation = transfer.generation().unwrap();
    let mut remote =
        crate::remote::Journal::new(&mut s, crate::remote::Options::default()).unwrap();
    let saved = remote.prepare(generation).unwrap();
    let _consumed = remote.claim(saved.operation, &saved.target).unwrap();
    assert!(pending(&s, &f).is_err());
    assert!(ledger::inspect(&s.connection).unwrap().records.is_empty());
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert!(pending(&s, &f).is_err());
    let mut remote =
        crate::remote::Journal::new(&mut s, crate::remote::Options::default()).unwrap();
    assert!(remote.claim(saved.operation, &saved.target).is_err());
    remote
        .verified(crate::remote::Verification {
            operation: saved.operation,
            target: saved.target,
            payload_sha256: saved.payload_sha256,
            size: saved.size,
            revision: crate::remote::Revision::new("\"observed-model\"", None).unwrap(),
        })
        .unwrap();
    remote.acknowledge(saved.operation).unwrap();
    // Observe the consumed claim without resending, then complete later objects.
    for t in pj(&mut s).transfers(p, Some(pos), 128).unwrap() {
        let t = pj(&mut s)
            .materialize(p, t.position(), crate::journal::Options::default())
            .unwrap()
            .unwrap();
        let mut remote =
            crate::remote::Journal::new(&mut s, crate::remote::Options::default()).unwrap();
        let saved = remote.prepare(t.generation().unwrap()).unwrap();
        let _permit = remote.claim(saved.operation, &saved.target).unwrap();
        remote
            .verified(crate::remote::Verification {
                operation: saved.operation,
                target: saved.target,
                payload_sha256: saved.payload_sha256,
                size: saved.size,
                revision: crate::remote::Revision::new("\"owned-model\"", None).unwrap(),
            })
            .unwrap();
        remote.acknowledge(saved.operation).unwrap();
    }
    pj(&mut s).tree_receipt(p).unwrap();
    assert!(pending(&s, &f).is_err());
    assert!(ledger::inspect(&s.connection).unwrap().records.is_empty());
}

#[test]
fn sparse_barrier_ack_of_one_plan_never_hides_another_plan_on_the_same_clean_node() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let r = lineage::first(&mut s, &f);
    let view = pj(&mut s).info(r.input.plan).unwrap().view().clone();
    let other = pj(&mut s).begin(request(&f, &view, 20)).unwrap();
    let r = lineage::commit(&mut s, r);
    let a = ack(&mut s, &r);
    assert!(a.acknowledgment().clean_view().is_some());
    assert!(pending(&s, &f).is_err());
    let node = &r.receipt().unwrap().node;
    assert!(names(&mut s, &key).begin(removal(&f, node, 80)).is_err());
    assert_eq!(pj(&mut s).info(other.id().unwrap()).unwrap().view(), &view);
    assert_eq!(ack(&mut s, &r), a);
}

#[test]
fn sparse_barrier_rejected_root_is_not_a_source_acknowledgment_or_discard() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let f = fixture();
    let r = lineage::first(&mut s, &f);
    let before = views::ledger::inspect(&s.connection).unwrap();
    j(&mut s)
        .reject(r.input.plan, r.input.reserve.clone(), ns::Error::Conflict)
        .unwrap();
    assert!(pending(&s, &f).is_err());
    assert!(
        j(&mut s)
            .acknowledge([40; 32], r.input.plan, views::Options::default())
            .is_err()
    );
    assert!(views::ledger::inspect(&s.connection).unwrap() == before);
}

#[test]
fn sparse_barrier_filters_nodes_only_after_validating_the_complete_source_graph() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let v = clean(&mut s, &f);
    pj(&mut s).begin(request(&f, &v, 9)).unwrap();
    let volume = f.binding.base().unwrap().volume().to_owned();
    namespace_pending(&s.connection, &volume, &hex(&[99; 16])).unwrap();
    namespace_pending(&s.connection, &hex(&[99; 16]), &f.node.id).unwrap();
    let mut unrelated = f.node.clone();
    unrelated.id = hex(&[99; 16]);
    unrelated.kind = ns::Kind::EmptyFile;
    unrelated.payload = None;
    // A real public namespace admission for another node remains available.
    names(&mut s, &key)
        .begin(removal(&f, &unrelated, 80))
        .unwrap();
    s.connection
        .execute("UPDATE sparse_preparations SET state_hash=zeroblob(32)", [])
        .unwrap();
    assert!(namespace_pending(&s.connection, &volume, &hex(&[99; 16])).is_err());
    assert!(namespace_pending(&s.connection, &hex(&[99; 16]), &f.node.id).is_err());
}

#[test]
fn sparse_barrier_pending_namespace_refuses_fresh_preparation_without_persisting_it() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let v = clean(&mut s, &f);
    let input = removal(&f, &f.node, 80);
    let saved = names(&mut s, &key).begin(input.clone()).unwrap();
    let before = super::super::super::ledger::inspect(&s.connection).unwrap();
    assert!(pj(&mut s).begin(request(&f, &v, 9)).is_err());
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == before);
    assert_eq!(names(&mut s, &key).begin(input.clone()).unwrap(), saved);
    names(&mut s, &key)
        .acknowledge(&input.request.operation, Err(ns::Error::Conflict))
        .unwrap();
    pj(&mut s).begin(request(&f, &v, 9)).unwrap();
}

#[test]
fn sparse_barrier_acknowledged_removal_requires_future_typed_owned_admission() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let v = clean(&mut s, &f);
    let mut input = removal(&f, &f.node, 80);
    let ns::Mutation::RemoveEntry { node, parent } = input.request.mutation.clone() else {
        panic!()
    };
    input.request.mutation = ns::Mutation::ArmRemoval {
        node,
        parent,
        owner: key.owner().into(),
    };
    names(&mut s, &key).begin(input.clone()).unwrap();
    let fence = ns::RemovalFence {
        intent: input.request.operation.clone(),
        owner: key.owner().into(),
    };
    let mut node = f.node.clone();
    node.revision = 6;
    names(&mut s, &key)
        .acknowledge(
            &input.request.operation,
            Ok(ns::Receipt {
                sequence: 6,
                node: node.clone(),
                artifact: Some(ns::Artifact::Removal(fence.clone())),
            }),
        )
        .unwrap();
    assert!(pj(&mut s).begin(request(&f, &v, 9)).is_err());
    let mut cancel = removal(&f, &node, 81);
    cancel.request.mutation = ns::Mutation::CancelRemoval {
        node: node.id.clone(),
        fence: fence.clone(),
    };
    names(&mut s, &key).begin(cancel.clone()).unwrap();
    node.revision = 7;
    names(&mut s, &key)
        .acknowledge(
            &cancel.request.operation,
            Ok(ns::Receipt {
                sequence: 7,
                node,
                artifact: Some(ns::Artifact::Removal(fence)),
            }),
        )
        .unwrap();
    pj(&mut s).begin(request(&f, &v, 9)).unwrap();
}

#[test]
fn sparse_barrier_settled_preparation_replays_original_bytes_during_later_namespace_uncertainty() {
    let t = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&t.path().join("s")).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let r = lineage::first(&mut s, &f);
    let info = pj(&mut s).info(r.input.plan).unwrap();
    let r = lineage::commit(&mut s, r);
    ack(&mut s, &r);
    let input = removal(&f, &r.receipt().unwrap().node, 80);
    names(&mut s, &key).begin(input).unwrap();
    let before = super::super::super::ledger::inspect(&s.connection).unwrap();
    let recovered = pj(&mut s).begin(request(&f, info.view(), 9)).unwrap();
    assert_eq!(recovered.id().unwrap(), info.id().unwrap());
    assert_eq!(recovered.root().unwrap(), info.root().unwrap());
    assert!(pj(&mut s).begin(request(&f, info.view(), 20)).is_err());
    assert!(super::super::super::ledger::inspect(&s.connection).unwrap() == before);
}

#[test]
fn sparse_barrier_retirement_waits_for_exact_ack_but_historical_retirement_stays_replayable() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("retire café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let input = open(&mut s, &f, &key);
    close(&mut s, &key, &input);
    let r = lineage::first(&mut s, &f);
    let original = opens(&mut s, &key).get(&input.id).unwrap();
    assert!(opens(&mut s, &key).retire(&input.id).is_err());
    let r = lineage::commit(&mut s, r);
    assert!(opens(&mut s, &key).retire(&input.id).is_err());
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert!(opens(&mut s, &key).retire(&input.id).is_err());
    ack(&mut s, &r);
    opens(&mut s, &key).retire(&input.id).unwrap();
    let info = pj(&mut s).info(r.input.plan).unwrap();
    pj(&mut s).begin(request(&f, info.view(), 20)).unwrap();
    assert!(pending(&s, &f).is_err());
    opens(&mut s, &key).retire(&input.id).unwrap();
    assert_eq!(opens(&mut s, &key).get(&input.id).unwrap(), original);
    assert_eq!(
        opens(&mut s, &key)
            .callback(&input.id, &hex(&[65; 16]), opens::Callback::Close)
            .unwrap(),
        original
    );
}

#[test]
fn sparse_barrier_obsolete_session_cannot_abandon_a_preparation_without_a_root_record() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("abandon café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let input = open(&mut s, &f, &key);
    let v = clean(&mut s, &f);
    pj(&mut s).begin(request(&f, &v, 9)).unwrap();
    let original = opens(&mut s, &key).get(&input.id).unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let mut j = opens(&mut s, &key);
    j.start_session(&hex(&[70; 16]), key.volume()).unwrap();
    assert!(
        j.prepare_recovery(&hex(&[70; 16]), key.volume(), &input.node, None)
            .is_err()
    );
    assert!(
        j.pending_recovery(key.volume(), &input.node)
            .unwrap()
            .is_none()
    );
    assert_eq!(j.get(&input.id).unwrap(), original);
}

#[test]
fn sparse_barrier_session_finish_rechecks_later_work_and_replays_only_a_committed_proof() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("finish café 🦀");
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let key = chunk(&mut s, &f);
    let input = open(&mut s, &f, &key);
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    let mut jopen = opens(&mut s, &key);
    jopen.start_session(&hex(&[70; 16]), key.volume()).unwrap();
    let recovery = jopen
        .prepare_recovery(&hex(&[70; 16]), key.volume(), &input.node, None)
        .unwrap();
    let r = lineage::first(&mut s, &f);
    assert!(
        opens(&mut s, &key)
            .finish_recovery(&hex(&[70; 16]), &recovery.input.id)
            .is_err()
    );
    assert!(!opens(&mut s, &key).recovered(&input.id).unwrap());
    let r = lineage::commit(&mut s, r);
    assert!(
        opens(&mut s, &key)
            .finish_recovery(&hex(&[70; 16]), &recovery.input.id)
            .is_err()
    );
    ack(&mut s, &r);
    assert!(
        opens(&mut s, &key)
            .finish_recovery(&hex(&[70; 16]), &recovery.input.id)
            .unwrap()
            .finished
    );
    let info = pj(&mut s).info(r.input.plan).unwrap();
    pj(&mut s).begin(request(&f, info.view(), 20)).unwrap();
    assert!(
        opens(&mut s, &key)
            .finish_recovery(&hex(&[70; 16]), &recovery.input.id)
            .unwrap()
            .finished
    );
    assert!(opens(&mut s, &key).retire(&input.id).is_err());
}

#[path = "owned.rs"]
mod owned;
