//! Buffered bytes the journal refuses at Close stay in memory with their file
//! until a later handle can journal them, and are reported until then.
//!
//! Uses the test-only refusal switch in `coalesce`, which is not compiled into
//! any non-test build. Loopback fixtures only; the child process keeps the
//! switch and the fixture credentials private to this test.
use super::retention_tests::{config, fixture, stop};
use super::*;
use crate::OpenFile;
use s3drive_authority::{Authority, Token, WireClient};
use std::path::Path;

const CHILD: &str = "managed::chunk_writes::kept_tests::kept_run_child";
const ROOT_VARIABLE: &str = "S3DRIVE_FS_KEPT_ROOT";
const SECRET: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PROFILE: &str =
    "[fixture]\naws_access_key_id=TESTPROFILE\naws_secret_access_key=TESTPROFILESECRET\n";

/// Runs the scenario in a child of this test binary with fixture credentials.
#[tokio::test]
async fn refused_bytes_survive_close_until_a_later_handle_journals_them() {
    let tmp = tempfile::tempdir().unwrap();
    let (credentials, aws) = (
        tmp.path().join("credentials"),
        tmp.path().join("aws-config"),
    );
    std::fs::write(&credentials, PROFILE).unwrap();
    std::fs::write(&aws, "").unwrap();
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD, "--nocapture"])
        .env(ROOT_VARIABLE, tmp.path())
        .env("AWS_SHARED_CREDENTIALS_FILE", credentials)
        .env("AWS_CONFIG_FILE", aws)
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .kill_on_drop(true);
    let status = tokio::time::timeout(Duration::from_secs(300), command.status()).await;
    let status = status.expect("kept-run child exceeded its deadline");
    assert!(status.unwrap().success(), "kept-run child failed");
}

/// The scenario itself; a no-op unless launched by the test above.
#[tokio::test]
async fn kept_run_child() {
    let Some(root) = std::env::var_os(ROOT_VARIABLE) else {
        return;
    };
    let root = Path::new(&root);
    let f = fixture::Fixture::start().await;
    f.writable.store(true, Ordering::SeqCst);
    let policy = s3drive_core::names::PORTABLE.into();
    let authority = Authority::open_with_policy(root.join("authority"), policy);
    let authority = authority.await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/namespace", listener.local_addr().unwrap());
    let token = Token::new(SECRET.into()).unwrap();
    let wire = WireClient::loopback(&url, token.clone()).unwrap();
    let (done, receive) = tokio::sync::oneshot::channel();
    let client = authority.client();
    let server = tokio::spawn(async move {
        let stopped = async {
            let _ = receive.await;
        };
        s3drive_authority::serve_loopback(listener, client, token, stopped)
            .await
            .unwrap();
    });
    let mut c = config(root, &f.endpoint, &wire, url).await;
    c.managed.as_mut().unwrap().sparse_lifecycle =
        Some(s3drive_core::SparseLifecycle::ComponentRetentionV1);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    kept_until_journaled(&fs).await;
    kept_at_unmount(fs).await;
    cold_contents(&c).await;
    done.send(()).unwrap();
    server.await.unwrap();
    authority.shutdown().await.unwrap();
}

/// Turns the test-only refusal of buffered runs on or off.
fn refuse(on: bool) {
    coalesce::REFUSE_RUNS.store(on, Ordering::Release);
}

/// Writes `bytes` at `offset` and requires the whole write to be accepted.
async fn put(file: &OpenFile, offset: usize, bytes: &[u8]) {
    let written = file.write(offset as u64, Arc::from(bytes), false, false);
    assert_eq!(written.await.unwrap(), bytes.len(), "write at {offset}");
}

/// Closes a handle the way a driver's Close callback does.
async fn close(file: OpenFile) {
    file.native_object().unwrap().close().await.unwrap();
}

/// Creates a file whose `saved` bytes are journaled and whose `tail` is still
/// buffered, then closes it while the journal refuses the tail. Close must
/// succeed. Returns the node id.
async fn close_refused(fs: &ReadOnlyFs, name: &str, saved: &[u8], tail: &[u8]) -> u64 {
    let created = fs.create_file(ROOT, name, |a, b| a == b, true);
    let file = created.await.unwrap();
    put(&file, 0, saved).await;
    assert_eq!(file.read(0, saved.len()).await.unwrap(), saved);
    put(&file, saved.len(), tail).await;
    let id = file.node.id;
    refuse(true);
    close(file).await;
    refuse(false);
    id
}

/// Kept bytes count toward the size, are reported by a flush with no handle
/// and by a read while still refused, survive a second refused Close, and are
/// journaled by the next Close once the journal accepts them.
async fn kept_until_journaled(fs: &ReadOnlyFs) {
    let id = close_refused(fs, "kept.bin", b"journaled ", b"kept tail").await;
    assert_eq!(fs.metadata(id).await.unwrap().unwrap().size, 19, "by path");
    let unsaved = fs.flush_all().await;
    assert!(matches!(unsaved, Err(Error::Conflict)), "{unsaved:?}");
    let file = fs.open_write(id).await.unwrap();
    assert_eq!(file.current_metadata().unwrap().size, 19, "by handle");
    refuse(true);
    let refused = file.read(0, 19).await;
    assert!(matches!(refused, Err(Error::NoSpace)), "{refused:?}");
    close(file).await;
    refuse(false);
    close(fs.open_write(id).await.unwrap()).await;
    fs.flush_all().await.expect("the last Close journaled them");
    let file = fs.open(id).await.unwrap();
    assert_eq!(file.read(0, 19).await.unwrap(), b"journaled kept tail");
    close(file).await;
    eprintln!("KEPT_RUN close=kept size=counted flush=conflict read=refused close=journaled");
}

/// Bytes still kept at unmount are reported, after everything the journal
/// holds has been published.
async fn kept_at_unmount(fs: ReadOnlyFs) {
    close_refused(&fs, "late.bin", b"saved", b" unsaved").await;
    let cache = fs.cache_control().unwrap();
    let stopped = fs.shutdown_writes().await;
    assert!(matches!(stopped, Err(Error::Conflict)), "{stopped:?}");
    drop(fs);
    cache.close().await.unwrap();
    eprintln!("KEPT_RUN unmount=conflict");
}

/// After a cold restart the journaled and published bytes read back exactly.
async fn cold_contents(c: &Config) {
    let fs = ReadOnlyFs::new(c).await.unwrap();
    let files: [(&str, &[u8]); 2] = [("kept.bin", b"journaled kept tail"), ("late.bin", b"saved")];
    for (name, expected) in files {
        let node = fs.lookup(ROOT, name).await.unwrap();
        let file = fs.open(node.id).await.unwrap();
        assert_eq!(file.read(0, expected.len()).await.unwrap(), expected);
        close(file).await;
    }
    stop(fs).await;
    eprintln!("KEPT_RUN cold_read=exact");
}
