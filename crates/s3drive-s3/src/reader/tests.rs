use super::*;
use aws_credential_types::Credentials;
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Mutex, Notify},
    task::JoinHandle,
};

struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    stall: bool,
}

impl Response {
    fn new(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![],
            body: body.into(),
            stall: false,
        }
    }
    fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
    fn head(size: usize, etag: Option<&str>) -> Self {
        let response = Self::new(200, "").header("Content-Length", &size.to_string());
        match etag {
            Some(tag) => response.header("ETag", tag),
            None => response,
        }
    }
    fn range(body: &str, range: &str, etag: &str) -> Self {
        Self::new(206, body)
            .header("Content-Range", range)
            .header("ETag", etag)
    }
}

/// Real TCP, kernel-selected port, bounded I/O, and explicit worker teardown.
struct Server {
    endpoint: String,
    requests: Arc<Mutex<Vec<String>>>,
    received: Arc<Notify>,
    task: Option<JoinHandle<()>>,
}

impl Server {
    async fn start(responses: Vec<Response>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let received = Arc::new(Notify::new());
        let capture = requests.clone();
        let signal = received.clone();
        let task = tokio::spawn(async move {
            for response in responses {
                let (mut socket, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let request = tokio::time::timeout(Duration::from_secs(5), async {
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        assert!(request.len() < 16384, "oversized request headers");
                        request.push(socket.read_u8().await.unwrap());
                    }
                    String::from_utf8(request).unwrap()
                })
                .await
                .unwrap();
                capture.lock().await.push(request);
                signal.notify_one();
                let mut wire =
                    format!("HTTP/1.1 {} Test\r\nConnection: close\r\n", response.status);
                if !response
                    .headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                {
                    wire.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
                }
                for (name, value) in response.headers {
                    wire.push_str(&format!("{name}: {value}\r\n"));
                }
                wire.push_str("\r\n");
                socket.write_all(wire.as_bytes()).await.unwrap();
                if response.stall {
                    std::future::pending::<()>().await;
                }
                socket.write_all(response.body.as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            }
        });
        Self {
            endpoint,
            requests,
            received,
            task: Some(task),
        }
    }

    fn reader(&self) -> S3Reader {
        S3Reader::with_credentials(
            source(&self.endpoint),
            SharedCredentialsProvider::new(Credentials::new(
                "TESTACCESS",
                "TESTSECRET",
                Some("TESTSESSION".into()),
                None,
                "test-fixture",
            )),
            Duration::from_secs(3),
            1,
        )
        .unwrap()
    }

    async fn wait_for_requests(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if self.requests.lock().await.len() >= count {
                    break;
                }
                self.received.notified().await;
            }
        })
        .await
        .expect("server did not receive the expected requests");
    }

    async fn finish(mut self) -> Vec<String> {
        tokio::time::timeout(Duration::from_secs(5), self.task.as_mut().unwrap())
            .await
            .expect("server did not finish")
            .expect("server failed");
        self.task.take();
        self.requests.lock().await.clone()
    }

    async fn shutdown(mut self) {
        let task = self.task.take().unwrap();
        task.abort();
        if let Err(error) = task.await {
            assert!(error.is_cancelled());
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn source(endpoint: &str) -> Source {
    Source {
        alias: "docs".into(),
        bucket: "fixture-bucket".into(),
        region: "us-east-1".into(),
        prefix: "root/".into(),
        endpoint: Some(endpoint.into()),
        path_style: true,
        allow_http_loopback: true,
        profile: None,
        list_url_encoding: ListUrlEncoding::Percent,
    }
}

fn list_xml(contents: &str, token: Option<&str>) -> String {
    format!(
        "<ListBucketResult><EncodingType>url</EncodingType><IsTruncated>{}</IsTruncated>{contents}{}</ListBucketResult>",
        token.is_some(),
        token
            .map(|t| format!("<NextContinuationToken>{t}</NextContinuationToken>"))
            .unwrap_or_default()
    )
}

fn query(request: &str) -> std::collections::HashMap<String, String> {
    let target = request.split_whitespace().nth(1).unwrap();
    url::Url::parse(&format!("http://fixture{target}"))
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

fn header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
}

#[tokio::test]
async fn listing_preserves_key_bytes_and_opaque_pagination_tokens() {
    let first = list_xml(
        "<Contents><Key>root%2Fa%252Fb%20%2B%20caf%C3%A9.txt</Key><Size>7</Size><ETag>&quot;rev&quot;</ETag></Contents><CommonPrefixes><Prefix>root%2Fsub%2F</Prefix></CommonPrefixes>",
        Some("next+/=%25"),
    );
    let server = Server::start(vec![
        Response::new(200, &first),
        Response::new(200, &list_xml("", None)),
    ])
    .await;
    let reader = server.reader();
    let page = reader.list_page("", None, 2).await.unwrap();
    assert_eq!(page.objects[0].key, "a%2Fb + café.txt");
    assert_eq!(page.objects[0].size, 7);
    assert_eq!(page.directories, ["sub/"]);
    let last = reader
        .list_page("", page.next_token.as_deref(), 2)
        .await
        .unwrap();
    assert!(last.next_token.is_none());
    let requests = server.finish().await;
    assert_eq!(query(&requests[0])["prefix"], "root/");
    assert_eq!(query(&requests[0])["max-keys"], "2");
    assert_eq!(query(&requests[0])["delimiter"], "/");
    assert_eq!(query(&requests[0])["encoding-type"], "url");
    assert_eq!(query(&requests[1])["continuation-token"], "next+/=%25");
    assert!(
        header(&requests[0], "authorization")
            .unwrap()
            .starts_with("AWS4-HMAC-SHA256 Credential=TESTACCESS/")
    );
    assert_eq!(
        header(&requests[0], "x-amz-security-token"),
        Some("TESTSESSION")
    );
}

#[tokio::test]
async fn bounded_range_reads_pin_revision_and_clip_eof() {
    let server = Server::start(vec![
        Response::head(6, Some("\"v1\"")),
        Response::range("cdef", "bytes 2-5/6", "\"v1\""),
    ])
    .await;
    let handle = server.reader().open("a%2Fb + café.txt").await.unwrap();
    assert_eq!(handle.read_range(2, 8).await.unwrap(), b"cdef");
    assert!(handle.read_range(6, 1).await.unwrap().is_empty());
    assert!(handle.read_range(u64::MAX, 1).await.unwrap().is_empty());
    assert!(handle.read_range(0, 0).await.unwrap().is_empty());
    assert_eq!(
        handle
            .read_range(0, MAX_READ_BYTES + 1)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::InvalidInput
    );
    let requests = server.finish().await;
    assert!(requests[0].starts_with("HEAD /fixture-bucket/root/a%252Fb%20%2B%20caf%C3%A9.txt "));
    assert_eq!(header(&requests[1], "range"), Some("bytes=2-5"));
    assert_eq!(header(&requests[1], "if-match"), Some("\"v1\""));
}

#[tokio::test]
async fn immutable_version_is_sent_and_checked() {
    let server = Server::start(vec![
        Response::head(3, Some("\"tag\"")).header("x-amz-version-id", "version+/=1"),
        Response::range("abc", "bytes 0-2/3", "\"tag\"").header("x-amz-version-id", "version+/=1"),
    ])
    .await;
    let handle = server.reader().open("file").await.unwrap();
    assert_eq!(handle.read_range(0, 3).await.unwrap(), b"abc");
    let requests = server.finish().await;
    assert_eq!(query(&requests[1])["versionId"], "version+/=1");
}

#[tokio::test]
async fn no_validator_or_mutable_null_version_cannot_open() {
    for response in [
        Response::head(0, None),
        Response::head(3, None).header("x-amz-version-id", "null"),
    ] {
        let server = Server::start(vec![response]).await;
        assert!(matches!(
            server.reader().open("file").await,
            Err(Error {
                kind: ErrorKind::Unsupported,
                ..
            })
        ));
        server.finish().await;
    }
}

#[tokio::test]
async fn empty_objects_do_not_issue_invalid_range_gets() {
    let server = Server::start(vec![Response::head(0, Some("\"empty\""))]).await;
    let handle = server.reader().open("empty").await.unwrap();
    assert!(handle.read_range(0, 10).await.unwrap().is_empty());
    assert_eq!(server.finish().await.len(), 1);
}

#[tokio::test]
async fn classifies_http_errors_without_echoing_remote_data() {
    for (status, kind) in [
        (403, ErrorKind::PermissionDenied),
        (404, ErrorKind::NotFound),
        (412, ErrorKind::RevisionChanged),
        (416, ErrorKind::InvalidRange),
        (429, ErrorKind::Throttled),
        (503, ErrorKind::ServiceUnavailable),
    ] {
        let server = Server::start(vec![Response::new(
            status,
            "<Error><Code>Failure</Code><Message>REMOTE_SECRET</Message></Error>",
        )])
        .await;
        let error = server.reader().list_page("", None, 10).await.unwrap_err();
        assert_eq!(error.kind, kind);
        assert_eq!(error.http_status, Some(status));
        assert!(!format!("{error:?} {error}").contains("REMOTE_SECRET"));
        server.finish().await;
    }
}

#[tokio::test]
async fn rejects_wrong_range_revision_and_short_body() {
    for (reply, expected) in [
        (
            Response::new(200, "abc").header("ETag", "\"v1\""),
            ErrorKind::InvalidResponse,
        ),
        (
            Response::range("abc", "bytes 1-3/3", "\"v1\""),
            ErrorKind::InvalidResponse,
        ),
        (
            Response::range("abc", "bytes 0-2/3", "\"v2\""),
            ErrorKind::RevisionChanged,
        ),
        (
            Response::range("ab", "bytes 0-2/3", "\"v1\"").header("Content-Length", "3"),
            ErrorKind::Transport,
        ),
    ] {
        let server = Server::start(vec![Response::head(3, Some("\"v1\"")), reply]).await;
        let handle = server.reader().open("file").await.unwrap();
        assert_eq!(handle.read_range(0, 3).await.unwrap_err().kind, expected);
        server.finish().await;
    }
}

#[tokio::test]
async fn rejects_malformed_pages_and_source_escape() {
    for body in [
        "<invalid>".into(),
        list_xml("<Contents><Key>elsewhere%2Fsecret</Key><Size>1</Size></Contents>", None),
        list_xml("<Contents><Key>root%2Fa%zz</Key><Size>1</Size></Contents>", None),
        list_xml("<Contents><Key>root%2Fa</Key><Size>-1</Size></Contents>", None),
        "<ListBucketResult><EncodingType>url</EncodingType><IsTruncated>true</IsTruncated></ListBucketResult>".into(),
        "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>".into(),
    ] {
        let server = Server::start(vec![Response::new(200, &body)]).await;
        assert_eq!(server.reader().list_page("", None, 10).await.unwrap_err().kind, ErrorKind::InvalidResponse);
        server.finish().await;
    }
    let server = Server::start(vec![Response::new(200, &list_xml("", Some("same")))]).await;
    assert_eq!(
        server
            .reader()
            .list_page("", Some("same"), 10)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::InvalidResponse
    );
    server.finish().await;
}

#[tokio::test]
async fn invalid_paths_and_page_limits_make_no_requests() {
    let server = Server::start(vec![]).await;
    let reader = server.reader();
    for key in ["", "/file", "../file", "a//b", "a/../b", "a\\b"] {
        assert_eq!(
            reader.head(key).await.unwrap_err().kind,
            ErrorKind::InvalidInput
        );
    }
    for path in ["dir", "../", "/dir/", "dir//"] {
        assert_eq!(
            reader.list_page(path, None, 10).await.unwrap_err().kind,
            ErrorKind::InvalidInput
        );
    }
    for limit in [0, 1001] {
        assert!(reader.list_page("", None, limit).await.is_err());
    }
    assert!(reader.list_page("", Some(""), 1).await.is_err());
    assert!(server.finish().await.is_empty());
}

#[tokio::test]
async fn deadline_covers_a_stalled_body_after_headers() {
    let mut stalled = Response::range("", "bytes 0-2/3", "\"v1\"").header("Content-Length", "3");
    stalled.stall = true;
    let server = Server::start(vec![Response::head(3, Some("\"v1\"")), stalled]).await;
    let mut handle = server.reader().open("file").await.unwrap();
    handle.reader.deadline = Duration::from_millis(150);
    assert_eq!(
        handle.read_range(0, 3).await.unwrap_err().kind,
        ErrorKind::Timeout
    );
    server.wait_for_requests(2).await;
    server.shutdown().await;
}

#[tokio::test]
async fn callers_can_cancel_in_flight_reads() {
    let mut stalled = Response::range("", "bytes 0-2/3", "\"v1\"").header("Content-Length", "3");
    stalled.stall = true;
    let server = Server::start(vec![Response::head(3, Some("\"v1\"")), stalled]).await;
    let handle = server.reader().open("file").await.unwrap();
    let task = tokio::spawn(async move { handle.read_range(0, 3).await });
    server.wait_for_requests(2).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    server.shutdown().await;
}

#[test]
fn fingerprints_distinguish_sources_and_profile_selection() {
    let original = source("https://s3.example.com");
    let fingerprint = source_fingerprint(&original);
    let mut changed = original.clone();
    changed.bucket = "another-bucket".into();
    assert_ne!(source_fingerprint(&changed), fingerprint);
    changed = original.clone();
    changed.prefix = "different/".into();
    assert_ne!(source_fingerprint(&changed), fingerprint);
    changed = original.clone();
    changed.profile = Some("default".into());
    assert_ne!(source_fingerprint(&changed), fingerprint);
    changed = original.clone();
    changed.endpoint = Some("https://other.example.com".into());
    assert_ne!(source_fingerprint(&changed), fingerprint);
    changed = original.clone();
    changed.list_url_encoding = ListUrlEncoding::Form;
    assert_ne!(source_fingerprint(&changed), fingerprint);
    assert_eq!(source_fingerprint(&original), fingerprint);
}

#[tokio::test]
async fn explicit_form_listing_dialect_preserves_spaces_pluses_and_percent_sequences() {
    // Captured key encoding from the approved Wasabi integration fixture.
    let body = list_xml(
        "<Contents><Key>root/nested/a%252Fb+%2B+caf%C3%A9.txt</Key><Size>21</Size></Contents><CommonPrefixes><Prefix>root/sub+dir%2B/</Prefix></CommonPrefixes>",
        None,
    );
    for (dialect, expected_file, expected_dir) in [
        (
            ListUrlEncoding::Percent,
            "nested/a%2Fb+++café.txt",
            "sub+dir+/",
        ),
        (
            ListUrlEncoding::Form,
            "nested/a%2Fb + café.txt",
            "sub dir+/",
        ),
    ] {
        let server = Server::start(vec![Response::new(200, &body)]).await;
        let mut reader = server.reader();
        reader.source.list_url_encoding = dialect;
        let page = reader.list_page("", None, 2).await.unwrap();
        assert_eq!(page.objects[0].key, expected_file);
        assert_eq!(page.directories, [expected_dir]);
        if dialect == ListUrlEncoding::Form {
            assert_eq!(
                reader.list_key(Some("root/a%252B%2B+b"), "root/").unwrap(),
                "a%2B+ b"
            );
            assert!(reader.list_key(Some("root/a%xy+b"), "root/").is_err());
            assert!(reader.list_key(Some("outside/a+b"), "root/").is_err());
            reader.source.prefix = "root +/".into();
            assert_eq!(
                reader.list_key(Some("root+%2B/a+b"), "root +/").unwrap(),
                "a b"
            );
        }
        server.finish().await;
    }
}
