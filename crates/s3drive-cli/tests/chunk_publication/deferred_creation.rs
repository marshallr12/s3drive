//! Owner-bearing canonical creation through real SDK/HTTP and durable recovery.
use super::empty_creation::{creator, draft, store};
use super::*;
use s3drive_managed::creation::{Creator, Recovery as CreationRecovery};
use s3drive_state::manifest_creation::Limits;
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e.clone(), s).await;
    let d = draft(s, 40).await;
    let result = c.create_deferred(d.clone(), id(99)).await.unwrap();
    let expected = result.metadata().clone();
    let original = result.original_removal().unwrap().clone();
    assert_eq!(original.intent, id(40));
    assert_eq!(original.owner, id(99));
    change(
        &s.direct,
        44,
        Mutation::CancelRemoval {
            node: id(40),
            fence: original.clone(),
        },
    )
    .await;
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
    let v = info(&s.direct).await;
    let current = node(&s.direct, &v.id, &id(40)).await;
    let parent = node(&s.direct, &v.id, s3drive_authority::ROOT).await;
    let next = change(
        &s.direct,
        45,
        Mutation::PrepareRemoval {
            node: s3drive_authority::NodeGuard {
                id: current.id,
                revision: current.revision,
            },
            parent: s3drive_authority::NodeGuard {
                id: parent.id,
                revision: parent.revision,
            },
            owner: id(100),
        },
    )
    .await;
    let Some(s3drive_authority::Artifact::Removal(successor)) = next.artifact else {
        panic!()
    };
    assert_ne!(successor, original);
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let replay = c.create_deferred(d.clone(), id(99)).await.unwrap();
    assert_eq!(replay.metadata(), &expected);
    assert_eq!(replay.original_removal(), Some(&original));
    assert!(c.create_deferred(d.clone(), id(100)).await.is_err());
    assert_eq!(
        c.recovery().resume(id(40)).await.unwrap().metadata(),
        &expected
    );
    let mut changed = d;
    changed.name = "different.bin".into();
    assert!(c.create_deferred(changed, id(99)).await.is_err());
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
    for mode in [3, 4, 5, 6, 7, 9, 19, 21] {
        s.proxy.mode.store(mode, Ordering::SeqCst);
        assert!(c.create_deferred(d.clone(), id(99)).await.is_err());
        assert!(c.recovery().records().await.unwrap().is_empty());
    }
    s.proxy.mode.store(0, Ordering::SeqCst);
    for owner in [id(0), "F".repeat(32), "a".repeat(31)] {
        assert!(c.create_deferred(d.clone(), owner).await.is_err());
        assert!(c.recovery().records().await.unwrap().is_empty());
    }
    let denied = executor(config, e.storage(), true, false).await;
    assert!(
        creator(denied, s)
            .await
            .create_deferred(d.clone(), id(99))
            .await
            .is_err()
    );
    assert!(c.recovery().records().await.unwrap().is_empty());
    // Exactly two total PUTs: winner and a different journal's name contender.
    // Every preflight/policy/owner refusal above must therefore send none.
    c.create_deferred(d.clone(), id(99)).await.unwrap();
    let other = executor(config, storage(root, "second").await, false, false).await;
    let rival = creator(other.clone(), s).await;
    let mut contender = draft(s, 42).await;
    contender.name = d.name;
    assert!(
        rival
            .create_deferred(contender.clone(), id(100))
            .await
            .is_err()
    );
    let retained = rival.recovery().records().await.unwrap().pop().unwrap();
    assert_eq!(
        retained.rejection(),
        Some(s3drive_authority::Error::AlreadyExists)
    );
    assert_eq!(retained.input().owner, Some(id(100)));
    assert!(retained.generation().is_some());
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert!(rival.create_deferred(contender, id(100)).await.is_err());
    assert!(s.proxy.requests().is_empty());
    other.storage().close().await.unwrap();
    e.storage().close().await.unwrap();
}
pub async fn recovery(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e.clone(), s).await;
    let d = draft(s, 40).await;
    let volume = info(&s.direct).await.id;
    let st = store(s).await;
    s.proxy.mode.store(20, Ordering::SeqCst);
    assert!(c.create_deferred(d.clone(), id(99)).await.is_err());
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
    assert_eq!(historical.original_removal().unwrap().owner, id(99));
    assert_eq!(historical.original_removal().unwrap().intent, id(40));
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
    assert!(c.create_deferred(d.clone(), id(99)).await.is_err());
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
    s.proxy.mode.store(19, Ordering::SeqCst);
    assert!(resumed.create_deferred(d.clone(), id(99)).await.is_err());
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
    resumed.create_deferred(d, id(99)).await.unwrap();
    e.storage().close().await.unwrap();
}
pub async fn crash(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let c = creator(e, s).await;
    s.proxy.mode.store(20, Ordering::SeqCst);
    assert!(c.create_deferred(draft(s, 40).await, id(99)).await.is_err());
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
    assert_eq!(result.original_removal().unwrap().owner, id(99));
    assert_eq!(result.original_removal().unwrap().intent, id(40));
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
    let task = tokio::spawn(async move { first.create_deferred(input, id(99)).await });
    tokio::time::timeout(Duration::from_secs(15), s.proxy.publish_gate.wait())
        .await
        .unwrap();
    let busy = c.create_deferred(d.clone(), id(99)).await.unwrap_err();
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
    let result = c.create_deferred(d, id(99)).await.unwrap();
    assert_eq!(result.metadata().node.id, id(40));
    assert_eq!(result.original_removal().unwrap().owner, id(99));
    e.storage().close().await.unwrap();
}
