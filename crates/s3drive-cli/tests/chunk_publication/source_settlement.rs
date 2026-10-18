//! Controller acknowledgment controls; all services and credentials are owned
//! dummy fixtures. State-level disk-full/SQL-commit faults have separate tests.
use super::*;
use s3drive_managed::chunks::SourceReceipt;

async fn status(
    s: &Storage,
    i: [u8; 32],
) -> s3drive_state::chunk_publication::sources::SourceStatus {
    s.chunk_publications()
        .sources()
        .source_status(i)
        .await
        .unwrap()
}
fn exact(a: &SourceReceipt, r: &Record) {
    assert_eq!(a.plan(), &r.input().plan().id);
    assert_eq!(a.snapshot(), r.input().snapshot());
    assert_eq!(a.manifest(), r.input().snapshot_manifest());
    assert_eq!(a.target(), &r.input().plan().local_target().unwrap());
    assert_eq!(a.root_generation(), r.root_generation().unwrap());
    assert_ne!(a.snapshot(), a.root_generation());
    assert_eq!(a.authority_receipt(), r.receipt().unwrap());
}
async fn dirty(store: &Storage, r: &Record) -> GenerationId {
    let j = store.staging();
    let g = j
        .begin(
            r.input().plan().local_target().unwrap(),
            journal::Base::Missing,
            Some(r.input().snapshot()),
            3,
        )
        .await
        .unwrap();
    j.append_at(g, 0, Arc::from(b"new".as_slice()))
        .await
        .unwrap();
    j.seal(g).await.unwrap();
    g
}
async fn recovery(store: &Storage, s: &Server, r: &Record) -> Recovery {
    Recovery::open(
        store.clone(),
        s.wire.clone(),
        r.input().reserve().volume.clone(),
        r.input().store().id.clone(),
    )
    .await
    .unwrap()
}

pub(super) async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let p = publisher(&e, s).await;
    let store = e.storage();
    let (i, g) = plan(&e, s, 10, 40, "binary\0 café 日本語 🦀".as_bytes(), None, 0).await;
    let r = p.begin(i, id(41)).await.unwrap();
    let q = recovery(&store, s, &r).await;
    s.proxy.clear();
    typed(q.resume_source(i).await, |e| {
        matches!(e, Failure::NotPrepared)
    });
    assert!(q.resume_source([99; 32]).await.is_err());
    for (volume, store_id) in [
        (id(999), id(200)),
        (r.input().reserve().volume.clone(), id(999)),
    ] {
        let wrong = Recovery::open(store.clone(), s.wire.clone(), volume, store_id)
            .await
            .unwrap();
        typed(wrong.resume_source(i).await, |e| {
            matches!(e, Failure::Binding)
        });
    }
    assert!(s.proxy.requests().is_empty());
    assert!(status(&store, i).await.settlement().is_none());
    p.resume(i).await.unwrap();
    assert!(status(&store, i).await.settlement().is_none());
    let old = store.chunk_publications().get(i).await.unwrap();
    let (new, ng) = plan(
        &e,
        s,
        10,
        50,
        b"tail\0",
        Some(2 * s3drive_core::manifest::CHUNK_BYTES + 5),
        0,
    )
    .await;
    p.begin(new, id(51)).await.unwrap();
    let a = p.resume_source(new).await.unwrap();
    exact(&a, &store.chunk_publications().get(new).await.unwrap());
    assert!(
        status(&store, i).await.settlement().is_none(),
        "newer acknowledgment settled ancestor"
    );
    assert_eq!(status(&store, new).await.settlement(), Some(&a));
    let newer = store.chunk_publications().get(new).await.unwrap();
    let dirty_head = dirty(&store, &newer).await;
    create(s, 11, "empty 日本語 🦀.txt").await;
    let (empty, eg) = plan(&e, s, 11, 60, b"", None, 0).await;
    p.begin(empty, id(61)).await.unwrap();
    let empty_ack = p.resume_source(empty).await.unwrap();
    exact(
        &empty_ack,
        &store.chunk_publications().get(empty).await.unwrap(),
    );
    assert_eq!(
        empty_ack
            .authority_receipt()
            .node
            .payload
            .as_ref()
            .unwrap()
            .reservation
            .logical_size()
            .unwrap(),
        0
    );
    checkpoint("NO_S3_FROM_HERE");
    wait(root, "no-s3-armed").await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let historic = q.resume_source(i).await.unwrap();
    exact(&historic, &old);
    assert_eq!(q.resume_source(i).await.unwrap(), historic);
    assert_eq!(p.resume_source(new).await.unwrap(), a);
    assert_eq!(q.resume_source(empty).await.unwrap(), empty_ack);
    assert!(
        s.proxy.requests().is_empty(),
        "known root recovery made metadata RPC"
    );
    assert_eq!(status(&store, i).await.current_head(), dirty_head);
    assert_eq!(
        node(&s.direct, &old.input().reserve().volume, &id(10))
            .await
            .payload
            .unwrap()
            .reservation
            .id,
        id(50)
    );
    for g in [g, ng, eg, dirty_head] {
        pending(&store, g).await;
    }
    assert!(store.namespace().client_identity().await.is_err());
    store.close().await.unwrap();
}

pub(super) async fn ack_crash(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let p = publisher(&e, s).await;
    let (i, g) = plan(
        &e,
        s,
        10,
        40,
        b"durable root before failed source ack\0",
        None,
        0,
    )
    .await;
    p.begin(i, id(41)).await.unwrap();
    let db = raw(root);
    db.execute_batch("CREATE TRIGGER no_source_ack BEFORE INSERT ON chunk_source_settlements BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert!(p.resume_source(i).await.is_err());
    let r = e.storage().chunk_publications().get(i).await.unwrap();
    assert!(r.receipt().is_some());
    assert!(status(&e.storage(), i).await.settlement().is_none());
    let rows: i64 = db
        .query_row("SELECT count(*) FROM chunk_source_role", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0, "partial acknowledgment role persisted");
    drop(db);
    pending(&e.storage(), g).await;
    dirty(&e.storage(), &r).await;
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}

pub(super) async fn recover(root: &Path, s: &Server, known: bool) {
    // Deliberately no Source/config, Executor or WritePolicy construction.
    assert!(!root.join("credentials").exists());
    assert!(!root.join("policy.toml").exists());
    let store = storage(root, "client").await;
    let records = store.chunk_publications().records().await.unwrap();
    assert_eq!(records.len(), 1);
    let r = &records[0];
    let i = r.input().plan().id;
    assert_eq!(r.receipt().is_some(), known);
    assert!(
        store
            .staging()
            .read(r.input().snapshot(), 0, 64)
            .await
            .is_err()
    );
    assert!(status(&store, i).await.settlement().is_none());
    let q = recovery(&store, s, r).await;
    s.proxy.clear();
    if known {
        s.proxy.mode.store(8, Ordering::SeqCst);
    }
    let a = q.resume_source(i).await.unwrap();
    if known {
        assert!(s.proxy.requests().is_empty());
    } else {
        only_exact_apply(s, r.publication().unwrap());
    }
    exact(&a, &store.chunk_publications().get(i).await.unwrap());
    s.proxy.clear();
    assert_eq!(q.resume_source(i).await.unwrap(), a);
    assert!(s.proxy.requests().is_empty());
    let current = status(&store, i).await;
    assert_eq!(current.settlement(), Some(&a));
    assert_ne!(current.current_head(), r.input().snapshot());
    let h = store
        .staging()
        .head(r.input().plan().local_target().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(h.id, current.current_head());
    assert_eq!(h.parent, Some(r.input().snapshot()));
    assert_eq!(h.size, 3);
    let remote = node(&s.direct, &r.input().reserve().volume, &id(10)).await;
    assert_eq!(
        remote.payload.unwrap().reservation.id,
        id(if known { 40 } else { 60 })
    );
    assert_eq!(
        a.authority_receipt()
            .node
            .payload
            .as_ref()
            .unwrap()
            .reservation
            .id,
        id(40)
    );
    pending(&store, r.input().snapshot()).await;
    pending(&store, h.id).await;
    store.close().await.unwrap();
}

pub(super) async fn policy_recovery(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let p = publisher(&e, s).await;
    let (i, g) = plan(&e, s, 10, 40, b"policy source recovery\0", None, 0).await;
    p.begin(i, id(41)).await.unwrap();
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(p.resume_source(i).await.is_err());
    let r = e.storage().chunk_publications().get(i).await.unwrap();
    assert!(r.receipt().is_none());
    assert!(status(&e.storage(), i).await.settlement().is_none());
    e.storage().close().await.unwrap();
    let db = raw(root);
    db.execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    drop(db);
    checkpoint("NO_S3_FROM_HERE");
    wait(root, "no-s3-armed").await;
    let store = storage(root, "client").await;
    let expired = executor(config, store.clone(), false, true).await;
    let reservation = r.input().reservation().unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                expired.check_create(&reservation.key, reservation.size),
                Err(s3drive_s3::uploads::Failure::PolicyExpired)
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let p = publisher(&expired, s).await;
    s.proxy.clear();
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(p.resume_source(i).await.is_err());
    only_exact_apply(s, r.publication().unwrap());
    assert_eq!(store.chunk_publications().get(i).await.unwrap(), r);
    let narrow = executor(config, store.clone(), true, false).await;
    assert!(
        narrow
            .check_create(&reservation.key, reservation.size)
            .is_err()
    );
    let p = publisher(&narrow, s).await;
    let db = raw(root);
    db.execute_batch("CREATE TRIGGER no_receipt BEFORE UPDATE ON chunk_publications BEGIN SELECT RAISE(IGNORE); END").unwrap();
    s.proxy.clear();
    assert!(p.resume_source(i).await.is_err());
    only_exact_apply(s, r.publication().unwrap());
    assert_eq!(store.chunk_publications().get(i).await.unwrap(), r);
    assert!(status(&store, i).await.settlement().is_none());
    db.execute_batch("DROP TRIGGER no_receipt").unwrap();
    drop(db);
    let independent = recovery(&store, s, &r).await;
    s.proxy.clear();
    s.proxy.mode.store(10, Ordering::SeqCst);
    let (first, second) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(p.resume_source(i), async {
            tokio::time::timeout(Duration::from_secs(5), async {
                while s.proxy.applies().len() != 1 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("first publisher never reached saved Apply barrier");
            typed(p.clone().resume_source(i).await, |e| {
                matches!(e, Failure::Busy)
            });
            assert_eq!(s.proxy.requests().len(), 1, "Busy call sent a request");
            independent.resume_source(i).await
        })
    })
    .await
    .unwrap();
    let first = first.unwrap();
    assert_eq!(first, second.unwrap());
    exact(&first, &store.chunk_publications().get(i).await.unwrap());
    let requests = s.proxy.requests();
    assert_eq!(requests.len(), 2);
    for request in requests {
        let Request::Apply { request } = request else {
            panic!("fresh metadata queried");
        };
        assert_eq!(
            serde_json::to_vec(&request).unwrap(),
            serde_json::to_vec(r.publication().unwrap()).unwrap()
        );
    }
    assert_eq!(status(&store, i).await.settlement(), Some(&first));
    let db = raw(root);
    let rows: i64 = db
        .query_row("SELECT count(*) FROM chunk_source_settlements", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(rows, 1);
    drop(db);
    assert_eq!(first.snapshot(), g);
    pending(&store, g).await;
    store.close().await.unwrap();
}
