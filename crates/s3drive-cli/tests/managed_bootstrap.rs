//! Real subprocess CLI / TCP / SQLite; synthetic metadata service fault hooks.
//! No live provider credentials, S3 operations or OS mount claims.
use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use s3drive_authority::{Authority, Reply, Request};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    process::Command,
    sync::{Notify, oneshot},
    task::JoinHandle,
};

#[derive(Clone)]
struct Fault {
    client: s3drive_authority::Client,
    applies: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
    mode: Arc<AtomicUsize>,
    committed: Arc<Notify>,
    release: Arc<Notify>,
}
async fn proxy(State(f): State<Fault>, Json(r): Json<Request>) -> Response {
    f.requests.fetch_add(1, Ordering::SeqCst);
    let apply = matches!(r, Request::Apply { .. });
    if apply {
        f.applies.fetch_add(1, Ordering::SeqCst);
    }
    let response = f.client.request(r).await;
    if apply {
        match f.mode.swap(0, Ordering::SeqCst) {
            1 => return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response(),
            2 => {
                f.committed.notify_one();
                f.release.notified().await;
            }
            _ => (),
        }
    }
    Json(response).into_response()
}
struct Harness {
    tmp: tempfile::TempDir,
    authority: Authority,
    endpoint: String,
    volume: String,
    source: PathBuf,
    token: PathBuf,
    ca: Option<PathBuf>,
    fault: Fault,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
    s3: tokio::net::TcpListener,
}
impl Harness {
    async fn new(tls: bool) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let authority = Authority::open_with_policy(
            tmp.path().join("authority"),
            "portable-collision-v1".into(),
        )
        .await
        .unwrap();
        let Reply::Info(info) = authority.client().request(Request::Info {}).await else {
            panic!("info")
        };
        let token = s3drive_state::create_private_secret(
            &tmp.path().join("secret"),
            "a".repeat(64).as_bytes(),
        )
        .unwrap();
        let fault = Fault {
            client: authority.client(),
            applies: Arc::new(AtomicUsize::new(0)),
            requests: Arc::new(AtomicUsize::new(0)),
            mode: Arc::new(AtomicUsize::new(0)),
            committed: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "{}://{}/v1/namespace",
            if tls { "https" } else { "http" },
            listener.local_addr().unwrap()
        );
        let (stop, receive) = oneshot::channel();
        let (ca, task) = if tls {
            let rcgen::CertifiedKey { cert, signing_key } =
                rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
            let path = s3drive_state::create_private_document(
                &tmp.path().join("tls"),
                "ca.pem",
                cert.pem().as_bytes(),
            )
            .unwrap();
            let tls = s3drive_authority::ServerTls::from_pem(
                cert.pem().as_bytes(),
                signing_key.serialize_pem().as_bytes(),
            )
            .unwrap();
            let client = authority.client();
            (
                Some(path),
                tokio::spawn(async move {
                    s3drive_authority::serve_tls(
                        listener,
                        client,
                        s3drive_authority::Token::new("a".repeat(64)).unwrap(),
                        tls,
                        async {
                            let _ = receive.await;
                        },
                    )
                    .await
                    .unwrap();
                }),
            )
        } else {
            let app = axum::Router::new()
                .route("/v1/namespace", axum::routing::post(proxy))
                .with_state(fault.clone());
            (
                None,
                tokio::spawn(async move {
                    axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = receive.await;
                        })
                        .await
                        .unwrap();
                }),
            )
        };
        let s3 = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let source = tmp.path().join("sources.toml");
        std::fs::write(&source, format!("version=1\nmode='read-only'\n[[sources]]\nalias='docs'\nbucket='test-bucket'\nregion='us-east-1'\nprefix='資料/'\nprofile='unconfigured-fixture-profile'\nendpoint='http://{}'\nallow_http_loopback=true\npath_style=true\n", s3.local_addr().unwrap())).unwrap();
        Self {
            tmp,
            authority,
            endpoint,
            volume: info.id,
            source,
            token,
            ca,
            fault,
            stop,
            task,
            s3,
        }
    }
    fn command(&self, verb: &str) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_s3drive"));
        c.args(["managed", verb])
            .arg("--token-file")
            .arg(&self.token)
            .env(
                "AWS_SHARED_CREDENTIALS_FILE",
                self.tmp.path().join("missing-credentials"),
            )
            .env("AWS_CONFIG_FILE", self.tmp.path().join("missing-config"))
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .kill_on_drop(true);
        if let Some(ca) = &self.ca {
            c.arg("--ca-file").arg(ca);
        }
        c
    }
    async fn output(c: &mut Command) -> Output {
        tokio::time::timeout(Duration::from_secs(20), c.output())
            .await
            .expect("CLI deadline")
            .unwrap()
    }
    async fn ok(c: &mut Command) -> Value {
        let out = Self::output(c).await;
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    async fn prepare(&self, name: &str) -> Value {
        Self::ok(
            self.command("store-plan")
                .arg(&self.source)
                .arg("docs")
                .args([
                    "--authority",
                    &self.endpoint,
                    "--volume",
                    &self.volume,
                    "--root",
                    "managed/.s3drive/",
                ])
                .arg("--directory")
                .arg(self.tmp.path().join(name)),
        )
        .await
    }
    fn apply(&self, p: &Value) -> Command {
        let mut c = self.command("store-apply");
        c.arg(&self.source)
            .arg("docs")
            .arg(p["plan"].as_str().unwrap())
            .arg("--confirm")
            .arg(p["plan_sha256"].as_str().unwrap());
        c
    }
    async fn status(&self, p: &Value) -> Value {
        Self::ok(
            self.command("store-status")
                .arg(p["plan"].as_str().unwrap()),
        )
        .await
    }
    async fn finish(self) {
        assert!(
            tokio::time::timeout(Duration::from_millis(30), self.s3.accept())
                .await
                .is_err(),
            "bootstrap contacted S3"
        );
        self.fault.release.notify_waiters();
        self.stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(15), self.task)
            .await
            .unwrap()
            .unwrap();
        self.authority.shutdown().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn operator_can_register_two_stores_and_generate_another_machines_config_without_credentials()
{
    let h = Harness::new(false).await;
    let first = h.prepare("first").await;
    assert_eq!(first["physical_prefix"], "資料/managed/.s3drive/");
    let plan_path = first["plan"].as_str().unwrap();
    let text = std::fs::read_to_string(plan_path).unwrap();
    std::fs::write(
        plan_path,
        format!("# Owner comment does not alter the frozen request\n{text}\n"),
    )
    .unwrap();
    assert_eq!(h.status(&first).await["status"], "not-recorded");
    assert_eq!(
        Harness::ok(&mut h.apply(&first)).await["status"],
        "registered"
    );
    assert_eq!(h.status(&first).await["status"], "registered");
    // Repeating an applied plan observes its exact retained receipt, no Apply.
    Harness::ok(&mut h.apply(&first)).await;
    assert_eq!(h.fault.applies.load(Ordering::SeqCst), 1);
    let second = h.prepare("second").await;
    Harness::ok(&mut h.apply(&second)).await;
    let p1 = Path::new(first["plan"].as_str().unwrap());
    let before = std::fs::read(p1).unwrap();
    let duplicate = Harness::output(
        h.command("store-plan")
            .arg(&h.source)
            .arg("docs")
            .args([
                "--authority",
                &h.endpoint,
                "--volume",
                &h.volume,
                "--root",
                "managed/.s3drive/",
            ])
            .arg("--directory")
            .arg(p1.parent().unwrap()),
    )
    .await;
    assert!(!duplicate.status.success());
    assert_eq!(std::fs::read(p1).unwrap(), before);
    let local = h.tmp.path().join("other-machine.toml");
    let text = std::fs::read_to_string(&h.source)
        .unwrap()
        .replace("alias='docs'", "alias='local'")
        .replace(
            "unconfigured-fixture-profile",
            "another-unconfigured-profile",
        );
    std::fs::write(&local, text).unwrap();
    let out = Harness::ok(
        h.command("configure")
            .arg(&local)
            .arg("--plan")
            .arg(p1)
            .arg("--plan")
            .arg(second["plan"].as_str().unwrap())
            .arg("--map")
            .arg(format!("{}=local", first["store_id"].as_str().unwrap()))
            .arg("--map")
            .arg(format!("{}=local", second["store_id"].as_str().unwrap()))
            .arg("--directory")
            .arg(h.tmp.path().join("generated")),
    )
    .await;
    let c = s3drive_core::Config::parse(
        &std::fs::read_to_string(out["config"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(c.version, 3);
    assert_eq!(c.managed.unwrap().stores.len(), 2);
    assert_eq!(
        c.sources[0].profile.as_deref(),
        Some("another-unconfigured-profile")
    );
    assert_eq!(h.fault.applies.load(Ordering::SeqCst), 2);
    h.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stale_registration_is_retained_and_never_rebased_or_sent_again() {
    let h = Harness::new(false).await;
    let a = h.prepare("a").await;
    let b = h.prepare("b").await;
    let mut command_a = h.apply(&a);
    let mut command_b = h.apply(&b);
    let (x, y) = tokio::join!(
        Harness::output(&mut command_a),
        Harness::output(&mut command_b)
    );
    assert_ne!(x.status.success(), y.status.success());
    let loser = if x.status.success() { &b } else { &a };
    assert_eq!(h.status(loser).await["error"], "conflict");
    assert!(!Harness::output(&mut h.apply(loser)).await.status.success());
    assert_eq!(h.fault.applies.load(Ordering::SeqCst), 2);
    let Reply::Info(v) = h.authority.client().request(Request::Info {}).await else {
        panic!("info")
    };
    assert_eq!(v.sequence, 1);
    h.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lost_reply_and_killed_client_recover_the_original_durable_registration() {
    let h = Harness::new(false).await;
    let a = h.prepare("a").await;
    h.fault.mode.store(1, Ordering::SeqCst);
    assert!(!Harness::output(&mut h.apply(&a)).await.status.success());
    assert_eq!(h.status(&a).await["status"], "registered");
    Harness::ok(&mut h.apply(&a)).await;
    assert_eq!(h.fault.applies.load(Ordering::SeqCst), 1);
    let b = h.prepare("b").await;
    h.fault.mode.store(2, Ordering::SeqCst);
    let mut child = h
        .apply(&b)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), h.fault.committed.notified())
        .await
        .unwrap();
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    assert_eq!(h.status(&b).await["status"], "registered");
    Harness::ok(&mut h.apply(&b)).await;
    assert_eq!(h.fault.applies.load(Ordering::SeqCst), 2);
    h.fault.release.notify_one();
    h.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalid_intent_is_refused_before_metadata_or_provider_requests() {
    let h = Harness::new(false).await;
    let p = h.prepare("a").await;
    let before = h.fault.requests.load(Ordering::SeqCst);
    let mut wrong = p.clone();
    wrong["plan_sha256"] = json!("0".repeat(64));
    assert!(!Harness::output(&mut h.apply(&wrong)).await.status.success());
    let original = std::fs::read_to_string(&h.source).unwrap();
    for (old, new) in [
        ("test-bucket", "another-bucket"),
        ("us-east-1", "us-west-1"),
        ("資料/", "different/"),
    ] {
        std::fs::write(&h.source, original.replace(old, new)).unwrap();
        assert!(!Harness::output(&mut h.apply(&p)).await.status.success());
    }
    std::fs::write(&h.source, &original).unwrap();
    let path = p["plan"].as_str().unwrap();
    let plan = std::fs::read_to_string(path).unwrap();
    std::fs::write(path, format!("unknown='PRIVATE_MARKER'\n{plan}")).unwrap();
    let out = Harness::output(h.command("store-status").arg(path)).await;
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stderr).contains("PRIVATE_MARKER"));
    std::fs::write(path, plan).unwrap();
    assert_eq!(h.fault.requests.load(Ordering::SeqCst), before);
    assert_eq!(h.fault.applies.load(Ordering::SeqCst), 0);
    h.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn configure_refuses_unregistered_stores_and_implicit_write_authority() {
    let h = Harness::new(false).await;
    let p = h.prepare("a").await;
    let target = h.tmp.path().join("must-not-exist");
    let mut missing = h.command("configure");
    missing
        .arg(&h.source)
        .arg("--plan")
        .arg(p["plan"].as_str().unwrap())
        .arg("--directory")
        .arg(&target);
    assert!(!Harness::output(&mut missing).await.status.success());
    assert!(!target.exists());
    let before = h.fault.requests.load(Ordering::SeqCst);
    for options in [
        vec!["--mode", "read-write"],
        vec!["--default-store", p["store_id"].as_str().unwrap()],
        vec!["--windows-disposition", "posix-v1"],
    ] {
        let mut c = h.command("configure");
        c.arg(&h.source)
            .arg("--plan")
            .arg(p["plan"].as_str().unwrap())
            .arg("--directory")
            .arg(&target)
            .args(options);
        assert!(!Harness::output(&mut c).await.status.success());
        assert!(!target.exists());
    }
    assert_eq!(h.fault.requests.load(Ordering::SeqCst), before);
    assert_eq!(h.fault.applies.load(Ordering::SeqCst), 0);
    h.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bootstrap_uses_https_and_pins_the_exact_custom_trust_material() {
    let h = Harness::new(true).await;
    let p = h.prepare("tls-plan").await;
    assert_eq!(Harness::ok(&mut h.apply(&p)).await["status"], "registered");
    assert_eq!(h.status(&p).await["status"], "registered");
    let out = Harness::ok(
        h.command("configure")
            .arg(&h.source)
            .arg("--plan")
            .arg(p["plan"].as_str().unwrap())
            .arg("--directory")
            .arg(h.tmp.path().join("tls-drive")),
    )
    .await;
    assert_eq!(out["provider_access_verified"], false);
    let path = h.ca.as_ref().unwrap();
    let original = std::fs::read(path).unwrap();
    let mut changed = original.clone();
    changed.push(b'\n');
    std::fs::write(path, changed).unwrap();
    let error = Harness::output(&mut h.apply(&p)).await;
    assert!(!error.status.success());
    assert!(String::from_utf8_lossy(&error.stderr).contains("CA bytes differ"));
    std::fs::write(path, original).unwrap();
    let wrong = Harness::output(h.command("info").args([
        "--authority",
        &h.endpoint,
        "--volume",
        &"b".repeat(32),
    ]))
    .await;
    assert!(!wrong.status.success());
    h.finish().await;
}
