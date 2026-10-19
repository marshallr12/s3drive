//! Actual SDK/authority and local epochs; not a mounted chunk-write acceptance.
use super::*;
use s3drive_managed::chunks::Reader;
use s3drive_state::sparse_views::{
    self,
    preparation::{self as plans, publication as roots},
};
const C: u64 = s3drive_core::manifest::CHUNK_BYTES;
#[inline(never)]
pub fn run_case<'a>(
    config: &'a s3drive_core::Config,
    root: &'a Path,
    s: &'a Server,
) -> std::pin::Pin<Box<impl std::future::Future<Output = ()> + 'a>> {
    Box::pin(cycles(config, root, s))
}
async fn cycles(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let old = publisher(&seed, s).await;
    let tail = b"unchanged tail!!\0";
    let (p, _) = plan(&seed, s, 10, 40, tail, Some(3 * C + 17), 0).await;
    old.begin(p, id(41)).await.unwrap();
    old.resume(p).await.unwrap();
    let volume = info(&s.direct).await.id;
    let mut scope: Option<sparse_views::Binding> = None;
    let mut saved = Vec::new();
    let mut future = None;
    let mut latest = Vec::new();
    for round in 0..4u8 {
        let st = storage(root, "epochs").await;
        let local = st.sparse_views(sparse_views::Options::default()).unwrap();
        let e = executor(config, st.clone(), false, false).await;
        let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
            .await
            .unwrap();
        let parent = if let Some(b) = &scope {
            local.current_head(b.clone()).await.unwrap()
        } else {
            let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
            let b = base.sparse_binding().unwrap();
            scope = Some(b.clone());
            local.begin([1; 32], b, base.size()).await.unwrap()
        };
        let view = if round == 2 {
            assert_eq!(Some(parent.id()), future.take());
            parent
        } else {
            let (offset, bytes) = match round {
                0 => (3, "café".as_bytes()),
                1 => (C + 3, "日本".as_bytes()),
                _ => (4, "é".as_bytes()),
            };
            local
                .write([10 + round; 32], parent.id(), offset, Arc::from(bytes))
                .await
                .unwrap()
        };
        let edit = reader
            .reopen_edited(local.clone(), view.id())
            .await
            .unwrap();
        let expected = edit.read_range(0, (3 * C + 17) as usize).await.unwrap();
        assert_eq!(&expected[3 * C as usize..], tail);
        for (old_id, old_bytes) in &saved {
            let pinned = reader.reopen_edited(local.clone(), *old_id).await.unwrap();
            assert_eq!(
                pinned.read_range(0, (3 * C + 17) as usize).await.unwrap(),
                *old_bytes,
                "old read changed after base adoption"
            );
        }
        let prep = edit
            .begin_preparation(
                [30 + round; 32],
                [140 + round; 16],
                plans::Options::default(),
            )
            .await
            .unwrap();
        prep.prepare().await.unwrap();
        let tree = prep.uploader(e).unwrap().upload_tree().await.unwrap();
        assert_eq!(
            tree.new_objects(),
            if round == 2 { 3 } else { 2 },
            "quiescent epoch must reuse unchanged chunks"
        );
        // The publisher uses that same owner and executor. Reopening one uses
        // the same route/principal, not a different local staging database.
        let publisher = prep
            .publisher(
                executor(config, st.clone(), false, false).await,
                roots::Limits::default(),
            )
            .unwrap();
        publisher.check_semantics().await.unwrap();
        publisher.begin(id(1000 + u64::from(round))).await.unwrap();
        let committed = publisher.resume().await.unwrap();
        assert_eq!(committed.view_id(), view.id());
        assert_eq!(
            reader
                .open_file(&id(10))
                .await
                .unwrap()
                .read_range(0, (3 * C + 17) as usize)
                .await
                .unwrap(),
            expected
        );
        if round == 1 {
            let v = local
                .write([90; 32], view.id(), 2 * C + 3, Arc::from("🦀".as_bytes()))
                .await
                .unwrap();
            future = Some(v.id());
        }
        assert!(
            std::mem::size_of_val(
                &publisher.acknowledge([60 + round; 32], sparse_views::Options::default())
            ) <= 8192,
            "ack future exceeds callback budget"
        );
        s.proxy.clear();
        let a = publisher
            .acknowledge([60 + round; 32], sparse_views::Options::default())
            .await
            .unwrap();
        assert!(
            s.proxy.requests().is_empty(),
            "local acknowledgment used authority RPC"
        );
        assert_eq!(a.view_id(), view.id());
        let head = local.current_head(scope.clone().unwrap()).await.unwrap();
        if round == 1 {
            assert_eq!(a.clean_view_id(), None);
            assert_eq!(Some(head.id()), future);
            assert_eq!(a.observed_head(), head.id());
        } else {
            assert_eq!(a.clean_view_id(), Some(head.id()));
            assert_ne!(head.id(), view.id());
            assert!(
                local.head(view.binding().clone()).await.is_err(),
                "stale exact binding silently adopted new epoch"
            );
            assert_eq!(
                head.binding().base().unwrap().payload_reference(),
                s3drive_authority::payload_reference(&committed.metadata().node.payload).unwrap()
            );
            assert_eq!(
                reader
                    .reopen_edited(local.clone(), head.id())
                    .await
                    .unwrap()
                    .read_range(0, (3 * C + 17) as usize)
                    .await
                    .unwrap(),
                expected
            );
        }
        s.proxy.clear(); // Scope replay observation after the authenticated reads above.
        assert_eq!(
            publisher
                .acknowledge([60 + round; 32], sparse_views::Options::default())
                .await
                .unwrap(),
            a
        );
        assert!(s.proxy.requests().is_empty());
        saved.push((view.id(), expected.clone()));
        latest = expected;
        st.close().await.unwrap();
        // The existing-only recovery capability returns the same local receipt
        // after close, without any source body read or network operation.
        let recovery = s3drive_managed::chunks::MixedRootRecovery::open_existing(
            &root.join("epochs"),
            s.wire.clone(),
            volume.clone(),
            id(200),
        )
        .await
        .unwrap();
        assert_eq!(recovery.acknowledged(prep.id()).await.unwrap(), Some(a));
        assert!(s.proxy.requests().is_empty());
        recovery.close().await.unwrap();
        if round == 0 {
            change(
                &s.direct,
                70,
                Mutation::Move {
                    node: id(10),
                    parent: s3drive_authority::ROOT.into(),
                    name: "renamed café 日本語 🦀.bin".into(),
                },
            )
            .await;
        }
    }
    assert_eq!(
        &latest[2 * C as usize + 3..2 * C as usize + 7],
        "🦀".as_bytes()
    );
    // Existing empty manifest origins also adopt an empty clean base; no fake
    // data object is uploaded and no one-object/full-snapshot receipt is cast.
    create(s, 11, "empty café 🦀").await;
    let (p, _) = plan(&seed, s, 11, 80, b"", None, 0).await;
    old.begin(p, id(81)).await.unwrap();
    old.resume(p).await.unwrap();
    let st = storage(root, "empty-epoch").await;
    let local = st.sparse_views(sparse_views::Options::default()).unwrap();
    let e = executor(config, st.clone(), false, false).await;
    let reader = Reader::open(s.wire.clone(), volume, vec![(id(200), e.reader())])
        .await
        .unwrap();
    let base = reader.open_file(&id(11)).await.unwrap().pinned_base();
    let v = local
        .begin([1; 32], base.sparse_binding().unwrap(), 0)
        .await
        .unwrap();
    let edit = reader.reopen_edited(local.clone(), v.id()).await.unwrap();
    let prep = edit
        .begin_preparation([2; 32], [150; 16], plans::Options::default())
        .await
        .unwrap();
    prep.prepare().await.unwrap();
    assert_eq!(
        prep.uploader(e.clone())
            .unwrap()
            .upload_tree()
            .await
            .unwrap()
            .new_objects(),
        1
    );
    let publisher = prep.publisher(e, roots::Limits::default()).unwrap();
    publisher.check_semantics().await.unwrap();
    publisher.begin(id(1005)).await.unwrap();
    publisher.resume().await.unwrap();
    let a = publisher
        .acknowledge([3; 32], sparse_views::Options::default())
        .await
        .unwrap();
    let head = local.current_head(v.binding().clone()).await.unwrap();
    assert_eq!(a.clean_view_id(), Some(head.id()));
    assert!(
        reader
            .reopen_edited(local, head.id())
            .await
            .unwrap()
            .read_range(0, 1)
            .await
            .unwrap()
            .is_empty()
    );
    st.close().await.unwrap();
    seed.storage().close().await.unwrap();
}
