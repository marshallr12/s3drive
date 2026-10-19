use super::*;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn pin_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_PIN_CRASH_DIR") else {
        return;
    };
    let path = std::path::Path::new(&path);
    let mode = std::env::var("S3DRIVE_PIN_CRASH_MODE").unwrap();
    let point = std::env::var("S3DRIVE_PIN_CRASH_POINT").unwrap();
    let ready = std::env::var_os("S3DRIVE_PIN_CRASH_READY").unwrap();
    let (mut s, v) = open(path);
    std::fs::write(
        path.parent().unwrap().join("baseline.json"),
        serde_json::to_vec(&rows(&s)).unwrap(),
    )
    .unwrap();
    let old_owner = s.incarnation.clone().unwrap();
    std::fs::write(path.parent().unwrap().join("prior-owner.txt"), &old_owner).unwrap();
    let pin = if mode == "acquire" {
        None
    } else {
        Some(j(&mut s).pin(v.id()).unwrap())
    };
    if mode == "rotate" {
        drop(s);
        s = State::open_managed(path).unwrap();
    }
    let hook = |at: Boundary| -> Result<()> {
        if format!("{at:?}") == point {
            let ready = std::path::PathBuf::from(&ready);
            let tmp = ready.with_extension("tmp");
            std::fs::write(&tmp, b"ready").unwrap();
            std::fs::rename(tmp, ready).unwrap();
            std::io::stdin().read_exact(&mut [0u8]).unwrap();
        }
        Ok(())
    };
    if mode == "release" {
        release_inner(&mut s, pin.as_ref().unwrap(), hook).unwrap();
    } else {
        acquire_inner(&mut s, v.id(), hook).unwrap();
    }
    panic!("pin operation missed checkpoint");
}
#[test]
fn killed_pin_owners_keep_old_projection_or_complete_commit_and_fence_late_releases() {
    for mode in ["acquire", "release", "rotate"] {
        for point in [
            "SessionCleared",
            "PinStored",
            "RoleStored",
            "Validated",
            "Committed",
        ] {
            if point == "SessionCleared" && mode != "rotate" {
                continue;
            }
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("pin crash café 🦀");
            let ready = temp.path().join("ready");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "sparse_views::pins::tests::crash::pin_crash_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_PIN_CRASH_DIR", &path)
                .env("S3DRIVE_PIN_CRASH_MODE", mode)
                .env("S3DRIVE_PIN_CRASH_POINT", point)
                .env("S3DRIVE_PIN_CRASH_READY", &ready)
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
                    panic!("{mode}/{point} exited {status}: {err}");
                }
                if start.elapsed() > Duration::from_secs(30) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("pin checkpoint timeout {mode}/{point}");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(State::open_managed(&path).is_err());
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut s = State::open_managed(&path).unwrap();
            let ledger = load(&s.connection).unwrap();
            let expected = if mode == "acquire" {
                usize::from(point == "Committed")
            } else if mode == "release" {
                usize::from(point != "Committed")
            } else {
                1
            };
            assert_eq!(ledger.pins.len(), expected, "{mode}/{point}");
            let previous =
                std::fs::read_to_string(path.parent().unwrap().join("prior-owner.txt")).unwrap();
            if mode == "rotate" {
                assert_eq!(
                    ledger.role.as_ref().unwrap().owner == previous,
                    point != "Committed"
                );
            }
            let before: Vec<Vec<String>> = serde_json::from_slice(
                &std::fs::read(path.parent().unwrap().join("baseline.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(rows(&s), before);
            let source = inspect(&s.connection).unwrap();
            let file = source.files.values().next().unwrap();
            let view = View(source.record(file.head).unwrap().clone());
            let old = Pin::new(previous, 1, view.id()).unwrap();
            let fresh = j(&mut s).pin(view.id()).unwrap();
            assert_ne!(old.id(), fresh.id());
            assert!(j(&mut s).release_pin(&old).is_err());
            assert!(j(&mut s).read_pinned(&fresh, 0, 32).is_ok());
            assert_eq!(rows(&s), before);
        }
    }
}
