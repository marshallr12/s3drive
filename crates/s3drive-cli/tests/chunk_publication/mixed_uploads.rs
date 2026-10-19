//! Real SDK and local conditional S3 fixture; no provider/root CAS/native claim.
use super::*;
use s3drive_core::manifest::{
    self, Entry,
    objects::{Namespace, Role},
};
use s3drive_managed::chunks::Reader;
use s3drive_state::sparse_views::{
    self,
    preparation::{self as plans, Position},
};
const C: u64 = manifest::CHUNK_BYTES;
async fn verify(e: &Executor, r: &plans::TreeReceipt, root: &str) {
    let descriptor = r.root();
    let ns = Namespace::new(
        root.into(),
        *descriptor.volume(),
        *descriptor.node(),
        *descriptor.page().store(),
    )
    .unwrap();
    let key = ns.root_key(descriptor).unwrap();
    let h = e.reader().open(key.as_str()).await.unwrap();
    let bytes = h.read_range(0, h.metadata().size as usize).await.unwrap();
    let mut pages = vec![descriptor.verify_page(&bytes).unwrap()];
    let mut data = 0;
    while let Some(page) = pages.pop() {
        for (index, entry) in page.page().entries().iter().enumerate() {
            let (role, reference) = match entry {
                Entry::Data { object, .. } => (Role::Data, object),
                Entry::Child { object, .. } => (Role::Page, object),
                Entry::Hole { .. } => continue,
            };
            let key = ns.key(role, reference).unwrap();
            let h = e.reader().open(key.as_str()).await.unwrap();
            let bytes = h.read_range(0, h.metadata().size as usize).await.unwrap();
            if role == Role::Page {
                pages.push(page.verify_child(index, &bytes).unwrap());
            } else {
                page.verify_data(index, &bytes).unwrap();
                data += 1;
            }
        }
    }
    assert_eq!(
        data, 2,
        "new Unicode region and inherited partial tail must verify"
    );
}
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let seed = setup(config, root, s).await;
    let p = publisher(&seed, s).await;
    let tail = b"unchanged tail!!\0";
    assert_eq!(tail.len(), 17);
    let (old, _) = plan(&seed, s, 10, 40, tail, Some(130 * C + 17), 0).await;
    p.begin(old, id(41)).await.unwrap();
    let committed = p.resume(old).await.unwrap();
    let original = committed.metadata().clone();
    let storage = storage(root, "mixed").await;
    let local = storage
        .sparse_views(sparse_views::Options::default())
        .unwrap();
    let e = executor(config, storage.clone(), false, false).await;
    let reader = Reader::open(
        s.wire.clone(),
        info(&s.direct).await.id,
        vec![(id(200), e.reader())],
    )
    .await
    .unwrap();
    let base = reader.open_file(&id(10)).await.unwrap().pinned_base();
    let view = local
        .begin([1; 32], base.sparse_binding().unwrap(), base.size())
        .await
        .unwrap();
    let edited = local
        .write(
            [2; 32],
            view.id(),
            3,
            Arc::from("café 日本語 🦀".as_bytes()),
        )
        .await
        .unwrap();
    let edit = reader
        .reopen_edited(local.clone(), edited.id())
        .await
        .unwrap();
    let preparation = edit
        .begin_preparation([3; 32], [91; 16], plans::Options::default())
        .await
        .unwrap();
    preparation.prepare().await.unwrap();
    let uploader = preparation.uploader(e.clone()).unwrap();
    assert!(
        preparation.uploader(seed.clone()).is_err(),
        "wrong SQLite owner accepted"
    );
    let data = Position { level: -1, slot: 0 };
    let leaf = Position { level: 0, slot: 0 };
    let root_position = Position { level: 1, slot: 0 };
    assert!(
        uploader.upload_object(root_position).await.is_err(),
        "root sent before new leaf receipt"
    );
    let denied = executor(config, storage.clone(), true, false).await;
    assert!(
        preparation
            .uploader(denied)
            .unwrap()
            .upload_object(data)
            .await
            .is_err()
    );
    let newer = local
        .write([4; 32], edited.id(), 5, Arc::from(&b"newer"[..]))
        .await
        .unwrap();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert!(
        uploader.upload_object(data).await.is_err(),
        "fresh upload bypassed source authorization"
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    assert!(
        local
            .preparations(plans::Options::default())
            .unwrap()
            .transfer(preparation.id(), data)
            .await
            .unwrap()
            .unwrap()
            .generation()
            .is_none(),
        "denied source created physical staging"
    );
    checkpoint("LOSE_PUT");
    wait(root, "put-enabled").await;
    assert!(
        uploader.upload_object(data).await.is_err(),
        "hidden committed object claimed verified"
    );
    let client = local.preparations(plans::Options::default()).unwrap();
    let t = client
        .transfer(preparation.id(), data)
        .await
        .unwrap()
        .unwrap();
    let before = storage
        .uploads()
        .for_generation(t.generation().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.phase, s3drive_state::remote::Phase::Uncertain);
    checkpoint("RESTORE_PUT");
    wait(root, "put-restored").await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    uploader.upload_object(data).await.unwrap().unwrap();
    assert!(
        s.proxy.requests().is_empty(),
        "claimed recovery reauthorized original source"
    );
    let after = storage
        .uploads()
        .for_generation(t.generation().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.operation, after.operation);
    uploader.upload_object(data).await.unwrap().unwrap();
    s.proxy.mode.store(0, Ordering::SeqCst);
    uploader.upload_object(leaf).await.unwrap().unwrap();
    let receipt = uploader.upload_tree().await.unwrap();
    assert_eq!(receipt.new_objects(), 3);
    assert_eq!(receipt.inherited_objects(), 1);
    assert_eq!(receipt.hole_records(), 127);
    assert_eq!(receipt.plan().view(), &edited);
    assert_eq!(local.head(edited.binding().clone()).await.unwrap(), newer);
    assert_eq!(
        node(&s.direct, &info(&s.direct).await.id, &id(10)).await,
        original.node,
        "physical uploads changed authority root"
    );
    verify(&e, &receipt, "writable/.s3drive/").await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert_eq!(uploader.upload_tree().await.unwrap().root(), receipt.root());
    assert!(
        s.proxy.requests().is_empty(),
        "completed historical inspection reauthorized source"
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    storage.close().await.unwrap();
    seed.storage().close().await.unwrap();
}
