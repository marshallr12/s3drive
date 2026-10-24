//! Fresh-store linked profile: 300 real saves across 30 independent owners.
//! Each ten-save mount sequence retains the existing 120-second syscall bound.
use super::*;
use s3drive_fs::{ROOT, ReadOnlyFs};
const NAME: &str = "retained 日本語 🦀.bin";
fn config(root: &Path, s: &Server) -> s3drive_core::Config {
    let path = root.join("native-drive.toml");
    let mut value: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["managed"]["authority"] = s.url.clone().into();
    let text = toml::to_string(&value).unwrap();
    let c = s3drive_core::Config::parse(&text).unwrap();
    std::fs::write(path, text).unwrap();
    c
}
pub async fn seed(c: &s3drive_core::Config, root: &Path, s: &Server) {
    native_writer::mount_configuration(c, root, s).await;
    let c = config(root, s);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let file = fs
        .create_file(ROOT, NAME, |a, b| a == b, true)
        .await
        .unwrap();
    drop(file);
    native_writer::stop(fs).await;
    let path = root.join("native-drive.toml");
    let mut value: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["writes"]["directory"] = root.join("native-retention-state").to_str().unwrap().into();
    value["managed"]
        .as_table_mut()
        .unwrap()
        .insert("sparse-lifecycle".into(), "linked-retention-v1".into());
    let text = toml::to_string(&value).unwrap();
    s3drive_core::Config::parse(&text).unwrap();
    std::fs::write(path, text).unwrap();
    std::fs::write(root.join("retention-cycle"), "0").unwrap();
}
fn sizes(state: &Path) -> (u64, u64) {
    (
        std::fs::metadata(state.join("metadata.sqlite3"))
            .unwrap()
            .len(),
        std::fs::metadata(state.join("metadata.sqlite3-wal"))
            .map(|m| m.len())
            .unwrap_or(0),
    )
}
fn inspect(root: &Path, done: u64, peak: (u64, u64)) {
    let state = root.join("native-retention-state");
    let db = rusqlite::Connection::open_with_flags(
        state.join("metadata.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    for (table, n) in [
        ("windows_open_journal", 0),
        ("windows_open_events", 0),
        ("retired_windows_opens", 0),
        ("retained_windows_opens", 0),
        ("sparse_preparations", 0),
        ("sparse_publications", 0),
        ("sparse_retention_anchors", 1),
        ("sparse_views", 1),
        ("staged_generations", 1),
        ("windows_mount_sessions", 1),
    ] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            n,
            "{table}"
        );
    }
    let staged: i64 = db
        .query_row(
            "SELECT coalesce(sum(length(data)),0) FROM staged_chunks",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(staged < 2 * 1024 * 1024);
    assert!(peak.0 < 4 * 1024 * 1024 && peak.1 < 8 * 1024 * 1024);
    eprintln!(
        "NATIVE_RETENTION done={done} peak_db={} peak_wal={} staged_bytes={staged}",
        peak.0, peak.1
    );
    std::fs::write(root.join("retention-cycle"), done.to_string()).unwrap();
}
pub async fn shared(root: &Path, s: &Server) {
    let c = config(root, s);
    let state = root.join("native-retention-state");
    let start = std::fs::read_to_string(root.join("retention-cycle"))
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let mut peak = (0, 0);
    for n in start..start + 10 {
        let node = fs.lookup(ROOT, NAME).await.unwrap();
        let file = fs.open_write(node.id).await.unwrap();
        let bytes = vec![(n % 251) as u8; 17];
        file.write(0, Arc::from(bytes.clone()), false, false)
            .await
            .unwrap();
        file.flush().await.unwrap();
        assert_eq!(file.read(0, 17).await.unwrap(), bytes);
        file.native_object().unwrap().close().await.unwrap();
        drop(file);
        let size = sizes(&state);
        peak = (peak.0.max(size.0), peak.1.max(size.1));
    }
    native_writer::stop(fs).await;
    inspect(root, start + 10, peak);
}
#[cfg(any(target_os = "linux", windows))]
pub async fn mounted(root: &Path, s: &Server) {
    use std::io::{Read, Seek, SeekFrom, Write};
    config(root, s);
    let start = std::fs::read_to_string(root.join("retention-cycle"))
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let state = root.join("native-retention-state");
    let mut mount = native_mount::Mount::start(root).await;
    let file = mount.path.join(NAME);
    let peak = native_mount::blocking(&mut mount, move || {
        let mut peak = (0, 0);
        for n in start..start + 10 {
            let mut handle = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&file)
                .unwrap();
            let bytes = vec![(n % 251) as u8; 17];
            handle.write_all(&bytes).unwrap();
            handle.sync_all().unwrap();
            handle.seek(SeekFrom::Start(0)).unwrap();
            let mut read = vec![0; 17];
            handle.read_exact(&mut read).unwrap();
            assert_eq!(read, bytes);
            drop(handle);
            assert_eq!(std::fs::read(&file).unwrap(), bytes);
            let size = sizes(&state);
            peak = (peak.0.max(size.0), peak.1.max(size.1));
        }
        peak
    })
    .await;
    mount.stop().await;
    inspect(root, start + 10, peak);
}
