//! Full shared file engine + real SDK/SQLite, without kernel mount privileges.
#[path = "support/fixture.rs"]
pub mod fixture;
#[path = "support/writes.rs"]
#[allow(dead_code)]
mod writes;
use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_engine_writes_restarts_and_preserves_conflicts() {
    let mut fixture = fixture::Fixture::start().await;
    let temp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&fixture, temp.path()).await;
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "engine_child", "--nocapture"])
        .env("S3DRIVE_ENGINE_CONFIG", &setup.config)
        .env("S3DRIVE_ENGINE_ROOT", temp.path())
        .env("AWS_SHARED_CREDENTIALS_FILE", &setup.credentials)
        .env("AWS_CONFIG_FILE", &setup.aws_config)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut errors = child.stderr.take().unwrap();
    let error_task = tokio::spawn(async move {
        let mut text = String::new();
        errors.read_to_string(&mut text).await.unwrap();
        text
    });
    let mut unknown_puts = None;
    let ready = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(line) = output.next_line().await.unwrap() {
            match line.as_str() {
                "READY_FOR_UNKNOWN" => {
                    fixture
                        .hide_committed
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    std::fs::write(temp.path().join("unknown-ready"), b"ready").unwrap();
                }
                "READY_TO_RESTART" => {
                    unknown_puts = Some(writes::put_count(&fixture));
                    writes::restore_observation(&fixture);
                    std::fs::write(temp.path().join("restart-ready"), b"ready").unwrap();
                }
                "READY_FOR_RIVAL" => {
                    assert_eq!(
                        Some(writes::put_count(&fixture)),
                        unknown_puts,
                        "restart replayed uncertain PUT"
                    );
                    return true;
                }
                _ => {}
            }
        }
        false
    })
    .await;
    if !matches!(ready, Ok(true)) {
        let _ = child.kill().await;
        let status = child.wait().await.unwrap();
        panic!(
            "engine child {status}; ready={ready:?}; {}",
            error_task.await.unwrap()
        );
    }
    fixture.put(writes::KEY, b"external rival", "rival");
    std::fs::write(temp.path().join("rival-ready"), b"ready").unwrap();
    let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
        .await
        .unwrap()
        .unwrap();
    let errors = error_task.await.unwrap();
    assert!(status.success(), "{status}: {errors}");
    assert_eq!(
        fixture.objects.lock().unwrap()[writes::KEY].0,
        b"external rival"
    );
    assert!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.starts_with("PUT ") && !r.contains("s3drive-conformance"))
    );
    assert!(!fixture.server.is_finished());
    fixture.server.abort();
    let _ = (&mut fixture.server).await;
}

#[tokio::test]
async fn engine_child() {
    let Ok(config) = std::env::var("S3DRIVE_ENGINE_CONFIG") else {
        return;
    };
    let root = PathBuf::from(std::env::var("S3DRIVE_ENGINE_ROOT").unwrap());
    let config = s3drive_core::Config::parse(&std::fs::read_to_string(config).unwrap()).unwrap();
    let fs = s3drive_fs::ReadOnlyFs::new(&config).await.unwrap();
    let alias = fs.lookup(s3drive_fs::ROOT, "docs").await.unwrap();
    let directory = fs.lookup(alias.id, "writable").await.unwrap();
    let node = fs.lookup(directory.id, writes::NAME).await.unwrap();
    assert!(std::mem::size_of_val(&fs.open_write(node.id)) < 16 * 1024);
    let file = fs.open_write(node.id).await.unwrap();
    let reader = fs.open(node.id).await.unwrap();
    assert!(
        reader
            .write(0, Arc::from(b"no".as_slice()), false, false)
            .await
            .is_err()
    );
    file.write(2, Arc::from(b"XY\0\xff".as_slice()), false, false)
        .await
        .unwrap();
    assert_eq!(reader.read(0, 100).await.unwrap(), b"abXY\0\xffghij");
    assert_eq!(file.current_metadata().unwrap().size, 10);
    file.truncate(5).await.unwrap();
    file.truncate(12).await.unwrap();
    file.write(0, Arc::from(b"tail".as_slice()), true, false)
        .await
        .unwrap();
    assert_eq!(
        reader.read(0, 100).await.unwrap(),
        b"abXY\0\0\0\0\0\0\0\0tail"
    );
    assert!(file.truncate(8 * 1024 * 1024 + 1).await.is_err());
    assert_eq!(
        file.write(15, Arc::from(b"XYZZ".as_slice()), false, true)
            .await
            .unwrap(),
        1
    );
    file.flush().await.unwrap();
    file.flush().await.unwrap();
    let mut expected = reader.read(0, 100).await.unwrap();
    println!("READY_FOR_UNKNOWN");
    wait_signal(&root, "unknown-ready").await;
    assert!(
        file.write(0, Arc::from(b"lost".as_slice()), false, false)
            .await
            .is_err()
    );
    expected[..4].copy_from_slice(b"lost");
    assert_eq!(reader.read(0, 100).await.unwrap(), expected);
    assert!(file.flush().await.is_err());
    fs.cache_control().unwrap().close().await.unwrap();
    drop(reader);
    drop(file);
    drop(fs);
    println!("READY_TO_RESTART");
    wait_signal(&root, "restart-ready").await;
    let fs = s3drive_fs::ReadOnlyFs::new(&config).await.unwrap();
    let alias = fs.lookup(s3drive_fs::ROOT, "docs").await.unwrap();
    let directory = fs.lookup(alias.id, "writable").await.unwrap();
    let node = fs.lookup(directory.id, writes::NAME).await.unwrap();
    let file = fs.open_write(node.id).await.unwrap();
    assert_eq!(file.read(0, 100).await.unwrap(), expected);
    let readonly = fs.lookup(alias.id, "readonly.txt").await.unwrap();
    assert!(fs.open_write(readonly.id).await.is_err());
    println!("READY_FOR_RIVAL");
    wait_signal(&root, "rival-ready").await;
    assert!(matches!(
        file.write(0, Arc::from(b"mine".as_slice()), false, false)
            .await,
        Err(s3drive_fs::Error::Conflict)
    ));
    assert_eq!(&file.read(0, 100).await.unwrap()[..4], b"mine");
    assert!(file.flush().await.is_err());
    assert!(fs.open_write(node.id).await.is_err());
    fs.cache_control().unwrap().close().await.unwrap();
    drop(file);
    drop(fs);
    let storage = s3drive_state::worker::Storage::open(
        config
            .writes
            .as_ref()
            .map(|w| w.directory.as_path())
            .unwrap(),
        s3drive_state::cache::Options::default(),
        s3drive_state::journal::Options::default(),
    )
    .await
    .unwrap();
    let pending = storage.staging().pending(None, 100).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        &storage.staging().read(pending[0].id, 0, 100).await.unwrap()[..4],
        b"mine"
    );
    assert_eq!(
        storage
            .uploads()
            .for_generation(pending[0].id)
            .await
            .unwrap()
            .unwrap()
            .phase,
        s3drive_state::remote::Phase::Rejected
    );
    storage.close().await.unwrap();
}

async fn wait_signal(root: &std::path::Path, name: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !root.join(name).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("parent signal deadline");
}
