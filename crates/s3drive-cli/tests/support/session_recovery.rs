//! Real SDK, process replacement, exact cancellation replay and retained names.
use super::*;
use s3drive_authority::Kind;
use s3drive_managed::disposition::Controller;
use s3drive_state::namespace_client::opens::OpenInput;
const FIRST: &[u8] = b"session original binary\0\xff";
const NEXT: &[u8] = b"session recovered binary\0\xfe";
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn replaced_mount_sessions_retain_missing_cleanup_replay_cancellation_and_complete_recorded_deletion()
 {
    pending_owner::run_children_retaining(
        &[
            "session_start",
            "session_cancel_unknown",
            "session_proof_unknown",
            "session_recovered",
            "session_cleanup_finish",
        ],
        &[2, 3, 3, 4, 4],
        &[FIRST, NEXT, b""],
    )
    .await;
}
async fn input(
    p: &Publisher,
    w: &WireClient,
    n: u64,
    object: u64,
    session: u64,
    doc: bool,
    create: bool,
) -> OpenInput {
    let mut i = disposition_controller::input(p, w, n, object, doc, create, Kind::EmptyFile).await;
    i.session = id(session);
    i.session_bound = true;
    i
}
pub(super) async fn run_mode(
    mode: &str,
    config: &s3drive_core::Config,
    root: &Path,
    w: &WireClient,
    volume: &str,
) {
    if mode == "session_start" {
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
        "session-client",
        WireClient::loopback(&endpoint, Token::new(SECRET.into()).unwrap()).unwrap(),
        volume.into(),
    )
    .await;
    Box::pin(step(mode, &p, w, root, &loss)).await;
    p.storage().close().await.unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();
}
async fn step(mode: &str, p: &Publisher, w: &WireClient, root: &Path, loss: &AtomicU8) {
    let n = match mode {
        "session_start" => 2000,
        "session_cancel_unknown" => 2001,
        "session_proof_unknown" => 2002,
        "session_recovered" => 2003,
        _ => 2004,
    };
    let c = Controller::attach(p.clone(), id(n)).await.unwrap();
    assert!(Controller::attach(p.clone(), id(n + 100)).await.is_err());
    if mode == "session_start" {
        c.open(input(p, w, 10, 1010, n, true, true).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(10)).await.unwrap();
        pending_owner::stage(p, w, 10, 5000, FIRST).await;
        pending_owner::prepare(p, w, 10, 5000, 6000, true).await;
        p.flush_edit(&id(5000)).await.unwrap();
        c.open(input(p, w, 10, 1011, n, false, false).await)
            .await
            .unwrap();
        c.open(input(p, w, 20, 1020, n, true, true).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(20)).await.unwrap();
        pending_owner::stage(p, w, 20, 5020, b"").await;
        pending_owner::prepare(p, w, 20, 5020, 6020, true).await;
        p.flush_edit(&id(5020)).await.unwrap();
        c.set_delete(&id(1010), &id(3000), true).await.unwrap();
        pending_owner::stage(p, w, 10, 5001, NEXT).await;
        // No cleanup or close fact is emitted by the process that is killed.
    } else if mode == "session_cancel_unknown" {
        assert!(Controller::attach(p.clone(), id(2000)).await.is_err());
        assert!(c.set_delete(&id(1010), &id(3001), false).await.is_err());
        let old = p.storage().opens().get(id(1010)).await.unwrap();
        assert!(old.cleanup().is_none() && old.close().is_none());
        assert!(
            Controller::new(p.clone())
                .set_delete(&id(1010), &id(3001), false)
                .await
                .is_err()
        );
        loss.store(5, Ordering::SeqCst);
        assert!(c.recover_previous(&id(10)).await.is_err());
        assert_eq!(loss.load(Ordering::SeqCst), 0);
        let r = p
            .storage()
            .opens()
            .pending_recovery(old.input.volume, id(10))
            .await
            .unwrap()
            .unwrap();
        let cancel = r.input.cancel.unwrap();
        assert!(
            p.storage()
                .namespace()
                .get(cancel.request.operation)
                .await
                .unwrap()
                .outcome
                .is_none()
        );
    } else if mode == "session_proof_unknown" {
        let db = rusqlite::Connection::open(root.join("session-client/metadata.sqlite3")).unwrap();
        db.execute_batch("CREATE TRIGGER ignore_session_proof BEFORE UPDATE ON windows_session_recoveries BEGIN SELECT RAISE(IGNORE);END").unwrap();
        assert!(c.recover_previous(&id(10)).await.is_err());
        assert!(p.owned_removal(&id(10)).await.unwrap().is_none());
        assert!(!p.storage().opens().recovered(id(1010)).await.unwrap());
        db.execute_batch("DROP TRIGGER ignore_session_proof")
            .unwrap();
        drop(db);
    } else if mode == "session_recovered" {
        assert_eq!(c.recover_previous(&id(10)).await.unwrap(), 2);
        assert_eq!(p.read(&id(10)).await.unwrap().bytes(), NEXT);
        let old = p.storage().opens().get(id(1010)).await.unwrap();
        assert!(old.cleanup().is_none() && old.close().is_none());
        assert!(p.storage().opens().recovered(id(1010)).await.unwrap());
        assert!(c.open(old.input.clone()).await.is_err());
        assert_eq!(c.recover_previous(&id(20)).await.unwrap(), 1);
        assert_eq!(p.read(&id(20)).await.unwrap().bytes(), b"");
        let mut next = old.input;
        next.id = id(2010);
        next.session = id(n);
        next.delete_on_close = false;
        next.admission = None;
        c.open(next).await.unwrap();
        c.cleanup(&id(2010), &id(3110), 0).await.unwrap();
        c.close(&id(2010), &id(3210)).await.unwrap();
        c.open(input(p, w, 30, 1030, n, true, true).await)
            .await
            .unwrap();
        p.bind_edit_file(&id(30)).await.unwrap();
        pending_owner::stage(p, w, 30, 5030, FIRST).await;
        loss.store(6, Ordering::SeqCst);
        assert!(c.cleanup(&id(1030), &id(3130), 1).await.is_err());
        loss.store(8, Ordering::SeqCst);
        assert!(
            c.open(input(p, w, 40, 1040, n, true, true).await)
                .await
                .is_err()
        );
        assert!(!p.storage().opens().get(id(1040)).await.unwrap().active());
    } else {
        assert_eq!(mode, "session_cleanup_finish");
        assert_eq!(c.recover_previous(&id(30)).await.unwrap(), 1);
        assert!(p.read(&id(30)).await.is_err());
        let old = p.storage().opens().get(id(1030)).await.unwrap();
        assert!(old.cleanup().is_some() && old.close().is_none());
        assert!(p.storage().opens().recovered(id(1030)).await.unwrap());
        assert_eq!(c.recover_previous(&id(40)).await.unwrap(), 1);
        let old = p.storage().opens().get(id(1040)).await.unwrap();
        assert!(!old.active() && old.events.is_empty());
        assert!(p.storage().opens().recovered(id(1040)).await.unwrap());
        assert!(p.read(&id(40)).await.unwrap().bytes().is_empty());
        assert!(p.storage().opens().records().await.unwrap().is_empty());
        for edit in [5000, 5001, 5020, 5030] {
            p.flush_edit(&id(edit)).await.unwrap();
        }
        return;
    }
    checkpoint("FENCED_CHECKPOINT");
    std::future::pending::<()>().await;
}
