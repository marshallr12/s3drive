//! Fresh-store linked profile: 300 real saves across 30 independent owners.
//! Each ten-save mount sequence retains the existing 120-second syscall bound.
use super::*;
use s3drive_fs::{ROOT, ReadOnlyFs};
const NAME: &str = "retained 日本語 🦀.bin";
fn shape(root: &Path) -> (u64, usize) {
    if root.join("retention-large-fixture").exists() {
        (4, 65536)
    } else {
        (1, 17)
    }
}
fn name(index: u64, files: u64) -> String {
    if files == 1 {
        NAME.into()
    } else {
        format!("{NAME}.{index}")
    }
}
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
    assert_eq!(expected_puts(1000), 1997);
    assert_eq!(expected_puts(300) + 3, 602);
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
    let (files, _) = shape(root);
    for index in 0..files {
        let file = fs
            .create_file(ROOT, &name(index, files), |a, b| a == b, true)
            .await
            .unwrap();
        drop(file);
    }
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
pub async fn seed_large(c: &s3drive_core::Config, root: &Path, s: &Server) {
    std::fs::write(root.join("retention-large-fixture"), b"four-files-64KiB").unwrap();
    seed(c, root, s).await;
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
    let files = i64::try_from(shape(root).0.min(done)).unwrap();
    for (table, n) in [
        ("windows_open_journal", 0),
        ("windows_open_events", 0),
        ("retired_windows_opens", 0),
        ("retained_windows_opens", 0),
        ("sparse_preparations", 0),
        ("sparse_publications", 0),
        ("sparse_retention_anchors", files),
        ("sparse_views", files),
        ("staged_generations", files),
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
    let (files, size) = shape(root);
    for n in start..start + count {
        let node = phase!(
            "lookup",
            fs.lookup(ROOT, &name(n % files, files)).await.unwrap()
        );
        let file = phase!("open", fs.open_write(node.id).await.unwrap());
        if n >= files {
            assert_eq!(
                phase!("previous read", file.read(0, size).await.unwrap()),
                vec![((n - files) % 251) as u8; size],
                "persisted bytes before overwrite"
            );
        }
        let bytes = vec![(n % 251) as u8; size];
        phase!(
            "write",
            file.write(0, Arc::from(bytes.clone()), false, false)
                .await
                .unwrap()
        );
        phase!("flush", file.flush().await.unwrap());
        assert_eq!(
            phase!("verification read", file.read(0, size).await.unwrap()),
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
// Endpoint samples belong to the owned mount daemon, not this fixture process.
// Linux units are CLK_TCK; Windows TotalProcessorTime ticks are 100 ns.
#[cfg(any(target_os = "linux", windows))]
async fn sample_daemon(mount: &mut native_mount::Mount) -> std::result::Result<u64, String> {
    let pid = mount.live_process_id()?;
    let result = daemon_cpu(pid).await;
    if mount.live_process_id()? != pid {
        return Err("owned daemon PID changed".into());
    }
    result
}
#[cfg(any(target_os = "linux", windows))]
async fn daemon_cpu(pid: u32) -> std::result::Result<u64, String> {
    #[cfg(target_os = "linux")]
    {
        let stat =
            std::fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|e| e.to_string())?;
        let (_, tail) = stat.rsplit_once(')').ok_or("missing process comm")?;
        let fields = tail.split_whitespace().collect::<Vec<_>>();
        let parse = |index: usize| -> std::result::Result<u64, String> {
            fields
                .get(index)
                .ok_or("missing CPU field")?
                .parse::<u64>()
                .map_err(|e| e.to_string())
        };
        parse(11)?
            .checked_add(parse(12)?)
            .ok_or_else(|| "CPU counter overflow".into())
    }
    #[cfg(windows)]
    {
        use tokio::io::AsyncReadExt;
        let mut command = tokio::process::Command::new("powershell.exe");
        command
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "$ErrorActionPreference='Stop'; (Get-Process -Id {pid}).TotalProcessorTime.Ticks"
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let mut stdout = child.stdout.take().expect("piped sampler output");
        let mut bytes = Vec::new();
        let outcome = tokio::time::timeout(Duration::from_secs(10), async {
            (&mut stdout).take(1024).read_to_end(&mut bytes).await?;
            child.wait().await
        })
        .await;
        let status = match outcome {
            Ok(result) => result.map_err(|e| e.to_string())?,
            Err(_) => {
                let _ = tokio::time::timeout(Duration::from_secs(5), child.kill()).await;
                let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
                return Err("owned CPU sampler timed out".into());
            }
        };
        if !status.success() {
            return Err(format!("owned CPU sampler exited {status}"));
        }
        std::str::from_utf8(&bytes)
            .map_err(|e| e.to_string())?
            .trim()
            .parse::<u64>()
            .map_err(|e| e.to_string())
    }
}
#[cfg(any(target_os = "linux", windows))]
pub async fn mounted(root: &Path, s: &Server) {
    mounted_inner(root, s, false).await;
}
#[cfg(any(target_os = "linux", windows))]
pub async fn mounted_diagnostic(root: &Path, s: &Server) {
    mounted_inner(root, s, true).await;
}
#[cfg(any(target_os = "linux", windows))]
async fn mounted_inner(root: &Path, s: &Server, diagnostic: bool) {
    use std::io::{Read, Seek, SeekFrom, Write};
    config(root, s);
    let start = std::fs::read_to_string(root.join("retention-cycle"))
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let state = root.join("native-retention-state");
    let total = std::time::Instant::now();
    let mut mount = native_mount::Mount::start(root).await;
    let startup = total.elapsed();
    let pid = mount.live_process_id().unwrap();
    let sample_start = std::time::Instant::now();
    let before = if diagnostic {
        Some(sample_daemon(&mut mount).await)
    } else {
        None
    };
    let first_sample = sample_start.elapsed();
    let (files, size) = shape(root);
    let mount_path = mount.path.clone();
    let (peak, phases, syscall_wall) = native_mount::blocking(&mut mount, move || {
        let mut peak = (0, 0);
        let mut phases = PhaseStats::default();
        let sequence = std::time::Instant::now();
        macro_rules! phase {
            ($name:literal, $work:expr) => {{
                let time = std::time::Instant::now();
                let value = $work;
                if diagnostic {
                    phases.record($name, time.elapsed(), None, None);
                }
                value
            }};
        }
        for n in start..start + 10 {
            let file = mount_path.join(name(n % files, files));
            let mut handle = phase!(
                "open",
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&file)
                    .unwrap()
            );
            if n >= files {
                let mut previous = vec![0; size];
                phase!("previous read", handle.read_exact(&mut previous).unwrap());
                assert_eq!(
                    previous,
                    vec![((n - files) % 251) as u8; size],
                    "persisted bytes before overwrite"
                );
                phase!("seek", handle.seek(SeekFrom::Start(0)).unwrap());
            }
            let bytes = vec![(n % 251) as u8; size];
            phase!("write", handle.write_all(&bytes).unwrap());
            phase!("sync_all", handle.sync_all().unwrap());
            phase!("seek", handle.seek(SeekFrom::Start(0)).unwrap());
            let mut read = vec![0; size];
            phase!("verification read", handle.read_exact(&mut read).unwrap());
            assert_eq!(read, bytes);
            phase!("handle drop return", drop(handle));
            assert_eq!(
                phase!("reopen read and drop", std::fs::read(&file).unwrap()),
                bytes
            );
            let size = sizes(&state);
            peak = (peak.0.max(size.0), peak.1.max(size.1));
        }
        (peak, phases, sequence.elapsed())
    })
    .await;
    let sample_start = std::time::Instant::now();
    let after = if diagnostic {
        Some(sample_daemon(&mut mount).await)
    } else {
        None
    };
    let second_sample = sample_start.elapsed();
    let stop_start = std::time::Instant::now();
    mount.stop().await;
    let shutdown = stop_start.elapsed();
    let owner_total = total.elapsed();
    if diagnostic {
        for (phase, (calls, micros, _)) in phases.0 {
            eprintln!(
                "NATIVE_MOUNT_PHASE start={start} phase={phase} calls={calls} wall_us={micros}"
            );
        }
        let cpu = before
            .as_ref()
            .and_then(|v| v.as_ref().ok())
            .zip(after.as_ref().and_then(|v| v.as_ref().ok()))
            .and_then(|(a, b)| b.checked_sub(*a));
        let unit = if cfg!(windows) { "100ns" } else { "CLK_TCK" };
        eprintln!(
            "NATIVE_MOUNT_OWNER start={start} saves=10 pid={pid} syscall_us={} startup_us={} shutdown_us={} sample_us={} daemon_cpu_start={before:?} daemon_cpu_end={after:?} daemon_cpu_delta={cpu:?} cpu_unit={unit} total_us={}",
            syscall_wall.as_micros(),
            startup.as_micros(),
            shutdown.as_micros(),
            (first_sample + second_sample).as_micros(),
            owner_total.as_micros()
        );
    }
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
    let (files, size) = shape(root);
    for index in 0..files {
        let node = fs.lookup(ROOT, &name(index, files)).await.unwrap();
        let file = fs.open(node.id).await.unwrap();
        let last = completed - 1 - ((completed - 1 - index) % files);
        assert_eq!(
            file.read(0, size).await.unwrap(),
            vec![(last % 251) as u8; size]
        );
        file.native_object().unwrap().close().await.unwrap();
        drop(file);
    }
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
    final_mounted_at(root, s, 300).await;
}
#[cfg(any(target_os = "linux", windows))]
pub async fn final_mounted_diagnostic(root: &Path, s: &Server) {
    final_mounted_at(root, s, 20).await;
}
#[cfg(any(target_os = "linux", windows))]
async fn final_mounted_at(root: &Path, s: &Server, completed: u64) {
    config(root, s);
    let mut mount = native_mount::Mount::start(root).await;
    let (files, size) = shape(root);
    let path = mount.path.clone();
    native_mount::blocking(&mut mount, move || {
        for index in 0..files {
            let last = completed - 1 - ((completed - 1 - index) % files);
            assert_eq!(
                std::fs::read(path.join(name(index, files))).unwrap(),
                vec![(last % 251) as u8; size]
            );
        }
    })
    .await;
    mount.stop().await;
    inspect(root, completed, sizes(&root.join("native-retention-state")));
}

#[cfg(any(target_os = "linux", windows))]
pub async fn final_mounted_soak(root: &Path, s: &Server) {
    final_mounted_at(root, s, 1000).await;
}
