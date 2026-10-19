use super::*;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn mode_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_SCOPED_MODE_DIR") else {
        return;
    };
    let mut state = State::open_managed(std::path::Path::new(&path)).unwrap();
    let point = std::env::var("S3DRIVE_SCOPED_MODE_POINT").unwrap();
    let ready = std::env::var_os("S3DRIVE_SCOPED_MODE_READY").unwrap();
    crate::sparse_views::scoped::enable_inner(&mut state, |at| {
        if at == point {
            std::fs::write(&ready, b"ready").unwrap();
            std::io::stdin().read_exact(&mut [0u8]).unwrap();
        }
        Ok(())
    })
    .unwrap();
    panic!("mode checkpoint missed");
}
#[test]
fn killed_mode_owner_rolls_back_or_retains_permanent_mode_and_retries_idempotently() {
    for point in ["Stored", "Validated", "Committed"] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("owner");
        let ready = temp.path().join("ready");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sparse_views::tests::scoped::mode_crash::mode_crash_child",
                "--nocapture",
            ])
            .env("S3DRIVE_SCOPED_MODE_DIR", &path)
            .env("S3DRIVE_SCOPED_MODE_POINT", point)
            .env("S3DRIVE_SCOPED_MODE_READY", &ready)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while !ready.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                let mut err = String::new();
                child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut err)
                    .unwrap();
                panic!("mode child exited {status}: {err}");
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("mode timeout {point}");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(State::open_managed(&path).is_err());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        let mut state = State::open_managed(&path).unwrap();
        assert_eq!(
            crate::sparse_views::scoped::enabled(&state.connection).unwrap(),
            point == "Committed"
        );
        Journal::new(&mut state, Options::default())
            .unwrap()
            .enable_scoped()
            .unwrap();
        Journal::new(&mut state, Options::default())
            .unwrap()
            .enable_scoped()
            .unwrap();
        assert!(crate::sparse_views::scoped::enabled(&state.connection).unwrap());
        assert!(rows(&state.connection).iter().all(Vec::is_empty));
    }
}
