//! Real SDK/HTTP upload and same-owner metadata import. No OS or live provider.
use super::*;
use s3drive_managed::creation::{Creator, Draft};
use s3drive_state::{
    chunk_mount, manifest_creation::Limits, namespace_client as names, namespace_client::opens,
};
async fn setup_owner(
    config: &s3drive_core::Config,
    root: &Path,
    s: &Server,
) -> (Executor, chunk_mount::Client, Creator, Draft) {
    let e = setup(config, root, s).await;
    let volume = info(&s.direct).await.id;
    let store = super::empty_creation::store(s).await;
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    let client = e
        .storage()
        .chunk_mount(chunk_mount::Input {
            volume,
            controls: vec![names::Control {
                store,
                source: *target.source(),
                fingerprint: target.fingerprint().into(),
            }],
        })
        .await
        .unwrap();
    assert!(client.same_storage(&e.storage()));
    let creator = super::empty_creation::creator(e.clone(), s).await;
    let draft = super::empty_creation::draft(s, 40).await;
    (e, client, creator, draft)
}
fn open(input: &names::Input, session: u64) -> opens::OpenInput {
    opens::OpenInput {
        id: id(71),
        session: id(session),
        volume: input.request.volume.clone(),
        node: input.request.operation.clone(),
        store: input.store.clone(),
        source: input.source,
        fingerprint: input.fingerprint.clone(),
        delete_on_close: true,
        controlled: true,
        session_bound: true,
        posix: false,
        admission: Some(Box::new(input.clone())),
        creation: None,
    }
}
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, client, c, d) = setup_owner(config, root, s).await;
    let result = c
        .create_deferred(d.clone(), client.key().owner().into())
        .await
        .unwrap();
    let expected = result.metadata().clone();
    let original = result.original_removal().unwrap().clone();
    let saved = c.recovery().records().await.unwrap().pop().unwrap();
    checkpoint("NO_S3_FROM_HERE");
    wait(root, "no-s3-armed").await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let ns = client.namespace();
    let record = ns.import_creation(d.operation.clone()).await.unwrap();
    assert_eq!(record.input.request, saved.request().unwrap().clone());
    assert_eq!(record.outcome, Some(Ok(expected)));
    assert!(ns.begin(record.input.clone()).await.is_err());
    assert!(
        e.storage()
            .namespace()
            .import_creation(d.operation.clone())
            .await
            .is_err()
    );
    assert_eq!(
        ns.import_creation(d.operation.clone()).await.unwrap(),
        record
    );
    let owned = ns
        .owned_removal(record.input.request.volume.clone(), d.operation.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(owned.fence, original);
    let opens = client.opens();
    opens
        .start_session(id(80), record.input.request.volume.clone())
        .await
        .unwrap();
    opens.prepare(open(&record.input, 80)).await.unwrap();
    assert!(opens.activate(id(71), id(72)).await.unwrap().active());
    assert!(ns.pending(None, 16).await.unwrap().is_empty());
    assert!(s.proxy.requests().is_empty());
    e.storage().close().await.unwrap();
    assert!(ns.import_creation(d.operation).await.is_err());
}
pub async fn controls(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, client, c, d) = setup_owner(config, root, s).await;
    let foreign = if client.key().owner() == id(99) {
        id(98)
    } else {
        id(99)
    };
    c.create_deferred(d.clone(), foreign).await.unwrap();
    let mut ordinary = super::empty_creation::draft(s, 41).await;
    ordinary.name = "ordinary café 日本語.bin".into();
    c.create(ordinary).await.unwrap();
    let records = c.recovery().records().await.unwrap();
    assert_eq!(records.len(), 2);
    checkpoint("NO_S3_FROM_HERE");
    wait(root, "no-s3-armed").await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let ns = client.namespace();
    assert!(ns.import_creation(id(40)).await.is_err());
    assert!(ns.import_creation(id(41)).await.is_err());
    assert!(ns.import_creation(id(999)).await.is_err());
    assert!(
        c.create_deferred(d, client.key().owner().into())
            .await
            .is_err()
    );
    assert!(ns.pending(None, 16).await.unwrap().is_empty());
    assert!(
        ns.owned_removal(client.key().volume().into(), id(40))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(c.recovery().records().await.unwrap(), records);
    assert!(s.proxy.requests().is_empty());
    e.storage().close().await.unwrap();
}
pub async fn crash(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (_e, client, c, d) = setup_owner(config, root, s).await;
    c.create_deferred(d, client.key().owner().into())
        .await
        .unwrap();
    // The local effect is complete; the encompassing caller never sees a reply.
    client.namespace().import_creation(id(40)).await.unwrap();
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}
pub async fn recover(root: &Path, s: &Server) {
    assert!(!root.join("credentials").exists());
    assert!(!root.join("policy.toml").exists());
    let storage = Storage::open(
        &root.join("client"),
        s3drive_state::cache::Options::default(),
        s3drive_state::journal::Options::default(),
    )
    .await
    .unwrap();
    let saved = storage
        .manifest_creations(Limits::default())
        .unwrap()
        .get(id(40))
        .await
        .unwrap();
    let original = saved.input();
    let client = storage
        .chunk_mount(chunk_mount::Input {
            volume: original.volume.clone(),
            controls: vec![names::Control {
                store: original.store.clone(),
                source: original.source,
                fingerprint: original.fingerprint.clone(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(Some(client.key().owner()), original.owner.as_deref());
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let ns = client.namespace();
    let before = ns.get(id(40)).await.unwrap();
    let repeated = ns.import_creation(id(40)).await.unwrap();
    assert_eq!(before, repeated);
    assert_eq!(repeated.input.request, saved.request().unwrap().clone());
    assert_eq!(repeated.outcome, Some(Ok(saved.receipt().unwrap().clone())));
    let opens = client.opens();
    assert!(opens.get(id(71)).await.is_err());
    opens
        .start_session(id(90), original.volume.clone())
        .await
        .unwrap();
    opens.prepare(open(&repeated.input, 90)).await.unwrap();
    assert!(opens.activate(id(71), id(72)).await.unwrap().active());
    assert!(s.proxy.requests().is_empty());
    storage.close().await.unwrap();
}
