//! Real SDK/SQLite creation acceptance without kernel mount privileges.
#[path = "support/creation.rs"]
#[allow(dead_code)]
mod creation;
#[path = "support/fixture.rs"]
pub mod fixture;
#[path = "support/writes.rs"]
#[allow(dead_code)]
mod writes;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_engine_creation_collisions_restart_and_catalog() {
    let mut fixture = fixture::Fixture::start().await;
    let temp = tempfile::tempdir().unwrap();
    let setup = writes::setup(&fixture, temp.path()).await;
    creation::prepare(&fixture);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "creation_child", "--nocapture"])
        .env("S3DRIVE_CREATE_CONFIG", &setup.config)
        .env("S3DRIVE_CREATE_ROOT", temp.path())
        .env("AWS_SHARED_CREDENTIALS_FILE", &setup.credentials)
        .env("AWS_CONFIG_FILE", &setup.aws_config)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = child.stderr.take().unwrap();
    let errors = tokio::spawn(async move {
        let mut text = String::new();
        stderr.read_to_string(&mut text).await.unwrap();
        text
    });
    let mut puts = None;
    let mut denied_puts = None;
    let protocol = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(line) = output.next_line().await.unwrap() {
            match line.as_str() {
                "GONE" => {
                    fixture.remove("/test-bucket/desktop/writable/created-gone");
                    signal(temp.path(), "gone");
                }
                "DENY" => {
                    denied_puts = Some(writes::put_count(&fixture));
                    fixture.deny_head.store(true, Ordering::SeqCst);
                    signal(temp.path(), "deny");
                }
                "ALLOW" => {
                    assert_eq!(Some(writes::put_count(&fixture)), denied_puts);
                    fixture.deny_head.store(false, Ordering::SeqCst);
                    signal(temp.path(), "allow");
                }
                "UNKNOWN" => {
                    fixture.hide_committed.store(true, Ordering::SeqCst);
                    signal(temp.path(), "unknown");
                }
                "RESTART" => {
                    puts = Some(writes::put_count(&fixture));
                    writes::restore_observation(&fixture);
                    signal(temp.path(), "restart");
                }
                "RACE" => {
                    assert_eq!(Some(writes::put_count(&fixture)), puts);
                    fixture.race_create.store(true, Ordering::SeqCst);
                    signal(temp.path(), "race");
                }
                _ => {}
            }
        }
    })
    .await;
    if protocol.is_err() {
        let _ = child.kill().await;
    }
    let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(
        protocol.is_ok() && status.success(),
        "child {status}: {}",
        errors.await.unwrap()
    );
    assert_eq!(
        writes::put_count(&fixture),
        puts.unwrap() + 1,
        "racing create attempted replacement"
    );
    assert_eq!(
        fixture.objects.lock().unwrap()[creation::KEY].0,
        creation::BODY
    );
    assert_eq!(
        fixture.objects.lock().unwrap()["/test-bucket/desktop/writable/created-race"].0,
        b"external creator"
    );
    assert!(!fixture.server.is_finished());
    fixture.server.abort();
    let _ = (&mut fixture.server).await;
}
fn signal(root: &Path, name: &str) {
    std::fs::write(root.join(name), b"ready").unwrap();
}
async fn wait(root: &Path, name: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !root.join(name).exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
async fn directory(fs: &s3drive_fs::ReadOnlyFs) -> u64 {
    let alias = fs.lookup(s3drive_fs::ROOT, "docs").await.unwrap();
    fs.lookup(alias.id, "writable").await.unwrap().id
}
#[tokio::test]
async fn creation_child() {
    let Ok(config) = std::env::var("S3DRIVE_CREATE_CONFIG") else {
        return;
    };
    let root = PathBuf::from(std::env::var("S3DRIVE_CREATE_ROOT").unwrap());
    let config = s3drive_core::Config::parse(&std::fs::read_to_string(config).unwrap()).unwrap();
    let fs = s3drive_fs::ReadOnlyFs::new(&config).await.unwrap();
    let dir = directory(&fs).await;
    let future = fs.create_file(dir, creation::NAME, |a, b| a == b, true);
    let bytes = std::mem::size_of_val(&future);
    assert!(bytes < 16 * 1024, "create callback future is {bytes} bytes");
    drop(future);
    let file = Box::pin(fs.create_file(dir, creation::NAME, |a, b| a == b, true))
        .await
        .unwrap();
    assert_eq!(file.current_metadata().unwrap().size, 0);
    file.write(0, Arc::from(creation::BODY), false, false)
        .await
        .unwrap();
    file.flush().await.unwrap();
    assert!(matches!(
        fs.create_file(dir, creation::NAME, |a, b| a == b, true)
            .await,
        Err(s3drive_fs::Error::AlreadyExists)
    ));
    let (a, b) = tokio::join!(
        fs.create_file(dir, "created-concurrent", |a, b| a == b, true),
        fs.create_file(dir, "created-concurrent", |a, b| a == b, true)
    );
    assert!(a.is_ok() ^ b.is_ok());
    assert!(matches!(
        a.as_ref().err().or(b.as_ref().err()),
        Some(s3drive_fs::Error::AlreadyExists)
    ));
    drop(a);
    drop(b);
    assert!(
        fs.create_file(dir, "existing-dir", |a, b| a == b, true)
            .await
            .is_err()
    );
    assert_eq!(
        fs.open(fs.lookup(dir, creation::NAME).await.unwrap().id)
            .await
            .unwrap()
            .read(0, 100)
            .await
            .unwrap(),
        creation::BODY
    );
    let mut deep = fs.lookup(s3drive_fs::ROOT, "docs").await.unwrap();
    for name in creation::deep_directory().trim_end_matches('/').split('/') {
        deep = fs.lookup(deep.id, name).await.unwrap();
    }
    let boundary = fs
        .create_file(deep.id, "_", |a, b| a == b, true)
        .await
        .unwrap();
    assert_eq!(
        boundary.current_metadata().unwrap().key.len() + "desktop/".len(),
        1024
    );
    assert!(
        fs.create_file(deep.id, "__", |a, b| a == b, true)
            .await
            .is_err()
    );
    drop(boundary);
    let gone = fs
        .create_file(dir, "created-gone", |a, b| a == b, true)
        .await
        .unwrap();
    drop(gone);
    println!("GONE");
    wait(&root, "gone").await;
    assert!(matches!(
        fs.lookup(dir, "created-gone").await,
        Err(s3drive_fs::Error::NotFound)
    ));
    assert!(matches!(
        fs.create_file(dir, "created-gone", |a, b| a == b, true)
            .await,
        Err(s3drive_fs::Error::Conflict)
    ));
    println!("DENY");
    wait(&root, "deny").await;
    assert!(
        fs.create_file(dir, "created-denied", |a, b| a == b, true)
            .await
            .is_err()
    );
    println!("ALLOW");
    wait(&root, "allow").await;
    println!("UNKNOWN");
    wait(&root, "unknown").await;
    assert!(
        fs.create_file(dir, creation::UNKNOWN, |a, b| a == b, true)
            .await
            .is_err()
    );
    fs.cache_control().unwrap().close().await.unwrap();
    drop(file);
    drop(fs);
    println!("RESTART");
    wait(&root, "restart").await;
    let fs = s3drive_fs::ReadOnlyFs::new(&config).await.unwrap();
    let dir = directory(&fs).await;
    let node = fs.lookup(dir, creation::UNKNOWN).await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert!(file.read(0, 100).await.unwrap().is_empty());
    assert_eq!(
        fs.open(fs.lookup(dir, creation::NAME).await.unwrap().id)
            .await
            .unwrap()
            .read(0, 100)
            .await
            .unwrap(),
        creation::BODY
    );
    println!("RACE");
    wait(&root, "race").await;
    assert!(matches!(
        fs.create_file(dir, creation::RACE, |a, b| a == b, true)
            .await,
        Err(s3drive_fs::Error::AlreadyExists)
    ));
    fs.cache_control().unwrap().close().await.unwrap();
    drop(file);
    drop(fs);
    creation::inspect(&config.writes.as_ref().unwrap().directory).await;
}
