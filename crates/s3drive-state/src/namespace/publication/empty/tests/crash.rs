use super::*;
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn atomic_empty_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_EMPTY_CREATE_DIRECTORY") else {
        return;
    };
    let mut s = setup(Path::new(&path));
    let q = request(&s, 4);
    s.namespace_apply(&q).unwrap();
}
#[test]
fn killed_atomic_empty_owners_never_leave_half_a_node_or_history_and_recover_exact_commits() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [
        "NodeCreated",
        "HistoryStored",
        "SequenceUpdated",
        "OutcomeStored",
        "Committed",
    ] {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("authority café 🦀");
        let ready = t.path().join("ready");
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "namespace::publication::empty::tests::crash::atomic_empty_crash_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_EMPTY_CREATE_DIRECTORY", &path)
                .env("S3DRIVE_EMPTY_CREATE_POINT", point)
                .env("S3DRIVE_EMPTY_CREATE_READY", &ready)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "never reached {point}");
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before {point}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(State::open_managed(&path).is_err());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut s = State::open_managed(&path).unwrap();
        let mut q = request(&s, 4);
        q.expected_sequence = 1;
        let known = s.namespace_operation(&q.volume, &q.operation).unwrap();
        assert_eq!(known.is_some(), point == "Committed");
        let count: i64 = s
            .connection
            .query_row("SELECT count(*) FROM namespace_uploads", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, if point == "Committed" { 1 } else { 0 });
        if point != "Committed" {
            assert_eq!(
                s.namespace_node(&q.volume, &q.operation),
                Err(Error::NotFound)
            );
            assert_eq!(s.namespace_volume().unwrap().sequence, 1);
        }
        let r = s.namespace_apply(&q).unwrap();
        if let Some(original) = known {
            assert_eq!(original, Ok(r.clone()));
        }
        assert_eq!(
            history(&mut s, r.node.payload.as_ref().unwrap()).unwrap(),
            r.node.payload.clone().unwrap()
        );
        assert_eq!(s.namespace_apply(&q).unwrap(), r);
    }
}
