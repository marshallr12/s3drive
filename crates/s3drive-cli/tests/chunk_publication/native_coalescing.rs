//! Shared-engine checks of the in-memory write buffer on the version 8 native
//! path: a save made of many small writes, every commit point, two handles on
//! one file and the idle timer. Loopback backend only; this is not an
//! OS-callback, Office or provider test.
use super::*;
use s3drive_fs::{OpenFile, ROOT, ReadOnlyFs};

const BLOCK: usize = 4096;
const MEBIBYTE: usize = 1024 * 1024;

/// Deterministic reference bytes for a file of `len` bytes.
fn body(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

/// Creates a new empty file open for writing.
async fn create(fs: &ReadOnlyFs, name: &str) -> OpenFile {
    fs.create_file(ROOT, name, |a, b| a == b, true)
        .await
        .unwrap()
}

/// Writes `bytes` at `offset` and requires the whole write to be accepted.
async fn put(file: &OpenFile, offset: usize, bytes: &[u8]) {
    let written = file.write(offset as u64, Arc::from(bytes), false, false);
    assert_eq!(written.await.unwrap(), bytes.len(), "write at {offset}");
}

/// Reads the whole file in bounded pieces.
async fn contents(file: &OpenFile) -> Vec<u8> {
    let size = file.current_metadata().unwrap().size as usize;
    let mut out = Vec::with_capacity(size);
    while out.len() < size {
        let wanted = (size - out.len()).min(MEBIBYTE);
        let piece = file.read(out.len() as u64, wanted).await.unwrap();
        assert!(!piece.is_empty(), "short read at {}", out.len());
        out.extend(piece);
    }
    out
}

/// Closes a handle the way a driver's Close callback does.
async fn close(file: OpenFile) {
    file.native_object().unwrap().close().await.unwrap();
}

/// Row count of one private-database table, read on a separate connection.
fn rows(root: &Path, table: &str) -> i64 {
    let path = root.join("native-state").join("metadata.sqlite3");
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
    let db = rusqlite::Connection::open_with_flags(path, flags).unwrap();
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

/// 300 sequential 4 KiB writes, which failed at write 127 before coalescing,
/// then a header rewrite. Checks sizes while buffered and bytes afterwards.
async fn many_small_writes(fs: &ReadOnlyFs, root: &Path) -> Vec<u8> {
    let file = create(fs, "save pattern 日本語.docx").await;
    let before = rows(root, "sparse_views");
    let mut expected = body(300 * BLOCK);
    for (i, block) in expected.chunks(BLOCK).enumerate() {
        put(&file, i * BLOCK, block).await;
    }
    let size = expected.len() as u64;
    assert_eq!(file.current_metadata().unwrap().size, size);
    assert_eq!(fs.metadata(file.node.id).await.unwrap().unwrap().size, size);
    let views = rows(root, "sparse_views") - before;
    assert!(views <= 4, "300 small writes journaled {views} views");
    expected[..8].copy_from_slice(b"REWRITE!");
    put(&file, 0, b"REWRITE!").await;
    assert_eq!(contents(&file).await, expected);
    close(file).await;
    eprintln!("COALESCING small_writes=300 journal_views={views} read=exact");
    expected
}

/// A handle closed straight after its writes, with no read or flush between.
async fn close_is_a_commit_point(fs: &ReadOnlyFs) {
    let file = create(fs, "closed at once.bin").await;
    let id = file.node.id;
    put(&file, 0, b"first ").await;
    put(&file, 6, b"second").await;
    close(file).await;
    let file = fs.open(id).await.unwrap();
    assert_eq!(contents(&file).await, b"first second");
    close(file).await;
}

/// Two handles on one file: each sees the other's buffered writes, in order,
/// and truncate, append and constrained writes use the buffered size.
async fn handles_share_one_order(fs: &ReadOnlyFs) {
    let first = create(fs, "two handles.bin").await;
    let second = fs.open_write(first.node.id).await.unwrap();
    put(&first, 0, b"aaaa").await;
    assert_eq!(second.read(0, 8).await.unwrap(), b"aaaa");
    put(&second, 2, b"bb").await;
    put(&first, 4, b"cc").await;
    assert_eq!(contents(&second).await, b"aabbcc");
    first.truncate(3).await.unwrap();
    let append = second.write(0, Arc::from(&b"Z"[..]), true, false);
    assert_eq!(append.await.unwrap(), 1, "append");
    let clipped = first.write(3, Arc::from(&b"XY"[..]), false, true);
    assert_eq!(clipped.await.unwrap(), 1, "constrained at the end");
    assert_eq!(second.current_metadata().unwrap().size, 4);
    assert_eq!(contents(&first).await, b"aabX");
    close(first).await;
    close(second).await;
}

/// A buffered write past the end of the file leaves a zero-filled gap.
async fn gap_reads_as_zeros(fs: &ReadOnlyFs) {
    let file = create(fs, "gap.bin").await;
    put(&file, 10, b"tail").await;
    assert_eq!(file.current_metadata().unwrap().size, 14);
    assert_eq!(contents(&file).await, [&[0u8; 10][..], b"tail"].concat());
    close(file).await;
}

/// A write of at least the buffer capacity is journaled at once, after the
/// small write before it; flush then publishes buffered bytes with the rest.
async fn large_write_and_flush(fs: &ReadOnlyFs) -> Vec<u8> {
    let file = create(fs, "large then flushed.bin").await;
    let expected = body(BLOCK + MEBIBYTE + BLOCK);
    put(&file, 0, &expected[..BLOCK]).await;
    put(&file, BLOCK, &expected[BLOCK..BLOCK + MEBIBYTE]).await;
    put(&file, BLOCK + MEBIBYTE, &expected[BLOCK + MEBIBYTE..]).await;
    file.flush().await.unwrap();
    assert_eq!(contents(&file).await, expected);
    close(file).await;
    expected
}

/// The idle timer journals a quiet run without any further call.
async fn idle_timer_journals(fs: &ReadOnlyFs, root: &Path) {
    let file = create(fs, "idle.bin").await;
    let before = rows(root, "sparse_view_blocks");
    put(&file, 0, b"left alone").await;
    let held = rows(root, "sparse_view_blocks");
    assert_eq!(held, before, "a small write reached the journal at once");
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let after = rows(root, "sparse_view_blocks");
    assert!(after > before, "the idle run was not journaled");
    close(file).await;
    eprintln!("COALESCING idle_timer=journaled blocks_before={before} after={after}");
}

/// A write that extends a run while its idle timer waits for the order locks
/// must not end the timer. The flush of a second file holds the mount-wide
/// lock, the second write queues behind it, and the timer queues behind that
/// write. The window is hit only if the flush outlasts the idle period; when
/// it does not, this is the same check as `idle_timer_journals`.
async fn idle_timer_follows_an_extended_run(fs: &ReadOnlyFs, root: &Path) {
    let blocker = create(fs, "blocker.bin").await;
    put(&blocker, 0, &body(MEBIBYTE / 4)).await;
    let file = create(fs, "extended.bin").await;
    put(&file, 0, b"first ").await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    let started = std::time::Instant::now();
    let late = async {
        tokio::time::sleep(Duration::from_millis(150)).await;
        put(&file, 6, b"second").await;
    };
    let (flushed, ()) = tokio::join!(blocker.flush(), late);
    flushed.unwrap();
    let held = started.elapsed().as_millis();
    let before = rows(root, "sparse_view_blocks");
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let after = rows(root, "sparse_view_blocks");
    assert!(after > before, "the extended run lost its idle timer");
    close(file).await;
    close(blocker).await;
    eprintln!("COALESCING extended_run=journaled lock_held_ms={held}");
}

/// Every journaled patch block in the private database.
fn blocks(root: &Path) -> Vec<Vec<u8>> {
    let path = root.join("native-state").join("metadata.sqlite3");
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
    let db = rusqlite::Connection::open_with_flags(path, flags).unwrap();
    let mut query = db.prepare("SELECT data FROM sparse_view_blocks").unwrap();
    let found = query.query_map([], |r| r.get(0)).unwrap();
    found.collect::<std::result::Result<_, _>>().unwrap()
}

/// Whether `needle` occurs in any block.
fn holds(blocks: &[Vec<u8>], needle: &[u8]) -> bool {
    let within = |block: &Vec<u8>| block.windows(needle.len()).any(|w| w == needle);
    blocks.iter().any(within)
}

/// An abrupt owner stop keeps what reached the journal and loses only the run
/// still in memory. The retained dirty epoch is then protected as before.
async fn abrupt_stop_keeps_only_journaled_bytes(c: &s3drive_core::Config, root: &Path) {
    let fs = ReadOnlyFs::new(c).await.unwrap();
    let file = create(&fs, "abrupt stop.bin").await;
    put(&file, 0, b"journaled by a read;").await;
    assert_eq!(file.read(0, 64).await.unwrap(), b"journaled by a read;");
    put(&file, 20, b" still only in memory").await;
    let cache = fs.cache_control().unwrap();
    drop((file, fs));
    cache.close().await.unwrap();
    drop(cache);
    let kept = blocks(root);
    assert!(
        holds(&kept, b"journaled by a read;"),
        "journaled bytes lost"
    );
    assert!(!holds(&kept, b"still only in memory"), "run was journaled");
    let adopted = ReadOnlyFs::new(c).await.is_ok();
    assert!(!adopted, "cold start adopted a retained dirty epoch");
    eprintln!("COALESCING abrupt_stop journaled=kept buffered=lost cold_start=refused");
}

/// Reads one published file after a cold restart of the engine.
async fn cold(fs: &ReadOnlyFs, name: &str) -> Vec<u8> {
    let node = fs.lookup(ROOT, name).await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    let bytes = contents(&file).await;
    close(file).await;
    bytes
}

/// Runs every buffer check on one engine, then verifies the published bytes
/// after a clean stop and cold restart.
pub async fn flow(config: &s3drive_core::Config, root: &Path, s: &Server) {
    let c = native_lifecycle::configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let saved = many_small_writes(&fs, root).await;
    close_is_a_commit_point(&fs).await;
    handles_share_one_order(&fs).await;
    gap_reads_as_zeros(&fs).await;
    let large = large_write_and_flush(&fs).await;
    idle_timer_journals(&fs, root).await;
    idle_timer_follows_an_extended_run(&fs, root).await;
    native_writer::stop(fs).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert_eq!(cold(&fs, "save pattern 日本語.docx").await, saved);
    assert_eq!(cold(&fs, "closed at once.bin").await, b"first second");
    assert_eq!(cold(&fs, "two handles.bin").await, b"aabX");
    assert_eq!(cold(&fs, "large then flushed.bin").await, large);
    assert_eq!(cold(&fs, "idle.bin").await, b"left alone");
    assert_eq!(cold(&fs, "extended.bin").await, b"first second");
    native_writer::stop(fs).await;
    eprintln!("COALESCING close=commit handles=ordered gap=zeros flush=published cold_read=exact");
    abrupt_stop_keeps_only_journaled_bytes(&c, root).await;
}

/// Writes single bytes two apart until the journal refuses one. Offsets that
/// never touch make every write commit the run before it. Returns how many
/// writes were accepted and the refusal.
async fn fill_until_refused(file: &OpenFile) -> (usize, s3drive_fs::Error) {
    for accepted in 0..1024 {
        let write = file.write((accepted * 2) as u64, Arc::from(&b"x"[..]), false, false);
        match write.await {
            Ok(1) => {}
            Ok(count) => panic!("write {accepted} accepted {count} bytes"),
            Err(failure) => return (accepted, failure),
        }
    }
    panic!("the journal never refused a run");
}

/// File content after `count` single-byte writes two apart.
fn scattered(count: usize) -> Vec<u8> {
    let byte = |i: usize| if i.is_multiple_of(2) { b'x' } else { 0 };
    (0..count * 2 - 1).map(byte).collect()
}

/// Drives one open file to a journal limit. The refused run must be reported
/// by the write and the read that try to commit it, and Close must save it:
/// Close publishes what the journal holds, which makes room, and journals
/// the run before the handle goes away. Nothing may be lost.
pub async fn refusal(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use s3drive_fs::Error::NoSpace;
    let c = native_lifecycle::configuration(config, root, s).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let file = create(&fs, "refused run.bin").await;
    let id = file.node.id;
    let (accepted, failure) = fill_until_refused(&file).await;
    assert!(matches!(failure, NoSpace), "refusal was {failure:?}");
    // The last accepted byte is still buffered and counts toward the size.
    let buffered = (accepted * 2 - 1) as u64;
    assert_eq!(file.current_metadata().unwrap().size, buffered);
    assert!(matches!(file.read(0, 1).await, Err(NoSpace)), "read");
    close(file).await;
    assert_eq!(fs.metadata(id).await.unwrap().unwrap().size, buffered);
    let file = fs.open_write(id).await.unwrap();
    let mut expected = scattered(accepted);
    assert_eq!(contents(&file).await, expected, "every byte after Close");
    expected[..5].copy_from_slice(b"again");
    put(&file, 0, b"again").await;
    close(file).await;
    native_writer::stop(fs).await;
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    assert_eq!(cold(&fs, "refused run.bin").await, expected);
    native_writer::stop(fs).await;
    eprintln!(
        "COALESCING refusal accepted={accepted} reported=write,read close=saved_all reopened=usable"
    );
}
