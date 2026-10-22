//! Actual SDK/HTTP and durable private-owner tests. No native activation.
use super::*;
use s3drive_managed::chunks::{
    MixedRootFailure, MixedRootRecovery, OwnedPreparation, Preparation, Reader,
};
use s3drive_state::{
    chunk_mount, namespace_client as names,
    namespace_client::opens,
    sparse_views::{
        self,
        preparation::{self as plans, owned::Settlement, publication as roots},
    },
};
struct Case {
    e: Executor,
    chunks: chunk_mount::Client,
    reader: Reader,
    local: sparse_views::Client,
    view: sparse_views::View,
    ownership: OwnedPreparation,
}
async fn setup_case(
    config: &s3drive_core::Config,
    root: &Path,
    s: &Server,
    committed: bool,
) -> Case {
    let (e, chunks, creator, draft) = creator_import::setup_owner(config, root, s).await;
    let created = creator.create(draft).await.unwrap();
    let n = created.metadata().node.clone();
    let volume = chunks.key().volume().to_owned();
    let store = empty_creation::store(s).await;
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    let opens = chunks.opens();
    opens.start_session(id(80), volume.clone()).await.unwrap();
    opens
        .prepare(opens::OpenInput {
            id: id(71),
            session: id(80),
            volume: volume.clone(),
            node: n.id.clone(),
            store: store.clone(),
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
            delete_on_close: true,
            controlled: true,
            session_bound: true,
            posix: false,
            admission: None,
            creation: None,
        })
        .await
        .unwrap();
    opens.activate(id(71), id(72)).await.unwrap();
    let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
        .await
        .unwrap();
    let base = reader.open_file(&n.id).await.unwrap().pinned_base();
    let local = e
        .storage()
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let clean = local
        .begin([1; 32], base.sparse_binding().unwrap(), 0)
        .await
        .unwrap();
    let parent = node(&s.direct, &volume, s3drive_authority::ROOT).await;
    let mut input = names::Input {
        request: Apply {
            volume: volume.clone(),
            operation: id(81),
            expected_sequence: info(&s.direct).await.sequence,
            mutation: Mutation::ArmRemoval {
                node: s3drive_authority::NodeGuard {
                    id: n.id.clone(),
                    revision: n.revision,
                },
                parent: s3drive_authority::NodeGuard {
                    id: parent.id.clone(),
                    revision: parent.revision,
                },
                owner: chunks.key().owner().into(),
            },
        },
        store,
        source: *target.source(),
        fingerprint: target.fingerprint().into(),
        entries: vec![names::Entry::from_node(&n).unwrap()],
        destination_control: None,
    };
    let ns = chunks.namespace();
    ns.begin(input.clone()).await.unwrap();
    let Reply::Applied(receipt) = ask(
        &s.direct,
        Request::Apply {
            request: input.request.clone(),
        },
    )
    .await
    else {
        panic!()
    };
    ns.acknowledge(id(81), Ok(receipt.clone())).await.unwrap();
    let fence = s3drive_authority::RemovalFence {
        intent: id(81),
        owner: chunks.key().owner().into(),
    };
    let mut claim = s3drive_authority::RemovalPublicationClaim {
        fence: fence.clone(),
        previous: id(81),
        mode: s3drive_authority::RemovalPublicationMode::Pending,
    };
    if committed {
        let parent = node(&s.direct, &volume, s3drive_authority::ROOT).await;
        input.request.operation = id(82);
        input.request.expected_sequence = info(&s.direct).await.sequence;
        input.request.mutation = Mutation::CommitRemoval {
            node: s3drive_authority::NodeGuard {
                id: n.id.clone(),
                revision: receipt.node.revision,
            },
            parent: s3drive_authority::NodeGuard {
                id: parent.id,
                revision: parent.revision,
            },
            fence,
        };
        input.entries = vec![names::Entry::from_node(&receipt.node).unwrap()];
        ns.begin(input.clone()).await.unwrap();
        let Reply::Applied(receipt) = ask(
            &s.direct,
            Request::Apply {
                request: input.request.clone(),
            },
        )
        .await
        else {
            panic!()
        };
        ns.acknowledge(id(82), Ok(receipt)).await.unwrap();
        claim.previous = id(82);
        claim.mode = s3drive_authority::RemovalPublicationMode::Committed;
    }
    let view = local
        .write(
            [2; 32],
            clean.id(),
            11 * 1024 * 1024 + 3,
            Arc::from("日本語 🦀\0".as_bytes()),
        )
        .await
        .unwrap();
    Case {
        e,
        chunks,
        reader,
        local,
        view,
        ownership: OwnedPreparation {
            open: id(71),
            claim,
        },
    }
}
impl Case {
    async fn prep(&self, n: u8, view: &sparse_views::View) -> Preparation {
        self.reader
            .reopen_edited(self.local.clone(), view.id())
            .await
            .unwrap()
            .begin_owned_preparation(
                [n; 32],
                [n; 16],
                plans::Options::default(),
                &self.chunks,
                self.ownership.clone(),
            )
            .await
            .unwrap()
    }
    async fn ready(&self, n: u8) -> (Preparation, s3drive_managed::chunks::MixedPublisher) {
        let prep = self.prep(n, &self.view).await;
        prep.prepare().await.unwrap();
        prep.uploader(self.e.clone())
            .unwrap()
            .upload_tree()
            .await
            .unwrap();
        let publisher = prep
            .publisher(self.e.clone(), roots::Limits::default())
            .unwrap();
        publisher.check_semantics().await.unwrap();
        (prep, publisher)
    }
}
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server, committed: bool) {
    let c = setup_case(config, root, s, committed).await;
    let (prep, p) = c.ready(3).await;
    let ownership = prep.info().await.unwrap().ownership().unwrap().clone();
    assert_eq!(ownership.original_open().id, id(71));
    assert_eq!(ownership.claim(), &c.ownership.claim);
    s.proxy.mode.store(24, Ordering::SeqCst);
    assert_eq!(
        p.begin(id(101))
            .await
            .unwrap_err()
            .downcast_ref::<MixedRootFailure>(),
        Some(&MixedRootFailure::UnsupportedFeature)
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    let r = p.begin(id(101)).await.unwrap();
    let Mutation::ReserveManifest { guard, .. } = &r.input().reserve().mutation else {
        panic!()
    };
    assert_eq!(guard.removal_claim.as_deref(), Some(&c.ownership.claim));
    assert_eq!(guard.detached, committed);
    let newer = c
        .local
        .write([4; 32], c.view.id(), 2, Arc::from(&b"newer"[..]))
        .await
        .unwrap();
    s.proxy.mode.store(1, Ordering::SeqCst);
    assert!(p.resume().await.is_err());
    assert!(
        p.recovery()
            .recover(prep.id())
            .await
            .unwrap()
            .reservation()
            .is_some()
    );
    for (narrow, expired) in [(true, false), (false, true)] {
        let denied = executor(config, c.e.storage(), narrow, expired).await;
        s.proxy.clear();
        assert!(
            prep.publisher(denied, roots::Limits::default())
                .unwrap()
                .resume()
                .await
                .is_err()
        );
        assert!(s.proxy.applies().is_empty());
    }
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(p.resume().await.is_err());
    let saved = p.recovery().recover(prep.id()).await.unwrap();
    assert_eq!(saved.input(), r.input());
    assert!(saved.receipt().is_some());
    let ack = p
        .acknowledge([5; 32], sparse_views::Options::default())
        .await
        .unwrap();
    assert_eq!(ack.observed_head(), newer.id());
    assert!(ack.clean_view_id().is_none());
    assert_eq!(
        c.local
            .current_head(c.view.binding().clone())
            .await
            .unwrap()
            .id(),
        newer.id()
    );
    let next = c.prep(6, &newer).await;
    next.prepare().await.unwrap();
    next.uploader(c.e.clone())
        .unwrap()
        .upload_tree()
        .await
        .unwrap();
    let np = next
        .publisher(c.e.clone(), roots::Limits::default())
        .unwrap();
    np.check_semantics().await.unwrap();
    let nr = np.begin(id(102)).await.unwrap();
    assert_eq!(nr.input().predecessor(), Some(prep.id()));
    np.resume().await.unwrap();
    let ack = np
        .acknowledge([7; 32], sparse_views::Options::default())
        .await
        .unwrap();
    assert!(ack.clean_view_id().is_some());
    assert!(
        prep.settle(Settlement::Rejected { operation: [8; 32] })
            .await
            .is_err()
    );
    c.e.storage().close().await.unwrap();
}
pub async fn controls(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = setup_case(config, root, s, false).await;
    let edit = c
        .reader
        .reopen_edited(c.local.clone(), c.view.id())
        .await
        .unwrap();
    assert!(
        edit.begin_preparation([3; 32], [3; 16], plans::Options::default())
            .await
            .is_err()
    );
    let mut wrong = c.ownership.clone();
    wrong.claim.previous = id(99);
    assert!(
        edit.begin_owned_preparation(
            [3; 32],
            [3; 16],
            plans::Options::default(),
            &c.chunks,
            wrong
        )
        .await
        .is_err()
    );
    let prep = c.prep(3, &c.view).await;
    s.proxy.mode.store(8, Ordering::SeqCst);
    s.proxy.clear();
    assert_eq!(
        edit.begin_owned_preparation(
            [3; 32],
            [3; 16],
            plans::Options::default(),
            &c.chunks,
            c.ownership.clone()
        )
        .await
        .unwrap()
        .id(),
        prep.id()
    );
    assert!(s.proxy.requests().is_empty());
    let settled = prep
        .settle(Settlement::NoEffect { operation: [4; 32] })
        .await
        .unwrap();
    assert_eq!(
        settled.settlement(),
        Some(&Settlement::NoEffect { operation: [4; 32] })
    );
    prep.settle(Settlement::NoEffect { operation: [4; 32] })
        .await
        .unwrap();
    assert!(
        prep.settle(Settlement::NoEffect { operation: [5; 32] })
            .await
            .is_err()
    );
    assert!(prep.prepare().await.is_err());
    assert_eq!(
        c.local
            .current_head(c.view.binding().clone())
            .await
            .unwrap()
            .id(),
        c.view.id()
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    let pending = c.prep(6, &c.view).await;
    pending.prepare().await.unwrap();
    let client = c.chunks.preparations(plans::Options::default()).unwrap();
    let transfer = client
        .transfers(pending.id(), None, 1)
        .await
        .unwrap()
        .remove(0);
    client
        .materialize(pending.id(), transfer.position(), &c.e.storage())
        .await
        .unwrap();
    assert!(
        pending
            .settle(Settlement::NoEffect { operation: [7; 32] })
            .await
            .is_err()
    );
    let st = c.e.storage();
    st.close().await.unwrap();
    let st = storage(root, "client").await;
    let e = executor(config, st.clone(), false, false).await;
    let local = st.sparse_views(sparse_views::Options::default()).unwrap();
    let reader = Reader::open(
        s.wire.clone(),
        c.chunks.key().volume().into(),
        vec![(id(200), e.reader())],
    )
    .await
    .unwrap();
    let pending = reader
        .reopen_preparation(local, pending.id(), plans::Options::default())
        .await
        .unwrap();
    assert!(
        pending
            .uploader(e)
            .unwrap()
            .upload_object(transfer.position())
            .await
            .is_err(),
        "obsolete unclaimed session was sent"
    );
    st.close().await.unwrap();
}
pub async fn rejected(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = setup_case(config, root, s, false).await;
    let (prep, p) = c.ready(3).await;
    p.begin(id(101)).await.unwrap();
    s.proxy.mode.store(23, Ordering::SeqCst);
    let task = {
        let p = p.clone();
        tokio::spawn(async move { p.resume().await })
    };
    s.proxy.publish_gate.wait().await;
    change(
        &s.direct,
        90,
        Mutation::CancelRemoval {
            node: id(40),
            fence: c.ownership.claim.fence.clone(),
        },
    )
    .await;
    s.proxy.publish_gate.wait().await;
    assert_eq!(
        task.await
            .unwrap()
            .unwrap_err()
            .downcast_ref::<MixedRootFailure>(),
        Some(&MixedRootFailure::RetainedConflict)
    );
    let records = c
        .chunks
        .preparations(plans::Options::default())
        .unwrap()
        .publications(roots::Limits::default())
        .unwrap();
    let r = records.get(prep.id()).await.unwrap();
    assert!(r.rejection().is_some());
    assert!(r.receipt().is_none());
    s.proxy.mode.store(8, Ordering::SeqCst);
    s.proxy.clear();
    assert!(p.recovery().recover(prep.id()).await.is_err());
    assert!(s.proxy.requests().is_empty());
    prep.settle(Settlement::Rejected { operation: [4; 32] })
        .await
        .unwrap();
    assert!(
        p.acknowledge([5; 32], sparse_views::Options::default())
            .await
            .is_err()
    );
    assert_eq!(
        c.local
            .current_head(c.view.binding().clone())
            .await
            .unwrap()
            .id(),
        c.view.id()
    );
    c.e.storage().close().await.unwrap();
}
pub async fn crash(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = setup_case(config, root, s, true).await;
    let (prep, p) = c.ready(3).await;
    p.begin(id(101)).await.unwrap();
    s.proxy.mode.store(1, Ordering::SeqCst);
    assert!(p.resume().await.is_err());
    p.recovery().recover(prep.id()).await.unwrap();
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(p.resume().await.is_err());
    let r = c
        .chunks
        .preparations(plans::Options::default())
        .unwrap()
        .publications(roots::Limits::default())
        .unwrap()
        .get(prep.id())
        .await
        .unwrap();
    assert!(r.publication().is_some());
    assert!(r.receipt().is_none());
    assert!(
        prep.settle(Settlement::Rejected { operation: [4; 32] })
            .await
            .is_err()
    );
    std::fs::write(
        root.join("owned-saved.json"),
        serde_json::to_vec(&r).unwrap(),
    )
    .unwrap();
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}
pub async fn recover(root: &Path, s: &Server) {
    let saved: roots::Record =
        serde_json::from_slice(&std::fs::read(root.join("owned-saved.json")).unwrap()).unwrap();
    assert!(
        Storage::open(
            &root.join("client"),
            cache::Options::default(),
            journal::Options::default()
        )
        .await
        .is_err()
    );
    let recovery = MixedRootRecovery::open_existing(
        &root.join("client"),
        s.wire.clone(),
        saved.input().reserve().volume.clone(),
        saved.input().store().id.clone(),
    )
    .await
    .unwrap();
    s.proxy.clear();
    let done = recovery.recover(saved.input().plan()).await.unwrap();
    assert_eq!(done.input(), saved.input());
    assert_eq!(done.publication(), saved.publication());
    assert!(done.receipt().is_some());
    assert!(
        recovery
            .acknowledged(saved.input().plan())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(s.proxy.requests().len(), 1);
    assert_eq!(
        s.proxy.applies(),
        vec![saved.publication().unwrap().clone()]
    );
    s.proxy.clear();
    recovery.recover(saved.input().plan()).await.unwrap();
    assert!(s.proxy.requests().is_empty());
    recovery.close().await.unwrap();
}
