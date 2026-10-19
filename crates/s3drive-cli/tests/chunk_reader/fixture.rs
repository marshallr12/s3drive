//! Owned dummy S3 memory and real authenticated Rust metadata authority.
//! Objects are seeded in memory, not uploaded to any provider. Reads use the SDK.
use s3drive_authority::{
    Apply, Authority, ContentGuard, Kind, ManifestRoot, Mutation, Node, Reply, Request, Token,
    WireClient,
};
use s3drive_core::{
    ListUrlEncoding, Source,
    manifest::{
        self, Entry, Page, Reference,
        objects::{Namespace, Role, RootDescriptor},
    },
};
use s3drive_managed::chunks::{ReadOptions, Reader};
use s3drive_s3::S3Reader;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Notify, Semaphore, oneshot},
    task::{JoinHandle, JoinSet},
};
pub const C: u64 = manifest::CHUNK_BYTES;
pub const TOKEN: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
pub fn id(n: u64) -> String {
    format!("{n:032x}")
}
fn binary(s: &str) -> [u8; 16] {
    std::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
}
fn hash(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|n| format!("{n:02x}"))
        .collect()
}
#[derive(Clone)]
pub struct Object {
    pub bytes: Vec<u8>,
    pub etag: String,
    pub version: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Event {
    pub method: String,
    pub path: String,
    pub range: Option<String>,
    pub matched: Option<String>,
    pub version: Option<String>,
    pub credential: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    HeadDenied,
    GetDenied,
    Missing,
    BigHead,
    BadRange,
    BadEtag,
    BadVersion,
    Short,
    Extra,
    Gate,
}
#[derive(Clone)]
pub enum Target {
    Exact(String),
    Data,
    Pages,
}
#[derive(Clone)]
pub struct S3State {
    pub objects: Arc<Mutex<BTreeMap<String, Object>>>,
    pub events: Arc<Mutex<Vec<Event>>>,
    fault: Arc<Mutex<Option<(Target, Fault)>>>,
    pub arrived: Arc<AtomicUsize>,
    notice: Arc<Notify>,
    pub permits: Arc<Semaphore>,
    pub served: Arc<AtomicU64>,
    pub expected_key: Arc<Mutex<String>>,
}
impl S3State {
    pub fn fault(&self, target: Target, fault: Fault) {
        *self.fault.lock().unwrap() = Some((target, fault));
    }
    pub fn clear(&self) {
        *self.fault.lock().unwrap() = None;
    }
    pub fn events(&self) -> Vec<Event> {
        self.events.lock().unwrap().clone()
    }
    pub fn put(&self, path: String, bytes: Vec<u8>, etag: &str) {
        self.objects.lock().unwrap().insert(
            path,
            Object {
                bytes,
                etag: format!("\"{etag}\""),
                version: None,
            },
        );
    }
    pub async fn wait(&self, n: usize) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while self.arrived.load(Ordering::SeqCst) < n {
                self.notice.notified().await;
            }
        })
        .await
        .expect("expected reader requests never reached owned gate");
    }
}
async fn response(
    socket: &mut TcpStream,
    status: u16,
    headers: String,
    body: &[u8],
    length: usize,
) -> bool {
    let wire = format!(
        "HTTP/1.1 {status} Test\r\nConnection: close\r\nContent-Length: {length}\r\n{headers}\r\n"
    );
    if socket.write_all(wire.as_bytes()).await.is_err() {
        return false;
    }
    if socket.write_all(body).await.is_err() {
        return false;
    }
    let _ = socket.shutdown().await;
    true
}
async fn serve(mut socket: TcpStream, s: S3State) {
    let request = tokio::time::timeout(Duration::from_secs(10), async {
        let mut raw = Vec::new();
        while !raw.ends_with(b"\r\n\r\n") {
            assert!(raw.len() < 16384, "fixture request headers exceeded bound");
            let Ok(byte) = socket.read_u8().await else {
                return None;
            };
            raw.push(byte);
        }
        Some(String::from_utf8(raw).unwrap())
    })
    .await
    .expect("fixture peer stalled before complete request headers");
    let Some(request) = request else {
        return;
    };
    let mut words = request.lines().next().unwrap().split_whitespace();
    let method = words.next().unwrap().to_owned();
    assert!(
        matches!(method.as_str(), "HEAD" | "GET"),
        "reader fixture received mutation"
    );
    let credential = request
        .split("Credential=")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .unwrap_or("")
        .to_owned();
    assert!(
        matches!(credential.as_str(), "TESTPROFILE" | "TESTROTATED"),
        "reader used unexpected credential profile"
    );
    let url = url::Url::parse(&format!("http://fixture{}", words.next().unwrap())).unwrap();
    let path = percent_encoding::percent_decode_str(url.path())
        .decode_utf8()
        .unwrap()
        .into_owned();
    let headers = request
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let version = url
        .query_pairs()
        .find(|(k, _)| k == "versionId")
        .map(|(_, v)| v.into_owned());
    let event = Event {
        method: method.clone(),
        path: path.clone(),
        range: headers.get("range").cloned(),
        matched: headers.get("if-match").cloned(),
        version: version.clone(),
        credential: credential.clone(),
    };
    s.events.lock().unwrap().push(event);
    let permitted = credential == *s.expected_key.lock().unwrap();
    if !permitted {
        response(&mut socket, 403, String::new(), b"", 0).await;
        return;
    }
    let fault = s.fault.lock().unwrap().clone().and_then(|(t, f)| {
        let matched = match t {
            Target::Exact(p) => p == path,
            Target::Data => path.contains("/data/"),
            Target::Pages => path.contains("/page/"),
        };
        matched.then_some(f)
    });
    if fault == Some(Fault::Missing) {
        response(&mut socket, 404, String::new(), b"", 0).await;
        return;
    }
    if (method == "HEAD" && fault == Some(Fault::HeadDenied))
        || (method == "GET" && fault == Some(Fault::GetDenied))
    {
        response(&mut socket, 403, String::new(), b"", 0).await;
        return;
    }
    let object = { s.objects.lock().unwrap().get(&path).cloned() };
    let Some(object) = object else {
        response(&mut socket, 404, String::new(), b"", 0).await;
        return;
    };
    let version_header = object
        .version
        .as_ref()
        .map(|v| format!("x-amz-version-id: {v}\r\n"))
        .unwrap_or_default();
    if method == "HEAD" {
        let size = if fault == Some(Fault::BigHead) {
            32 * C as usize
        } else {
            object.bytes.len()
        };
        response(
            &mut socket,
            200,
            format!("ETag: {}\r\n{version_header}", object.etag),
            b"",
            size,
        )
        .await;
        return;
    }
    if headers.get("if-match") != Some(&object.etag)
        || version
            .as_ref()
            .is_some_and(|v| object.version.as_ref() != Some(v))
    {
        response(&mut socket, 412, String::new(), b"", 0).await;
        return;
    }
    let range = headers
        .get("range")
        .unwrap()
        .strip_prefix("bytes=")
        .unwrap();
    let (a, b) = range.split_once('-').unwrap();
    let a = a.parse::<usize>().unwrap();
    let b = b.parse::<usize>().unwrap();
    assert!(
        a <= b && b < object.bytes.len(),
        "fixture received invalid bounded range"
    );
    let mut body = object.bytes[a..=b].to_vec();
    let mut length = body.len();
    let content_range = if fault == Some(Fault::BadRange) {
        "bytes 1-1/2".into()
    } else {
        format!("bytes {a}-{b}/{}", object.bytes.len())
    };
    let etag = if fault == Some(Fault::BadEtag) {
        "\"rival\""
    } else {
        object.etag.as_str()
    };
    let version_header = if fault == Some(Fault::BadVersion) {
        "x-amz-version-id: different\r\n".into()
    } else {
        version_header
    };
    if fault == Some(Fault::Short) {
        body.pop();
    }
    if fault == Some(Fault::Extra) {
        body.push(9);
        length += 1;
    }
    if fault == Some(Fault::Gate) {
        s.arrived.fetch_add(1, Ordering::SeqCst);
        s.notice.notify_one();
        let permit = s.permits.acquire().await.unwrap();
        permit.forget();
    }
    if response(
        &mut socket,
        206,
        format!("ETag: {etag}\r\nContent-Range: {content_range}\r\n{version_header}"),
        &body,
        length,
    )
    .await
    {
        s.served.fetch_add(body.len() as u64, Ordering::SeqCst);
    }
}
#[derive(Clone)]
struct Proxy {
    upstream: Arc<Mutex<WireClient>>,
    mode: Arc<AtomicU8>,
    requests: Arc<Mutex<Vec<Request>>>,
    next_node: Arc<Mutex<Option<Node>>>,
}
async fn proxy(
    axum::extract::State(p): axum::extract::State<Proxy>,
    headers: axum::http::HeaderMap,
    axum::Json(request): axum::Json<Request>,
) -> axum::Json<Reply> {
    assert_eq!(
        headers.get("authorization").unwrap(),
        format!("Bearer {TOKEN}").as_str()
    );
    p.requests.lock().unwrap().push(request.clone());
    let upstream = p.upstream.lock().unwrap().clone();
    let mut r = upstream.request(&request).await.unwrap();
    if matches!(request, Request::Node { .. })
        && let Some(node) = p.next_node.lock().unwrap().take()
    {
        r = Reply::Node(node);
    }
    match (&request, &mut r, p.mode.load(Ordering::SeqCst)) {
        (Request::Features {}, Reply::Features(f), 1) => f.retain(|f| f != "manifest-root-v1"),
        (Request::Features {}, Reply::Features(f), 30) => f.retain(|f| f != "manifest-history-v1"),
        (Request::Info {}, Reply::Info(v), 2) => v.id = id(999),
        (Request::Info {}, Reply::Info(v), 3) => v.name_policy = "unsupported".into(),
        (Request::Store { .. }, Reply::Store(s), 4) => s.id = id(999),
        (Request::Store { .. }, Reply::Store(s), 5) => s.route = "f".repeat(64),
        (Request::Node { .. }, Reply::Node(n), 6) => n.id = id(999),
        (Request::Node { .. }, Reply::Node(n), 7) => n.deleted = true,
        (Request::Node { .. }, Reply::Node(n), 8) => n.payload = None,
        (Request::Node { .. }, Reply::Node(n), 9) => n.placement.as_mut().unwrap().id = id(999),
        (Request::Node { .. }, Reply::Node(n), 10) => {
            n.payload.as_mut().unwrap().reservation.volume = id(999)
        }
        (Request::Node { .. }, Reply::Node(n), 11) => {
            n.payload.as_mut().unwrap().reservation.key = "elsewhere".into()
        }
        (Request::Node { .. }, Reply::Node(n), 12) => n.name = "CON:bad".into(),
        (Request::Node { .. }, Reply::Node(n), 14) => n.kind = Kind::EmptyFile,
        (Request::Node { .. }, Reply::Node(n), 15) => {
            n.payload.as_mut().unwrap().version = Some("v1".into())
        }
        (Request::Node { .. }, Reply::Node(n), 16) => {
            n.payload.as_mut().unwrap().reservation.sha256 = "0".repeat(64)
        }
        (Request::ManifestPayload { .. }, Reply::ManifestPayload(v), 31) => {
            v.reservation.node = id(999)
        }
        (Request::ManifestPayload { .. }, Reply::ManifestPayload(v), 32) => {
            v.reservation.store.route = "f".repeat(64)
        }
        (Request::ManifestPayload { .. }, Reply::ManifestPayload(v), 33) => {
            v.etag = "\"other\"".into()
        }
        (Request::ManifestPayload { .. }, Reply::ManifestPayload(v), 34) => {
            v.reservation.id = id(999)
        }
        (Request::ManifestPayload { .. }, Reply::ManifestPayload(v), 35) => {
            v.reservation.volume = id(999)
        }
        _ => (),
    }
    if p.mode.load(Ordering::SeqCst) == 36 && matches!(request, Request::ManifestPayload { .. }) {
        r = Reply::Features(vec![]);
    }
    if p.mode.load(Ordering::SeqCst) == 37 && matches!(request, Request::ManifestPayload { .. }) {
        r = Reply::Error(s3drive_authority::Error::Unavailable);
    }
    if p.mode.load(Ordering::SeqCst) == 13 && matches!(request, Request::Store { .. }) {
        r = Reply::Features(vec![]);
    }
    if p.mode.load(Ordering::SeqCst) == 17 && matches!(request, Request::Features {}) {
        r = Reply::List(s3drive_authority::Listing {
            sequence: 0,
            children: vec![],
        });
    }
    if let Reply::List(list) = &mut r {
        match p.mode.load(Ordering::SeqCst) {
            20 => list.sequence = u64::MAX,
            21 => list.children[0].parent = Some(id(999)),
            22 => list.children[0].deleted = true,
            23 => list.children.push(list.children[0].clone()),
            24 => list.children[1].id = list.children[0].id.clone(),
            25 => list.children[0].placement.as_mut().unwrap().id = id(999),
            26 => list.children[0].name = "CON:bad".into(),
            27 => list.children = vec![list.children[0].clone(); 1001],
            _ => (),
        }
    }
    axum::Json(r)
}
pub struct Fixture {
    authority: Option<Authority>,
    upstream: Arc<Mutex<WireClient>>,
    pub direct: WireClient,
    pub wire: WireClient,
    pub volume: String,
    pub source: Source,
    authority_endpoint: String,
    pub s3: S3State,
    pub mode: Arc<AtomicU8>,
    requests: Arc<Mutex<Vec<Request>>>,
    stops: Vec<oneshot::Sender<()>>,
    tasks: Vec<JoinHandle<()>>,
    operation: AtomicU64,
    next_node: Arc<Mutex<Option<Node>>>,
}
pub struct Tree {
    pub root: String,
    pub data: Vec<String>,
    pub pages: Vec<String>,
    pub root_bytes: usize,
    pub size: u64,
}
impl Fixture {
    pub async fn open(path: &Path, exact: bool) -> Self {
        let state = S3State {
            objects: Arc::new(Mutex::new(BTreeMap::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            fault: Arc::new(Mutex::new(None)),
            arrived: Arc::new(AtomicUsize::new(0)),
            notice: Arc::new(Notify::new()),
            permits: Arc::new(Semaphore::new(0)),
            served: Arc::new(AtomicU64::new(0)),
            expected_key: Arc::new(Mutex::new("TESTPROFILE".into())),
        };
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", l.local_addr().unwrap());
        let (stop, mut receive) = oneshot::channel();
        let shared = state.clone();
        let server = tokio::spawn(async move {
            let mut workers = JoinSet::new();
            loop {
                tokio::select! {
                    _=&mut receive=>break,
                    accepted=l.accept()=>{let(s,_) =accepted.unwrap();workers.spawn(serve(s,shared.clone()));},
                    result=workers.join_next(),if !workers.is_empty()=>{result.unwrap().unwrap();}
                }
            }
            workers.abort_all();
            while let Some(r) = workers.join_next().await {
                if let Err(e) = r {
                    assert!(e.is_cancelled());
                }
            }
        });
        let authority = Authority::open_with_policy(
            path.join("metadata"),
            if exact {
                s3drive_core::names::EXACT
            } else {
                s3drive_core::names::PORTABLE
            }
            .into(),
        )
        .await
        .unwrap();
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/namespace", l.local_addr().unwrap());
        let direct = WireClient::loopback(&url, Token::new(TOKEN.into()).unwrap()).unwrap();
        let (stop2, receive) = oneshot::channel();
        let client = authority.client();
        let task = tokio::spawn(async move {
            s3drive_authority::serve_loopback(
                l,
                client,
                Token::new(TOKEN.into()).unwrap(),
                async {
                    let _ = receive.await;
                },
            )
            .await
            .unwrap();
        });
        let mode = Arc::new(AtomicU8::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let next_node = Arc::new(Mutex::new(None));
        let upstream = Arc::new(Mutex::new(direct.clone()));
        let p = Proxy {
            upstream: upstream.clone(),
            mode: mode.clone(),
            requests: requests.clone(),
            next_node: next_node.clone(),
        };
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/namespace", l.local_addr().unwrap());
        let wire = WireClient::loopback(&url, Token::new(TOKEN.into()).unwrap()).unwrap();
        let app = axum::Router::new()
            .route("/v1/namespace", axum::routing::post(proxy))
            .with_state(p);
        let (stop3, receive) = oneshot::channel();
        let task2 = tokio::spawn(async move {
            axum::serve(l, app)
                .with_graceful_shutdown(async {
                    let _ = receive.await;
                })
                .await
                .unwrap();
        });
        let Reply::Info(v) = direct.request(&Request::Info {}).await.unwrap() else {
            panic!()
        };
        let f = Self {
            authority: Some(authority),
            upstream,
            direct,
            wire,
            volume: v.id,
            source: Source {
                alias: "fixture".into(),
                bucket: "fixture-bucket".into(),
                region: "us-east-1".into(),
                prefix: "root/".into(),
                endpoint: Some(endpoint),
                profile: Some("fixture".into()),
                path_style: true,
                allow_http_loopback: true,
                list_url_encoding: ListUrlEncoding::Percent,
            },
            authority_endpoint: url,
            s3: state,
            mode,
            requests,
            stops: vec![stop3, stop2, stop],
            tasks: vec![task2, task, server],
            operation: AtomicU64::new(40),
            next_node,
        };
        f.change(
            200,
            Mutation::RegisterStore {
                route: s3drive_s3::storage_fingerprint(&f.source).unwrap(),
                root: "managed/".into(),
            },
        )
        .await;
        f.create(10, "café 日本語 🦀.bin", 200, exact).await;
        f
    }
    pub fn endpoint(&self) -> &str {
        &self.authority_endpoint
    }
    pub async fn restart_authority(&mut self, path: &Path) {
        let Reply::Info(before) = self.direct.request(&Request::Info {}).await.unwrap() else {
            panic!("info")
        };
        self.stops.remove(1).send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(15), self.tasks.remove(1))
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(
            Duration::from_secs(30),
            self.authority.take().unwrap().shutdown(),
        )
        .await
        .unwrap()
        .unwrap();
        let authority =
            Authority::open_with_policy(path.join("metadata"), before.name_policy.clone())
                .await
                .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
        let direct = WireClient::loopback(&url, Token::new(TOKEN.into()).unwrap()).unwrap();
        let (stop, receive) = oneshot::channel();
        let client = authority.client();
        let task = tokio::spawn(async move {
            s3drive_authority::serve_loopback(
                listener,
                client,
                Token::new(TOKEN.into()).unwrap(),
                async {
                    let _ = receive.await;
                },
            )
            .await
            .unwrap();
        });
        assert_eq!(
            direct.request(&Request::Info {}).await.unwrap(),
            Reply::Info(before)
        );
        *self.upstream.lock().unwrap() = direct.clone();
        self.direct = direct;
        self.authority = Some(authority);
        self.stops.insert(1, stop);
        self.tasks.insert(1, task);
    }
    pub async fn change(&self, op: u64, mutation: Mutation) -> s3drive_authority::Receipt {
        let Reply::Info(v) = self.direct.request(&Request::Info {}).await.unwrap() else {
            panic!()
        };
        let Reply::Applied(r) = self
            .direct
            .request(&Request::Apply {
                request: Apply {
                    volume: v.id,
                    operation: id(op),
                    expected_sequence: v.sequence,
                    mutation,
                },
            })
            .await
            .unwrap()
        else {
            panic!("fixture metadata mutation failed")
        };
        r
    }
    pub async fn node(&self, n: u64) -> Node {
        let Reply::Node(n) = self
            .direct
            .request(&Request::Node {
                volume: self.volume.clone(),
                node: id(n),
            })
            .await
            .unwrap()
        else {
            panic!()
        };
        n
    }
    pub async fn create(&self, n: u64, name: &str, store: u64, exact: bool) {
        let mutation = if exact {
            Mutation::Create {
                parent: s3drive_authority::ROOT.into(),
                name: name.into(),
                node_kind: Kind::EmptyFile,
            }
        } else {
            let Reply::Node(parent) = self
                .direct
                .request(&Request::Node {
                    volume: self.volume.clone(),
                    node: s3drive_authority::ROOT.into(),
                })
                .await
                .unwrap()
            else {
                panic!()
            };
            Mutation::CreateEntry {
                parent: parent.id,
                parent_revision: parent.revision,
                store: Some(id(store)),
                name: name.into(),
                node_kind: Kind::EmptyFile,
            }
        };
        self.change(n, mutation).await;
    }
    pub fn absolute(&self, key: &str) -> String {
        format!("/{}/{}{key}", self.source.bucket, self.source.prefix)
    }
    pub async fn reader(&self) -> Reader {
        Reader::open(
            self.wire.clone(),
            self.volume.clone(),
            vec![(id(200), S3Reader::from_source(&self.source).await.unwrap())],
        )
        .await
        .unwrap()
    }
    pub async fn reader_options(&self, options: ReadOptions) -> Reader {
        Reader::open_with_options(
            self.wire.clone(),
            self.volume.clone(),
            vec![(id(200), S3Reader::from_source(&self.source).await.unwrap())],
            options,
        )
        .await
        .unwrap()
    }
    pub fn requests(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
    pub fn reply_next_node(&self, node: Node) {
        let mut next = self.next_node.lock().unwrap();
        assert!(next.is_none());
        *next = Some(node);
    }
    pub fn config(&self, path: &Path, extra: &[(u64, Source)]) -> s3drive_core::Config {
        let token =
            s3drive_state::create_private_secret(&path.join("secrets"), TOKEN.as_bytes()).unwrap();
        let mut text = format!(
            "version=3\nmode='read-only'\n[managed]\nauthority={:?}\nvolume={:?}\ntoken_file={:?}\n",
            self.authority_endpoint,
            self.volume,
            token.to_str().unwrap(),
        );
        for (store, source) in std::iter::once(&(200, self.source.clone())).chain(extra.iter()) {
            text.push_str(&format!(
                "[[managed.stores]]\nid={:?}\nsource={:?}\n",
                id(*store),
                source.alias
            ));
        }
        for source in std::iter::once(&self.source).chain(extra.iter().map(|(_, s)| s)) {
            text.push_str("[[sources]]\n");
            text.push_str(&toml::to_string(source).unwrap());
        }
        std::fs::write(path.join("drive.toml"), &text).unwrap();
        s3drive_core::Config::parse(&text).unwrap()
    }
    pub async fn publish_page(
        &self,
        node: u64,
        store: u64,
        page: Page,
        root_hash: Option<[u8; 32]>,
    ) -> Tree {
        let op = self.operation.fetch_add(2, Ordering::SeqCst);
        let n = self.node(node).await;
        let reference = Reference::new(
            binary(&id(store)),
            binary(&id(op)),
            root_ordinal(page.length()),
            root_hash.unwrap_or_else(|| page.digest()),
        )
        .unwrap();
        let descriptor = RootDescriptor::new(
            binary(&self.volume),
            binary(&n.id),
            page.length(),
            reference,
        )
        .unwrap();
        let manifest = ManifestRoot::new(descriptor.clone()).unwrap();
        let ns = Namespace::new(
            "managed/".into(),
            binary(&self.volume),
            binary(&n.id),
            binary(&id(store)),
        )
        .unwrap();
        let key = ns.root_key(&descriptor).unwrap();
        let root = self.absolute(key.as_str());
        let bytes = page.encode();
        let size = page.length();
        let root_bytes = bytes.len();
        let etag = format!("root-{op}");
        self.s3.put(root.clone(), bytes.clone(), &etag);
        self.change(
            op,
            Mutation::ReserveManifest {
                node: n.id,
                expected_revision: n.revision,
                store: id(store),
                size: bytes.len() as u64,
                sha256: hash(&bytes),
                manifest: Box::new(manifest),
                guard: ContentGuard {
                    expected: s3drive_authority::payload_reference(&n.payload),
                    detached: false,
                    removal: None,
                },
            },
        )
        .await;
        self.change(
            op + 1,
            Mutation::Publish {
                reservation: id(op),
                upload_operation: "d".repeat(64),
                etag: format!("\"{etag}\""),
                version: None,
            },
        )
        .await;
        Tree {
            root,
            data: vec![],
            pages: vec![],
            root_bytes,
            size,
        }
    }
    pub async fn tree(&self, node: u64, size: u64, data: BTreeMap<u64, Vec<u8>>) -> Tree {
        let op = self.operation.load(Ordering::SeqCst);
        let ns = Namespace::new(
            "managed/".into(),
            binary(&self.volume),
            binary(&id(node)),
            binary(&id(200)),
        )
        .unwrap();
        let mut entries = Vec::new();
        let mut paths = Vec::new();
        let chunks = size.div_ceil(C);
        for i in 0..chunks {
            let len = (size - i * C).min(C);
            if let Some(bytes) = data.get(&i) {
                assert_eq!(bytes.len() as u64, len);
                let object = Reference::new(
                    binary(&id(200)),
                    binary(&id(op)),
                    i,
                    Sha256::digest(bytes).into(),
                )
                .unwrap();
                let key = ns.key(Role::Data, &object).unwrap();
                let path = self.absolute(key.as_str());
                self.s3
                    .put(path.clone(), bytes.clone(), &format!("data-{op}-{i}"));
                paths.push(path);
                entries.push(Entry::Data {
                    length: len,
                    object,
                });
            } else {
                entries.push(Entry::Hole { length: len });
            }
        }
        let mut pages = Vec::new();
        let mut level = 0;
        let mut row = Vec::new();
        for (slot, part) in entries.chunks(128).enumerate() {
            row.push(
                Page::new(
                    binary(&self.volume),
                    binary(&id(node)),
                    slot as u64 * 128 * C,
                    0,
                    coalesce(part),
                )
                .unwrap(),
            );
        }
        if row.is_empty() {
            row.push(Page::new(binary(&self.volume), binary(&id(node)), 0, 0, vec![]).unwrap());
        }
        while row.len() > 1 {
            let mut refs = Vec::new();
            for (slot, page) in row.into_iter().enumerate() {
                let object = Reference::new(
                    binary(&id(200)),
                    binary(&id(op)),
                    ((level as u64 + 1) << 56) | slot as u64,
                    page.digest(),
                )
                .unwrap();
                let path = self.absolute(ns.key(Role::Page, &object).unwrap().as_str());
                self.s3.put(
                    path.clone(),
                    page.encode(),
                    &format!("page-{op}-{level}-{slot}"),
                );
                pages.push(path);
                refs.push(Entry::Child {
                    length: page.length(),
                    object,
                });
            }
            level += 1;
            row = refs
                .chunks(128)
                .map(|p| {
                    let start = match &p[0] {
                        Entry::Child { object, .. } => {
                            (object.ordinal() & 0x00ff_ffff_ffff_ffff)
                                * 128u64.pow(level as u32)
                                * C
                        }
                        _ => unreachable!(),
                    };
                    Page::new(
                        binary(&self.volume),
                        binary(&id(node)),
                        start,
                        level,
                        p.to_vec(),
                    )
                    .unwrap()
                })
                .collect();
        }
        let mut tree = self.publish_page(node, 200, row.pop().unwrap(), None).await;
        tree.data = paths;
        tree.pages = pages;
        tree
    }
    pub async fn close(self) {
        for s in self.stops {
            let _ = s.send(());
        }
        for task in self.tasks {
            tokio::time::timeout(Duration::from_secs(15), task)
                .await
                .unwrap()
                .unwrap();
        }
        self.authority.unwrap().shutdown().await.unwrap();
    }
}
fn coalesce(entries: &[Entry]) -> Vec<Entry> {
    let mut out = Vec::new();
    for e in entries {
        match (out.last_mut(), e) {
            (Some(Entry::Hole { length: a }), Entry::Hole { length: b }) => *a += b,
            _ => out.push(e.clone()),
        }
    }
    out
}
pub fn root_ordinal(size: u64) -> u64 {
    let mut pages = size.div_ceil(C).div_ceil(128).max(1);
    let mut level = 0;
    while pages > 1 {
        pages = pages.div_ceil(128);
        level += 1;
    }
    (level + 1) << 56
}
pub fn pattern(chunk: u64, size: u64) -> Vec<u8> {
    (0..size).map(|p| ((chunk * 17 + p) % 251) as u8).collect()
}
