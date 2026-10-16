//! Real SDK hydration into the local edit coordinator. No cloud account or
//! mount is involved; the fixture refuses all HTTP mutation methods.
#[path = "support/fixture.rs"]
pub mod fixture;
use s3drive_core::Config;
use s3drive_fs::edits::Coordinator;
use s3drive_s3::{ErrorKind, S3Reader};
use s3drive_state::{cache, journal, worker::Storage};
use std::{error::Error as _, sync::Arc, time::Duration};

const KEY: &str = "edit/日本語 + café.txt";
fn original() -> Vec<u8> {
    (0..journal::CHUNK_BYTES * 2 + 17)
        .map(|i| (i % 251) as u8)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sdk_hydrates_exact_unicode_key_and_rejects_changed_base() {
    let fixture = fixture::Fixture::start().await;
    fixture.put(
        &format!("/test-bucket/desktop/{KEY}"),
        &original(),
        "original",
    );
    fixture.put("/test-bucket/desktop/stale", b"abcdef", "old");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::write(root.join("drive.toml"),format!("version=1\nmode='read-only'\n[[sources]]\nalias='docs'\nbucket='test-bucket'\nregion='us-east-1'\nprefix='desktop/'\nendpoint='{}'\nprofile='fixture'\npath_style=true\nallow_http_loopback=true\n",fixture.endpoint)).unwrap();
    std::fs::write(
        root.join("credentials"),
        "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTSECRET\n",
    )
    .unwrap();
    std::fs::write(root.join("aws-config"), "").unwrap();
    let child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sdk_edit_child", "--nocapture"])
        .env("S3DRIVE_EDIT_FIXTURE_DIR", root)
        .env("AWS_SHARED_CREDENTIALS_FILE", root.join("credentials"))
        .env("AWS_CONFIG_FILE", root.join("aws-config"))
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut child_output = Box::pin(child.wait_with_output());
    let ready = tokio::time::timeout(Duration::from_secs(30), async {
        while !root.join("ready").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    tokio::select! {
        output=&mut child_output=>{
            let output=output.unwrap();
            panic!("child exited before ready: {}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
        },
        result=ready=>result.expect("child did not finish opening its pinned base"),
    }
    fixture.put("/test-bucket/desktop/stale", b"UVWXYZ", "new");
    std::fs::write(root.join("go"), b"go").unwrap();
    let output = tokio::time::timeout(Duration::from_secs(30), child_output)
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let requests = fixture.requests.lock().unwrap();
    assert!(requests.iter().any(|r| r.starts_with("GET ")));
    assert!(
        requests
            .iter()
            .all(|r| r.starts_with("HEAD ") || r.starts_with("GET "))
    );
}

#[tokio::test]
async fn sdk_edit_child() {
    let Some(root) = std::env::var_os("S3DRIVE_EDIT_FIXTURE_DIR") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let config = Config::parse(&std::fs::read_to_string(root.join("drive.toml")).unwrap()).unwrap();
    let reader = S3Reader::from_source(&config.sources[0]).await.unwrap();
    let storage = Storage::open(
        &root.join("state"),
        cache::Options::default(),
        journal::Options::default(),
    )
    .await
    .unwrap();
    let coordinator = Coordinator::new(storage.staging());
    let handle = reader.open(KEY).await.unwrap();
    assert_eq!(
        handle.configuration_fingerprint(),
        reader.configuration_fingerprint()
    );
    let offset = journal::CHUNK_BYTES - 2;
    let patch = "PATCH€".as_bytes();
    let snapshot = coordinator
        .from_s3(handle)
        .unwrap()
        .write(offset as u64, Arc::from(patch))
        .unwrap()
        .execute()
        .await
        .unwrap();
    let mut expected = original();
    expected[offset..offset + patch.len()].copy_from_slice(patch);
    assert_eq!(snapshot.read(0, expected.len()).await.unwrap(), expected);
    assert_eq!(snapshot.metadata().target.object_key(), KEY);
    let stale = reader.open("stale").await.unwrap();
    std::fs::write(root.join("ready"), b"ready").unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        while !root.join("go").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let failure = coordinator
        .from_s3(stale)
        .unwrap()
        .truncate(3)
        .unwrap()
        .execute()
        .await
        .err()
        .unwrap();
    assert_eq!(
        failure
            .source()
            .unwrap()
            .downcast_ref::<s3drive_s3::Error>()
            .unwrap()
            .kind,
        ErrorKind::RevisionChanged
    );
    assert_eq!(
        storage
            .staging()
            .info(failure.generation.unwrap())
            .await
            .unwrap()
            .filled,
        0
    );
    assert_eq!(storage.staging().pending(None, 100).await.unwrap().len(), 1);
    storage.close().await.unwrap();
}
