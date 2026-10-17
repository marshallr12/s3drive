use super::fixture::Fixture;
use std::{io::Write, path::Path, sync::atomic::Ordering};
pub const NAME: &str = "created-日本語 %2F + café.txt";
pub const KEY: &str = "/test-bucket/desktop/writable/created-日本語 %2F + café.txt";
pub const UNKNOWN: &str = "created-unknown";
pub const RACE: &str = "created-race";
pub const BODY: &[u8] = b"new file\0\xff exact bytes";
pub fn deep_directory() -> String {
    format!("writable/{}x/", format!("{}/", "d".repeat(250)).repeat(4))
}
pub fn prepare(fixture: &Fixture) {
    fixture.put(
        "/test-bucket/desktop/writable/existing-dir/sentinel",
        b"sentinel",
        "sentinel",
    );
    fixture.put(
        &format!("/test-bucket/desktop/{}", deep_directory()),
        b"",
        "deep-marker",
    );
    fixture.hide_created_listing.store(true, Ordering::SeqCst);
}
pub fn exercise(mount: &Path, fixture: &Fixture) {
    let directory = mount.join("docs/writable");
    let path = directory.join(NAME);
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    assert_eq!(file.metadata().unwrap().len(), 0);
    assert!(fixture.objects.lock().unwrap()[KEY].0.is_empty());
    file.write_all(BODY).unwrap();
    file.sync_all().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), BODY);
    assert_eq!(fixture.objects.lock().unwrap()[KEY].0, BODY);
    let before = super::writes::put_count(fixture);
    let error = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(super::writes::put_count(fixture), before);
    // Existing O_CREAT must open rather than truncate or conditionally recreate.
    drop(
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .unwrap(),
    );
    assert_eq!(std::fs::read(&path).unwrap(), BODY);
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("existing-dir"))
            .is_err()
    );
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(mount.join("docs/outside-policy"))
            .is_err()
    );
    assert!(std::fs::create_dir(directory.join("no-mkdir")).is_err());
    assert!(
        std::fs::read_dir(&directory)
            .unwrap()
            .any(|entry| entry.unwrap().file_name() == NAME)
    );
    let gone = directory.join("created-gone");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&gone)
        .unwrap()
        .sync_all()
        .unwrap();
    fixture.remove("/test-bucket/desktop/writable/created-gone");
    assert!(
        std::fs::metadata(&gone).is_err(),
        "catalog projected a deleted remote key"
    );
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(gone)
            .is_err(),
        "create discarded the retained lineage"
    );
    let save = directory.join("created-save");
    std::fs::write(&save, b"first save").unwrap();
    std::fs::write(&save, b"replacement save").unwrap();
    assert_eq!(std::fs::read(&save).unwrap(), b"replacement save");
    #[cfg(windows)]
    {
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(NAME.to_uppercase()))
                .is_err()
        );
    }
    #[cfg(target_os = "linux")]
    {
        for name in ["created-Case", "created-case"] {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(name))
                .unwrap()
                .sync_all()
                .unwrap();
        }
    }
    eprintln!(
        "PASS: native Unicode create/write/reopen, exclusive collision, LIST omission and read-only parent"
    );
}
pub fn uncertain(mount: &Path, fixture: &Fixture) {
    let before = super::writes::put_count(fixture);
    fixture.hide_committed.store(true, Ordering::SeqCst);
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(mount.join("docs/writable").join(UNKNOWN))
            .is_err()
    );
    assert!(
        fixture.objects.lock().unwrap()[&format!("/test-bucket/desktop/writable/{UNKNOWN}")]
            .0
            .is_empty()
    );
    assert_eq!(super::writes::put_count(fixture), before + 1);
}
pub fn recovered(mount: &Path, fixture: &Fixture, puts: usize) {
    assert_eq!(
        std::fs::read(mount.join("docs/writable").join(NAME)).unwrap(),
        BODY
    );
    assert!(
        std::fs::read(mount.join("docs/writable").join(UNKNOWN))
            .unwrap()
            .is_empty()
    );
    assert!(
        std::fs::read_dir(mount.join("docs/writable"))
            .unwrap()
            .any(|entry| entry.unwrap().file_name() == UNKNOWN)
    );
    assert_eq!(
        super::writes::put_count(fixture),
        puts,
        "restart replayed creation"
    );
    eprintln!("PASS: native uncertain create reconciled after restart without another PUT");
}
pub fn rival_and_denied(mount: &Path, fixture: &Fixture) {
    let before = super::writes::put_count(fixture);
    fixture.deny_head.store(true, Ordering::SeqCst);
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(mount.join("docs/writable/created-denied"))
            .is_err()
    );
    fixture.deny_head.store(false, Ordering::SeqCst);
    assert_eq!(
        super::writes::put_count(fixture),
        before,
        "denied HEAD granted create"
    );
    fixture.race_create.store(true, Ordering::SeqCst);
    let error = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(mount.join("docs/writable").join(RACE))
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(super::writes::put_count(fixture), before + 1);
    assert_eq!(
        fixture.objects.lock().unwrap()[&format!("/test-bucket/desktop/writable/{RACE}")].0,
        b"external creator"
    );
    eprintln!("PASS: native HEAD denial and racing creator never fall back to replacement");
}
pub async fn inspect(state: &Path) {
    let storage = s3drive_state::worker::Storage::open(
        state,
        s3drive_state::cache::Options::default(),
        s3drive_state::journal::Options::default(),
    )
    .await
    .unwrap();
    let pending = storage.staging().pending(None, 100).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].target.object_key().ends_with(RACE));
    assert_eq!(pending[0].size, 0);
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
