//! Private shared-runtime regression: no production test hooks or manual GC.
use super::*;
use s3drive_authority::{Apply, Authority, Mutation, Token, WireClient};
use std::path::Path;
#[allow(dead_code)]
#[path = "../../../../s3drive-cli/tests/support/fixture.rs"]
mod fixture;

const CHILD: &str = "managed::chunk_writes::retention_tests::native_retention_runtime_child";
#[tokio::test]
async fn native_retention_runtime_reader_release_reopens_at_capacity() {
    let tmp = tempfile::tempdir().unwrap();
    let credentials = tmp.path().join("credentials");
    let aws_config = tmp.path().join("aws-config");
    std::fs::write(
        &credentials,
        "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n",
    )
    .unwrap();
    std::fs::write(&aws_config, "").unwrap();
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD, "--nocapture"])
        .env("S3DRIVE_FS_RETENTION_ROOT", tmp.path())
        .env("AWS_SHARED_CREDENTIALS_FILE", credentials)
        .env("AWS_CONFIG_FILE", aws_config)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .kill_on_drop(true);
    // Keep child progress visible even when the bounded wait expires.
    command
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    let mut child = command.spawn().unwrap();
    let status = match tokio::time::timeout(Duration::from_secs(900), child.wait()).await {
        Ok(status) => status.unwrap(),
        Err(_) => {
            child.kill().await.unwrap();
            child.wait().await.unwrap();
            panic!("native runtime capacity child exceeded 900 seconds");
        }
    };
    assert!(
        status.success(),
        "native runtime capacity child failed: {status}"
    );
}
async fn stop(fs: ReadOnlyFs) {
    let cache = fs.cache_control().unwrap();
    fs.shutdown_writes().await.unwrap();
    drop(fs);
    cache.close().await.unwrap();
}
async fn config(root: &Path, endpoint: &str, wire: &WireClient, url: String) -> Config {
    let source = format!(
        "[[sources]]\nalias='docs'\nbucket='test-bucket'\nregion='us-east-1'\nprefix='desktop/'\nendpoint={endpoint:?}\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n"
    );
    let mut c = Config::parse(&format!("version=1\nmode='read-only'\n{source}")).unwrap();
    let plan = s3drive_s3::probe::Plan::prepare(&c.sources[0]).unwrap();
    let report = plan
        .execute(&c.sources[0], &plan.digest().unwrap())
        .await
        .unwrap();
    let report = report.to_toml().unwrap();
    let value: toml::Value = toml::from_str(&report).unwrap();
    assert_eq!(value["passed"].as_bool(), Some(true));
    let plan_path = root.join("plan.toml");
    let report_path = root.join("report.toml");
    let policy_path = root.join("policy.toml");
    std::fs::write(&plan_path, plan.to_toml().unwrap()).unwrap();
    std::fs::write(&report_path, &report).unwrap();
    std::fs::write(&policy_path, format!("version=1\nsource_fingerprint='{}'\nreport_sha256='{}'\nwritable_prefix='writable/'\nmax_object_bytes=8388608\noperations='create-and-replace'\nobject_metadata='s3drive-markers-only'\nevidence_max_age_seconds=86400\n", value["source_fingerprint"].as_str().unwrap(), hex(&Sha256::digest(report.as_bytes())))).unwrap();
    c = Config::parse(&format!("version=2\nmode='read-write'\n{source}\n[writes]\ndirectory={:?}\n[[writes.sources]]\nalias='docs'\npolicy={:?}\nplan={:?}\nreport={:?}\n", root.join("seed").to_str().unwrap(), policy_path.to_str().unwrap(), plan_path.to_str().unwrap(), report_path.to_str().unwrap())).unwrap();
    let Reply::Info(volume) = wire.request(&Request::Info {}).await.unwrap() else {
        panic!()
    };
    let store = "00000000000000000000000000000200".to_owned();
    let reader = s3drive_s3::S3Reader::from_source(&c.sources[0])
        .await
        .unwrap();
    assert!(matches!(
        wire.request(&Request::Apply {
            request: Apply {
                volume: volume.id.clone(),
                operation: store.clone(),
                expected_sequence: volume.sequence,
                mutation: Mutation::RegisterStore {
                    route: reader.storage_fingerprint(),
                    root: "writable/.s3drive/".into()
                }
            }
        })
        .await
        .unwrap(),
        Reply::Applied(_)
    ));
    c.version = 8;
    c.managed = Some(s3drive_core::ManagedConfig {
        authority_ca_file: None,
        windows_disposition: None,
        sparse_lifecycle: None,
        default_store: Some(store.clone()),
        authority: url,
        volume: volume.id,
        token_file: s3drive_state::create_private_secret(
            &root.join("token"),
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap(),
        stores: vec![s3drive_core::ManagedStore {
            id: store,
            source: "docs".into(),
        }],
    });
    c.validate().unwrap();
    c
}
#[tokio::test]
async fn native_retention_runtime_child() {
    let Some(root) = std::env::var_os("S3DRIVE_FS_RETENTION_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let f = fixture::Fixture::start().await;
    f.writable.store(true, Ordering::SeqCst);
    let authority =
        Authority::open_with_policy(root.join("authority"), s3drive_core::names::PORTABLE.into())
            .await
            .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let token =
        Token::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into())
            .unwrap();
    let wire = WireClient::loopback(&url, token.clone()).unwrap();
    let (done, receive) = tokio::sync::oneshot::channel();
    let client = authority.client();
    let server = tokio::spawn(async move {
        s3drive_authority::serve_loopback(listener, client, token, async {
            let _ = receive.await;
        })
        .await
        .unwrap();
    });
    let mut c = config(root, &f.endpoint, &wire, url).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    drop(
        fs.create_file(ROOT, "pressure", |a, b| a == b, true)
            .await
            .unwrap(),
    );
    stop(fs).await;
    c.writes.as_mut().unwrap().directory = root.join("fresh");
    c.managed.as_mut().unwrap().sparse_lifecycle =
        Some(s3drive_core::SparseLifecycle::LinkedRetentionV1);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, "pressure").await.unwrap();
    let file = fs.open_write(node.id).await.unwrap();
    file.write(0, Arc::from(&b"held reader bytes"[..]), false, false)
        .await
        .unwrap();
    assert_eq!(file.read(0, 17).await.unwrap(), b"held reader bytes");
    let guard = file
        .chunk_write
        .as_ref()
        .unwrap()
        .1
        .edited
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone();
    let clone = guard.clone();
    drop(guard);
    file.flush().await.unwrap();
    file.native_object().unwrap().close().await.unwrap();
    drop(file);
    let mut open_time = std::time::Duration::ZERO;
    let mut close_time = std::time::Duration::ZERO;
    for i in 0..255 {
        let start = std::time::Instant::now();
        let file = fs.open(node.id).await.unwrap();
        open_time += start.elapsed();
        let start = std::time::Instant::now();
        file.native_object().unwrap().close().await.unwrap();
        close_time += start.elapsed();
        drop(file);
        if i % 16 == 0 {
            eprintln!(
                "NATIVE_RUNTIME_PRESSURE retained={} open_ms={} close_ms={}",
                i + 2,
                open_time.as_millis(),
                close_time.as_millis()
            );
            open_time = std::time::Duration::ZERO;
            close_time = std::time::Duration::ZERO;
        }
    }
    assert!(
        fs.open(node.id).await.is_err(),
        "reader must impose bounded admission backpressure"
    );
    assert_eq!(clone.read_range(0, 17).await.unwrap(), b"held reader bytes");
    drop(clone);
    // The only trigger is ordinary open. No explicit collector call here.
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.read(0, 17).await.unwrap(), b"held reader bytes");
    file.native_object().unwrap().close().await.unwrap();
    drop(file);
    stop(fs).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, "pressure").await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(file.read(0, 17).await.unwrap(), b"held reader bytes");
    file.native_object().unwrap().close().await.unwrap();
    drop(file);
    stop(fs).await;
    done.send(()).unwrap();
    server.await.unwrap();
    authority.shutdown().await.unwrap();
}
