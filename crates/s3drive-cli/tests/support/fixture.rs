use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    task::{JoinHandle, JoinSet},
};

type Objects = BTreeMap<String, (Vec<u8>, String)>;
pub struct Fixture {
    pub endpoint: String,
    pub objects: Arc<Mutex<Objects>>,
    pub requests: Arc<Mutex<Vec<String>>>,
    pub server: JoinHandle<()>,
    pub stalled: Arc<tokio::sync::Notify>,
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
                                if first.starts_with("GET /other-bucket/stalled") {
                                    stalled.notify_one();
                                    let mut byte = [0];
                                    let _ = socket.read(&mut byte).await;
                                    return;
                                }
                                let response = respond(&request, &objects.lock().unwrap());
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
        }
    }
    pub fn put(&self, path: &str, bytes: &[u8], etag: &str) {
        self.objects
            .lock()
            .unwrap()
            .insert(path.into(), (bytes.to_vec(), format!("\"{etag}\"")));
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
fn respond(request: &str, objects: &Objects) -> Vec<u8> {
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
    if method == "HEAD" {
        return response("200 OK", &format!("ETag: {etag}\r\n"), b"", bytes.len());
    }
    if headers.get("if-match") != Some(&etag.as_str()) {
        return response("412 Precondition Failed", "", b"", 0);
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
            "ETag: {etag}\r\nContent-Range: bytes {start}-{end}/{}\r\n",
            bytes.len()
        ),
        &bytes[start..=end],
        end - start + 1,
    )
}
