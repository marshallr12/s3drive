//! Owned SDK/HTTP fixtures, not provider or mounted chunk-write acceptance.
use super::*;
use s3drive_managed::creation::{Creator, Draft, Recovery as CreationRecovery};
use s3drive_state::manifest_creation::Limits;
async fn store(s: &Server) -> s3drive_authority::Store {
    let Reply::Store(st) = ask(
        &s.direct,
        Request::Store {
            volume: info(&s.direct).await.id,
            store: id(200),
        },
    )
    .await
    else {
        panic!()
    };
    st
}
async fn draft(s: &Server, op: u64) -> Draft {
    let v = info(&s.direct).await;
    let p = node(&s.direct, &v.id, s3drive_authority::ROOT).await;
    Draft {
        operation: id(op),
        expected_sequence: v.sequence,
        parent: s3drive_authority::NodeGuard {
            id: p.id,
            revision: p.revision,
        },
        name: "empty café 日本語 🦀 %2F.bin".into(),
    }
}
async fn creator(e: Executor, s: &Server) -> Creator {
    Creator::bind(
        e,
        s.wire.clone(),
        info(&s.direct).await.id,
        store(s).await,
        Limits::default(),
        Duration::from_secs(60),
    )
    .unwrap()
}
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e.clone(), s).await;
    let d = draft(s, 40).await;
    let result = c.create(d.clone()).await.unwrap();
    let expected = result.metadata().clone();
    assert_eq!(expected.node.name, d.name);
    let p = expected.node.payload.as_ref().unwrap();
    assert_eq!(p.reservation.size, 64);
    assert_eq!(
        p.reservation
            .manifest
            .as_ref()
            .unwrap()
            .root()
            .unwrap()
            .length(),
        0
    );
    let reader = s3drive_managed::chunks::Reader::open(
        s.wire.clone(),
        info(&s.direct).await.id,
        vec![(id(200), e.reader())],
    )
    .await
    .unwrap();
    let f = reader.open_file(&id(40)).await.unwrap();
    assert_eq!(f.size(), 0);
    assert!(f.read_range(0, 64).await.unwrap().is_empty());
    change(
        &s.direct,
        41,
        Mutation::Move {
            node: id(40),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed 日本語.bin".into(),
        },
    )
    .await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert_eq!(c.create(d.clone()).await.unwrap().metadata(), &expected);
    assert_eq!(
        c.recovery().resume(id(40)).await.unwrap().metadata(),
        &expected
    );
    let mut changed = d;
    changed.name = "different.bin".into();
    assert!(c.create(changed).await.is_err());
    assert!(s.proxy.requests().is_empty());
    assert_eq!(
        node(&s.direct, &info(&s.direct).await.id, &id(40))
            .await
            .name,
        "renamed 日本語.bin"
    );
    e.storage().close().await.unwrap();
}
pub async fn controls(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e.clone(), s).await;
    let d = draft(s, 40).await;
    for mode in [3, 4, 5, 6, 7, 9] {
        s.proxy.mode.store(mode, Ordering::SeqCst);
        assert!(c.create(d.clone()).await.is_err());
        assert!(c.recovery().records().await.unwrap().is_empty());
    }
    s.proxy.mode.store(0, Ordering::SeqCst);
    let denied = executor(config, e.storage(), true, false).await;
    assert!(creator(denied, s).await.create(d.clone()).await.is_err());
    assert!(c.recovery().records().await.unwrap().is_empty());
    let limited = Creator::bind(
        e.clone(),
        s.wire.clone(),
        info(&s.direct).await.id,
        store(s).await,
        Limits {
            records: 1,
            ..Limits::default()
        },
        Duration::from_secs(60),
    )
    .unwrap();
    let other = executor(config, storage(root, "second").await, false, false).await;
    let rival = creator(other.clone(), s).await;
    c.create(d.clone()).await.unwrap();
    assert_eq!(
        limited.create(d.clone()).await.unwrap().metadata().node.id,
        id(40)
    );
    let mut over = draft(s, 43).await;
    over.name = "over quota.bin".into();
    assert!(limited.create(over).await.is_err());
    let mut second = d;
    second.operation = id(42);
    // Use the live parent guard to reach the actual atomic name collision.
    let fresh = draft(s, 42).await;
    second.parent = fresh.parent;
    second.expected_sequence = fresh.expected_sequence;
    assert!(rival.create(second.clone()).await.is_err());
    let record = rival.recovery().records().await.unwrap().pop().unwrap();
    assert_eq!(
        record.rejection(),
        Some(s3drive_authority::Error::AlreadyExists)
    );
    assert!(record.generation().is_some());
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert!(rival.create(second).await.is_err());
    assert!(s.proxy.requests().is_empty());
    e.storage().close().await.unwrap();
    other.storage().close().await.unwrap();
}
pub async fn recovery(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e.clone(), s).await;
    let d = draft(s, 40).await;
    let volume = info(&s.direct).await.id;
    let st = store(s).await;
    s.proxy.mode.store(12, Ordering::SeqCst);
    assert!(c.create(d.clone()).await.is_err());
    let record = c.recovery().records().await.unwrap().pop().unwrap();
    assert!(record.request().is_some());
    assert!(record.receipt().is_none());
    assert_eq!(node(&s.direct, &volume, &id(40)).await.name, d.name);
    let request = record.request().unwrap().clone();
    e.storage().close().await.unwrap();
    drop(c);
    drop(e);
    std::fs::rename(root.join("credentials"), root.join("credentials.disabled")).unwrap();
    std::fs::rename(root.join("policy.toml"), root.join("policy.disabled")).unwrap();
    checkpoint("NO_S3_FROM_HERE");
    wait(root, "no-s3-armed").await;
    let r = CreationRecovery::open_existing(
        &root.join("client"),
        s.wire.clone(),
        volume,
        st,
        Duration::from_secs(30),
    )
    .await
    .unwrap();
    s.proxy.clear();
    let historical = r.resume(id(40)).await.unwrap();
    assert_eq!(historical.metadata().node.name, d.name);
    assert_eq!(s.proxy.applies(), vec![request]);
    assert!(
        s.proxy
            .requests()
            .iter()
            .all(|r| matches!(r, Request::Apply { .. }))
    );
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    r.resume(id(40)).await.unwrap();
    assert!(s.proxy.requests().is_empty());
    r.close().await.unwrap();
}
pub async fn physical(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e.clone(), s).await;
    let d = draft(s, 40).await;
    checkpoint("LOSE_PUT");
    wait(root, "put-enabled").await;
    assert!(c.create(d.clone()).await.is_err());
    let r = c.recovery().records().await.unwrap().pop().unwrap();
    let g = r.generation().unwrap();
    assert!(r.request().is_none());
    assert_eq!(
        e.storage()
            .uploads()
            .for_generation(g)
            .await
            .unwrap()
            .unwrap()
            .phase,
        s3drive_state::remote::Phase::Uncertain
    );
    checkpoint("RESTORE_PUT");
    wait(root, "put-restored").await;
    let narrow = executor(config, e.storage(), true, false).await;
    let resumed = creator(narrow, s).await;
    s.proxy.clear();
    s.proxy.mode.store(6, Ordering::SeqCst);
    assert!(resumed.create(d.clone()).await.is_err());
    // Observation succeeded despite policy narrowing and a denied fresh feature.
    assert_eq!(
        e.storage()
            .uploads()
            .for_generation(g)
            .await
            .unwrap()
            .unwrap()
            .phase,
        s3drive_state::remote::Phase::Verified
    );
    assert!(
        e.storage()
            .uploads()
            .flush_receipt(g)
            .await
            .unwrap()
            .is_some()
    );
    assert!(s.proxy.applies().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    resumed.create(d).await.unwrap();
    e.storage().close().await.unwrap();
}
pub async fn crash(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e, s).await;
    s.proxy.mode.store(12, Ordering::SeqCst);
    assert!(c.create(draft(s, 40).await).await.is_err());
    let r = c.recovery().records().await.unwrap().pop().unwrap();
    assert!(r.request().is_some() && r.receipt().is_none());
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}
pub async fn killed_recovery(root: &Path, s: &Server) {
    assert!(!root.join("credentials").exists());
    assert!(!root.join("policy.toml").exists());
    let r = CreationRecovery::open_existing(
        &root.join("client"),
        s.wire.clone(),
        info(&s.direct).await.id,
        store(s).await,
        Duration::from_secs(30),
    )
    .await
    .unwrap();
    let before = r.records().await.unwrap().pop().unwrap();
    assert!(before.request().is_some() && before.receipt().is_none());
    s.proxy.clear();
    let result = r.resume(id(40)).await.unwrap();
    assert_eq!(result.metadata().node.name, before.input().name);
    assert_eq!(s.proxy.applies(), vec![before.request().unwrap().clone()]);
    r.close().await.unwrap();
}

pub async fn deadline(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use s3drive_managed::creation::Failure as CreationFailure;
    let e = setup(config, root, s).await;
    let c = Creator::bind(
        e.clone(),
        s.wire.clone(),
        info(&s.direct).await.id,
        store(s).await,
        Limits::default(),
        Duration::from_secs(10),
    )
    .unwrap();
    let d = draft(s, 40).await;
    s.proxy.mode.store(13, Ordering::SeqCst);
    let first = c.clone();
    let input = d.clone();
    let task = tokio::spawn(async move { first.create(input).await });
    tokio::time::timeout(Duration::from_secs(15), s.proxy.publish_gate.wait())
        .await
        .unwrap();
    let busy = c.create(d.clone()).await.unwrap_err();
    assert!(matches!(
        busy.downcast_ref::<CreationFailure>(),
        Some(CreationFailure::Busy)
    ));
    let deadline = tokio::time::timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        deadline.downcast_ref::<CreationFailure>(),
        Some(CreationFailure::Deadline)
    ));
    let r = c.recovery().records().await.unwrap().pop().unwrap();
    assert!(r.request().is_some() && r.receipt().is_none());
    s.proxy.mode.store(0, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(5), s.proxy.publish_gate.wait())
        .await
        .unwrap();
    let result = c.create(d).await.unwrap();
    assert_eq!(result.metadata().node.id, id(40));
    e.storage().close().await.unwrap();
}
