//! Real SDK + authenticated Rust authority + private SQLite, owned loopback
//! fixtures/dummy profiles only. No provider or mounted chunk-format acceptance.
#[path = "chunk_publication/composition.rs"]
mod composition;
#[path = "chunk_publication/creator_import.rs"]
mod creator_import;
#[path = "chunk_publication/creator_open.rs"]
mod creator_open;
#[path = "chunk_publication/deferred_creation.rs"]
mod deferred_creation;
#[path = "chunk_publication/empty_creation.rs"]
mod empty_creation;
#[path = "support/fixture.rs"]
pub mod fixture;
#[path = "chunk_publication/local_retention.rs"]
mod local_retention;
#[path = "chunk_publication/mixed_epochs.rs"]
mod mixed_epochs;
#[path = "chunk_publication/mixed_roots.rs"]
mod mixed_roots;
#[path = "chunk_publication/mixed_uploads.rs"]
mod mixed_uploads;
#[path = "chunk_publication/namespace_recovery.rs"]
mod namespace_recovery;
#[path = "chunk_publication/native_directories.rs"]
mod native_directories;
#[path = "chunk_publication/native_lifecycle.rs"]
mod native_lifecycle;
#[cfg(any(target_os = "linux", windows))]
#[path = "chunk_publication/native_mount.rs"]
mod native_mount;
#[path = "chunk_publication/native_retention.rs"]
mod native_retention;
#[path = "chunk_publication/native_sparse_sdk.rs"]
mod native_sparse_sdk;
#[path = "chunk_publication/native_writer.rs"]
mod native_writer;
#[path = "chunk_publication/owned_sparse.rs"]
mod owned_sparse;
#[path = "chunk_publication/source_settlement.rs"]
mod source_settlement;
#[path = "support/writes.rs"]
pub mod writes;
use s3drive_authority::{
    Apply, Authority, Kind, Mutation, Node, Reply, Request, Token, WireClient,
};
use s3drive_managed::chunks::{Failure, Publisher, Recovery};
use s3drive_s3::uploads::{Executor, WritePolicy, chunks::Uploader};
use s3drive_state::{
    Result, cache,
    chunk_publication::Record,
    chunked,
    journal::{self, GenerationId},
    worker::Storage,
};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    sync::oneshot,
    task::JoinHandle,
};
const SECRET: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn binary(s: &str) -> [u8; 16] {
    std::array::from_fn(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
}
fn content(n: &Node) -> Option<[u8; 32]> {
    s3drive_authority::payload_reference(&n.payload)
        .map(|s| std::array::from_fn(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap()))
}
fn checkpoint(s: &str) {
    println!("{s}");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
}
fn puts(f: &fixture::Fixture) -> usize {
    f.requests
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.starts_with("PUT ") && s.contains("/writable/.s3drive/"))
        .count()
}
async fn ask(w: &WireClient, r: Request) -> Reply {
    w.request(&r).await.unwrap()
}
async fn info(w: &WireClient) -> s3drive_authority::Volume {
    let Reply::Info(v) = ask(w, Request::Info {}).await else {
        panic!()
    };
    v
}
async fn node(w: &WireClient, v: &str, n: &str) -> Node {
    let Reply::Node(n) = ask(
        w,
        Request::Node {
            volume: v.into(),
            node: n.into(),
        },
    )
    .await
    else {
        panic!()
    };
    n
}
async fn change(w: &WireClient, op: u64, m: Mutation) -> s3drive_authority::Receipt {
    let v = info(w).await;
    let Reply::Applied(r) = ask(
        w,
        Request::Apply {
            request: Apply {
                volume: v.id,
                operation: id(op),
                expected_sequence: v.sequence,
                mutation: m,
            },
        },
    )
    .await
    else {
        panic!()
    };
    r
}
#[derive(Clone)]
struct Proxy {
    upstream: WireClient,
    mode: Arc<AtomicU8>,
    seen: Arc<Mutex<Vec<Request>>>,
    publish_gate: Arc<tokio::sync::Barrier>,
}
impl Proxy {
    fn clear(&self) {
        self.seen.lock().unwrap().clear();
    }
    fn requests(&self) -> Vec<Request> {
        self.seen.lock().unwrap().clone()
    }
    fn applies(&self) -> Vec<Apply> {
        self.requests()
            .into_iter()
            .filter_map(|r| {
                if let Request::Apply { request } = r {
                    Some(request)
                } else {
                    None
                }
            })
            .collect()
    }
}
async fn proxy(
    axum::extract::State(p): axum::extract::State<Proxy>,
    headers: axum::http::HeaderMap,
    axum::Json(request): axum::Json<Request>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    assert_eq!(
        headers.get("authorization").unwrap(),
        format!("Bearer {SECRET}").as_str()
    );
    p.seen.lock().unwrap().push(request.clone());
    if p.mode.load(Ordering::SeqCst) == 11 && matches!(&request, Request::ManifestPayload { .. }) {
        let checks = p
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|r| matches!(r, Request::ManifestPayload { .. }))
            .count();
        if checks >= 2 {
            return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
    if p.mode.load(Ordering::SeqCst) == 8 {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if let Request::Apply { request: apply } = &request
        && let Mutation::CreateEntry {
            parent,
            parent_revision,
            node_kind: Kind::Directory,
            ..
        } = &apply.mutation
        && p.mode
            .compare_exchange(18, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        let root = node(&p.upstream, &apply.volume, &id(0)).await;
        let mut rival = apply.clone();
        rival.operation = id(8888);
        rival.mutation = Mutation::MoveEntry {
            node: s3drive_authority::NodeGuard {
                id: parent.clone(),
                revision: *parent_revision,
            },
            source_parent: s3drive_authority::NodeGuard {
                id: root.id.clone(),
                revision: root.revision,
            },
            parent: s3drive_authority::NodeGuard {
                id: root.id,
                revision: root.revision,
            },
            name: "rival directory".into(),
            destination: None,
            destination_removal: None,
        };
        assert!(matches!(
            p.upstream
                .request(&Request::Apply { request: rival })
                .await
                .unwrap(),
            Reply::Applied(_)
        ));
    }
    if p.mode.load(Ordering::SeqCst) == 23
        && matches!(
            &request,
            Request::Apply {
                request: Apply {
                    mutation: Mutation::Publish { .. },
                    ..
                }
            }
        )
    {
        p.publish_gate.wait().await;
        p.publish_gate.wait().await;
    }
    let mut reply = p.upstream.request(&request).await.unwrap();
    if p.mode.load(Ordering::SeqCst) == 13
        && matches!(
            &request,
            Request::Apply {
                request: Apply {
                    mutation: Mutation::CreateEmptyManifest { .. }
                        | Mutation::CreateDeferredEmptyManifest { .. },
                    ..
                }
            }
        )
    {
        p.publish_gate.wait().await;
        p.publish_gate.wait().await;
    }
    if p.mode.load(Ordering::SeqCst) == 14 {
        let clear = |n: &mut Node| {
            if n.kind == Kind::File {
                n.kind = Kind::EmptyFile;
                n.payload = None;
            }
        };
        match &mut reply {
            Reply::Node(n) => clear(n),
            Reply::List(l) => {
                for n in &mut l.children {
                    clear(n)
                }
            }
            _ => {}
        }
    }
    let loss = match &request {
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::ReserveManifest { .. },
                    ..
                },
        } => 1,
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::Publish { .. },
                    ..
                },
        } => 2,
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::CreateEmptyManifest { .. },
                    ..
                },
        } => 12,
        Request::Apply {
            request:
                Apply {
                    mutation: Mutation::CreateDeferredEmptyManifest { .. },
                    ..
                },
        } => 20,
        Request::Apply {
            request:
                Apply {
                    mutation:
                        Mutation::CreateEntry {
                            node_kind: Kind::Directory,
                            ..
                        },
                    ..
                },
        } => 16,
        _ => 0,
    };
    if loss != 0
        && p.mode
            .compare_exchange(loss, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if loss == 2 && p.mode.load(Ordering::SeqCst) == 15 {
        p.publish_gate.wait().await;
        p.publish_gate.wait().await;
    }
    if loss == 2 && p.mode.load(Ordering::SeqCst) == 10 {
        // Force both independent recoveries to read Prepared before either can
        // persist its identical receipt. No sleeping/concurrency assumption.
        p.publish_gate.wait().await;
    }
    match (&request, &mut reply, p.mode.load(Ordering::SeqCst)) {
        (Request::Store { .. }, Reply::Store(s), 3) => s.id = id(999),
        (Request::Store { .. }, Reply::Store(s), 4) => s.route = "f".repeat(64),
        (Request::Node { .. }, Reply::Node(n), 5) => n.id = id(999),
        (Request::Features {}, Reply::Features(fs), 6) => fs.retain(|f| f != "manifest-root-v1"),
        (Request::Features {}, Reply::Features(fs), 17) => {
            fs.retain(|f| f != "namespace-create-v1")
        }
        (Request::Features {}, Reply::Features(fs), 19) => {
            fs.retain(|f| f != "atomic-deferred-empty-manifest-v1")
        }
        (Request::Features {}, Reply::Features(fs), 21) => {
            fs.retain(|f| f != "manifest-history-v1")
        }
        (Request::Features {}, Reply::Features(fs), 24) => {
            fs.retain(|f| f != "event-fenced-manifest-v1")
        }
        (Request::Info {}, Reply::Info(v), 7) => {
            v.name_policy = "unsupported-fixture-policy".into()
        }
        (Request::Info {}, Reply::Info(v), 9) => v.id = id(999),
        _ => (),
    }
    axum::Json(reply).into_response()
}
struct Server {
    url: String,
    authority: Authority,
    wire: WireClient,
    direct: WireClient,
    proxy: Proxy,
    stops: Vec<oneshot::Sender<()>>,
    tasks: Vec<JoinHandle<()>>,
}
impl Server {
    async fn open(root: &Path, policy: &str) -> Self {
        let authority = Authority::open_with_policy(root.join("authority"), policy.into())
            .await
            .unwrap();
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/namespace", l.local_addr().unwrap());
        let direct = WireClient::loopback(&url, Token::new(SECRET.into()).unwrap()).unwrap();
        let (stop, receive) = oneshot::channel();
        let client = authority.client();
        let task = tokio::spawn(async move {
            s3drive_authority::serve_loopback(
                l,
                client,
                Token::new(SECRET.into()).unwrap(),
                async {
                    let _ = receive.await;
                },
            )
            .await
            .unwrap();
        });
        let p = Proxy {
            upstream: direct.clone(),
            mode: Arc::new(AtomicU8::new(0)),
            seen: Arc::new(Mutex::new(Vec::new())),
            publish_gate: Arc::new(tokio::sync::Barrier::new(2)),
        };
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/namespace", l.local_addr().unwrap());
        let wire = WireClient::loopback(&url, Token::new(SECRET.into()).unwrap()).unwrap();
        let router = axum::Router::new()
            .route("/v1/namespace", axum::routing::post(proxy))
            .with_state(p.clone());
        let (stop2, receive) = oneshot::channel();
        let task2 = tokio::spawn(async move {
            axum::serve(l, router)
                .with_graceful_shutdown(async {
                    let _ = receive.await;
                })
                .await
                .unwrap();
        });
        Self {
            url,
            authority,
            wire,
            direct,
            proxy: p,
            stops: vec![stop2, stop],
            tasks: vec![task2, task],
        }
    }
    async fn close(self) {
        for s in self.stops {
            let _ = s.send(());
        }
        for t in self.tasks {
            tokio::time::timeout(Duration::from_secs(10), t)
                .await
                .unwrap()
                .unwrap();
        }
        self.authority.shutdown().await.unwrap();
    }
}
async fn storage(root: &Path, name: &str) -> Storage {
    Storage::open(
        &root.join(name),
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap()
}
fn policy(config: &s3drive_core::Config, narrow: bool, expired: bool) -> WritePolicy {
    let c = &config.writes.as_ref().unwrap().sources[0];
    let mut text = std::fs::read_to_string(&c.policy).unwrap();
    if narrow {
        text = text.replace(
            "writable_prefix='writable/'",
            "writable_prefix='writable/.s3drive/other/'",
        );
    }
    if expired {
        text = text.replace(
            "evidence_max_age_seconds=86400",
            "evidence_max_age_seconds=1",
        );
    }
    WritePolicy::from_toml(
        &config.sources[0],
        &text,
        &std::fs::read_to_string(&c.plan).unwrap(),
        &std::fs::read_to_string(&c.report).unwrap(),
    )
    .unwrap()
}
async fn executor(
    config: &s3drive_core::Config,
    s: Storage,
    narrow: bool,
    expired: bool,
) -> Executor {
    Executor::open(
        s3drive_s3::S3Reader::from_source(&config.sources[0])
            .await
            .unwrap(),
        s,
        policy(config, narrow, expired),
    )
    .await
    .unwrap()
}
async fn publisher(e: &Executor, s: &Server) -> Publisher {
    Publisher::open(
        e.clone(),
        s.wire.clone(),
        info(&s.direct).await.id,
        id(200),
        chunked::Options::default(),
    )
    .await
    .unwrap()
}
async fn setup(config: &s3drive_core::Config, root: &Path, s: &Server) -> Executor {
    let e = executor(config, storage(root, "client").await, false, false).await;
    change(
        &s.direct,
        200,
        Mutation::RegisterStore {
            route: e.reader().storage_fingerprint(),
            root: "writable/.s3drive/".into(),
        },
    )
    .await;
    create(s, 10, "café 日本語 🦀.bin").await;
    e
}
async fn create(s: &Server, op: u64, name: &str) -> Node {
    let v = info(&s.direct).await;
    let parent = node(&s.direct, &v.id, s3drive_authority::ROOT).await;
    change(
        &s.direct,
        op,
        if v.name_policy == s3drive_core::names::PORTABLE {
            Mutation::CreateEntry {
                parent: parent.id,
                parent_revision: parent.revision,
                store: Some(id(200)),
                name: name.into(),
                node_kind: Kind::EmptyFile,
            }
        } else {
            Mutation::Create {
                parent: parent.id,
                name: name.into(),
                node_kind: Kind::EmptyFile,
            }
        },
    )
    .await
    .node
}
async fn plan(
    e: &Executor,
    s: &Server,
    n: u64,
    op: u64,
    bytes: &[u8],
    sparse_size: Option<u64>,
    bad: u8,
) -> ([u8; 32], GenerationId) {
    let v = info(&s.direct).await;
    let n = node(&s.direct, &v.id, &id(n)).await;
    let target = e.recovery_target("writable/.s3drive/binding").unwrap();
    let mut i = chunked::Input {
        id: [op as u8; 32],
        volume: binary(&v.id),
        node: binary(&n.id),
        store: binary(&id(200)),
        generation: binary(&id(op)),
        root: "writable/.s3drive/".into(),
        source: *target.source(),
        fingerprint: target.fingerprint().into(),
        parent: None,
        size: sparse_size.unwrap_or(bytes.len() as u64),
        expected_content: content(&n),
    };
    match bad {
        1 => i.source[0] ^= 1,
        2 => i.fingerprint = "f".repeat(64),
        3 => i.expected_content = Some([9; 32]),
        _ => (),
    }
    let j = e.storage().staging();
    i.parent = j
        .head(i.local_target().unwrap())
        .await
        .unwrap()
        .map(|g| g.id);
    let p = e.storage().chunked(chunked::Options::default()).unwrap();
    p.freeze(i.clone()).await.unwrap();
    let g = j
        .begin_operation(
            i.staging_operation().unwrap(),
            i.local_target().unwrap(),
            journal::Base::Missing,
            i.parent,
            i.size,
        )
        .await
        .unwrap();
    if let Some(size) = sparse_size {
        let zeros = size - bytes.len() as u64;
        j.append_zero_at(g, 0, zeros).await.unwrap();
        j.append_at(g, zeros, Arc::from(bytes)).await.unwrap();
    } else {
        for (idx, b) in bytes
            .chunks(s3drive_core::manifest::CHUNK_BYTES as usize)
            .enumerate()
        {
            let at = idx as u64 * s3drive_core::manifest::CHUNK_BYTES;
            if b.iter().all(|b| *b == 0) {
                j.append_zero_at(g, at, b.len() as u64).await.unwrap();
            } else {
                j.append_at(g, at, Arc::from(b)).await.unwrap();
            }
        }
    }
    j.seal(g).await.unwrap();
    p.attach(i.id, g).await.unwrap();
    p.prepare(i.id).await.unwrap();
    (i.id, g)
}
async fn pending(s: &Storage, g: GenerationId) {
    assert!(s.uploads().flush_receipt(g).await.unwrap().is_none());
    assert!(
        s.staging()
            .pending(None, 256)
            .await
            .unwrap()
            .iter()
            .any(|i| i.id == g)
    );
}
fn raw(root: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(root.join("client/metadata.sqlite3")).unwrap()
}
fn typed<T>(r: Result<T>, predicate: impl FnOnce(&Failure) -> bool) {
    let e = match r {
        Ok(_) => panic!("unexpected success"),
        Err(e) => e,
    };
    assert!(e.downcast_ref::<Failure>().is_some_and(predicate), "{e}");
}
fn only_exact_apply(s: &Server, expected: &Apply) {
    let rs = s.proxy.requests();
    assert_eq!(rs.len(), 1);
    let Request::Apply { request } = &rs[0] else {
        panic!("recovery queried current metadata")
    };
    assert_eq!(
        serde_json::to_vec(request).unwrap(),
        serde_json::to_vec(expected).unwrap()
    );
}
async fn reserve(s: &Server, storage: &Storage, r: &Record) {
    let Reply::Applied(a) = ask(
        &s.wire,
        Request::Apply {
            request: r.input().reserve().clone(),
        },
    )
    .await
    else {
        panic!()
    };
    storage
        .chunk_publications()
        .reserved(r.input().plan().id, a)
        .await
        .unwrap();
}
async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let p = publisher(&e, s).await;
    assert_ne!(p.store().route, e.reader().configuration_fingerprint());
    let mut bytes = (0..s3drive_core::manifest::CHUNK_BYTES)
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    bytes.resize(2 * s3drive_core::manifest::CHUNK_BYTES as usize, 0);
    bytes.extend_from_slice("tail\0 café 日本語 🦀".as_bytes());
    let (i, g) = plan(&e, s, 10, 40, &bytes, None, 0).await;
    s.proxy.clear();
    let r = p.begin(i, id(41)).await.unwrap();
    assert!(s.proxy.applies().is_empty());
    let recovery = Recovery::open(
        e.storage(),
        s.wire.clone(),
        info(&s.direct).await.id,
        id(200),
    )
    .await
    .unwrap();
    s.proxy.clear();
    typed(recovery.resume(i).await, |e| {
        matches!(e, Failure::NotPrepared)
    });
    assert!(s.proxy.requests().is_empty());
    s.proxy.mode.store(1, Ordering::SeqCst);
    assert!(p.resume(i).await.is_err());
    assert!(
        e.storage()
            .chunk_publications()
            .get(i)
            .await
            .unwrap()
            .reservation()
            .is_none()
    );
    let q = publisher(&e, s).await;
    s.proxy.clear();
    assert_eq!(q.begin(i, id(41)).await.unwrap(), r);
    assert!(s.proxy.requests().is_empty());
    checkpoint("LOSE_PUT");
    wait(root, "put-enabled").await;
    assert!(q.resume(i).await.is_err());
    let retained = e.storage().chunk_publications().get(i).await.unwrap();
    assert!(retained.reservation().is_some() && retained.publication().is_none());
    let transfer = e
        .storage()
        .chunked(chunked::Options::default())
        .unwrap()
        .transfer(i, -1, 0)
        .await
        .unwrap()
        .unwrap();
    let physical = transfer.generation().unwrap();
    let uncertain = e
        .storage()
        .uploads()
        .for_generation(physical)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        uncertain.phase,
        s3drive_state::remote::Phase::Uncertain | s3drive_state::remote::Phase::InFlight
    ));
    let q = publisher(&e, s).await;
    checkpoint("RESTORE_PUT");
    wait(root, "put-restored").await;
    let a = q.resume(i).await.unwrap();
    let verified = e
        .storage()
        .uploads()
        .for_generation(physical)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(verified.operation, uncertain.operation);
    assert_eq!(verified.phase, s3drive_state::remote::Phase::Verified);
    assert_eq!(a.plan(), i);
    assert_eq!(a.source_snapshot(), g);
    assert_ne!(a.root_generation(), g);
    pending(&e.storage(), g).await;
    let rs = s.proxy.applies();
    assert_eq!(rs.len(), 2);
    assert_eq!(rs[0], *r.input().reserve());
    let saved = e.storage().chunk_publications().get(i).await.unwrap();
    assert_eq!(rs[1], *saved.publication().unwrap());
    let (i2, g2) = plan(&e, s, 10, 50, b"second manifest generation\0", None, 0).await;
    let r2 = q.begin(i2, id(51)).await.unwrap();
    assert!(
        r2.input()
            .base()
            .payload
            .as_ref()
            .unwrap()
            .reservation
            .manifest
            .is_some()
    );
    q.resume(i2).await.unwrap();
    pending(&e.storage(), g).await;
    pending(&e.storage(), g2).await;
    create(s, 11, "empty 🦀.txt").await;
    let (empty, eg) = plan(&e, s, 11, 60, b"", None, 0).await;
    q.begin(empty, id(61)).await.unwrap();
    let a = q.resume(empty).await.unwrap();
    assert_eq!(
        a.metadata()
            .node
            .payload
            .as_ref()
            .unwrap()
            .reservation
            .logical_size()
            .unwrap(),
        0
    );
    pending(&e.storage(), eg).await;
    assert!(e.storage().namespace().client_identity().await.is_err());
    s.proxy.clear();
    s.proxy.mode.store(8, Ordering::SeqCst);
    let a = recovery.resume(i).await.unwrap();
    assert_eq!(a.metadata(), saved.receipt().unwrap());
    assert!(s.proxy.requests().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    e.storage().close().await.unwrap();
}
async fn races(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let a = setup(config, root, s).await;
    let b = executor(config, storage(root, "other").await, false, false).await;
    let pa = publisher(&a, s).await;
    let pb = publisher(&b, s).await;
    let (ia, ga) = plan(&a, s, 10, 40, b"rival A\0", None, 0).await;
    let (ib, gb) = plan(&b, s, 10, 50, b"rival B\xff", None, 0).await;
    let ar = pa.begin(ia, id(41)).await.unwrap();
    let br = pb.begin(ib, id(51)).await.unwrap();
    reserve(s, &a.storage(), &ar).await;
    reserve(s, &b.storage(), &br).await;
    let (ra, rb) = tokio::join!(pa.resume(ia), pb.resume(ib));
    assert_ne!(ra.is_ok(), rb.is_ok());
    let (loser, li) = if ra.is_ok() {
        typed(rb, |e| {
            matches!(e, Failure::Authority(s3drive_authority::Error::Conflict))
        });
        (&b, ib)
    } else {
        typed(ra, |e| {
            matches!(e, Failure::Authority(s3drive_authority::Error::Conflict))
        });
        (&a, ia)
    };
    let r = loser.storage().chunk_publications().get(li).await.unwrap();
    assert_eq!(
        r.rejection().unwrap().error(),
        s3drive_authority::Error::Conflict
    );
    assert_eq!(r.rejection().unwrap().request(), r.publication().unwrap());
    s.proxy.clear();
    let recovery = Recovery::open(
        loser.storage(),
        s.wire.clone(),
        info(&s.direct).await.id,
        id(200),
    )
    .await
    .unwrap();
    typed(recovery.resume(li).await, |e| {
        matches!(e, Failure::RetainedConflict)
    });
    typed(recovery.resume_source(li).await, |e| {
        matches!(e, Failure::RetainedConflict)
    });
    assert!(
        loser
            .storage()
            .chunk_publications()
            .sources()
            .source_status(li)
            .await
            .unwrap()
            .settlement()
            .is_none()
    );
    assert!(s.proxy.requests().is_empty());
    pending(&a.storage(), ga).await;
    pending(&b.storage(), gb).await;
    a.storage().close().await.unwrap();
    b.storage().close().await.unwrap();
}
async fn refusals(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    for mode in [3, 4, 6, 7, 9] {
        s.proxy.mode.store(mode, Ordering::SeqCst);
        assert!(
            Publisher::open(
                e.clone(),
                s.wire.clone(),
                info(&s.direct).await.id,
                id(200),
                chunked::Options::default()
            )
            .await
            .is_err()
        );
        assert!(
            e.storage()
                .chunk_publications()
                .records()
                .await
                .unwrap()
                .is_empty()
        );
    }
    s.proxy.mode.store(0, Ordering::SeqCst);
    let p = publisher(&e, s).await;
    for (op, bad) in [(40, 1), (50, 2), (60, 3)] {
        // Keep deliberately inconsistent bindings in separate owners so one
        // negative fixture cannot poison the next fixture's source registry.
        let bad_e = executor(
            config,
            storage(root, &format!("bad-{op}")).await,
            false,
            false,
        )
        .await;
        let bad_p = publisher(&bad_e, s).await;
        let (i, g) = plan(&bad_e, s, 10, op, b"wrong binding", None, bad).await;
        s.proxy.clear();
        assert!(bad_p.begin(i, id(op + 1)).await.is_err());
        assert!(s.proxy.applies().is_empty());
        assert!(
            bad_e
                .storage()
                .chunk_publications()
                .records()
                .await
                .unwrap()
                .is_empty()
        );
        pending(&bad_e.storage(), g).await;
        bad_e.storage().close().await.unwrap();
    }
    let (i, g) = plan(&e, s, 10, 70, b"valid binding", None, 0).await;
    s.proxy.mode.store(5, Ordering::SeqCst);
    s.proxy.clear();
    typed(p.begin(i, id(71)).await, |e| matches!(e, Failure::Binding));
    assert!(s.proxy.applies().is_empty());
    s.proxy.mode.store(0, Ordering::SeqCst);
    let db = raw(root);
    db.execute_batch("CREATE TRIGGER no_admit BEFORE INSERT ON chunk_publications BEGIN SELECT RAISE(IGNORE); END").unwrap();
    s.proxy.clear();
    assert!(p.begin(i, id(71)).await.is_err());
    assert!(s.proxy.applies().is_empty());
    assert!(
        e.storage()
            .chunk_publications()
            .records()
            .await
            .unwrap()
            .is_empty()
    );
    db.execute_batch("DROP TRIGGER no_admit").unwrap();
    drop(db);
    p.begin(i, id(71)).await.unwrap();
    let r = e.storage().chunk_publications().get(i).await.unwrap();
    s.proxy.clear();
    let wrong = Recovery::open(
        e.storage(),
        s.wire.clone(),
        info(&s.direct).await.id,
        id(999),
    )
    .await
    .unwrap();
    typed(wrong.resume(i).await, |e| matches!(e, Failure::Binding));
    assert!(s.proxy.requests().is_empty());
    let recovery = Recovery::open(
        e.storage(),
        s.wire.clone(),
        r.input().reserve().volume.clone(),
        id(200),
    )
    .await
    .unwrap();
    // Corrupt saved intent cannot be reinterpreted as Missing and readmitted.
    let db = raw(root);
    db.execute("UPDATE chunk_publications SET state_hash=zeroblob(32)", [])
        .unwrap();
    s.proxy.clear();
    assert!(p.begin(i, id(72)).await.is_err());
    assert!(p.resume_source(i).await.is_err());
    assert!(recovery.resume_source(i).await.is_err());
    assert!(s.proxy.requests().is_empty());
    let h = hash_record(&r);
    db.execute(
        "UPDATE chunk_publications SET state_hash=?1",
        [h.as_slice()],
    )
    .unwrap();
    drop(db);
    pending(&e.storage(), g).await;
    e.storage().close().await.unwrap();
}
fn hash_record(r: &Record) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"s3drive-chunk-publication-v1\0");
    h.update(serde_json::to_vec(r).unwrap());
    h.finalize().into()
}
async fn proof_fault(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let p = publisher(&e, s).await;
    let size = 128 * s3drive_core::manifest::CHUNK_BYTES + 64;
    let (i, g) = plan(&e, s, 10, 40, "sparse café 🦀".as_bytes(), Some(size), 0).await;
    let r = p.begin(i, id(41)).await.unwrap();
    reserve(s, &e.storage(), &r).await;
    Uploader::new(e.clone(), chunked::Options::default())
        .unwrap()
        .upload_tree(i)
        .await
        .unwrap();
    let planner = e.storage().chunked(chunked::Options::default()).unwrap();
    let child = planner
        .transfer(i, -1, 128)
        .await
        .unwrap()
        .unwrap()
        .generation()
        .unwrap();
    let db = raw(root);
    db.execute(
        "DELETE FROM write_settlements WHERE generation_id=?1",
        [child.value()],
    )
    .unwrap();
    db.execute(
        "DELETE FROM remote_flushes WHERE generation_id=?1",
        [child.value()],
    )
    .unwrap();
    db.execute_batch(&format!("CREATE TRIGGER no_ack BEFORE INSERT ON remote_flushes WHEN NEW.generation_id={} BEGIN SELECT RAISE(IGNORE); END",child.value())).unwrap();
    planner.require_dependencies(i, 1, 0).await.unwrap();
    assert!(planner.tree_receipt(i).await.is_err());
    s.proxy.clear();
    assert!(p.resume(i).await.is_err());
    assert!(s.proxy.applies().is_empty());
    assert!(
        e.storage()
            .chunk_publications()
            .get(i)
            .await
            .unwrap()
            .publication()
            .is_none()
    );
    db.execute_batch("DROP TRIGGER no_ack").unwrap();
    e.flush(child).await.unwrap();
    db.execute_batch("CREATE TRIGGER no_prepare BEFORE UPDATE ON chunk_publications BEGIN SELECT RAISE(IGNORE); END").unwrap();
    s.proxy.clear();
    assert!(p.resume(i).await.is_err());
    assert!(s.proxy.applies().is_empty());
    assert!(
        e.storage()
            .chunk_publications()
            .get(i)
            .await
            .unwrap()
            .publication()
            .is_none()
    );
    db.execute_batch("DROP TRIGGER no_prepare").unwrap();
    drop(db);
    p.resume(i).await.unwrap();
    pending(&e.storage(), g).await;
    e.storage().close().await.unwrap();
}
async fn crash(config: &s3drive_core::Config, root: &Path, s: &Server, source: bool) {
    let e = setup(config, root, s).await;
    let p = publisher(&e, s).await;
    let (i, g) = plan(&e, s, 10, 40, b"committed but reply lost\0", None, 0).await;
    p.begin(i, id(41)).await.unwrap();
    s.proxy.mode.store(2, Ordering::SeqCst);
    let result = if source {
        p.resume_source(i).await.map(|_| ())
    } else {
        p.resume(i).await.map(|_| ())
    };
    assert!(result.is_err());
    let r = e.storage().chunk_publications().get(i).await.unwrap();
    assert!(r.publication().is_some() && r.receipt().is_none());
    pending(&e.storage(), g).await;
    assert_eq!(
        e.storage().staging().read(g, 0, 64).await.unwrap(),
        b"committed but reply lost\0"
    );
    let staging = e.storage().staging();
    let dirty = staging
        .begin(
            r.input().plan().local_target().unwrap(),
            journal::Base::Missing,
            Some(g),
            3,
        )
        .await
        .unwrap();
    staging
        .append_at(dirty, 0, Arc::from(b"new".as_slice()))
        .await
        .unwrap();
    staging.seal(dirty).await.unwrap();
    let other = executor(config, storage(root, "other").await, false, false).await;
    let q = publisher(&other, s).await;
    let (new, _) = plan(&other, s, 10, 60, b"newer actual uploaded root\0", None, 0).await;
    q.begin(new, id(61)).await.unwrap();
    q.resume(new).await.unwrap();
    checkpoint("CRASH_ROOT");
    std::future::pending::<()>().await;
}
async fn recover(root: &Path, s: &Server) {
    // No Source/Executor/WritePolicy construction occurs in this process.
    assert!(!root.join("credentials").exists());
    assert!(!root.join("policy.toml").exists());
    let store = storage(root, "client").await;
    let records = store.chunk_publications().records().await.unwrap();
    assert_eq!(records.len(), 1);
    let r = &records[0];
    let i = r.input().plan().id;
    assert!(
        store
            .staging()
            .read(r.input().snapshot(), 0, 64)
            .await
            .is_err()
    );
    let recovery = Recovery::open(
        store.clone(),
        s.wire.clone(),
        r.input().reserve().volume.clone(),
        r.input().store().id.clone(),
    )
    .await
    .unwrap();
    s.proxy.clear();
    let commit = recovery.resume(i).await.unwrap();
    only_exact_apply(s, r.publication().unwrap());
    assert_eq!(
        commit
            .metadata()
            .node
            .payload
            .as_ref()
            .unwrap()
            .reservation
            .id,
        id(40)
    );
    let now = node(&s.direct, &r.input().reserve().volume, &id(10)).await;
    assert_eq!(now.payload.unwrap().reservation.id, id(60));
    assert!(commit.metadata().sequence < info(&s.direct).await.sequence);
    pending(&store, r.input().snapshot()).await;
    let head = store
        .staging()
        .head(r.input().plan().local_target().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_ne!(head.id, r.input().snapshot());
    assert_eq!(head.parent, Some(r.input().snapshot()));
    assert_eq!(head.size, 3);
    pending(&store, head.id).await;
    store.close().await.unwrap();
}
async fn policy_recovery(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    let p = publisher(&e, s).await;
    let (i, g) = plan(&e, s, 10, 40, b"policy recovery bytes\0", None, 0).await;
    p.begin(i, id(41)).await.unwrap();
    s.proxy.mode.store(2, Ordering::SeqCst);
    assert!(p.resume(i).await.is_err());
    let r = e.storage().chunk_publications().get(i).await.unwrap();
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
    assert!(p.resume(i).await.is_err());
    only_exact_apply(s, r.publication().unwrap());
    assert_eq!(store.chunk_publications().get(i).await.unwrap(), r);
    let narrow = executor(config, store.clone(), true, false).await;
    assert!(
        narrow
            .check_create(&reservation.key, reservation.size)
            .is_err()
    );
    let p = publisher(&narrow, s).await;
    s.proxy.clear();
    // Failure AFTER the authority has returned Applied must never be exposed
    // as a RootCommit before its local receipt is stored.
    let db = raw(root);
    db.execute_batch("CREATE TRIGGER no_receipt BEFORE UPDATE ON chunk_publications BEGIN SELECT RAISE(IGNORE); END").unwrap();
    assert!(p.resume(i).await.is_err());
    only_exact_apply(s, r.publication().unwrap());
    assert_eq!(store.chunk_publications().get(i).await.unwrap(), r);
    db.execute_batch("DROP TRIGGER no_receipt").unwrap();
    drop(db);
    let independent = Recovery::open(
        store.clone(),
        s.wire.clone(),
        info(&s.direct).await.id,
        id(200),
    )
    .await
    .unwrap();
    s.proxy.clear();
    s.proxy.mode.store(10, Ordering::SeqCst);
    let (left, right) = tokio::join!(p.resume(i), independent.resume(i));
    let result = left.unwrap();
    let other = right.unwrap();
    assert_eq!(result.metadata(), other.metadata());
    let requests = s.proxy.requests();
    assert_eq!(requests.len(), 2);
    for request in requests {
        let Request::Apply { request } = request else {
            panic!("recovery queried fresh metadata")
        };
        assert_eq!(
            serde_json::to_vec(&request).unwrap(),
            serde_json::to_vec(r.publication().unwrap()).unwrap()
        );
    }
    s.proxy.mode.store(0, Ordering::SeqCst);
    assert_eq!(result.source_snapshot(), g);
    pending(&store, g).await;
    store.close().await.unwrap();
}
async fn wait(root: &Path, name: &str) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while !root.join(name).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_child() {
    let Ok(mode) = std::env::var("S3DRIVE_CHUNK_CASE") else {
        return;
    };
    let root = PathBuf::from(std::env::var("S3DRIVE_CHUNK_ROOT").unwrap());
    let policy = if mode == "exact" {
        s3drive_core::names::EXACT
    } else {
        s3drive_core::names::PORTABLE
    };
    let s = Server::open(&root, policy).await;
    if matches!(
        mode.as_str(),
        "cold_namespace_recover"
            | "owned_sparse_recover"
            | "recover"
            | "source_recover_unknown"
            | "source_recover_known"
            | "empty_recover"
            | "deferred_client_recover"
            | "creator_import_recover"
            | "creator_open_recover"
    ) {
        if mode == "cold_namespace_recover" {
            Box::pin(namespace_recovery::recover(&root, &s)).await;
        } else if mode == "owned_sparse_recover" {
            Box::pin(owned_sparse::recover(&root, &s)).await;
        } else if mode == "creator_open_recover" {
            Box::pin(creator_open::recover(&root, &s)).await;
        } else if mode == "creator_import_recover" {
            Box::pin(creator_import::recover(&root, &s)).await;
        } else if mode == "deferred_client_recover" {
            Box::pin(deferred_creation::killed_recovery(&root, &s)).await;
        } else if mode == "empty_recover" {
            Box::pin(empty_creation::killed_recovery(&root, &s)).await;
        } else if mode == "recover" {
            Box::pin(recover(&root, &s)).await;
        } else {
            Box::pin(source_settlement::recover(
                &root,
                &s,
                mode == "source_recover_known",
            ))
            .await;
        }
    } else {
        let config = s3drive_core::Config::parse(
            &std::fs::read_to_string(std::env::var("S3DRIVE_CHUNK_CONFIG").unwrap()).unwrap(),
        )
        .unwrap();
        dispatch(&mode, &config, &root, &s).await;
    }
    s.close().await;
}
// Build immediately and exactly once, but outside the dispatcher's stack frame.
// Passing a constructed future to a generic boxer would retain the large caller
// temporary. This factory receives only its small borrowed-argument closure.
#[inline(never)]
fn boxed_case<Build, F>(build: Build) -> std::pin::Pin<Box<F>>
where
    Build: FnOnce() -> F,
    F: std::future::Future<Output = ()>,
{
    Box::pin(build())
}
#[test]
fn chunk_dispatch_factory_constructs_once_now_and_polls_later() {
    let constructed = std::cell::Cell::new(0);
    let polled = std::cell::Cell::new(0);
    let borrowed = String::from("borrowed non-static fixture input");
    let future = boxed_case(|| {
        constructed.set(constructed.get() + 1);
        async {
            polled.set(polled.get() + 1);
            assert_eq!(borrowed, "borrowed non-static fixture input");
        }
    });
    assert_eq!(constructed.get(), 1);
    assert_eq!(polled.get(), 0);
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future);
    assert_eq!(constructed.get(), 1);
    assert_eq!(polled.get(), 1);
}
// Construct one heap-owned case before polling it. Keeping every case's
// concrete future in the shared async dispatcher inflated unrelated cases'
// debug-build stack frames under workspace feature unification.
fn dispatch<'a>(
    mode: &str,
    config: &'a s3drive_core::Config,
    root: &'a Path,
    s: &'a Server,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
    match mode {
        "cold_namespace_crash" => {
            boxed_case(|| namespace_recovery::seed(config, root, s, "cold_namespace_crash"))
        }
        "cold_namespace_known" => {
            boxed_case(|| namespace_recovery::seed(config, root, s, "cold_namespace_known"))
        }
        "cold_namespace_rejected" => {
            boxed_case(|| namespace_recovery::seed(config, root, s, "cold_namespace_rejected"))
        }
        "native_sparse_sdk_controller_interleaving" => {
            boxed_case(|| native_sparse_sdk::flow(config, root, s, 6))
        }
        "native_sparse_sdk_controller_linked" => {
            boxed_case(|| native_sparse_sdk::flow(config, root, s, 3))
        }
        "native_sparse_sdk_controller_unlinked" => {
            boxed_case(|| native_sparse_sdk::flow(config, root, s, 5))
        }
        "native_sparse_sdk_linked" => boxed_case(|| native_sparse_sdk::flow(config, root, s, 0)),
        "native_sparse_sdk_pending" => boxed_case(|| native_sparse_sdk::flow(config, root, s, 1)),
        "native_sparse_sdk_committed" => boxed_case(|| native_sparse_sdk::flow(config, root, s, 2)),
        "owned_sparse_pending" => boxed_case(|| owned_sparse::flow(config, root, s, false)),
        "owned_sparse_committed" => boxed_case(|| owned_sparse::flow(config, root, s, true)),
        "owned_sparse_physical" => boxed_case(|| owned_sparse::physical(config, root, s)),
        "owned_sparse_controls" => boxed_case(|| owned_sparse::controls(config, root, s)),
        "owned_sparse_rejected" => boxed_case(|| owned_sparse::rejected(config, root, s)),
        "owned_sparse_crash" => boxed_case(|| owned_sparse::crash(config, root, s)),
        #[cfg(any(target_os = "linux", windows))]
        "native_writer_mount" => boxed_case(|| native_writer::mount(config, root, s)),
        "native_lifecycle_disposition" => {
            boxed_case(|| native_lifecycle::disposition(config, root, s, false))
        }
        "native_lifecycle_disposition_concurrent" => {
            boxed_case(|| native_lifecycle::disposition(config, root, s, true))
        }
        "native_lifecycle_history" => {
            boxed_case(|| native_lifecycle::acknowledged_history(config, root, s))
        }
        "native_lifecycle_ordered" => {
            boxed_case(|| native_lifecycle::ordered_flush(config, root, s))
        }
        "native_lifecycle_profile" => {
            boxed_case(|| native_lifecycle::profile_transition(config, root, s))
        }
        "native_lifecycle_unlink" => boxed_case(|| native_lifecycle::unlink(config, root, s)),
        #[cfg(any(target_os = "linux", windows))]
        "native_retention_seed" => boxed_case(|| native_retention::seed(config, root, s)),
        "native_retention_shared" => boxed_case(|| native_retention::shared(root, s)),
        "native_retention_profile_warmup" => {
            boxed_case(|| native_retention::profile_warmup(root, s))
        }
        "native_retention_profile_saves" => boxed_case(|| native_retention::profile_saves(root, s)),
        "native_retention_final_profile" => boxed_case(|| native_retention::final_profile(root, s)),
        "native_retention_final_shared" => boxed_case(|| native_retention::final_shared(root, s)),
        "native_retention_final_diagnostic" => {
            boxed_case(|| native_retention::final_diagnostic(root, s))
        }
        #[cfg(any(target_os = "linux", windows))]
        "native_retention_final_mounted" => boxed_case(|| native_retention::final_mounted(root, s)),
        #[cfg(any(target_os = "linux", windows))]
        "native_retention_mounted" => boxed_case(|| native_retention::mounted(root, s)),
        "native_lifecycle_mounted" => boxed_case(|| native_lifecycle::mounted(config, root, s)),
        #[cfg(any(target_os = "linux", windows))]
        "native_lifecycle_mounted_known" => {
            boxed_case(|| native_lifecycle::mounted_crash(config, root, s, true))
        }
        #[cfg(any(target_os = "linux", windows))]
        "native_lifecycle_mounted_unknown" => {
            boxed_case(|| native_lifecycle::mounted_crash(config, root, s, false))
        }
        "native_lifecycle_replace" => boxed_case(|| native_lifecycle::replace(config, root, s)),
        "native_lifecycle_cold_known" => {
            boxed_case(|| native_lifecycle::cold_known(config, root, s))
        }
        "native_lifecycle_cold_unknown" => {
            boxed_case(|| native_lifecycle::cold_unknown(config, root, s))
        }
        "native_writer" => boxed_case(|| native_writer::flow(config, root, s)),
        "native_directories" => boxed_case(|| native_directories::flow(config, root, s)),
        "native_directories_only" => boxed_case(|| native_directories::only(config, root, s)),
        "native_directory_concurrency" => {
            boxed_case(|| native_directories::concurrency(config, root, s))
        }
        "native_small_stack" => boxed_case(|| native_writer::small_stack(config, root, s)),
        "native_policy" => boxed_case(|| native_writer::policy(config, root, s)),
        "native_concurrency" => boxed_case(|| native_writer::concurrency(config, root, s)),
        "composition" => boxed_case(|| composition::flow(config, root, s)),
        "creator_open_flow" => boxed_case(|| creator_open::flow(config, root, s)),
        "creator_open_controls" => boxed_case(|| creator_open::controls(config, root, s)),
        "creator_open_quotas" => boxed_case(|| creator_open::quotas(config, root, s)),
        "creator_open_recovery" => boxed_case(|| creator_open::recovery(config, root, s)),
        "creator_open_physical" => boxed_case(|| creator_open::physical(config, root, s)),
        "creator_open_unclaimed" => boxed_case(|| creator_open::unclaimed(config, root, s)),
        "creator_open_deadline" => boxed_case(|| creator_open::deadline(config, root, s)),
        "creator_open_crash" => boxed_case(|| creator_open::crash(config, root, s)),
        "creator_open_stores" => boxed_case(|| creator_open::stores(config, root, s)),
        "creator_open_rejected" => boxed_case(|| creator_open::rejected(config, root, s)),
        "creator_import_flow" => boxed_case(|| creator_import::flow(config, root, s)),
        "creator_import_controls" => boxed_case(|| creator_import::controls(config, root, s)),
        "creator_import_crash" => boxed_case(|| creator_import::crash(config, root, s)),
        "deferred_client_flow" => boxed_case(|| deferred_creation::flow(config, root, s)),
        "deferred_client_controls" => boxed_case(|| deferred_creation::controls(config, root, s)),
        "deferred_client_recovery" => boxed_case(|| deferred_creation::recovery(config, root, s)),
        "deferred_client_physical" => boxed_case(|| deferred_creation::physical(config, root, s)),
        "deferred_client_crash" => boxed_case(|| deferred_creation::crash(config, root, s)),
        "deferred_client_deadline" => boxed_case(|| deferred_creation::deadline(config, root, s)),
        "empty_deadline" => boxed_case(|| empty_creation::deadline(config, root, s)),
        "empty_physical" => boxed_case(|| empty_creation::physical(config, root, s)),
        "empty_crash" => boxed_case(|| empty_creation::crash(config, root, s)),
        "empty_flow" => boxed_case(|| empty_creation::flow(config, root, s)),
        "empty_controls" => boxed_case(|| empty_creation::controls(config, root, s)),
        "empty_recovery" => boxed_case(|| empty_creation::recovery(config, root, s)),
        "local_retention" => boxed_case(|| local_retention::flow(config, root, s)),
        "mixed_scoped" => boxed_case(|| mixed_epochs::run_scoped_case(config, root, s)),
        "mixed_epochs" => boxed_case(|| mixed_epochs::run_case(config, root, s)),
        "mixed_lineage" => boxed_case(|| mixed_roots::lineage(config, root, s)),
        "mixed_roots_controls" => boxed_case(|| mixed_roots::controls(config, root, s)),
        "mixed_roots_rivals" => boxed_case(|| mixed_roots::rivals(config, root, s)),
        "mixed_roots_crash" => boxed_case(|| mixed_roots::crash(config, root, s)),
        "mixed_conflict" => boxed_case(|| mixed_uploads::conflict(config, root, s)),
        "mixed_concurrency" => boxed_case(|| mixed_uploads::concurrency(config, root, s)),
        "mixed_controls" => boxed_case(|| mixed_uploads::controls(config, root, s)),
        "mixed_faults" => boxed_case(|| mixed_uploads::faults(config, root, s)),
        "mixed_uploads" => boxed_case(|| mixed_uploads::flow(config, root, s)),
        "flow" => boxed_case(|| flow(config, root, s)),
        "exact" => boxed_case(|| exact_names(config, root, s)),
        "races" => boxed_case(|| races(config, root, s)),
        "refusals" => boxed_case(|| refusals(config, root, s)),
        "proof_fault" => boxed_case(|| proof_fault(config, root, s)),
        "policy" => boxed_case(|| policy_recovery(config, root, s)),
        "source_flow" => boxed_case(|| source_settlement::flow(config, root, s)),
        "source_ack_crash" => boxed_case(|| source_settlement::ack_crash(config, root, s)),
        "source_policy" => boxed_case(|| source_settlement::policy_recovery(config, root, s)),
        "mixed_roots_recover" => boxed_case(|| mixed_roots::recover(root, s)),
        "crash" => boxed_case(|| crash(config, root, s, false)),
        "source_crash" => boxed_case(|| crash(config, root, s, true)),
        _ => panic!("unknown owned fixture case"),
    }
}
async fn run(mode: &str) {
    let put_gate = fixture::PutGate::default();
    let f = fixture::Fixture::start_with_gate(Some(put_gate.clone())).await;
    let tmp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&f, tmp.path()).await;
    let mut counts = None;
    let mut mixed_recovery_window = None;
    let mut mixed_corrupt_tail = None;
    let mut mixed_batch_window = None;
    let mut creator_open_objects = None;
    let mut retention_completed = 0_u64;
    for stage in match mode {
        "native_retention_cpu_profile" => vec![
            "native_retention_seed",
            "native_retention_profile_warmup",
            "native_retention_profile_saves",
            "native_retention_final_profile",
        ],
        "native_retention_validation_work" => vec![
            "native_retention_seed",
            "native_retention_shared",
            "native_retention_shared",
            "native_retention_final_diagnostic",
        ],
        "native_retention_shared_cycles" | "native_retention_mounted_cycles" => {
            let mut stages = vec!["native_retention_seed"];
            stages.extend(std::iter::repeat_n(
                if mode == "native_retention_shared_cycles" {
                    "native_retention_shared"
                } else {
                    "native_retention_mounted"
                },
                30,
            ));
            stages.push(if mode == "native_retention_shared_cycles" {
                "native_retention_final_shared"
            } else {
                "native_retention_final_mounted"
            });
            stages
        }
        "cold_namespace_crash" | "cold_namespace_known" | "cold_namespace_rejected" => {
            vec![mode, "cold_namespace_recover"]
        }
        "owned_sparse_crash" => vec!["owned_sparse_crash", "owned_sparse_recover"],
        "mixed_roots_crash" => vec!["mixed_roots_crash", "mixed_roots_recover"],
        "crash" => vec!["crash", "recover"],
        "empty_crash" => vec!["empty_crash", "empty_recover"],
        "creator_import_crash" => vec!["creator_import_crash", "creator_import_recover"],
        "creator_open_crash" => vec!["creator_open_crash", "creator_open_recover"],
        "deferred_client_crash" => vec!["deferred_client_crash", "deferred_client_recover"],
        "source_crash" => vec!["source_crash", "source_recover_unknown"],
        "source_ack_crash" => vec!["source_ack_crash", "source_recover_known"],
        _ => vec![mode],
    } {
        // Test-only external profiling of the actual FS child, never the
        // authority/loopback fixture or cargo process. No production hook.
        let profiling = stage == "native_retention_profile_saves";
        let tool = if profiling {
            std::env::var("S3DRIVE_NATIVE_PROFILE_TOOL").unwrap_or_else(|_| "none".into())
        } else {
            "none".into()
        };
        let executable = std::env::current_exe().unwrap();
        let mut command = match tool.as_str() {
            "none" => tokio::process::Command::new(&executable),
            "perf" | "callgrind" => {
                let output =
                    std::env::var("S3DRIVE_NATIVE_PROFILE_OUTPUT").expect("profile output path");
                let mut command = if tool == "perf" {
                    let mut c = tokio::process::Command::new("timeout");
                    c.args(["--kill-after=5s", "890s", "perf"]);
                    c.args([
                        "record",
                        "--call-graph",
                        "dwarf,16384",
                        "-F",
                        "99",
                        "--output",
                        &output,
                        "--",
                    ]);
                    c
                } else {
                    let mut c = tokio::process::Command::new("timeout");
                    c.args(["--kill-after=5s", "890s", "valgrind"]);
                    c.args([
                        "--tool=callgrind",
                        "--trace-children=no",
                        "--cache-sim=no",
                        "--branch-sim=no",
                    ])
                    .arg(format!("--callgrind-out-file={output}"));
                    c
                };
                command.arg(&executable);
                command
            }
            _ => panic!("unsupported diagnostic profiler"),
        };
        command
            .args(["--exact", "chunk_child", "--nocapture"])
            .env("S3DRIVE_CHUNK_CASE", stage)
            .env("S3DRIVE_CHUNK_ROOT", tmp.path())
            .env("S3DRIVE_CHUNK_CONFIG", &setup.config)
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .env("AWS_SHARED_CREDENTIALS_FILE", &setup.credentials)
            .env("AWS_CONFIG_FILE", &setup.aws_config)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut stderr = child.stderr.take().unwrap();
        let errors = tokio::spawn(async move {
            let mut out = String::new();
            stderr.read_to_string(&mut out).await.unwrap();
            out
        });
        // The separate profiler diagnostic has its own overhead budget;
        // ordinary and acceptance stages retain the original 180-second cap.
        let child_seconds = if profiling && tool != "none" {
            900
        } else {
            180
        };
        let result = tokio::time::timeout(Duration::from_secs(child_seconds), async {
            while let Some(line) = lines.next_line().await.unwrap() {
                match line.as_str() {
                    "LOSE_PUT" => {
                        f.hide_committed.store(true, Ordering::SeqCst);
                        std::fs::write(tmp.path().join("put-enabled"), b"ack").unwrap();
                    }
                    "RESTORE_PUT" => {
                        f.deny_head.store(false, Ordering::SeqCst);
                        f.deny_get.store(false, Ordering::SeqCst);
                        std::fs::write(tmp.path().join("put-restored"), b"ack").unwrap();
                    }
                    "MIXED_RIVAL_CREATE" => {
                        f.race_create.store(true, Ordering::SeqCst);
                        std::fs::write(tmp.path().join("mixed-rival-create.ack"), b"ack").unwrap();
                    }
                    "CREATOR_OPEN_GATE_ARM" => {
                        assert_eq!(put_gate.arrived.load(Ordering::SeqCst),0);
                        creator_open_objects=Some(f.objects.lock().unwrap().len());
                        put_gate.before_commit.store(true,Ordering::SeqCst);
                        put_gate.enabled.store(true,Ordering::SeqCst);
                        std::fs::write(tmp.path().join("creator-open-gate-armed"),b"ack").unwrap();
                    }
                    "CREATOR_OPEN_GATE_INSPECT" => {
                        tokio::time::timeout(Duration::from_secs(10),async {
                            while put_gate.arrived.load(Ordering::SeqCst)!=1 {tokio::time::sleep(Duration::from_millis(10)).await;}
                        }).await.unwrap();
                        assert_eq!(f.objects.lock().unwrap().len(),creator_open_objects.unwrap());
                        let db=raw(tmp.path());
                        let bytes:Vec<u8>=db.query_row("SELECT record FROM manifest_creations WHERE operation=?",[id(40)],|r|r.get(0)).unwrap();
                        let creator:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
                        assert_eq!(creator["format"],3);assert_eq!(creator["input"]["open"]["id"],id(71));
                        let (owner,bytes,count):(String,Vec<u8>,i64)=db.query_row("SELECT owner_id,input,event_count FROM windows_open_journal WHERE open_id=?",[id(71)],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
                        let open:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
                        assert_eq!(creator["input"]["owner"],owner);assert_eq!(open["creation"],id(40));
                        assert_eq!(open["session"],id(80));assert_eq!(open["controlled"],true);assert_eq!(open["session_bound"],true);assert_eq!(count,0);
                        assert_eq!(db.query_row("SELECT count(*) FROM retained_windows_opens",[],|r|r.get::<_,i64>(0)).unwrap(),1);
                        assert_eq!(db.query_row("SELECT count(*) FROM namespace_intents",[],|r|r.get::<_,i64>(0)).unwrap(),0);
                        drop(db);put_gate.enabled.store(false,Ordering::SeqCst);put_gate.release.add_permits(1);
                        std::fs::write(tmp.path().join("creator-open-gate-checked"),b"ack").unwrap();
                    }
                    "MIXED_GATE_ARM" => {
                        assert_eq!(put_gate.arrived.load(Ordering::SeqCst), 0);
                        put_gate.enabled.store(true, Ordering::SeqCst);
                        std::fs::write(tmp.path().join("mixed-gate-arm.ack"), b"ack").unwrap();
                    }
                    "MIXED_GATE_FOUR" => {
                        tokio::time::timeout(Duration::from_secs(10), async {
                            while put_gate.arrived.load(Ordering::SeqCst) < 4 {
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        })
                        .await
                        .unwrap();
                        assert_eq!(put_gate.arrived.load(Ordering::SeqCst), 4);
                        std::fs::write(tmp.path().join("mixed-gate-four.ack"), b"ack").unwrap();
                    }
                    "MIXED_GATE_RELEASE" => {
                        put_gate.enabled.store(false, Ordering::SeqCst);
                        put_gate.release.add_permits(4);
                        std::fs::write(tmp.path().join("mixed-gate-release.ack"), b"ack").unwrap();
                    }
                    "MIXED_CORRUPT_TAIL" => {
                        let mut objects = f.objects.lock().unwrap();
                        let keys = objects
                            .keys()
                            .filter(|k| k.contains("/writable/.s3drive/") && k.contains("/data/"))
                            .cloned()
                            .collect::<Vec<_>>();
                        assert_eq!(keys.len(), 1);
                        let object = objects.get_mut(&keys[0]).unwrap();
                        assert_eq!(object.0.len(), 17);
                        mixed_corrupt_tail = Some((keys[0].clone(), object.0.clone()));
                        object.0[16] ^= 1;
                        drop(objects);
                        std::fs::write(tmp.path().join("mixed-corrupt-tail.ack"), b"ack").unwrap();
                    }
                    "MIXED_RESTORE_TAIL" => {
                        let (key, bytes) = mixed_corrupt_tail.take().unwrap();
                        f.objects.lock().unwrap().get_mut(&key).unwrap().0 = bytes;
                        std::fs::write(tmp.path().join("mixed-restore-tail.ack"), b"ack").unwrap();
                    }
                    "MIXED_BATCH_BEGIN" => {
                        assert!(mixed_batch_window.is_none());
                        mixed_batch_window = Some(f.requests.lock().unwrap().len());
                        std::fs::write(tmp.path().join("mixed-batch-begin.ack"), b"ack").unwrap();
                    }
                    "MIXED_BATCH_END" => {
                        let begin = mixed_batch_window.take().unwrap();
                        let key =
                            std::fs::read_to_string(tmp.path().join("batch-base-key.txt")).unwrap();
                        let requests = f.requests.lock().unwrap();
                        let observed = &requests[begin..];
                        let base_heads = observed
                            .iter()
                            .filter(|r| {
                                let mut words = r.split_whitespace();
                                if words.next() != Some("HEAD") {
                                    return false;
                                }
                                let url = url::Url::parse(&format!(
                                    "http://fixture{}",
                                    words.next().unwrap()
                                ))
                                .unwrap();
                                let path = percent_encoding::percent_decode_str(url.path())
                                    .decode_utf8()
                                    .unwrap();
                                path.ends_with(&format!("/{key}"))
                            })
                            .count();
                        assert_eq!(
                            base_heads, 6,
                            "page fanout repeated root HEAD authorization"
                        );
                        assert_eq!(observed.iter().filter(|r| r.starts_with("PUT ")).count(), 3);
                        drop(requests);
                        std::fs::write(tmp.path().join("mixed-batch-end.ack"), b"ack").unwrap();
                    }
                    "MIXED_RECOVERY_BEGIN" => {
                        mixed_recovery_window = Some(f.requests.lock().unwrap().len());
                        std::fs::write(tmp.path().join("mixed-recovery-begin.ack"), b"ack")
                            .unwrap();
                    }
                    "MIXED_RECOVERY_END" => {
                        let begin = mixed_recovery_window.take().unwrap();
                        let expected =
                            std::fs::read_to_string(tmp.path().join("mixed-recovery-target.txt"))
                                .unwrap();
                        let requests = f.requests.lock().unwrap();
                        let observed = &requests[begin..];
                        assert!(
                            observed.iter().any(|r| r.starts_with("GET ")),
                            "recovery did not verify the observed new object body"
                        );
                        for request in observed {
                            let mut words = request.split_whitespace();
                            let method = words.next().unwrap();
                            assert!(
                                matches!(method, "HEAD" | "GET"),
                                "recovery issued a mutation"
                            );
                            let url = url::Url::parse(&format!(
                                "http://fixture{}",
                                words.next().unwrap()
                            ))
                            .unwrap();
                            let path = percent_encoding::percent_decode_str(url.path())
                                .decode_utf8()
                                .unwrap();
                            assert!(
                                path.ends_with(&format!("/{expected}")),
                                "recovery read original or unrelated objects"
                            );
                        }
                        drop(requests);
                        std::fs::write(tmp.path().join("mixed-recovery-end.ack"), b"ack").unwrap();
                    }
                    "NO_S3_FROM_HERE" => {
                        counts = Some(f.requests.lock().unwrap().len());
                        std::fs::write(tmp.path().join("no-s3-armed"), b"ack").unwrap();
                    }
                    "CRASH_ROOT" => return true,
                    _ => (),
                }
            }
            false
        })
        .await;
        let killed = result.as_ref().copied().unwrap_or(false);
        if killed || result.is_err() {
            child.kill().await.unwrap();
        }
        let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
            .await
            .unwrap()
            .unwrap();
        let err = errors.await.unwrap();
        if stage.starts_with("native_lifecycle_mounted") || stage.starts_with("native_retention_") {
            eprintln!("{err}");
        }
        assert!(
            result.is_ok()
                && if matches!(
                    stage,
                    "crash"
                        | "cold_namespace_crash"
                        | "cold_namespace_known"
                        | "cold_namespace_rejected"
                        | "source_crash"
                        | "source_ack_crash"
                        | "owned_sparse_crash"
                        | "mixed_roots_crash"
                        | "empty_crash"
                        | "deferred_client_crash"
                        | "creator_import_crash"
                        | "creator_open_crash"
                ) {
                    killed && !status.success()
                } else {
                    status.success() && !killed
                },
            "case {stage}: {status}: {err}"
        );
        if stage.starts_with("native_retention_") {
            if matches!(
                stage,
                "native_retention_shared" | "native_retention_mounted"
            ) {
                retention_completed += 10;
            } else if stage == "native_retention_profile_warmup" {
                retention_completed += 1;
            } else if stage == "native_retention_profile_saves" {
                retention_completed += 2;
            }
            let completed = std::fs::read_to_string(tmp.path().join("retention-cycle"))
                .unwrap()
                .parse::<u64>()
                .unwrap();
            assert_eq!(completed, retention_completed, "exact completed save count");
            assert_eq!(
                puts(&f),
                native_retention::expected_puts(retention_completed),
                "exact sparse PUT schedule after {stage}, {completed} saves"
            );
        }
        if matches!(
            stage,
            "empty_crash" | "deferred_client_crash" | "creator_import_crash" | "creator_open_crash"
        ) {
            assert_eq!(puts(&f), 1);
            let db = raw(tmp.path());
            db.execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
                .unwrap();
            drop(db);
            std::fs::rename(&setup.credentials, tmp.path().join("credentials.disabled")).unwrap();
            std::fs::rename(
                tmp.path().join("policy.toml"),
                tmp.path().join("policy.disabled"),
            )
            .unwrap();
            counts = Some(f.requests.lock().unwrap().len());
        }
        if matches!(
            stage,
            "owned_sparse_crash"
                | "cold_namespace_crash"
                | "cold_namespace_known"
                | "cold_namespace_rejected"
        ) {
            assert_eq!(puts(&f), if stage == "owned_sparse_crash" { 3 } else { 1 });
            let db = raw(tmp.path());
            db.execute(
                "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
                [],
            )
            .unwrap();
            db.execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
                .unwrap();
            drop(db);
            std::fs::rename(&setup.credentials, tmp.path().join("credentials.disabled")).unwrap();
            std::fs::rename(
                tmp.path().join("policy.toml"),
                tmp.path().join("policy.disabled"),
            )
            .unwrap();
            counts = Some(f.requests.lock().unwrap().len());
        }
        if matches!(stage, "owned_sparse_recover" | "cold_namespace_recover") {
            assert_eq!(
                f.requests.lock().unwrap().len(),
                counts.unwrap(),
                "owned metadata recovery accessed S3"
            );
        }
        if stage == "mixed_roots_crash" {
            assert_eq!(puts(&f), 7);
            let db =
                rusqlite::Connection::open(tmp.path().join("mixed-root/metadata.sqlite3")).unwrap();
            db.execute(
                "UPDATE sparse_view_blocks SET data=zeroblob(length(data))",
                [],
            )
            .unwrap();
            drop(db);
            std::fs::rename(&setup.credentials, tmp.path().join("credentials.disabled")).unwrap();
            std::fs::rename(
                tmp.path().join("policy.toml"),
                tmp.path().join("policy.disabled"),
            )
            .unwrap();
            counts = Some(f.requests.lock().unwrap().len());
        }
        if stage == "mixed_roots_recover" {
            assert_eq!(
                f.requests.lock().unwrap().len(),
                counts.unwrap(),
                "root recovery accessed S3"
            );
            assert_eq!(puts(&f), 7);
        }
        if matches!(stage, "crash" | "source_crash" | "source_ack_crash") {
            assert_eq!(puts(&f), if stage == "source_ack_crash" { 2 } else { 4 });
            let db = raw(tmp.path());
            if stage == "source_ack_crash" {
                db.execute_batch("DROP TRIGGER no_source_ack").unwrap();
            }
            db.execute("UPDATE staged_chunks SET data=zeroblob(length(data))", [])
                .unwrap();
            drop(db);
            std::fs::rename(&setup.credentials, tmp.path().join("credentials.disabled")).unwrap();
            std::fs::rename(
                tmp.path().join("policy.toml"),
                tmp.path().join("policy.disabled"),
            )
            .unwrap();
            counts = Some(f.requests.lock().unwrap().len());
        }
        if matches!(
            stage,
            "recover"
                | "policy"
                | "source_recover_unknown"
                | "source_recover_known"
                | "source_policy"
                | "source_flow"
                | "empty_recovery"
                | "deferred_client_recovery"
                | "empty_recover"
                | "deferred_client_recover"
                | "creator_import_recover"
                | "creator_import_flow"
                | "creator_import_controls"
                | "creator_open_flow"
                | "creator_open_recovery"
                | "creator_open_recover"
        ) {
            assert_eq!(
                f.requests.lock().unwrap().len(),
                counts.unwrap(),
                "metadata recovery accessed S3"
            );
        }
    }
    let expected = match mode {
        "native_retention_cpu_profile" => {
            assert_eq!(retention_completed, 3);
            native_retention::expected_puts(3)
        }
        "native_retention_validation_work" => {
            assert_eq!(retention_completed, 20);
            native_retention::expected_puts(20)
        }
        "native_retention_shared_cycles" | "native_retention_mounted_cycles" => {
            assert_eq!(retention_completed, 300);
            native_retention::expected_puts(300)
        }
        "cold_namespace_crash" | "cold_namespace_known" | "cold_namespace_rejected" => 1,
        "native_sparse_sdk_controller_interleaving"
        | "native_sparse_sdk_controller_linked"
        | "native_sparse_sdk_controller_unlinked"
        | "native_sparse_sdk_linked"
        | "native_sparse_sdk_pending"
        | "native_sparse_sdk_committed" => 5,
        "owned_sparse_pending" | "owned_sparse_committed" => 6,
        "owned_sparse_controls" => 1,
        "owned_sparse_physical" => 2,
        "owned_sparse_rejected" | "owned_sparse_crash" => 3,
        "creator_open_flow"
        | "creator_open_recovery"
        | "creator_open_physical"
        | "creator_open_deadline"
        | "creator_open_crash" => 1,
        "creator_open_stores" => 4,
        "creator_open_rejected" => 2,
        "creator_open_controls" | "creator_open_quotas" | "creator_open_unclaimed" => 0,
        "creator_import_flow" | "creator_import_crash" => 1,
        "creator_import_controls" => 2,
        "deferred_client_flow"
        | "deferred_client_recovery"
        | "deferred_client_crash"
        | "deferred_client_physical"
        | "deferred_client_deadline" => 1,
        "deferred_client_controls" => 2,
        "empty_flow" | "empty_recovery" | "empty_crash" | "empty_physical" | "empty_deadline" => 1,
        "native_writer_mount" => 6,
        "native_lifecycle_cold_known" => 3,
        "native_lifecycle_cold_unknown" => 1,
        "native_lifecycle_unlink" => 5,
        "native_lifecycle_mounted" => 13,
        "native_lifecycle_mounted_known" => 3,
        "native_lifecycle_mounted_unknown" => 1,
        "native_lifecycle_replace" => 8,
        "native_lifecycle_profile" => 5,
        "native_lifecycle_ordered" => 5,
        "native_lifecycle_history" => 7,
        "native_lifecycle_disposition" | "native_lifecycle_disposition_concurrent" => 5,
        "native_writer" => 5,
        "native_directories" => 3,
        "native_directories_only" => 0,
        "native_directory_concurrency" => 5,
        "native_small_stack" => 3,
        "native_policy" => 3,
        "native_concurrency" => 5,
        "composition" => 1,
        "empty_controls" => 2,
        "local_retention" => 10,
        "mixed_epochs" | "mixed_scoped" => 13,
        "mixed_lineage" => 12,
        "mixed_roots_controls" => 9,
        "mixed_roots_rivals" => 8,
        "mixed_roots_crash" | "mixed_uploads" => 7,
        "mixed_conflict" => 3,
        "mixed_concurrency" => 8,
        "mixed_controls" => 10,
        "mixed_faults" => 2,
        "flow" => 6,
        "source_flow" => 5,
        "races" | "crash" | "proof_fault" | "source_crash" => 4,
        "policy" | "exact" | "source_ack_crash" | "source_policy" => 2,
        "refusals" => 0,
        _ => panic!(),
    };
    assert_eq!(puts(&f), expected);
    let requests = f.requests.lock().unwrap();
    let keys = requests
        .iter()
        .filter(|s| s.starts_with("PUT ") && s.contains("/writable/.s3drive/"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys.len(),
        expected,
        "a physical immutable target was PUT twice"
    );
    drop(requests);
    assert!(!f.server.is_finished());
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_roots_flow_lost_reserve_put_reply_unicode_manifest_base_and_empty() {
    run("flow").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_roots_rival_publishers_retain_exact_content_conflict() {
    run("races").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_roots_bindings_corrupt_records_and_failed_admission_never_send() {
    run("refusals").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_roots_missing_grandchild_and_failed_prepare_block_publication() {
    run("proof_fault").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_roots_killed_owner_recovers_old_commit_without_credentials_or_source_bytes() {
    run("crash").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_roots_expired_narrowed_policy_replay_prepared_request_without_upload() {
    run("policy").await;
}

async fn exact_names(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let e = setup(config, root, s).await;
    change(
        &s.direct,
        20,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "CON:a. ".into(),
        },
    )
    .await;
    let p = publisher(&e, s).await;
    let (i, g) = plan(&e, s, 10, 40, "exact café 日本語 🦀".as_bytes(), None, 0).await;
    let r = p.begin(i, id(41)).await.unwrap();
    assert_eq!(r.input().base().name, "CON:a. ");
    change(
        &s.direct,
        21,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "a:b".into(),
        },
    )
    .await;
    let a = p.resume(i).await.unwrap();
    assert_eq!(a.metadata().node.name, "a:b");
    pending(&e.storage(), g).await;
    e.storage().close().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_roots_exact_policy_names_survive_rename_during_publication() {
    run("exact").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_sources_publish_unicode_sparse_empty_and_acknowledge_exact_history() {
    run("source_flow").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_sources_killed_unknown_publish_recovers_without_credentials_or_source_bytes() {
    run("source_crash").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_sources_killed_after_failed_ack_recovers_known_root_without_rpc() {
    run("source_ack_crash").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chunk_sources_expired_narrow_policy_busy_and_concurrent_first_ack() {
    run("source_policy").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_dependencies_upload_only_new_objects_and_recover_claimed_put_without_source_reauthorization()
 {
    run("mixed_uploads").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_recovery_child() {
    mixed_uploads::recovery_child().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_additional_empty_partial_unicode_growth_and_zero_objects() {
    run("mixed_controls").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_additional_forged_data_holes_and_corrupt_unread_base_tail_never_send() {
    run("mixed_faults").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_concurrent_uploads_bound_slots_release_sql_and_recover_cancelled_claim_without_resend()
 {
    run("mixed_concurrency").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_racing_creator_retains_rejected_identity_dirty_bytes_and_rival_object_after_restart()
{
    run("mixed_conflict").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_roots_exact_cas_rename_lost_replies_and_killed_metadata_recovery_without_source_bytes()
 {
    run("mixed_roots_crash").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_roots_independent_clients_preserve_rival_conflict_and_refuse_changed_or_deleted_base()
 {
    run("mixed_roots_rivals").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_roots_publish_empty_zero_partial_unicode_and_preserve_exact_local_view() {
    run("mixed_roots_controls").await;
}

#[tokio::test]
async fn mixed_publication_lineage_repeats_after_restart_recovers_exact_reply_and_refuses_foreign_root()
 {
    run("mixed_lineage").await;
}

#[tokio::test]
async fn mixed_clean_epochs_acknowledge_exact_heads_preserve_old_reads_and_reuse_published_base() {
    run("mixed_epochs").await;
}

#[tokio::test]
async fn scoped_mixed_epochs_preserve_old_reads_and_replay_local_acknowledgments() {
    run("mixed_scoped").await;
}

#[tokio::test]
async fn empty_manifest_creation_unicode_reads_zero_and_replays_historical_receipt_without_rpc() {
    run("empty_flow").await;
}
#[tokio::test]
async fn empty_manifest_creation_preflight_and_policy_refuse_before_put_and_collision_is_retained()
{
    run("empty_controls").await;
}
#[tokio::test]
async fn empty_manifest_creation_lost_authority_reply_recovers_without_credentials_or_source_reads()
{
    run("empty_recovery").await;
}

#[tokio::test]
async fn empty_manifest_creation_claimed_put_observes_before_fresh_feature_and_narrowed_policy() {
    run("empty_physical").await;
}
#[tokio::test]
async fn empty_manifest_creation_killed_owner_recovers_exact_request_with_corrupt_body_and_no_credentials()
 {
    run("empty_crash").await;
}

#[tokio::test]
async fn empty_manifest_creation_deadline_retains_committed_request_and_releases_bounded_slot() {
    run("empty_deadline").await;
}

#[tokio::test]
async fn chunk_client_composes_canonical_creation_sparse_large_edits_and_namespace_receipts() {
    run("composition").await;
}

#[tokio::test]
async fn native_chunk_shared_engine_large_unicode_edits_exact_flush_and_restart() {
    run("native_writer").await;
}

#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native chunk-write acceptance: requires real FUSE/WinFsp runtime"]
async fn actual_native_chunk_writes_unicode_sparse_flush_resize_and_remount() {
    run("native_writer_mount").await;
}

#[tokio::test]
async fn native_chunk_policy_narrowing_preserves_dirty_reads_and_rejects_changed_format() {
    run("native_policy").await;
}
#[tokio::test]
async fn native_chunk_flush_captures_exact_view_and_preserves_newer_edits() {
    run("native_concurrency").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_chunk_callbacks_use_owned_runtime_tasks_on_a_small_driver_stack() {
    run("native_small_stack").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_chunk_directory_recovery_preserves_dirty_files_and_original_request() {
    run("native_directories").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_chunk_directory_during_unknown_root_keeps_newer_local_edits() {
    run("native_directory_concurrency").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_chunk_directory_only_performs_zero_s3_puts() {
    run("native_directories_only").await;
}

#[tokio::test]
async fn deferred_client_flow() {
    run("deferred_client_flow").await;
}

#[tokio::test]
async fn deferred_client_controls() {
    run("deferred_client_controls").await;
}

#[tokio::test]
async fn deferred_client_recovery() {
    run("deferred_client_recovery").await;
}

#[tokio::test]
async fn deferred_client_physical() {
    run("deferred_client_physical").await;
}

#[tokio::test]
async fn deferred_client_crash() {
    run("deferred_client_crash").await;
}

#[tokio::test]
async fn deferred_client_deadline() {
    run("deferred_client_deadline").await;
}

#[tokio::test]
async fn creator_import_flow() {
    run("creator_import_flow").await;
}

#[tokio::test]
async fn creator_import_controls() {
    run("creator_import_controls").await;
}

#[tokio::test]
async fn creator_import_crash() {
    run("creator_import_crash").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_flow() {
    run("creator_open_flow").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_controls() {
    run("creator_open_controls").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_quotas() {
    run("creator_open_quotas").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_recovery() {
    run("creator_open_recovery").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_physical() {
    run("creator_open_physical").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_unclaimed() {
    run("creator_open_unclaimed").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_deadline() {
    run("creator_open_deadline").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_crash() {
    run("creator_open_crash").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_stores() {
    run("creator_open_stores").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn creator_open_rejected() {
    run("creator_open_rejected").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_sparse_pending() {
    run("owned_sparse_pending").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_sparse_committed() {
    run("owned_sparse_committed").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_sparse_controls() {
    run("owned_sparse_controls").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_sparse_rejected() {
    run("owned_sparse_rejected").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_sparse_crash() {
    run("owned_sparse_crash").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owned_sparse_physical() {
    run("owned_sparse_physical").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cold_namespace_crash() {
    run("cold_namespace_crash").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cold_namespace_known() {
    run("cold_namespace_known").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cold_namespace_rejected() {
    run("cold_namespace_rejected").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_sparse_sdk_linked() {
    run("native_sparse_sdk_linked").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_sparse_sdk_pending() {
    run("native_sparse_sdk_pending").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_sparse_sdk_committed() {
    run("native_sparse_sdk_committed").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_sparse_sdk_controller_linked() {
    run("native_sparse_sdk_controller_linked").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_sparse_sdk_controller_unlinked() {
    run("native_sparse_sdk_controller_unlinked").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_unlink_close_and_remount() {
    run("native_lifecycle_unlink").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_replacement_preserves_old_handles() {
    run("native_lifecycle_replace").await;
}

#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "dedicated NEW native sparse lifecycle; real FUSE/WinFsp required"]
async fn native_lifecycle_actual_unlink_replace_dirty_handles_and_remount() {
    run("native_lifecycle_mounted").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_cold_known_replays_original_root() {
    run("native_lifecycle_cold_known").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_cold_unknown_retains_original_dirty_epoch() {
    run("native_lifecycle_cold_unknown").await;
}

#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "dedicated NEW native sparse killed-owner known outcome recovery"]
async fn native_lifecycle_actual_killed_owner_known_root_recovery() {
    run("native_lifecycle_mounted_known").await;
}
#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "dedicated NEW native sparse killed-owner unknown epoch refusal"]
async fn native_lifecycle_actual_killed_owner_unknown_epoch_refusal() {
    run("native_lifecycle_mounted_unknown").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_profile_transition_preserves_original_epochs() {
    run("native_lifecycle_profile").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_flush_orders_later_write_and_close() {
    run("native_lifecycle_ordered").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_disposition_owner_close_preserves_other_handles() {
    run("native_lifecycle_disposition").await;
}

#[tokio::test]
async fn native_lifecycle_shared_surviving_write_joins_original_close() {
    run("native_lifecycle_disposition_concurrent").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_lifecycle_shared_acknowledged_history_never_reuploads() {
    run("native_lifecycle_history").await;
}

#[tokio::test]
async fn local_retention_sdk_migrates_legacy_and_preserves_cloned_reads_across_scoped_cycles() {
    run("local_retention").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_retention_validation_work_shared_phases() {
    run("native_retention_validation_work").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_retention_shared_300_saves_close_and_30_restarts() {
    run("native_retention_shared_cycles").await;
}
#[cfg(any(target_os = "linux", windows))]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "fresh linked profile; real FUSE/WinFsp required"]
async fn native_retention_actual_300_saves_close_and_30_remounts() {
    run("native_retention_mounted_cycles").await;
}

#[tokio::test]
async fn native_sparse_sdk_controller_interleaving() {
    run("native_sparse_sdk_controller_interleaving").await;
}

// External-profiler diagnostic only; the 20/300-save acceptance fixtures above
// are deliberately independent and retain their original bounds.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn native_retention_cpu_profile_post_anchor_saves() {
    run("native_retention_cpu_profile").await;
}
