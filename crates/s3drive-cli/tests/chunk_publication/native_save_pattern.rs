//! Diagnostic shared-engine probe of an application save pattern: many small
//! sequential writes to one open file, optionally flushed after each write.
//! Reports wall time and private database growth; it accepts no OS behavior.
use super::*;
use s3drive_fs::{ROOT, ReadOnlyFs};

/// Reads a positive integer diagnostic setting, falling back to its default.
fn setting(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Combined size of the private database and its write-ahead log.
fn database_bytes(root: &Path) -> u64 {
    ["metadata.sqlite3", "metadata.sqlite3-wal"]
        .iter()
        .filter_map(|n| std::fs::metadata(root.join("native-state").join(n)).ok())
        .map(|m| m.len())
        .sum()
}

/// Writes `writes` blocks of `block` bytes, flushing every `flush_every`
/// writes (0 = never), then closes and reports each phase.
pub async fn probe(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let (writes, block) = (setting("SAVE_WRITES", 128), setting("SAVE_BLOCK", 4096));
    let flush_every = setting("SAVE_FLUSH_EVERY", 0);
    let c = native_lifecycle::configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let file = fs
        .create_file(ROOT, "save pattern.bin", |a, b| a == b, true)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    for i in 0..writes {
        let body: Arc<[u8]> = Arc::from(vec![(i % 251) as u8; block]);
        let call = std::time::Instant::now();
        file.write((i * block) as u64, body, false, false)
            .await
            .unwrap_or_else(|e| panic!("write {i} failed: {e:?}"));
        let wrote = call.elapsed();
        let call = std::time::Instant::now();
        if flush_every != 0 && (i + 1) % flush_every == 0 {
            file.flush()
                .await
                .unwrap_or_else(|e| panic!("flush {i} failed: {e:?}"));
        }
        if i % 16 == 0 || i + 1 == writes {
            eprintln!(
                "SAVE_PATTERN write={i} write_time={wrote:?} flush_time={:?} total={:?} db={}",
                call.elapsed(),
                started.elapsed(),
                database_bytes(root)
            );
        }
    }
    let call = std::time::Instant::now();
    file.native_object().unwrap().close().await.unwrap();
    eprintln!(
        "SAVE_PATTERN close={:?} total={:?} db={}",
        call.elapsed(),
        started.elapsed(),
        database_bytes(root)
    );
    drop(file);
    native_writer::stop(fs).await;
}
