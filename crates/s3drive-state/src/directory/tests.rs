use super::*;
use crate::State;
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn ownership_is_exclusive_and_released_on_drop() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("state café");
    let mut first = State::open_managed(&path).unwrap();
    first.register_source(&[7; 32], &"a".repeat(64)).unwrap();
    for contender in [&path, &path.join(".")] {
        let error = State::open_managed(contender)
            .err()
            .expect("duplicate owner was allowed");
        assert!(error.to_string().contains("already in use"), "{error}");
    }
    drop(first);
    let mut reopened = State::open_managed(&path).unwrap();
    assert!(reopened.register_source(&[7; 32], &"b".repeat(64)).is_err());
    assert_eq!(reopened.info().unwrap().journal_mode, "wal");
    assert!(path.join(LOCK).is_file());
}

#[test]
fn refuses_unmanaged_entries_and_parent_traversal() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("state");
    drop(State::open_managed(&path).unwrap());
    fs::write(path.join("personal.txt"), b"keep").unwrap();
    assert!(State::open_managed(&path).is_err());
    assert_eq!(fs::read(path.join("personal.txt")).unwrap(), b"keep");
    assert!(State::open_managed(&path.join("../other")).is_err());
    assert!(!temporary.path().join("other").exists());
    assert!(State::open_managed(&temporary.path().join("missing/child")).is_err());
    assert!(!temporary.path().join("missing").exists());
}

#[test]
fn hard_links_to_database_or_lock_are_refused() {
    for name in [
        DATABASE,
        LOCK,
        "metadata.sqlite3-wal",
        "metadata.sqlite3-shm",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("state");
        drop(State::open_managed(&path).unwrap());
        let outside = temporary.path().join("outside");
        if path.join(name).exists() {
            fs::remove_file(path.join(name)).unwrap();
        }
        fs::write(&outside, b"preserve outside bytes").unwrap();
        let permissions = fs::metadata(&outside).unwrap().permissions();
        fs::hard_link(&outside, path.join(name)).unwrap();
        let error = State::open_managed(&path)
            .err()
            .expect("hard link was allowed");
        assert!(
            error.to_string().contains("hard links"),
            "wrong refusal for {name}: {error}"
        );
        assert_eq!(fs::read(&outside).unwrap(), b"preserve outside bytes");
        assert_eq!(fs::metadata(outside).unwrap().permissions(), permissions);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_permissions_and_symlinks_are_checked_before_sqlite() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("state");
    drop(State::open_managed(&path).unwrap());
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o700);
    for name in [DATABASE, LOCK] {
        assert_eq!(fs::metadata(path.join(name)).unwrap().mode() & 0o777, 0o600);
    }
    // Legacy bootstrap files are tightened only after type/owner/link checks.
    fs::set_permissions(path.join(DATABASE), fs::Permissions::from_mode(0o644)).unwrap();
    drop(State::open_managed(&path).unwrap());
    assert_eq!(
        fs::metadata(path.join(DATABASE)).unwrap().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(State::open_managed(&path).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();

    let link = temporary.path().join("linked-parent");
    symlink(&path, &link).unwrap();
    assert!(State::open_managed(&link).is_err());
    assert!(State::open_managed(&link.join("child")).is_err());
    assert!(!path.join("child").exists());
    for name in [
        DATABASE,
        LOCK,
        "metadata.sqlite3-wal",
        "metadata.sqlite3-shm",
    ] {
        let target = path.join(name);
        if target.exists() {
            fs::remove_file(&target).unwrap();
        }
        let outside = temporary.path().join("outside");
        fs::write(&outside, b"untouched").unwrap();
        symlink(&outside, &target).unwrap();
        assert!(State::open_managed(&path).is_err());
        assert_eq!(fs::read(outside).unwrap(), b"untouched");
        fs::remove_file(target).unwrap();
    }
}

#[cfg(windows)]
#[test]
fn windows_rejects_broad_acl_and_junctions() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("state");
    drop(State::open_managed(&path).unwrap());
    let command = Command::new("icacls.exe")
        .arg(path.join(DATABASE))
        .args(["/grant", "*S-1-1-0:R"])
        .output()
        .unwrap();
    assert!(command.status.success(), "icacls failed");
    assert!(State::open_managed(&path).is_err());

    let target = temporary.path().join("junction-target");
    drop(State::open_managed(&target).unwrap());
    let link = temporary.path().join("junction");
    let command = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "New-Item -ItemType Junction -Path $env:S3DRIVE_TEST_LINK -Target $env:S3DRIVE_TEST_TARGET -ErrorAction Stop | Out-Null"])
        .env("S3DRIVE_TEST_LINK", &link).env("S3DRIVE_TEST_TARGET", &target)
        .output().unwrap();
    assert!(command.status.success(), "junction fixture creation failed");
    assert!(State::open_managed(&link).is_err());
    assert!(State::open_managed(&link.join("child")).is_err());
    assert!(!target.join("child").exists());
    fs::remove_dir(link).unwrap();
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn child_process_owns_state() {
    let Some(path) = std::env::var_os("S3DRIVE_TEST_OWNED_DIRECTORY") else {
        return;
    };
    let ready = std::env::var_os("S3DRIVE_TEST_OWNER_READY").unwrap();
    let mut state = State::open_managed(Path::new(&path)).unwrap();
    state.register_source(&[9; 32], &"a".repeat(64)).unwrap();
    let mut marker = File::create(ready).unwrap();
    marker.write_all(b"ready").unwrap();
    marker.sync_all().unwrap();
    // The parent controls this pipe. Kill tests bypass all Rust destructors.
    let mut byte = [0];
    let _ = std::io::stdin().read(&mut byte);
}

#[test]
fn cross_process_lock_survives_contenders_and_releases_after_kill() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("state");
    let ready = temporary.path().join("ready");
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "directory::tests::child_process_owns_state",
            "--nocapture",
        ])
        .env("S3DRIVE_TEST_OWNED_DIRECTORY", &path)
        .env("S3DRIVE_TEST_OWNER_READY", &ready)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready.exists() {
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "owner exited before readiness"
        );
        assert!(Instant::now() < deadline, "owner did not become ready");
        std::thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..3 {
        let error = State::open_managed(&path)
            .err()
            .expect("duplicate owner was allowed");
        assert!(error.to_string().contains("already in use"), "{error}");
    }
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let mut reopened = State::open_managed(&path).unwrap();
    assert!(reopened.register_source(&[9; 32], &"b".repeat(64)).is_err());
    assert!(path.join(LOCK).is_file());
}
