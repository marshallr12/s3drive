//! Actual SDK/TCP/SQLite prospective admission; no OS mount or live provider.
use super::*;
use s3drive_managed::creation::{
    CreationOpen, Creator, Draft, Failure as CreationFailure, Recovery,
};
use s3drive_state::{
    chunk_mount,
    manifest_creation::{Input, Limits, Record},
    namespace_client as names,
    namespace_client::opens,
};

fn claim(id_number: u64) -> CreationOpen {
    CreationOpen {
        id: id(id_number),
        session: id(80),
        delete_on_close: true,
        posix: false,
    }
}
async fn setup_open(
    config: &s3drive_core::Config,
    root: &Path,
    s: &Server,
) -> (Executor, chunk_mount::Client, Creator, Draft) {
    let (e, chunks, c, d) = creator_import::setup_owner(config, root, s).await;
    chunks
        .opens()
        .start_session(id(80), chunks.key().volume().into())
        .await
        .unwrap();
    (e, chunks, c, d)
}
fn input(
    e: &Executor,
    chunks: &chunk_mount::Client,
    d: &Draft,
    store: s3drive_authority::Store,
    open: CreationOpen,
) -> Input {
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    Input {
        volume: chunks.key().volume().into(),
        operation: d.operation.clone(),
        expected_sequence: d.expected_sequence,
        parent: d.parent.clone(),
        name: d.name.clone(),
        store,
        source: *target.source(),
        fingerprint: target.fingerprint().into(),
        owner: Some(chunks.key().owner().into()),
        open: Some(open),
    }
}
async fn mount(
    e: &Executor,
    volume: String,
    store: s3drive_authority::Store,
) -> chunk_mount::Client {
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    e.storage()
        .chunk_mount(chunk_mount::Input {
            volume,
            controls: vec![names::Control {
                store,
                source: *target.source(),
                fingerprint: target.fingerprint().into(),
            }],
        })
        .await
        .unwrap()
}
async fn reopened(root: &Path) -> (Storage, chunk_mount::Client, Record) {
    let storage = storage(root, "client").await;
    let record = storage
        .manifest_creations(Limits::default())
        .unwrap()
        .get(id(40))
        .await
        .unwrap();
    let i = record.input();
    let chunks = storage
        .chunk_mount(chunk_mount::Input {
            volume: i.volume.clone(),
            controls: vec![names::Control {
                store: i.store.clone(),
                source: i.source,
                fingerprint: i.fingerprint.clone(),
            }],
        })
        .await
        .unwrap();
    (storage, chunks, record)
}
async fn metadata(chunks: &chunk_mount::Client, record: &Record, s: &Server) -> Recovery {
    Recovery::for_chunks(
        chunks,
        s.wire.clone(),
        record.input().store.clone(),
        Limits::default(),
        Duration::from_secs(30),
    )
    .await
    .unwrap()
}

pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, c, d) = setup_open(config, root, s).await;
    checkpoint("CREATOR_OPEN_GATE_ARM");
    wait(root, "creator-open-gate-armed").await;
    let task = {
        let c = c.clone();
        let chunks = chunks.clone();
        let d = d.clone();
        tokio::spawn(async move { c.create_for_open(d, &chunks, claim(71)).await })
    };
    checkpoint("CREATOR_OPEN_GATE_INSPECT");
    wait(root, "creator-open-gate-checked").await;
    let result = task.await.unwrap().unwrap();
    assert_eq!(result.metadata().node.name, d.name);
    assert_eq!(
        result.original_removal().unwrap().owner,
        chunks.key().owner()
    );
    let record = chunks
        .creations(Limits::default())
        .unwrap()
        .get(d.operation.clone())
        .await
        .unwrap();
    assert_eq!(record.input().open.as_ref(), Some(&claim(71)));
    assert!(!chunks.opens().get(id(71)).await.unwrap().active());
    chunks
        .namespace()
        .import_creation(d.operation.clone())
        .await
        .unwrap();
    assert!(
        chunks
            .opens()
            .activate(id(71), id(72))
            .await
            .unwrap()
            .active()
    );
    let expected = result.metadata().clone();
    change(
        &s.direct,
        44,
        Mutation::CancelRemoval {
            node: d.operation.clone(),
            fence: result.original_removal().unwrap().clone(),
        },
    )
    .await;
    checkpoint("NO_S3_FROM_HERE");
    wait(root, "no-s3-armed").await;
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    assert_eq!(
        c.create_for_open(d.clone(), &chunks, claim(71))
            .await
            .unwrap()
            .metadata(),
        &expected
    );
    let mut changed = claim(71);
    changed.posix = true;
    assert!(
        c.create_for_open(d.clone(), &chunks, changed)
            .await
            .is_err()
    );
    let r = metadata(&chunks, &record, s).await;
    assert_eq!(r.resume(d.operation).await.unwrap().metadata(), &expected);
    assert!(s.proxy.requests().is_empty());
    e.storage().close().await.unwrap();
}

pub async fn controls(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, c, d) = setup_open(config, root, s).await;
    let foreign = executor(config, storage(root, "foreign").await, false, false).await;
    let cap = mount(
        &foreign,
        chunks.key().volume().into(),
        empty_creation::store(s).await,
    )
    .await;
    s.proxy.clear();
    assert!(c.create_for_open(d.clone(), &cap, claim(71)).await.is_err());
    assert!(s.proxy.requests().is_empty());
    foreign.storage().close().await.unwrap();
    let mut absent = claim(71);
    absent.session = id(81);
    assert!(c.create_for_open(d.clone(), &chunks, absent).await.is_err());
    assert!(
        chunks
            .creations(Limits::default())
            .unwrap()
            .records()
            .await
            .unwrap()
            .is_empty()
    );
    assert!(chunks.opens().records().await.unwrap().is_empty());
    assert!(s.proxy.applies().is_empty());
    let mut invalid = claim(71);
    invalid.id = id(0);
    assert!(
        c.create_for_open(d.clone(), &chunks, invalid)
            .await
            .is_err()
    );
    let wrong = executor(config, storage(root, "wrong-store").await, false, false).await;
    let mut st = empty_creation::store(s).await;
    st.id = id(201);
    let cap = mount(&wrong, chunks.key().volume().into(), st).await;
    cap.opens()
        .start_session(id(80), cap.key().volume().into())
        .await
        .unwrap();
    let cc = empty_creation::creator(wrong.clone(), s).await;
    s.proxy.clear();
    assert!(cc.create_for_open(d, &cap, claim(71)).await.is_err());
    assert!(s.proxy.requests().is_empty());
    assert!(
        cap.creations(Limits::default())
            .unwrap()
            .records()
            .await
            .unwrap()
            .is_empty()
    );
    wrong.storage().close().await.unwrap();
    e.storage().close().await.unwrap();
}

fn digest<T: serde::Serialize>(tag: &[u8], value: &T) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(tag);
    h.update(serde_json::to_vec(value).unwrap());
    h.finalize().to_vec()
}
// Deliberately modeled bulk history, validated by actual public worker loading.
fn seed_quota(root: &Path, which: u8, i: &Input) {
    let mut db = rusqlite::Connection::open(root.join("metadata.sqlite3")).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    let tx = db.transaction().unwrap();
    if which == 1 {
        for n in 0..4094 {
            let operation = id(10000 + n);
            let name = format!("modeled-dir-{n}");
            let sequence = 1000 + n;
            let input = names::Input {
                request: Apply {
                    volume: i.volume.clone(),
                    operation: operation.clone(),
                    expected_sequence: 0,
                    mutation: Mutation::CreateEntry {
                        parent: s3drive_authority::ROOT.into(),
                        parent_revision: 0,
                        name: name.clone(),
                        node_kind: Kind::Directory,
                        store: None,
                    },
                },
                store: i.store.clone(),
                source: i.source,
                fingerprint: i.fingerprint.clone(),
                entries: vec![],
                destination_control: None,
            };
            let outcome = Some(Ok::<_, s3drive_authority::Error>(
                s3drive_authority::Receipt {
                    sequence,
                    node: Node {
                        id: operation.clone(),
                        parent: Some(s3drive_authority::ROOT.into()),
                        name,
                        kind: Kind::Directory,
                        revision: sequence,
                        deleted: false,
                        payload: None,
                        placement: None,
                    },
                    artifact: None,
                },
            ));
            let seal = digest(b"s3drive-namespace-intent-v1\0", &(&input, &outcome));
            tx.execute(
                "INSERT INTO namespace_intents VALUES(?1,?2,?3,?4)",
                rusqlite::params![
                    operation,
                    serde_json::to_vec(&input).unwrap(),
                    serde_json::to_vec(outcome.as_ref().unwrap()).unwrap(),
                    seal
                ],
            )
            .unwrap();
        }
    } else {
        let count = if which == 2 { 256 } else { 227 };
        let eventful = which == 3;
        for n in 0..count {
            let open = opens::OpenInput {
                id: id(10000 + n),
                session: id(80),
                volume: i.volume.clone(),
                node: id(10000 + n),
                store: i.store.clone(),
                source: i.source,
                fingerprint: i.fingerprint.clone(),
                delete_on_close: false,
                controlled: eventful,
                session_bound: false,
                posix: false,
                admission: None,
                creation: None,
            };
            let events = if eventful {
                vec![
                    opens::Event {
                        id: id(20000 + n),
                        action: opens::Action::Activate { proof: None },
                    },
                    opens::Event {
                        id: id(30000 + n),
                        action: opens::Action::Callback(opens::Callback::SetDelete {
                            delete: false,
                        }),
                    },
                ]
            } else {
                vec![]
            };
            let mut previous = None;
            let mut saved = vec![];
            for (index, event) in events.iter().enumerate() {
                let hash = digest(
                    b"s3drive-open-event-v1\0",
                    &(&open.id, index + 1, &event.id, &event.action, &previous),
                );
                saved.push((event, index, previous.clone(), hash.clone()));
                previous = Some(hash);
            }
            let owner = i.owner.as_ref().unwrap();
            let seal = digest(
                b"s3drive-open-row-v1\0",
                &(&open, owner, events.len(), &previous),
            );
            let anchor = digest(b"s3drive-open-input-v1\0", &(&open, owner));
            tx.execute(
                "INSERT INTO windows_open_journal VALUES(?1,?2,?3,?4,?5,?6)",
                rusqlite::params![
                    open.id,
                    owner,
                    serde_json::to_vec(&open).unwrap(),
                    events.len() as i64,
                    previous,
                    seal
                ],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO retained_windows_opens VALUES(?1,?2)",
                rusqlite::params![open.id, anchor],
            )
            .unwrap();
            for (event, index, previous, hash) in saved {
                tx.execute(
                    "INSERT INTO windows_open_events VALUES(?1,?2,?3,?4,?5,?6)",
                    rusqlite::params![
                        event.id,
                        open.id,
                        index as i64 + 1,
                        serde_json::to_vec(&event.action).unwrap(),
                        previous,
                        hash
                    ],
                )
                .unwrap();
            }
        }
    }
    tx.commit().unwrap();
}
pub async fn quotas(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (first, _cap, _c, d) = setup_open(config, root, s).await;
    first.storage().close().await.unwrap();
    for which in 0..4 {
        let directory = format!("quota-{which}");
        let e = executor(config, storage(root, &directory).await, false, false).await;
        let cap = mount(&e, info(&s.direct).await.id, empty_creation::store(s).await).await;
        cap.opens()
            .start_session(id(80), cap.key().volume().into())
            .await
            .unwrap();
        let i = input(&e, &cap, &d, empty_creation::store(s).await, claim(71));
        let limits = if which == 0 {
            Limits {
                records: 1,
                metadata_bytes: 65536,
            }
        } else {
            Limits::default()
        };
        if which == 0 {
            let mut old = i.clone();
            old.operation = id(41);
            old.open.as_mut().unwrap().id = id(75);
            cap.creations(Limits::default())
                .unwrap()
                .begin(old)
                .await
                .unwrap();
        } else {
            seed_quota(&root.join(&directory), which, &i);
        }
        // Prove seeded history is accepted before invoking the SDK, rather
        // than mistaking a damaged fixture for quota enforcement.
        let before = cap
            .creations(Limits::default())
            .unwrap()
            .records()
            .await
            .unwrap();
        assert_eq!(before.len(), usize::from(which == 0));
        if which >= 2 {
            assert_eq!(
                cap.opens().records().await.unwrap().len(),
                if which == 2 { 256 } else { 227 }
            );
        }
        let c = Creator::bind(
            e.clone(),
            s.wire.clone(),
            cap.key().volume().into(),
            i.store.clone(),
            limits,
            Duration::from_secs(60),
        )
        .unwrap();
        s.proxy.clear();
        let error = c
            .create_for_open(d.clone(), &cap, claim(71))
            .await
            .unwrap_err();
        match which {
            0 => assert!(matches!(
                error.downcast_ref::<s3drive_state::manifest_creation::Failure>(),
                Some(s3drive_state::manifest_creation::Failure::Quota)
            )),
            1 => assert!(error.to_string().contains("closing"), "{error}"),
            2 => assert!(error.to_string().contains("per-open limit"), "{error}"),
            _ => assert!(
                error.to_string().contains("preserves activation"),
                "{error}"
            ),
        }
        assert!(s.proxy.applies().is_empty());
        assert_eq!(
            cap.creations(Limits::default())
                .unwrap()
                .records()
                .await
                .unwrap(),
            before
        );
        assert!(
            cap.creations(Limits::default())
                .unwrap()
                .get(id(40))
                .await
                .is_err()
        );
        e.storage().close().await.unwrap();
    }
}

pub async fn recovery(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, c, d) = setup_open(config, root, s).await;
    s.proxy.mode.store(20, Ordering::SeqCst);
    assert!(c.create_for_open(d, &chunks, claim(71)).await.is_err());
    let record = chunks
        .creations(Limits::default())
        .unwrap()
        .get(id(40))
        .await
        .unwrap();
    assert!(record.request().is_some() && record.receipt().is_none());
    e.storage().close().await.unwrap();
    drop(c);
    drop(chunks);
    drop(e);
    std::fs::rename(root.join("credentials"), root.join("credentials.disabled")).unwrap();
    std::fs::rename(root.join("policy.toml"), root.join("policy.disabled")).unwrap();
    let db = raw(root);
    db.execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
        .unwrap();
    drop(db);
    checkpoint("NO_S3_FROM_HERE");
    wait(root, "no-s3-armed").await;
    recover(root, s).await;
}
pub async fn recover(root: &Path, s: &Server) {
    assert!(!root.join("credentials").exists() && !root.join("policy.toml").exists());
    let (storage, chunks, before) = reopened(root).await;
    let r = metadata(&chunks, &before, s).await;
    s.proxy.clear();
    s.proxy.mode.store(0, Ordering::SeqCst);
    let result = r.resume(id(40)).await.unwrap();
    assert_eq!(result.metadata().node.name, before.input().name);
    assert_eq!(
        result.original_removal().unwrap().owner,
        chunks.key().owner()
    );
    assert_eq!(s.proxy.applies(), vec![before.request().unwrap().clone()]);
    assert!(
        s.proxy
            .requests()
            .iter()
            .all(|r| matches!(r, Request::Apply { .. }))
    );
    chunks.namespace().import_creation(id(40)).await.unwrap();
    assert!(!chunks.opens().get(id(71)).await.unwrap().active());
    assert!(chunks.opens().activate(id(71), id(72)).await.is_err());
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    r.resume(id(40)).await.unwrap();
    assert!(s.proxy.requests().is_empty());
    assert!(r.close().await.is_err());
    storage.close().await.unwrap();
}
pub async fn physical(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, c, d) = setup_open(config, root, s).await;
    checkpoint("LOSE_PUT");
    wait(root, "put-enabled").await;
    assert!(
        c.create_for_open(d.clone(), &chunks, claim(71))
            .await
            .is_err()
    );
    let before = chunks
        .creations(Limits::default())
        .unwrap()
        .get(id(40))
        .await
        .unwrap();
    let g = before.generation().unwrap();
    assert!(before.request().is_none());
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
    e.storage().close().await.unwrap();
    drop(c);
    drop(chunks);
    drop(e);
    checkpoint("RESTORE_PUT");
    wait(root, "put-restored").await;
    let storage = storage(root, "client").await;
    let narrow = executor(config, storage.clone(), true, false).await;
    let chunks = mount(
        &narrow,
        before.input().volume.clone(),
        before.input().store.clone(),
    )
    .await;
    let c = empty_creation::creator(narrow, s).await;
    assert!(
        chunks
            .opens()
            .obsolete_session(id(80), before.input().volume.clone())
            .await
            .unwrap()
    );
    s.proxy.clear();
    s.proxy.mode.store(19, Ordering::SeqCst);
    assert!(
        c.create_for_open(d.clone(), &chunks, claim(71))
            .await
            .is_err()
    );
    assert_eq!(
        storage
            .uploads()
            .for_generation(g)
            .await
            .unwrap()
            .unwrap()
            .phase,
        s3drive_state::remote::Phase::Verified
    );
    assert!(storage.uploads().flush_receipt(g).await.unwrap().is_some());
    assert!(s.proxy.applies().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    c.create_for_open(d, &chunks, claim(71)).await.unwrap();
    chunks.namespace().import_creation(id(40)).await.unwrap();
    assert!(chunks.opens().activate(id(71), id(72)).await.is_err());
    storage.close().await.unwrap();
}
pub async fn unclaimed(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, c, d) = setup_open(config, root, s).await;
    let i = input(&e, &chunks, &d, empty_creation::store(s).await, claim(71));
    let before = chunks
        .creations(Limits::default())
        .unwrap()
        .begin(i)
        .await
        .unwrap();
    e.storage().close().await.unwrap();
    drop(c);
    drop(chunks);
    drop(e);
    let storage = storage(root, "client").await;
    let e = executor(config, storage.clone(), false, false).await;
    let chunks = mount(
        &e,
        before.input().volume.clone(),
        before.input().store.clone(),
    )
    .await;
    let c = empty_creation::creator(e, s).await;
    s.proxy.clear();
    assert!(c.create_for_open(d, &chunks, claim(71)).await.is_err());
    assert!(s.proxy.applies().is_empty());
    assert_eq!(
        chunks
            .creations(Limits::default())
            .unwrap()
            .get(id(40))
            .await
            .unwrap(),
        before
    );
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let error = metadata(&chunks, &before, s)
        .await
        .resume(id(40))
        .await
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CreationFailure>(),
        Some(CreationFailure::NotPrepared)
    ));
    assert!(s.proxy.requests().is_empty());
    storage.close().await.unwrap();
}
pub async fn deadline(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, _c, d) = setup_open(config, root, s).await;
    let c = Creator::bind(
        e.clone(),
        s.wire.clone(),
        chunks.key().volume().into(),
        empty_creation::store(s).await,
        Limits::default(),
        Duration::from_secs(10),
    )
    .unwrap();
    s.proxy.mode.store(13, Ordering::SeqCst);
    let task = {
        let c = c.clone();
        let chunks = chunks.clone();
        let d = d.clone();
        tokio::spawn(async move { c.create_for_open(d, &chunks, claim(71)).await })
    };
    tokio::time::timeout(Duration::from_secs(15), s.proxy.publish_gate.wait())
        .await
        .unwrap();
    for error in [
        c.create_for_open(d.clone(), &chunks, claim(71))
            .await
            .unwrap_err(),
        c.create_deferred(d.clone(), chunks.key().owner().into())
            .await
            .unwrap_err(),
        c.recovery().resume(id(40)).await.unwrap_err(),
    ] {
        assert!(matches!(
            error.downcast_ref::<CreationFailure>(),
            Some(CreationFailure::Busy)
        ));
    }
    let error = tokio::time::timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CreationFailure>(),
        Some(CreationFailure::Deadline)
    ));
    assert!(
        chunks
            .creations(Limits::default())
            .unwrap()
            .get(id(40))
            .await
            .unwrap()
            .request()
            .is_some()
    );
    s.proxy.mode.store(0, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(5), s.proxy.publish_gate.wait())
        .await
        .unwrap();
    c.create_for_open(d, &chunks, claim(71)).await.unwrap();
    e.storage().close().await.unwrap();
}
pub async fn crash(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (_e, chunks, c, d) = setup_open(config, root, s).await;
    s.proxy.mode.store(20, Ordering::SeqCst);
    assert!(c.create_for_open(d, &chunks, claim(71)).await.is_err());
    let r = chunks
        .creations(Limits::default())
        .unwrap()
        .get(id(40))
        .await
        .unwrap();
    assert!(r.request().is_some() && r.receipt().is_none());
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}

pub async fn stores(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, c, d) = setup_open(config, root, s).await;
    let one = empty_creation::store(s).await;
    change(
        &s.direct,
        201,
        Mutation::RegisterStore {
            route: one.route.clone(),
            root: "writable/.s3drive/second/".into(),
        },
    )
    .await;
    let Reply::Store(two) = ask(
        &s.direct,
        Request::Store {
            volume: chunks.key().volume().into(),
            store: id(201),
        },
    )
    .await
    else {
        panic!()
    };
    let target = e
        .recovery_target("writable/.s3drive/second/binding")
        .unwrap();
    let chunks = e
        .storage()
        .chunk_mount(chunk_mount::Input {
            volume: chunks.key().volume().into(),
            controls: vec![names::Control {
                store: two.clone(),
                source: *target.source(),
                fingerprint: target.fingerprint().into(),
            }],
        })
        .await
        .unwrap();
    let mut first = d;
    first.expected_sequence = info(&s.direct).await.sequence;
    let parent = node(&s.direct, chunks.key().volume(), s3drive_authority::ROOT).await;
    first.parent = s3drive_authority::NodeGuard {
        id: parent.id,
        revision: parent.revision,
    };
    c.create_for_open(first, &chunks, claim(71)).await.unwrap();
    let mut second = empty_creation::draft(s, 42).await;
    second.name = "other store 日本語".into();
    let cc = Creator::bind(
        e.clone(),
        s.wire.clone(),
        chunks.key().volume().into(),
        two.clone(),
        Limits::default(),
        Duration::from_secs(60),
    )
    .unwrap();
    cc.create_for_open(second, &chunks, claim(73))
        .await
        .unwrap();
    // Preserve legacy APIs, but do not adopt their foreign deferred owner through
    // the new capability-bound metadata recovery interface.
    let mut foreign = empty_creation::draft(s, 44).await;
    foreign.name = "foreign legacy owner 日本語".into();
    let foreign_owner = if chunks.key().owner() == id(99) {
        id(98)
    } else {
        id(99)
    };
    assert_eq!(
        c.create_deferred(foreign, foreign_owner.clone())
            .await
            .unwrap()
            .original_removal()
            .unwrap()
            .owner,
        foreign_owner
    );
    let mut ordinary = empty_creation::draft(s, 45).await;
    ordinary.name = "ordinary legacy 日本語".into();
    assert!(
        c.create(ordinary)
            .await
            .unwrap()
            .original_removal()
            .is_none()
    );
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let a = Recovery::for_chunks(
        &chunks,
        s.wire.clone(),
        one,
        Limits::default(),
        Duration::from_secs(30),
    )
    .await
    .unwrap();
    let b = Recovery::for_chunks(
        &chunks,
        s.wire.clone(),
        two.clone(),
        Limits::default(),
        Duration::from_secs(30),
    )
    .await
    .unwrap();
    assert_eq!(
        a.records()
            .await
            .unwrap()
            .iter()
            .map(|r| r.input().operation.clone())
            .collect::<Vec<_>>(),
        vec![id(40), id(45)]
    );
    assert_eq!(b.records().await.unwrap().len(), 1);
    assert!(a.resume(id(42)).await.is_err());
    assert!(a.resume(id(44)).await.is_err());
    assert!(a.resume(id(45)).await.unwrap().original_removal().is_none());
    assert!(b.resume(id(40)).await.is_err());
    let mut invalid = two;
    invalid.root = "writable/not-a-store/".into();
    assert!(
        Recovery::for_chunks(
            &chunks,
            s.wire.clone(),
            invalid,
            Limits::default(),
            Duration::from_secs(30)
        )
        .await
        .is_err()
    );
    assert!(s.proxy.requests().is_empty());
    let db = raw(root);
    db.execute(
        "UPDATE manifest_creations SET state_hash=zeroblob(32) WHERE operation=?",
        [id(42)],
    )
    .unwrap();
    drop(db);
    assert!(a.records().await.is_err()); // complete role checked before store filter
    e.storage().close().await.unwrap();
}

pub async fn rejected(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (e, chunks, c, d) = setup_open(config, root, s).await;
    let original = c
        .create_for_open(d.clone(), &chunks, claim(71))
        .await
        .unwrap()
        .metadata()
        .clone();
    let mut rival = empty_creation::draft(s, 42).await;
    rival.name = d.name;
    assert!(
        c.create_for_open(rival.clone(), &chunks, claim(73))
            .await
            .is_err()
    );
    let record = chunks
        .creations(Limits::default())
        .unwrap()
        .get(id(42))
        .await
        .unwrap();
    assert_eq!(
        record.rejection(),
        Some(s3drive_authority::Error::AlreadyExists)
    );
    assert!(
        record.generation().is_some() && record.request().is_some() && record.receipt().is_none()
    );
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let error = c
        .create_for_open(rival, &chunks, claim(73))
        .await
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CreationFailure>(),
        Some(CreationFailure::RetainedConflict)
    ));
    assert!(s.proxy.requests().is_empty());
    e.storage().close().await.unwrap();
    drop(c);
    drop(chunks);
    drop(e);
    let storage = storage(root, "client").await;
    let i = record.input();
    let chunks = storage
        .chunk_mount(chunk_mount::Input {
            volume: i.volume.clone(),
            controls: vec![names::Control {
                store: i.store.clone(),
                source: i.source,
                fingerprint: i.fingerprint.clone(),
            }],
        })
        .await
        .unwrap();
    let recovery = metadata(&chunks, &record, s).await;
    let error = recovery.resume(id(42)).await.unwrap_err();
    assert!(matches!(
        error.downcast_ref::<CreationFailure>(),
        Some(CreationFailure::RetainedConflict)
    ));
    chunks
        .opens()
        .start_session(id(90), i.volume.clone())
        .await
        .unwrap();
    let cleanup = chunks
        .opens()
        .prepare_recovery(id(90), i.volume.clone(), i.operation.clone(), None)
        .await
        .unwrap();
    chunks
        .opens()
        .finish_recovery(id(90), cleanup.input.id)
        .await
        .unwrap();
    chunks.opens().retire(id(73)).await.unwrap();
    assert!(!chunks.opens().get(id(73)).await.unwrap().active());
    assert!(s.proxy.requests().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    assert_eq!(node(&s.direct, &i.volume, &id(40)).await, original.node);
    storage.close().await.unwrap();
}
