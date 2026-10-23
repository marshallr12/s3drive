//! Real SDK and authenticated loopback authority/provider fixtures only.
use super::*;
use s3drive_managed::chunks::Reader;
use s3drive_state::sparse_views::{
    self as views,
    preparation::{self as plans, publication as roots, retry},
};

pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let initial = publisher(&seed, s).await;
    let (plan, _) = plan(&seed, s, 10, 40, b"original binary\0!", None, 0).await;
    initial.begin(plan, id(41)).await.unwrap();
    initial.resume(plan).await.unwrap();
    let volume = info(&s.direct).await.id;
    let mut binding: Option<views::Binding> = None;
    for round in 0..4u8 {
        let st = storage(root, "local-retention").await;
        let local = st.sparse_views(views::Options::default()).unwrap();
        let plans = local.preparations(plans::Options::default()).unwrap();
        let roots = plans.publications(roots::Limits::default()).unwrap();
        let e = executor(config, st.clone(), false, false).await;
        let reader = Reader::open(s.wire.clone(), volume.clone(), vec![(id(200), e.reader())])
            .await
            .unwrap();
        let parent = if let Some(b) = &binding {
            local.current_head(b.clone()).await.unwrap()
        } else {
            let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
            let b = base.sparse_binding().unwrap();
            binding = Some(b.clone());
            local.begin([1; 32], b, base.size()).await.unwrap()
        };
        let mut bytes = b"caf\xc3\xa9 binary\0\xff!!!".to_vec();
        bytes[0] = round + 65;
        let view = if round == 0 {
            local
                .write([2; 32], parent.id(), 0, Arc::from(bytes.clone()))
                .await
                .unwrap()
        } else {
            local
                .write_scoped(
                    views::ScopedOperation::new(parent.binding(), [2; 32]).unwrap(),
                    parent.id(),
                    0,
                    Arc::from(bytes.clone()),
                )
                .await
                .unwrap()
        };
        let edit = reader
            .reopen_edited(local.clone(), view.id())
            .await
            .unwrap();
        let held = edit.clone();
        let token = retry::Operation::new(view.binding(), [9; 32]).unwrap();
        let prep = if round == 0 {
            edit.begin_preparation([9; 32], [9; 16], plans::Options::default())
                .await
                .unwrap()
        } else {
            edit.begin_scoped_preparation(token, plans::Options::default())
                .await
                .unwrap()
        };
        if round != 0 {
            s.proxy.clear();
            let repeated = edit
                .begin_scoped_preparation(token, plans::Options::default())
                .await
                .unwrap();
            assert_eq!(repeated.id(), prep.id());
            assert!(
                s.proxy.requests().is_empty(),
                "exact scoped retry used fresh authority"
            );
            drop(repeated);
        }
        prep.prepare().await.unwrap();
        prep.uploader(e).unwrap().upload_tree().await.unwrap();
        let publisher = prep
            .publisher(
                executor(config, st.clone(), false, false).await,
                roots::Limits::default(),
            )
            .unwrap();
        publisher.check_semantics().await.unwrap();
        if round == 0 {
            publisher.begin(id(70)).await.unwrap();
        } else {
            publisher.begin_scoped(token).await.unwrap();
        }
        if round == 2 {
            s.proxy.mode.store(2, Ordering::SeqCst);
            assert!(
                publisher.resume().await.is_err(),
                "lost reply became known success"
            );
            assert!(plans.checkpoint(prep.id()).await.is_err());
            s.proxy.mode.store(0, Ordering::SeqCst);
        }
        publisher.resume().await.unwrap();
        let recovery = publisher.recovery();
        let ack = if round == 0 {
            publisher
                .acknowledge([40; 32], views::Options::default())
                .await
                .unwrap()
        } else {
            publisher
                .acknowledge_scoped(
                    views::ScopedOperation::new(view.binding(), [40; 32]).unwrap(),
                    views::Options::default(),
                )
                .await
                .unwrap()
        };
        let clean = ack.clean_view_id().unwrap();
        let old_plan = prep.id();
        s.proxy.clear();
        if round == 0 {
            plans.migrate_retry_safety().await.unwrap();
            assert!(plans.checkpoint(old_plan).await.is_err());
        } else {
            let cp = plans.checkpoint(old_plan).await.unwrap();
            assert!(!cp.unblocked());
            assert!(
                roots
                    .retire_checkpoint(cp, views::Options::default())
                    .await
                    .is_err()
            );
        }
        assert!(
            s.proxy.requests().is_empty(),
            "local migration/retirement used RPC"
        );
        assert_eq!(held.read_range(0, bytes.len()).await.unwrap(), bytes);
        drop(publisher);
        drop(prep);
        drop(edit);
        // A surviving clone independently protects the old source.
        if round != 0 {
            assert!(!plans.checkpoint(old_plan).await.unwrap().unblocked());
        }
        drop(held);
        if round != 0 {
            let cp = tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let cp = plans.checkpoint(old_plan).await.unwrap();
                    if cp.unblocked() {
                        break cp;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            s.proxy.clear();
            assert_eq!(
                roots
                    .retire_checkpoint(cp, views::Options::default())
                    .await
                    .unwrap(),
                clean
            );
            assert!(s.proxy.requests().is_empty());
            assert!(local.view(view.id()).await.is_err());
            assert!(roots.get(old_plan).await.is_err());
            assert!(recovery.acknowledged(old_plan).await.is_err());
            assert!(
                s.proxy.requests().is_empty(),
                "retired replay used authority RPC"
            );
            assert_eq!(
                reader
                    .reopen_edited(local.clone(), clean)
                    .await
                    .unwrap()
                    .read_range(0, bytes.len())
                    .await
                    .unwrap(),
                bytes
            );
        }
        st.close().await.unwrap();
        let st = storage(root, "local-retention").await;
        let local = st.sparse_views(views::Options::default()).unwrap();
        assert_eq!(
            local
                .current_head(binding.clone().unwrap())
                .await
                .unwrap()
                .id(),
            clean
        );
        st.close().await.unwrap();
    }
}
