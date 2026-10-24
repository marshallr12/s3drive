//! Fresh-store linked profile: 300 real saves across 30 independent owners.
//! Each ten-save mount sequence retains the existing 120-second syscall bound.
use super::*;
use s3drive_fs::{ROOT, ReadOnlyFs};
const NAME: &str = "retained 日本語 🦀.bin";
// The seed publishes one empty manifest page. Each save publishes a new page,
// but all-zero payloads (cycles 0 and 251) are structural holes, not data PUTs.
pub fn expected_puts(completed: u64) -> usize {
    1 + (0..completed)
        .map(|cycle| 1 + usize::from(cycle % 251 != 0))
        .sum::<usize>()
}

#[test]
fn native_retention_put_schedule_counts_zero_payloads_without_skipping_saves() {
    assert_eq!(expected_puts(0), 1);
    assert_eq!(expected_puts(1), 2);
    assert_eq!(expected_puts(251), 502);
    assert_eq!(expected_puts(252), 503);
    assert_eq!(expected_puts(300), 599);
}
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
// Process CPU ticks cover all threads, but not the loopback fixture/authority
// subprocesses. The diagnostic job reports CLK_TCK separately on Linux.
fn cpu_ticks() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let (_, tail) = stat.rsplit_once(')')?;
        let fields = tail.split_whitespace().collect::<Vec<_>>();
        let user = fields.get(11)?.parse::<u64>().ok()?;
        let system = fields.get(12)?.parse::<u64>().ok()?;
        user.checked_add(system)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
#[derive(Default)]
struct PhaseStats(std::collections::BTreeMap<&'static str, (u64, u128, Option<u64>)>);
impl PhaseStats {
    fn record(
        &mut self,
        name: &'static str,
        elapsed: std::time::Duration,
        before: Option<u64>,
        after: Option<u64>,
    ) {
        let ticks = before
            .zip(after)
            .and_then(|(before, after)| after.checked_sub(before));
        let entry = self.0.entry(name).or_insert((0, 0, Some(0)));
        entry.0 += 1;
        entry.1 += elapsed.as_micros();
        entry.2 = entry.2.zip(ticks).and_then(|(a, b)| a.checked_add(b));
    }
    fn report(&self, start: u64, count: u64, total: std::time::Duration) {
        for (phase, (calls, micros, ticks)) in &self.0 {
            eprintln!(
                "NATIVE_SAVE_PHASE start={start} phase={phase} calls={calls} wall_us={micros} process_cpu_ticks={ticks:?}"
            );
        }
        eprintln!(
            "NATIVE_SAVE_OWNER start={start} saves={count} wall_us={}",
            total.as_micros()
        );
    }
}
pub async fn shared(root: &Path, s: &Server) {
    shared_count(root, s, 10).await;
}
pub async fn profile_warmup(root: &Path, s: &Server) {
    shared_count(root, s, 1).await;
}
pub async fn profile_saves(root: &Path, s: &Server) {
    shared_count(root, s, 2).await;
}
async fn shared_count(root: &Path, s: &Server, count: u64) {
    let mut phases = PhaseStats::default();
    let total = std::time::Instant::now();
    macro_rules! phase {
        ($name:literal, $work:expr) => {{
            let cpu = cpu_ticks();
            let start = std::time::Instant::now();
            let result = $work;
            let elapsed = start.elapsed();
            phases.record($name, elapsed, cpu, cpu_ticks());
            result
        }};
    }
    let c = config(root, s);
    let state = root.join("native-retention-state");
    let start = std::fs::read_to_string(root.join("retention-cycle"))
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let fs = phase!("construct", ReadOnlyFs::new(&c).await.unwrap());
    let mut peak = (0, 0);
    for n in start..start + count {
        let node = phase!("lookup", fs.lookup(ROOT, NAME).await.unwrap());
        let file = phase!("open", fs.open_write(node.id).await.unwrap());
        if n > 0 {
            assert_eq!(
                phase!("previous read", file.read(0, 17).await.unwrap()),
                vec![((n - 1) % 251) as u8; 17],
                "persisted bytes before overwrite"
            );
        }
        let bytes = vec![(n % 251) as u8; 17];
        phase!(
            "write",
            file.write(0, Arc::from(bytes.clone()), false, false)
                .await
                .unwrap()
        );
        phase!("flush", file.flush().await.unwrap());
        assert_eq!(
            phase!("verification read", file.read(0, 17).await.unwrap()),
            bytes
        );
        phase!("actual Close", {
            file.native_object().unwrap().close().await.unwrap();
            drop(file);
        });
        let size = phase!("size probe", sizes(&state));
        peak = (peak.0.max(size.0), peak.1.max(size.1));
    }
    phase!("shutdown", native_writer::stop(fs).await);
    phase!("bounds", inspect(root, start + count, peak));
    phases.report(start, count, total.elapsed());
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
            if n > 0 {
                let mut previous = vec![0; 17];
                handle.read_exact(&mut previous).unwrap();
                assert_eq!(
                    previous,
                    vec![((n - 1) % 251) as u8; 17],
                    "persisted bytes before overwrite"
                );
                handle.seek(SeekFrom::Start(0)).unwrap();
            }
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

pub async fn final_shared(root: &Path, s: &Server) {
    final_shared_at(root, s, 300).await;
}
pub async fn final_diagnostic(root: &Path, s: &Server) {
    final_shared_at(root, s, 20).await;
}
pub async fn final_profile(root: &Path, s: &Server) {
    final_shared_at(root, s, 3).await;
}
async fn final_shared_at(root: &Path, s: &Server, completed: u64) {
    let started = std::time::Instant::now();
    let cpu = cpu_ticks();
    let c = config(root, s);
    let fs = ReadOnlyFs::new(&c).await.unwrap();
    let node = fs.lookup(ROOT, NAME).await.unwrap();
    let file = fs.open(node.id).await.unwrap();
    assert_eq!(
        file.read(0, 17).await.unwrap(),
        vec![((completed - 1) % 251) as u8; 17]
    );
    file.native_object().unwrap().close().await.unwrap();
    drop(file);
    native_writer::stop(fs).await;
    inspect(root, completed, sizes(&root.join("native-retention-state")));
    eprintln!(
        "NATIVE_SAVE_COLD completed={completed} wall_us={} process_cpu_ticks={:?}",
        started.elapsed().as_micros(),
        cpu.zip(cpu_ticks()).and_then(|(a, b)| b.checked_sub(a))
    );
}
#[cfg(any(target_os = "linux", windows))]
pub async fn final_mounted(root: &Path, s: &Server) {
    config(root, s);
    let mut mount = native_mount::Mount::start(root).await;
    let path = mount.path.join(NAME);
    native_mount::blocking(&mut mount, move || {
        assert_eq!(std::fs::read(path).unwrap(), vec![(299 % 251) as u8; 17])
    })
    .await;
    mount.stop().await;
    inspect(root, 300, sizes(&root.join("native-retention-state")));
}
