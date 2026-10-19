//! Live authority + SDK, local dummy objects only. Never provider writes.
use super::*;
use s3drive_managed::chunks::{CleanObservation, ObservationKind};
use s3drive_state::{
    sparse_views::{self, ScopedOperation, View},
    worker::Storage,
};
use std::{path::Path, sync::Arc};
fn token(v: &View, n: u8) -> ScopedOperation {
    ScopedOperation::new(v.binding(), [n; 32]).unwrap()
}
async fn setup(f: &Fixture, r: &Reader, path: &Path) -> (Storage, sparse_views::Client, View) {
    let base = r.open_file(&id(10)).await.unwrap();
    assert!(base.metadata().name.contains("🦀"));
    let b = base.pinned_base().sparse_binding().unwrap();
    let size = b.base_size();
    let (storage, local) = composition::storage(path).await;
    local.enable_scoped().await.unwrap();
    let v = local
        .begin_scoped(ScopedOperation::new(&b, [1; 32]).unwrap(), b, size)
        .await
        .unwrap();
    let _ = f;
    (storage, local, v)
}
async fn no_rpc(
    f: &Fixture,
    local: &sparse_views::Client,
    r: &Reader,
    v: &View,
    op: ScopedOperation,
) -> CleanObservation {
    f.s3.settle().await;
    let events = f.s3.events().len();
    let requests = f.requests();
    let result = r.observe_clean(local.clone(), op, v.id()).await.unwrap();
    assert_eq!(f.requests(), requests, "known admission queried authority");
    assert_eq!(f.s3.events().len(), events, "known admission touched S3");
    result
}
async fn conflict_without_rpc(
    f: &Fixture,
    local: &sparse_views::Client,
    r: &Reader,
    v: &View,
    n: u8,
) {
    f.s3.settle().await;
    let events = f.s3.events().len();
    let requests = f.requests();
    let e = r
        .observe_clean(local.clone(), token(v, n), v.id())
        .await
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<sparse_views::Failure>(),
        Some(&sparse_views::Failure::Conflict)
    );
    assert_eq!(f.requests(), requests);
    assert_eq!(f.s3.events().len(), events);
}
async fn available(local: &sparse_views::Client, id: sparse_views::Id) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match local.pin(id).await {
                Ok(p) => {
                    assert!(local.release_pin(p).await.unwrap());
                    return;
                }
                Err(e)
                    if e.downcast_ref::<sparse_views::pins::Failure>()
                        == Some(&sparse_views::pins::Failure::Quota)
                        || e.downcast_ref::<s3drive_state::worker::Failure>()
                            == Some(&s3drive_state::worker::Failure::QueueFull) =>
                {
                    tokio::task::yield_now().await
                }
                Err(e) => panic!("cleanup: {e}"),
            }
        }
    })
    .await
    .expect("read guard cleanup did not restore capacity");
}
pub async fn flow(f: &Fixture, path: &Path) {
    let original = "original\0café 🦀".as_bytes().to_vec();
    f.tree(
        10,
        original.len() as u64,
        BTreeMap::from([(0, original.clone())]),
    )
    .await;
    let r = f.reader().await;
    let local_path = path.join("observed café 🦀");
    let (storage, local, first) = setup(f, &r, &local_path).await;
    let old = r.reopen_edited(local.clone(), first.id()).await.unwrap();
    let narrow = storage
        .sparse_views(
            sparse_views::Options::default()
                .with_limits(sparse_views::Limits {
                    views: 2,
                    ..sparse_views::Limits::default()
                })
                .unwrap(),
        )
        .unwrap();
    let op = token(&first, 2);
    for _ in 0..8 {
        assert_eq!(
            r.observe_clean(narrow.clone(), op, first.id())
                .await
                .unwrap()
                .kind(),
            ObservationKind::Unchanged
        );
    }
    assert_eq!(local.recover_adoption(op, first.id()).await.unwrap(), None);
    let size = 129 * C + 3;
    f.tree(
        10,
        size,
        BTreeMap::from([(0, pattern(0, C)), (129, b"new".to_vec())]),
    )
    .await;
    let data_before = data_gets(f);
    let result = r.observe_clean(narrow, op, first.id()).await.unwrap();
    assert_eq!(result.kind(), ObservationKind::Adopted);
    assert_eq!(result.view().size(), size);
    assert_eq!(data_gets(f), data_before, "observation fetched file data");
    let v = result.view().clone();
    let edited = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    assert_eq!(edited.read_range(129 * C, 8).await.unwrap(), b"new");
    drop(edited);
    assert_eq!(old.read_range(0, 100).await.unwrap(), original);
    let mut manual = Vec::new();
    for _ in 0..sparse_views::pins::MAX_ACTIVE - 1 {
        manual.push(local.pin(v.id()).await.unwrap());
    }
    f.mode.store(37, Ordering::SeqCst);
    f.s3.fault(Target::Pages, Fault::HeadDenied);
    let recovered = no_rpc(f, &local, &r, &first, op).await;
    assert_eq!(recovered.kind(), ObservationKind::Recovered);
    assert_eq!(recovered.view(), &v);
    let requests = f.requests();
    let events = f.s3.events().len();
    let e = r
        .observe_clean(local.clone(), token(&v, 3), v.id())
        .await
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<sparse_views::pins::Failure>(),
        Some(&sparse_views::pins::Failure::Quota)
    );
    assert_eq!(f.requests(), requests);
    assert_eq!(f.s3.events().len(), events);
    f.mode.store(0, Ordering::SeqCst);
    f.s3.clear();
    for p in manual {
        local.release_pin(p).await.unwrap();
    }
    let mut head = v;
    for (n, bytes) in [(3, b"abc".to_vec()), (4, vec![])] {
        f.tree(
            10,
            bytes.len() as u64,
            if bytes.is_empty() {
                BTreeMap::new()
            } else {
                BTreeMap::from([(0, bytes.clone())])
            },
        )
        .await;
        let next = r
            .observe_clean(local.clone(), token(&head, n), head.id())
            .await
            .unwrap();
        assert_eq!(next.kind(), ObservationKind::Adopted);
        head = next.view().clone();
        let file = r.reopen_edited(local.clone(), head.id()).await.unwrap();
        assert_eq!(file.read_range(0, 10).await.unwrap(), bytes);
    }
    drop(old);
    storage.close().await.unwrap();
    let (storage, local) = composition::storage(&local_path).await;
    assert_eq!(
        no_rpc(f, &local, &r, &first, op).await.view(),
        result.view()
    );
    assert_eq!(
        local.current_head(first.binding().clone()).await.unwrap(),
        head
    );
    storage.close().await.unwrap();
}
pub async fn faults(f: &Fixture, path: &Path) {
    f.tree(10, 17, BTreeMap::from([(0, pattern(0, 17))])).await;
    let r = f.reader().await;
    let (storage, local, v) = setup(f, &r, &path.join("faults")).await;
    let changed = f.tree(10, 3, BTreeMap::from([(0, b"new".to_vec())])).await;
    for fault in [
        Fault::HeadDenied,
        Fault::GetDenied,
        Fault::Missing,
        Fault::BigHead,
        Fault::BadRange,
        Fault::BadEtag,
        Fault::Short,
        Fault::Extra,
    ] {
        f.s3.fault(Target::Exact(changed.root.clone()), fault);
        assert!(
            r.observe_clean(local.clone(), token(&v, 2), v.id())
                .await
                .is_err(),
            "{fault:?}"
        );
        assert_eq!(local.head(v.binding().clone()).await.unwrap(), v);
        assert_eq!(
            local.recover_adoption(token(&v, 2), v.id()).await.unwrap(),
            None
        );
        f.s3.clear();
    }
    for mode in [7, 8, 9, 10, 11, 14, 16, 31, 32, 33, 34, 35, 36, 37] {
        f.mode.store(mode, Ordering::SeqCst);
        assert!(
            r.observe_clean(local.clone(), token(&v, 2), v.id())
                .await
                .is_err(),
            "mode {mode}"
        );
        assert_eq!(local.head(v.binding().clone()).await.unwrap(), v);
    }
    f.mode.store(0, Ordering::SeqCst);
    *f.s3.expected_key.lock().unwrap() = "TESTROTATED".into();
    assert!(
        r.observe_clean(local.clone(), token(&v, 2), v.id())
            .await
            .is_err()
    );
    *f.s3.expected_key.lock().unwrap() = "TESTPROFILE".into();
    let next = r
        .observe_clean(local.clone(), token(&v, 2), v.id())
        .await
        .unwrap();
    let clean = next.view().clone();
    let dirty = local
        .resize_scoped(token(&clean, 3), clean.id(), clean.size())
        .await
        .unwrap();
    conflict_without_rpc(f, &local, &r, &dirty, 4).await;
    storage.close().await.unwrap();
    let (storage, local, v) = setup(f, &r, &path.join("pending")).await;
    let file = r.reopen_edited(local.clone(), v.id()).await.unwrap();
    let prep = file
        .begin_preparation(
            [55; 32],
            [55; 16],
            sparse_views::preparation::Options::default(),
        )
        .await
        .unwrap();
    conflict_without_rpc(f, &local, &r, &v, 5).await;
    drop(prep);
    drop(file);
    storage.close().await.unwrap();
    let base = r
        .open_file(&id(10))
        .await
        .unwrap()
        .pinned_base()
        .sparse_binding()
        .unwrap();
    let foreign = sparse_views::Binding::new(
        base.base().unwrap(),
        *base.store(),
        [9; 32],
        base.fingerprint().into(),
        base.root().into(),
        base.base_size(),
    )
    .unwrap();
    let (storage, local) = composition::storage(&path.join("foreign")).await;
    local.enable_scoped().await.unwrap();
    let v = local
        .begin_scoped(
            ScopedOperation::new(&foreign, [1; 32]).unwrap(),
            foreign.clone(),
            foreign.base_size(),
        )
        .await
        .unwrap();
    failed(
        r.observe_clean(local.clone(), token(&v, 2), v.id()).await,
        ReadFailure::Binding,
    );
    assert_eq!(local.head(foreign).await.unwrap(), v);
    storage.close().await.unwrap();
    f.create(12, "metadata empty 🦀", 200, false).await;
    let base = r
        .open_file(&id(10))
        .await
        .unwrap()
        .pinned_base()
        .sparse_binding()
        .unwrap();
    let descriptor = base.base().unwrap();
    let empty = sparse_views::Binding::new(
        s3drive_core::manifest::base::Descriptor::from_hex(
            descriptor.volume(),
            &id(12),
            descriptor.reservation(),
            descriptor.payload_reference(),
        )
        .unwrap(),
        *base.store(),
        *base.source(),
        base.fingerprint().into(),
        base.root().into(),
        0,
    )
    .unwrap();
    let (storage, local) = composition::storage(&path.join("metadata empty")).await;
    local.enable_scoped().await.unwrap();
    let v = local
        .begin_scoped(
            ScopedOperation::new(&empty, [1; 32]).unwrap(),
            empty.clone(),
            0,
        )
        .await
        .unwrap();
    failed(
        r.observe_clean(local.clone(), token(&v, 2), v.id()).await,
        ReadFailure::UnsupportedContent,
    );
    assert_eq!(local.head(empty).await.unwrap(), v);
    storage.close().await.unwrap();
}
pub async fn races(f: &Fixture, path: &Path) {
    f.tree(10, 17, BTreeMap::from([(0, pattern(0, 17))])).await;
    let r = f.reader().await;
    let (storage, local, v) = setup(f, &r, &path.join("race")).await;
    let changed = f.tree(10, 3, BTreeMap::from([(0, b"new".to_vec())])).await;
    f.s3.fault(Target::Exact(changed.root.clone()), Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let r = r.clone();
        let local = local.clone();
        let v = v.clone();
        tokio::spawn(async move { r.observe_clean(local, token(&v, 2), v.id()).await })
    };
    f.s3.wait(arrived + 1).await;
    f.tree(10, 4, BTreeMap::from([(0, b"rival"[..4].to_vec())]))
        .await;
    f.s3.clear();
    f.s3.permits.add_permits(1);
    failed(task.await.unwrap(), ReadFailure::Binding);
    assert_eq!(local.head(v.binding().clone()).await.unwrap(), v);
    // Prove the final provider HEAD is required, independently of initial HEAD
    // and verified GET. The handler handshake freezes a good root response first.
    for denied in [true, false] {
        let late = f.tree(10, 17, BTreeMap::from([(0, pattern(1, 17))])).await;
        let original_etag = f.s3.objects.lock().unwrap()[&late.root].etag.clone();
        f.s3.fault(Target::Exact(late.root.clone()), Fault::Gate);
        let arrived = f.s3.arrived.load(Ordering::SeqCst);
        let task = {
            let r = r.clone();
            let local = local.clone();
            let v = v.clone();
            tokio::spawn(async move { r.observe_clean(local, token(&v, 2), v.id()).await })
        };
        f.s3.wait(arrived + 1).await;
        if denied {
            f.s3.fault(Target::Exact(late.root.clone()), Fault::HeadDenied);
        } else {
            f.s3.objects
                .lock()
                .unwrap()
                .get_mut(&late.root)
                .unwrap()
                .etag = "\"late-revision\"".into();
        }
        f.s3.permits.add_permits(1);
        assert!(
            task.await.unwrap().is_err(),
            "late provider change was accepted"
        );
        assert_eq!(
            f.s3.events()
                .iter()
                .filter(|e| e.path == late.root && e.method == "HEAD")
                .count(),
            2
        );
        f.s3.clear();
        f.s3.objects
            .lock()
            .unwrap()
            .get_mut(&late.root)
            .unwrap()
            .etag = original_etag;
        assert_eq!(local.head(v.binding().clone()).await.unwrap(), v);
        assert_eq!(
            local.recover_adoption(token(&v, 2), v.id()).await.unwrap(),
            None
        );
    }
    // A rename during the same root handshake preserves payload/placement.
    let current = f
        .tree(10, 5, BTreeMap::from([(0, b"valid".to_vec())]))
        .await;
    f.s3.fault(Target::Exact(current.root.clone()), Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let r = r.clone();
        let local = local.clone();
        let v = v.clone();
        tokio::spawn(async move { r.observe_clean(local, token(&v, 2), v.id()).await })
    };
    f.s3.wait(arrived + 1).await;
    f.change(
        600,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed 日本語 🦀".into(),
        },
    )
    .await;
    f.s3.clear();
    f.s3.permits.add_permits(1);
    let next = task.await.unwrap().unwrap();
    let clean = next.view().clone();
    let again = f
        .tree(10, 6, BTreeMap::from([(0, b"remote".to_vec())]))
        .await;
    f.s3.fault(Target::Exact(again.root.clone()), Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let r = r.clone();
        let local = local.clone();
        let clean = clean.clone();
        tokio::spawn(async move { r.observe_clean(local, token(&clean, 3), clean.id()).await })
    };
    f.s3.wait(arrived + 1).await;
    let dirty = local
        .write_scoped(
            token(&clean, 4),
            clean.id(),
            0,
            Arc::from(b"local".as_slice()),
        )
        .await
        .unwrap();
    f.s3.clear();
    f.s3.permits.add_permits(1);
    let e = task.await.unwrap().unwrap_err();
    assert_eq!(
        e.downcast_ref::<sparse_views::Failure>(),
        Some(&sparse_views::Failure::Conflict)
    );
    assert_eq!(local.head(dirty.binding().clone()).await.unwrap(), dirty);
    storage.close().await.unwrap();
    // Independent clean local state exercises actual retained pin quota/cancel.
    let (storage, local, v) = setup(f, &r, &path.join("cancel")).await;
    let mut manual = Vec::new();
    for _ in 0..255 {
        manual.push(local.pin(v.id()).await.unwrap());
    }
    let root = f
        .tree(10, 7, BTreeMap::from([(0, b"pending".to_vec())]))
        .await;
    f.s3.fault(Target::Exact(root.root.clone()), Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let r = r.clone();
        let local = local.clone();
        let v = v.clone();
        tokio::spawn(async move { r.observe_clean(local, token(&v, 2), v.id()).await })
    };
    f.s3.wait(arrived + 1).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    available(&local, v.id()).await;
    assert_eq!(
        local.recover_adoption(token(&v, 2), v.id()).await.unwrap(),
        None
    );
    f.s3.clear();
    f.s3.permits.add_permits(1);
    f.s3.settle().await;
    for p in manual {
        local.release_pin(p).await.unwrap();
    }
    // The same active budget covers opens and observations; no fifth RPC.
    f.s3.fault(Target::Pages, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let mut tasks = Vec::new();
    for n in 10..14 {
        let r = r.clone();
        let local = local.clone();
        let v = v.clone();
        tasks.push(tokio::spawn(async move {
            r.observe_clean(local, token(&v, n), v.id()).await
        }));
    }
    f.s3.wait(arrived + 4).await;
    let requests = f.requests();
    failed(
        r.observe_clean(local.clone(), token(&v, 14), v.id()).await,
        ReadFailure::Busy,
    );
    assert_eq!(f.requests(), requests);
    for task in tasks {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    f.s3.clear();
    f.s3.permits.add_permits(4);
    f.s3.settle().await;
    f.create(11, "snapshot empty 🦀", 200, false).await;
    let mut snapshots = Vec::new();
    for _ in 0..256 {
        snapshots.push(r.open_file(&id(11)).await.unwrap());
    }
    let requests = f.requests();
    let events = f.s3.events().len();
    failed(
        r.observe_clean(local.clone(), token(&v, 14), v.id()).await,
        ReadFailure::Busy,
    );
    assert_eq!(f.requests(), requests);
    assert_eq!(f.s3.events().len(), events);
    drop(snapshots);
    let short = f
        .reader_options(ReadOptions::new(Duration::from_secs(2)).unwrap())
        .await;
    f.s3.fault(Target::Pages, Fault::Gate);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    let task = {
        let local = local.clone();
        let v = v.clone();
        tokio::spawn(async move { short.observe_clean(local, token(&v, 15), v.id()).await })
    };
    f.s3.wait(arrived + 1).await;
    failed(task.await.unwrap(), ReadFailure::Timeout);
    f.s3.clear();
    f.s3.permits.add_permits(1);
    f.s3.settle().await;
    available(&local, v.id()).await;
    assert_eq!(local.head(v.binding().clone()).await.unwrap(), v);
    storage.close().await.unwrap();
}
