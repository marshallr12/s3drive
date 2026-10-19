use super::*;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn adoption_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_ADOPTION_CRASH_DIRECTORY") else {
        return;
    };
    let ready = std::env::var_os("S3DRIVE_ADOPTION_CRASH_READY").unwrap();
    let point = std::env::var("S3DRIVE_ADOPTION_CRASH_POINT").unwrap();
    let mut s = State::open_managed(Path::new(&path)).unwrap();
    let v = initial(&mut s);
    j(&mut s)
        .adopt_inner(token(&v, 8), v.id(), binding(8, 0), |at| {
            if format!("{at:?}") == point {
                let tmp = std::path::PathBuf::from(&ready).with_extension("tmp");
                std::fs::write(&tmp, b"ready")?;
                std::fs::rename(tmp, &ready)?;
                let _ = std::io::stdin().read(&mut [0u8]);
            }
            Ok(())
        })
        .unwrap();
}
#[test]
fn killed_adoption_owners_keep_old_head_or_exact_commit_and_never_lose_old_read_intent() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [
        "ViewStored",
        "OperationStored",
        "HeadUpdated",
        "Validated",
        "Committed",
    ] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("state café 🦀");
        let ready = t.path().join("ready");
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "sparse_views::adoption::tests::crash::adoption_crash_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_ADOPTION_CRASH_DIRECTORY", &path)
                .env("S3DRIVE_ADOPTION_CRASH_READY", &ready)
                .env("S3DRIVE_ADOPTION_CRASH_POINT", point)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let until = Instant::now() + Duration::from_secs(30);
        while !ready.exists() {
            assert!(Instant::now() < until, "did not reach {point}");
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "exited before {point}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(State::open_managed(&path).is_err());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut s = State::open_managed(&path).unwrap();
        let b = binding(7, 17);
        let v = j(&mut s)
            .begin_scoped(ScopedOperation::new(&b, [1; 32]).unwrap(), b, 17)
            .unwrap();
        let op = token(&v, 8);
        let known = j(&mut s).recover_adoption(op, v.id()).unwrap();
        assert_eq!(known.is_some(), point == "Committed");
        let head = j(&mut s).current_head(v.binding()).unwrap();
        assert_eq!(head.id(), known.as_ref().unwrap_or(&v).id());
        assert_eq!(
            j(&mut s).read(v.id(), 0, 17).unwrap(),
            vec![Piece::Base {
                offset: 0,
                length: 17
            }]
        );
        let next = adopted(
            j(&mut s)
                .adopt_clean_intent(op, v.id(), binding(8, 0))
                .unwrap(),
        );
        assert_eq!(j(&mut s).current_head(v.binding()).unwrap(), next);
        assert_eq!(j(&mut s).recover_adoption(op, v.id()).unwrap(), Some(next));
    }
}
