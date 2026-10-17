//! Actual executable callback policy through SDK/HTTP and process death.
use super::*;
use s3drive_authority::{ROOT, RemovalMode};
use s3drive_managed::disposition::Controller;
use s3drive_state::namespace_client::opens::OpenInput;
const FIRST: &[u8] = b"controller first\0\xff";
const NEXT: &[u8] = b"controller next\0\xfe";
const LATE: &[u8] = b"controller late\0\xfd";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disposition_controller_runs_mode_cleanup_late_io_and_recovers_rejected_and_abandoned_callbacks()
 {
    pending_owner::run_children_retaining(
        &[
            "controller_start",
            "controller_reject_recover",
            "controller_false_recover",
            "controller_orphan_recover",
            "controller_finish",
            "controller_retirement_recover",
        ],
        &[5, 5, 5, 6, 6, 6],
        &[FIRST, NEXT, LATE, b""],
    )
    .await;
}
async fn input(
    p: &Publisher,
    w: &WireClient,
    n: u64,
    object: u64,
    doc: bool,
    create: bool,
    kind: Kind,
) -> OpenInput {
    let owner = p.client_identity().await.unwrap();
    let req = pending_owner::request(
        w,
        n,
        Mutation::CreateDeferredEntry {
            parent: ROOT.into(),
            parent_revision: 0,
            store: (kind == Kind::EmptyFile).then(|| id(200)),
            name: format!("controller café 日本語 🦀 {n}"),
            node_kind: kind,
            owner,
        },
    )
    .await;
    let admission = p.namespace_change_input(req, &[]).unwrap();
    OpenInput {
        id: id(object),
        session: id(2000),
        volume: admission.request.volume.clone(),
        node: id(n),
        store: admission.store.clone(),
        source: admission.source,
        fingerprint: admission.fingerprint.clone(),
        delete_on_close: doc,
        controlled: true,
        admission: if create {
            Some(Box::new(admission))
        } else {
            None
        },
    }
}
async fn mode(p: &Publisher, n: u64) -> RemovalMode {
    p.owned_removal(&id(n)).await.unwrap().unwrap().mode
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "controller_start" {
        let reader = s3drive_s3::S3Reader::from_source(&config.sources[0])
            .await
            .unwrap();
        change(
            w,
            200,
            Mutation::RegisterStore {
                route: reader.storage_fingerprint(),
                root: "writable/.s3drive/".into(),
            },
        )
        .await;
    }
    let loss = Arc::new(AtomicU8::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let app = axum::Router::new()
        .route("/v1/namespace", axum::routing::post(proxy))
        .with_state(Proxy {
            upstream: w.clone(),
            loss: loss.clone(),
        });
    let (stop, receive) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = receive.await;
            })
            .await
            .unwrap();
    });
    let p = publisher(
        config,
        root,
        "controller-client",
        WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap(),
        volume.into(),
    )
    .await;
    Box::pin(step(mode, &p, w, root, volume, &loss)).await;
    p.storage().close().await.unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();
}
async fn step(
    mode_name: &str,
    p: &Publisher,
    w: &WireClient,
    root: &Path,
    volume: &str,
    loss: &AtomicU8,
) {
    let c = Controller::new(p.clone());
    if mode_name == "controller_start" {
        c.open(input(p, w, 10, 1010, true, true, Kind::EmptyFile).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(10)).await.unwrap();
        pending_owner::stage(p, w, 10, 5000, FIRST).await;
        pending_owner::prepare(p, w, 10, 5000, 6000, true).await;
        p.flush_edit(&id(5000)).await.unwrap();
        let retained = p.read(&id(10)).await.unwrap();
        c.open(input(p, w, 10, 1012, false, false, Kind::EmptyFile).await)
            .await
            .unwrap();
        c.set_delete(&id(1010), &id(3000), true).await.unwrap();
        assert_eq!(mode(p, 10).await, RemovalMode::Pending);
        c.open(input(p, w, 10, 1012, false, false, Kind::EmptyFile).await)
            .await
            .unwrap();
        assert!(
            c.open(input(p, w, 10, 1014, false, false, Kind::EmptyFile).await)
                .await
                .is_err()
        );
        assert_eq!(p.storage().opens().records().await.unwrap().len(), 2);
        c.cleanup(&id(1010), &id(3100), 0).await.unwrap();
        c.set_delete(&id(1012), &id(3001), false).await.unwrap();
        assert_eq!(mode(p, 10).await, RemovalMode::Deferred);
        c.open(input(p, w, 10, 1013, true, false, Kind::EmptyFile).await)
            .await
            .unwrap();
        c.close(&id(1010), &id(3200)).await.unwrap();
        c.cleanup(&id(1013), &id(3103), 0).await.unwrap();
        assert_eq!(mode(p, 10).await, RemovalMode::Pending);
        c.set_delete(&id(1012), &id(3002), false).await.unwrap();
        assert_eq!(mode(p, 10).await, RemovalMode::Deferred);
        c.close(&id(1013), &id(3203)).await.unwrap();
        c.set_delete(&id(1012), &id(3003), true).await.unwrap();
        c.set_delete(&id(1012), &id(3004), false).await.unwrap();
        c.set_delete(&id(1012), &id(3005), true).await.unwrap();
        assert_eq!(
            p.owned_removal(&id(10))
                .await
                .unwrap()
                .unwrap()
                .fence
                .intent,
            id(10)
        );
        pending_owner::stage(p, w, 10, 5001, NEXT).await;
        c.cleanup(&id(1012), &id(3112), 1).await.unwrap();
        assert!(p.read(&id(10)).await.is_err());
        assert_eq!(retained.bytes(), FIRST);
        change(
            w,
            30,
            Mutation::CreateEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                store: Some(id(200)),
                name: "controller café 日本語 🦀 10".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        pending_owner::stage(p, w, 10, 5002, LATE).await;
        c.close(&id(1012), &id(3212)).await.unwrap();
        assert!(!node(w, volume, 30).await.deleted);
        assert!(p.read(&id(10)).await.is_err());
        let late = p
            .storage()
            .logical()
            .file(volume.into(), id(10))
            .await
            .unwrap();
        assert_eq!(late.head, late.settled);
        assert!(late.base.deleted);
        assert!(
            c.open(input(p, w, 10, 1012, false, false, Kind::EmptyFile).await)
                .await
                .is_err()
        );
        c.open(input(p, w, 11, 1011, true, true, Kind::EmptyFile).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(11)).await.unwrap();
        pending_owner::stage(p, w, 11, 5003, b"").await;
        c.cleanup(&id(1011), &id(3111), 1).await.unwrap();
        c.close(&id(1011), &id(3211)).await.unwrap();
        c.open(input(p, w, 20, 1020, true, true, Kind::Directory).await)
            .await
            .unwrap();
        let dir = node(w, volume, 20).await;
        change(
            w,
            21,
            Mutation::CreateEntry {
                parent: id(20),
                parent_revision: dir.revision,
                store: Some(id(200)),
                name: "child 🦀".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        c.cleanup(&id(1020), &id(3120), 1).await.unwrap();
        c.close(&id(1020), &id(3220)).await.unwrap();
        assert!(!node(w, volume, 20).await.deleted);
        assert_eq!(
            node(w, volume, 21).await.parent.as_deref(),
            Some(id(20).as_str())
        );
        assert!(p.owned_removal(&id(20)).await.unwrap().is_none());
        change(
            w,
            40,
            Mutation::CreateEntry {
                parent: ROOT.into(),
                parent_revision: 0,
                store: Some(id(200)),
                name: "ordinary 🦀".into(),
                node_kind: Kind::EmptyFile,
            },
        )
        .await;
        c.open(input(p, w, 40, 1040, false, false, Kind::EmptyFile).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(40)).await.unwrap();
        let db =
            rusqlite::Connection::open(root.join("controller-client/metadata.sqlite3")).unwrap();
        db.execute_batch(r#"CREATE TRIGGER fail_completion BEFORE INSERT ON windows_open_events WHEN instr(CAST(NEW.action AS TEXT),'"type":"completed"')>0 BEGIN SELECT RAISE(IGNORE); END"#).unwrap();
        let accepted = c.set_delete(&id(1040), &id(3040), true).await.unwrap();
        assert!(accepted.maintenance_pending);
        assert_eq!(mode(p, 40).await, RemovalMode::Pending);
        db.execute_batch("DROP TRIGGER fail_completion").unwrap();
        drop(db);
        assert!(
            !c.set_delete(&id(1040), &id(3040), true)
                .await
                .unwrap()
                .maintenance_pending
        );
        let fence = p.owned_removal(&id(40)).await.unwrap().unwrap().fence;
        c.set_delete(&id(1040), &id(3041), false).await.unwrap();
        assert_eq!(
            p.owned_removal(&id(40)).await.unwrap().unwrap().fence,
            fence
        );
        c.cleanup(&id(1040), &id(3140), 0).await.unwrap();
        c.close(&id(1040), &id(3240)).await.unwrap();
        assert!(p.owned_removal(&id(40)).await.unwrap().is_none());
        assert!(!node(w, volume, 40).await.deleted);
        c.open(input(p, w, 50, 1050, true, true, Kind::EmptyFile).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(50)).await.unwrap();
        pending_owner::stage(p, w, 50, 5050, FIRST).await;
        pending_owner::prepare(p, w, 50, 5050, 6050, true).await;
        p.flush_edit(&id(5050)).await.unwrap();
        loss.store(11, Ordering::SeqCst);
        assert!(c.set_delete(&id(1050), &id(6050), true).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        assert_eq!(
            p.storage()
                .opens()
                .get(id(1050))
                .await
                .unwrap()
                .reply(&id(6050)),
            Some(Some(s3drive_authority::Error::Unavailable))
        );
    } else if mode_name == "controller_reject_recover" {
        c.recover(&id(50)).await.unwrap();
        assert_eq!(mode(p, 50).await, RemovalMode::Deferred);
        assert_eq!(
            p.owned_removal(&id(50))
                .await
                .unwrap()
                .unwrap()
                .fence
                .intent,
            id(50)
        );
        assert!(c.set_delete(&id(1050), &id(6050), true).await.is_err());
        assert_eq!(mode(p, 50).await, RemovalMode::Deferred);
        c.set_delete(&id(1050), &id(6051), true).await.unwrap();
        loss.store(11, Ordering::SeqCst);
        assert!(c.set_delete(&id(1050), &id(6052), false).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
    } else if mode_name == "controller_false_recover" {
        c.recover(&id(50)).await.unwrap();
        assert_eq!(mode(p, 50).await, RemovalMode::Pending);
        assert_eq!(
            p.owned_removal(&id(50))
                .await
                .unwrap()
                .unwrap()
                .fence
                .intent,
            id(50)
        );
        assert!(c.set_delete(&id(1050), &id(6052), false).await.is_err());
        loss.store(10, Ordering::SeqCst);
        let _ = c.set_delete(&id(1050), &id(6053), false).await;
        panic!("parent must kill controller with no callback reply");
    } else if mode_name == "controller_orphan_recover" {
        let r = p.storage().opens().get(id(1050)).await.unwrap();
        assert!(r.reply(&id(6053)).is_none());
        c.recover(&id(50)).await.unwrap();
        assert_eq!(mode(p, 50).await, RemovalMode::Pending);
        assert_eq!(
            p.owned_removal(&id(50))
                .await
                .unwrap()
                .unwrap()
                .fence
                .intent,
            id(50)
        );
        pending_owner::stage(p, w, 50, 5051, NEXT).await;
        loss.store(6, Ordering::SeqCst);
        assert!(c.cleanup(&id(1050), &id(6150), 1).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
    } else if mode_name == "controller_finish" {
        c.recover(&id(50)).await.unwrap();
        assert!(p.read(&id(50)).await.is_err());
        let db =
            rusqlite::Connection::open(root.join("controller-client/metadata.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER ignore_retirement BEFORE INSERT ON retired_windows_opens BEGIN SELECT RAISE(IGNORE); END").unwrap();
        assert!(c.close(&id(1050), &id(6250)).await.is_err());
        db.execute_batch("DROP TRIGGER ignore_retirement").unwrap();
        drop(db);
        let r = p.storage().opens().get(id(1050)).await.unwrap();
        assert!(r.pending().is_empty());
        assert!(r.completed(&id(6150)) && r.completed(&id(6250)));
        assert!(p.owned_removal(&id(50)).await.unwrap().is_none());
        for edit in [5000, 5001, 5002, 5003, 5050, 5051] {
            p.flush_edit(&id(edit)).await.unwrap();
        }
    } else {
        assert_eq!(mode_name, "controller_retirement_recover");
        c.recover(&id(50)).await.unwrap();
        let r = p.storage().opens().get(id(1050)).await.unwrap();
        assert!(r.completed(&id(6250)));
        assert!(
            p.storage()
                .opens()
                .records()
                .await
                .unwrap()
                .iter()
                .all(|r| r.input.id != id(1050))
        );
        assert!(c.open(r.input.clone()).await.is_err());
        c.close(&id(1050), &id(6250)).await.unwrap();
        for edit in [5000, 5001, 5002, 5003, 5050, 5051] {
            p.flush_edit(&id(edit)).await.unwrap();
        }
        return;
    }
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
