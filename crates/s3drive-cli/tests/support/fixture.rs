use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};

type Objects = BTreeMap<String, (Vec<u8>, String)>;
type Metadata = BTreeMap<String, (String, String)>;
pub struct Fixture {
    pub endpoint: String,
    pub objects: Arc<Mutex<Objects>>,
    pub requests: Arc<Mutex<Vec<String>>>,
    pub server: JoinHandle<()>,
    pub stalled: Arc<tokio::sync::Notify>,
    pub deny_head: Arc<AtomicBool>,
    pub deny_get: Arc<AtomicBool>,
    pub offline: Arc<AtomicBool>,
    pub writable: Arc<AtomicBool>,
    pub hide_created_listing: Arc<AtomicBool>,
    pub race_create: Arc<AtomicBool>,
    pub lose_put_reply: Arc<AtomicBool>,
    pub hide_committed: Arc<AtomicBool>,
    metadata: Arc<Mutex<Metadata>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let objects = Arc::new(Mutex::new(BTreeMap::<String, (Vec<u8>, String)>::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stalled = Arc::new(tokio::sync::Notify::new());
        let stalled_request = stalled.clone();
        let deny_head = Arc::new(AtomicBool::new(false));
        let deny_get = Arc::new(AtomicBool::new(false));
        let offline = Arc::new(AtomicBool::new(false));
        let head_policy = deny_head.clone();
        let get_policy = deny_get.clone();
        let availability = offline.clone();
        let writable = Arc::new(AtomicBool::new(false));
        let writes = writable.clone();
        let hide_created_listing = Arc::new(AtomicBool::new(false));
        let hidden_listing = hide_created_listing.clone();
        let race_create = Arc::new(AtomicBool::new(false));
        let competing_create = race_create.clone();
        let lose_put_reply = Arc::new(AtomicBool::new(false));
        let lost = lose_put_reply.clone();
        let hide_committed = Arc::new(AtomicBool::new(false));
        let hidden = hide_committed.clone();
        let metadata = Arc::new(Mutex::new(Metadata::new()));
        let metas = metadata.clone();
        let data = objects.clone();
        let seen = requests.clone();
        let server = tokio::spawn(async move {
            let mut workers = JoinSet::new();
            loop {
                tokio::select! {
                    connection = listener.accept() => {
                        let (mut socket, _) = connection.unwrap();
                        let objects = data.clone(); let requests = seen.clone();
                        let stalled = stalled_request.clone();
                        let deny_head = head_policy.clone();
                        let deny_get = get_policy.clone();
                        let offline = availability.clone();
                        let writable=writes.clone();let metadata=metas.clone();
                        let hide_created_listing=hidden_listing.clone();let race_create=competing_create.clone();
                        let lose_put_reply=lost.clone(); let hide_committed=hidden.clone();
                        workers.spawn(async move {
                            tokio::time::timeout(Duration::from_secs(20), async move {
                                let mut bytes = Vec::new();
                                while !bytes.ends_with(b"\r\n\r\n") {
                                    assert!(bytes.len() < 16_384);
                                    bytes.push(socket.read_u8().await.unwrap());
                                }
                                let request = String::from_utf8(bytes).unwrap();
                                let first = request.lines().next().unwrap();
                                requests.lock().unwrap().push(first.to_owned());
                                assert!(request.contains("Credential=TESTPROFILE/"));
                                if offline.load(Ordering::SeqCst) {return;}
                                let method=first.split_whitespace().next().unwrap();
                                let headers=request.lines().skip(1).filter_map(|l|l.split_once(':')).map(|(k,v)|(k.to_ascii_lowercase(),v.trim().to_owned())).collect::<BTreeMap<_,_>>();
                                let url=url::Url::parse(&format!("http://fixture{}",first.split_whitespace().nth(1).unwrap())).unwrap();
                                let key=percent_encoding::percent_decode_str(url.path()).decode_utf8().unwrap().into_owned();
                                if writable.load(Ordering::SeqCst) && method=="GET" && url.query_pairs().any(|(k,_)|k=="versioning") {
                                    let body=b"<VersioningConfiguration/>";socket.write_all(&response("200 OK","Content-Type: application/xml\r\n",body,body.len())).await.unwrap();return;
                                }
                                if method=="PUT" || method=="DELETE" {
                                    assert!(writable.load(Ordering::SeqCst),"unexpected mutation");
                                    if method=="DELETE" {
                                        assert!(key.contains("/s3drive-conformance/"),"application delete unsupported");
                                        objects.lock().unwrap().remove(&key);metadata.lock().unwrap().remove(&key);
                                        socket.write_all(&response("204 No Content","",b"",0)).await.unwrap();return;
                                    }
                                    let length=headers["content-length"].parse::<usize>().unwrap();assert!(length<=8*1024*1024);assert!(!headers.contains_key("transfer-encoding"));
                                    if headers.get("expect").is_some_and(|h|h.eq_ignore_ascii_case("100-continue")){socket.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await.unwrap();}
                                    let mut body=vec![0;length];socket.read_exact(&mut body).await.unwrap();
                                    let result={
                                        let mut data=objects.lock().unwrap();
                                        if headers.get("if-none-match").is_some_and(|h|h=="*") && race_create.swap(false,Ordering::SeqCst) {
                                            assert!(!data.contains_key(&key));
                                            data.insert(key.clone(),(b"external creator".to_vec(),"\"external-create\"".into()));
                                            metadata.lock().unwrap().remove(&key);
                                        }
                                        let allowed=if headers.get("if-none-match").is_some_and(|h|h=="*"){!data.contains_key(&key)}else{headers.get("if-match").is_some_and(|tag|data.get(&key).is_some_and(|(_,old)|old==tag))};
                                        if !allowed{response("412 Precondition Failed","",b"",0)}else{
                                            use sha2::{Digest,Sha256};let hash=Sha256::digest(&body).iter().map(|b|format!("{b:02x}")).collect::<String>();
                                            assert_eq!(headers["x-amz-meta-s3drive-sha256"],hash);
                                            let operation=headers["x-amz-meta-s3drive-operation"].clone();assert_eq!(operation.len(),64);
                                            let tag=format!("\"write-{}\"",requests.lock().unwrap().len());
                                            data.insert(key.clone(),(body,tag.clone()));metadata.lock().unwrap().insert(key,(operation,hash));
                                            response("200 OK",&format!("ETag: {tag}\r\n"),b"",0)
                                        }
                                    };
                                    if result.starts_with(b"HTTP/1.1 200") {
                                        if hide_committed.swap(false,Ordering::SeqCst) {
                                            deny_head.store(true,Ordering::SeqCst); deny_get.store(true,Ordering::SeqCst);
                                            return;
                                        }
                                        if lose_put_reply.swap(false,Ordering::SeqCst) {return;}
                                    }
                                    socket.write_all(&result).await.unwrap();return;
                                }
                                if offline.load(Ordering::SeqCst) { return; }
                                if (first.starts_with("HEAD ") && deny_head.load(Ordering::SeqCst)) ||
                                    (first.starts_with("GET ") && !first.contains("list-type=2") && deny_get.load(Ordering::SeqCst)) {
                                    let _ = socket.write_all(&response("403 Forbidden", "", b"", 0)).await;
                                    return;
                                }
                                if first.starts_with("GET /other-bucket/stalled") || first.starts_with("GET /test-bucket/cache/stalled") {
                                    stalled.notify_one();
                                    let mut byte = [0];
                                    let _ = socket.read(&mut byte).await;
                                    return;
                                }
                                let response = respond(&request, &objects.lock().unwrap(), &metadata.lock().unwrap(), hide_created_listing.load(Ordering::SeqCst));
                                let _ = socket.write_all(&response).await;
                            }).await.unwrap();
                        });
                    }
                    result = workers.join_next(), if !workers.is_empty() => { result.unwrap().unwrap(); }
                }
            }
        });
        Self {
            endpoint,
            objects,
            requests,
            server,
            stalled,
            deny_head,
            deny_get,
            offline,
            writable,
            hide_created_listing,
            race_create,
            lose_put_reply,
            hide_committed,
            metadata,
        }
    }
    pub fn remove(&self, path: &str) {
        let mut objects = self.objects.lock().unwrap();
        let mut metadata = self.metadata.lock().unwrap();
        objects.remove(path);
        metadata.remove(path);
    }
    pub fn put(&self, path: &str, bytes: &[u8], etag: &str) {
        self.objects
            .lock()
            .unwrap()
            .insert(path.into(), (bytes.to_vec(), format!("\"{etag}\"")));
        self.metadata.lock().unwrap().remove(path);
    }
}
fn response(status: &str, headers: &str, body: &[u8], length: usize) -> Vec<u8> {
    let mut result = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n{headers}\r\n"
    )
    .into_bytes();
    result.extend_from_slice(body);
    result
}
fn encoded(key: &str) -> String {
    percent_encoding::utf8_percent_encode(key, percent_encoding::NON_ALPHANUMERIC).to_string()
}
fn respond(
    request: &str,
    objects: &Objects,
    metadata: &Metadata,
    hide_created_listing: bool,
) -> Vec<u8> {
    let mut words = request.split_whitespace();
    let method = words.next().unwrap();
    let target = words.next().unwrap();
    assert!(
        matches!(method, "GET" | "HEAD"),
        "unexpected mutation: {method}"
    );
    let url = url::Url::parse(&format!("http://fixture{target}")).unwrap();
    let query: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    let path = percent_encoding::percent_decode_str(url.path())
        .decode_utf8()
        .unwrap();
    if query.get("list-type").is_some_and(|s| s == "2") {
        let bucket = path.trim_matches('/');
        let prefix = query.get("prefix").unwrap();
        let base = format!("/{bucket}/");
        let mut entries = BTreeMap::new();
        for (key, (bytes, etag)) in objects {
            if hide_created_listing && key.contains("/desktop/writable/created-") {
                continue;
            }
            if let Some(relative) = key.strip_prefix(&base).and_then(|s| s.strip_prefix(prefix)) {
                if let Some((directory, _)) = relative.split_once('/') {
                    entries
                        .entry(format!("{prefix}{directory}/"))
                        .or_insert_with(|| {
                            format!(
                                "<CommonPrefixes><Prefix>{}</Prefix></CommonPrefixes>",
                                encoded(&format!("{prefix}{directory}/"))
                            )
                        });
                } else {
                    entries.insert(
                        format!("{prefix}{relative}"),
                        format!(
                            "<Contents><Key>{}</Key><Size>{}</Size><ETag>{etag}</ETag></Contents>",
                            encoded(&format!("{prefix}{relative}")),
                            bytes.len()
                        ),
                    );
                }
            }
        }
        let start: usize = query
            .get("continuation-token")
            .map(|s| s.strip_prefix("page-").unwrap().parse().unwrap())
            .unwrap_or(0);
        let end = (start + 50).min(entries.len());
        let truncated = end < entries.len();
        let next = if truncated {
            format!("<NextContinuationToken>page-{end}</NextContinuationToken>")
        } else {
            String::new()
        };
        let body = format!(
            "<ListBucketResult><EncodingType>url</EncodingType><IsTruncated>{truncated}</IsTruncated>{next}{}</ListBucketResult>",
            entries
                .values()
                .skip(start)
                .take(end - start)
                .cloned()
                .collect::<String>()
        );
        return response(
            "200 OK",
            "Content-Type: application/xml\r\n",
            body.as_bytes(),
            body.len(),
        );
    }
    let Some((bytes, etag)) = objects.get(path.as_ref()) else {
        return response("404 Not Found", "", b"", 0);
    };
    let headers: BTreeMap<_, _> = request
        .lines()
        .skip(1)
        .filter_map(|s| s.split_once(':'))
        .map(|(k, v)| (k.to_lowercase(), v.trim()))
        .collect();
    let extra = metadata
        .get(path.as_ref())
        .map(|(operation, hash)| {
            format!(
                "x-amz-meta-s3drive-operation: {operation}\r\nx-amz-meta-s3drive-sha256: {hash}\r\n"
            )
        })
        .unwrap_or_default();
    if method == "HEAD" {
        return response(
            "200 OK",
            &format!("ETag: {etag}\r\n{extra}"),
            b"",
            bytes.len(),
        );
    }
    if headers.get("if-match") != Some(&etag.as_str()) {
        return response("412 Precondition Failed", "", b"", 0);
    }
    if !headers.contains_key("range") {
        assert!(
            metadata.contains_key(path.as_ref()),
            "unexpected full GET outside write verification"
        );
        return response(
            "200 OK",
            &format!("ETag: {etag}\r\n{extra}"),
            bytes,
            bytes.len(),
        );
    }
    let (start, end) = headers["range"]
        .strip_prefix("bytes=")
        .unwrap()
        .split_once('-')
        .unwrap();
    let start: usize = start.parse().unwrap();
    let end: usize = end.parse().unwrap();
    if end >= bytes.len() || start > end {
        return response("416 Range Not Satisfiable", "", b"", 0);
    }
    response(
        "206 Partial Content",
        &format!(
            "ETag: {etag}\r\n{extra}Content-Range: bytes {start}-{end}/{}\r\n",
            bytes.len()
        ),
        &bytes[start..=end],
        end - start + 1,
    )
}
