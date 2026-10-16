//! Explicit native acceptance gate; never silently substitutes a mock for FUSE.
#![cfg(target_os = "linux")]
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::Command,
    task::{JoinHandle, JoinSet},
};

type Objects = BTreeMap<String, (Vec<u8>, String)>;
struct Fixture {
    endpoint: String,
    objects: Arc<Mutex<Objects>>,
    requests: Arc<Mutex<Vec<String>>>,
    server: JoinHandle<()>,
    stalled: Arc<tokio::sync::Notify>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn start() -> Self {
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
    fn put(&self, path: &str, bytes: &[u8], etag: &str) {
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

struct MountCleanup(PathBuf);
impl Drop for MountCleanup {
    fn drop(&mut self) {
        // Last-resort cleanup also runs on a test panic; never traverse a live mount.
        let _ = std::process::Command::new("fusermount3")
            .args(["-uz"])
            .arg(&self.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "native acceptance: requires /dev/fuse and fusermount3"]
async fn actual_kernel_mount_reads_pagination_revisions_and_teardown() {
    let mut fixture = Fixture::start().await;
    let data: Vec<u8> = (0..350_123).map(|n| (n % 251) as u8).collect();
    fixture.put("/test-bucket/desktop/large.bin", &data, "large-v1");
    fixture.put("/test-bucket/desktop/nested/", b"", "marker");
    fixture.put(
        "/test-bucket/desktop/nested/a%2Fb + café.txt",
        b"literal key bytes",
        "literal",
    );
    fixture.put("/test-bucket/desktop/empty", b"", "empty");
    fixture.put("/other-bucket/readme", b"other bucket", "other");
    for i in 0..310 {
        fixture.put(
            &format!("/test-bucket/desktop/many/item-{i:04}"),
            b"x",
            "one",
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let mountpoint = dir.path().join("mount");
    std::fs::create_dir(&mountpoint).unwrap();
    let _cleanup = MountCleanup(mountpoint.clone());
    let config = dir.path().join("drive.toml");
    std::fs::write(&config,format!("version=1\nmode='read-only'\n[[sources]]\nalias='docs'\nbucket='test-bucket'\nregion='us-east-1'\nprefix='desktop/'\nendpoint='{}'\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n[[sources]]\nalias='other'\nbucket='other-bucket'\nregion='us-east-1'\nendpoint='{}'\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n",fixture.endpoint,fixture.endpoint)).unwrap();
    let credentials = dir.path().join("credentials");
    std::fs::write(
        &credentials,
        "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n",
    )
    .unwrap();
    let aws_config = dir.path().join("aws-config");
    std::fs::write(&aws_config, "").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_s3drive"));
    command
        .arg("mount")
        .arg(&config)
        .arg(&mountpoint)
        .env("AWS_SHARED_CREDENTIALS_FILE", &credentials)
        .env("AWS_CONFIG_FILE", aws_config)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().unwrap();
    wait_ready(&mut child).await;
    let mount = mountpoint.clone();
    let objects = fixture.objects.clone();
    let mut checks = tokio::task::spawn_blocking(move || {
        assert_eq!(
            names(&mount),
            BTreeSet::from(["docs".into(), "other".into()])
        );
        assert_eq!(
            std::fs::read(mount.join("other/readme")).unwrap(),
            b"other bucket"
        );
        assert_eq!(
            std::fs::read(mount.join("docs/nested/a%2Fb + café.txt")).unwrap(),
            b"literal key bytes"
        );
        assert!(std::fs::read(mount.join("docs/empty")).unwrap().is_empty());
        assert_eq!(
            names(&mount.join("docs/many")),
            (0..310).map(|n| format!("item-{n:04}")).collect()
        );
        let path = mount.join("docs/large.bin");
        let mut file = std::fs::File::open(&path).unwrap();
        let mut sequential = Vec::new();
        file.read_to_end(&mut sequential).unwrap();
        assert_eq!(sequential, data);
        for offset in [0, 1, 65535, 262144, data.len() - 10, data.len()] {
            let mut buffer = [0; 79];
            let len = file.read_at(&mut buffer, offset as u64).unwrap();
            assert_eq!(&buffer[..len], &data[offset..(offset + 79).min(data.len())]);
        }
        std::thread::scope(|scope| {
            for n in 0..8 {
                let file = &file;
                let data = &data;
                scope.spawn(move || {
                    let mut b = [0; 1024];
                    file.read_exact_at(&mut b, n * 4096).unwrap();
                    assert_eq!(&b[..], &data[n as usize * 4096..n as usize * 4096 + 1024]);
                });
            }
        });
        assert!(std::fs::write(mount.join("docs/new"), b"no").is_err());
        assert!(std::fs::remove_file(&path).is_err());
        assert!(std::fs::rename(&path, mount.join("docs/renamed")).is_err());
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
        objects.lock().unwrap().insert(
            "/test-bucket/desktop/large.bin".into(),
            (b"changed".to_vec(), "\"large-v2\"".into()),
        );
        file.seek(SeekFrom::Start(0)).unwrap();
        let err = file.read(&mut [0; 3]).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(116), "expected Linux ESTALE");
        assert_eq!(std::fs::read(&path).unwrap(), b"changed");
        objects.lock().unwrap().insert(
            "/test-bucket/desktop/nested".into(),
            (b"collision".to_vec(), "\"conflict\"".into()),
        );
        assert!(
            std::fs::read_dir(mount.join("docs")).is_err(),
            "ambiguous file/directory must fail"
        );
    });
    let result = tokio::time::timeout(Duration::from_secs(90), &mut checks).await;
    let unmount = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new("fusermount3")
            .arg("-u")
            .arg(&mountpoint)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        unmount.status.success(),
        "{}",
        String::from_utf8_lossy(&unmount.stderr)
    );
    let exit = tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(exit.success(), "mount daemon failed");
    if result.is_err() {
        let _ = tokio::time::timeout(Duration::from_secs(35), checks).await;
        panic!("filesystem checks timed out");
    }
    result.unwrap().unwrap();
    assert!(std::fs::read_dir(&mountpoint).unwrap().next().is_none());
    // Remount, then interrupt an in-flight HTTP read with SIGTERM. A Notify
    // proves that GET reached the server; no timing sleep is used as readiness.
    fixture.put("/other-bucket/stalled", b"stalled", "stalled");
    let mut child = command.spawn().unwrap();
    wait_ready(&mut child).await;
    let stalled_path = mountpoint.join("other/stalled");
    let reading = tokio::task::spawn_blocking(move || std::fs::read(stalled_path));
    tokio::time::timeout(Duration::from_secs(10), fixture.stalled.notified())
        .await
        .unwrap();
    assert!(
        Command::new("kill")
            .arg("-TERM")
            .arg(child.id().unwrap().to_string())
            .status()
            .await
            .unwrap()
            .success()
    );
    let exit = tokio::time::timeout(Duration::from_secs(10), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(exit.success(), "signal teardown failed");
    assert!(
        tokio::time::timeout(Duration::from_secs(5), reading)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert!(std::fs::read_dir(&mountpoint).unwrap().next().is_none());
    assert!(!fixture.server.is_finished(), "fixture server failed");
    {
        let requests = fixture.requests.lock().unwrap();
        assert!(requests.iter().any(|r| r.contains("continuation-token=")));
        assert!(
            requests
                .iter()
                .all(|r| r.starts_with("GET ") || r.starts_with("HEAD "))
        );
        eprintln!(
            "Verified {} read-only S3 requests through a real FUSE mount",
            requests.len()
        );
    }
    fixture.server.abort();
    // Abort and await the fixture task; its JoinSet aborts all connection workers.
    let _ = (&mut fixture.server).await;
}
fn names(path: &Path) -> BTreeSet<String> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect()
}

async fn wait_ready(child: &mut tokio::process::Child) {
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let ready = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(line) = stderr.next_line().await.unwrap() {
            eprintln!("{line}");
            if line.starts_with("Mounted read-only") {
                return true;
            }
        }
        false
    })
    .await;
    assert!(
        matches!(ready, Ok(true)),
        "mount failed or did not become ready"
    );
}
