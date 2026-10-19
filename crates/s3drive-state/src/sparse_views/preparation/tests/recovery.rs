use super::*;
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn old_view(s: &mut State, b: &Base) -> View {
    let initial = super::super::super::Journal::new(s, super::super::super::Options::default())
        .unwrap()
        .begin([1; 32], binding(b.root.length()), b.root.length())
        .unwrap();
    super::super::super::Journal::new(s, super::super::super::Options::default())
        .unwrap()
        .write([2; 32], initial.id(), 0, b"old")
        .unwrap()
}
fn changed() -> Vec<u8> {
    let mut bytes = pattern(0, C);
    bytes[..3].copy_from_slice(b"old");
    bytes
}
#[test]
fn mixed_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_MIXED_CRASH_DIRECTORY") else {
        return;
    };
    let kind = std::env::var("S3DRIVE_MIXED_CRASH_KIND").unwrap();
    let b = base(C, BTreeMap::from([(0, pattern(0, C))]));
    let mut s = State::open_managed(Path::new(&path)).unwrap();
    let v = old_view(&mut s, &b);
    let info = Journal::new(&mut s, Options::default())
        .unwrap()
        .begin(request(&v, &b, 9))
        .unwrap();
    if kind == "begin" {
        return;
    }
    let ready = Journal::new(&mut s, Options::default())
        .unwrap()
        .append(
            info.id().unwrap(),
            info.cursor(),
            Candidate::Bytes(changed()),
        )
        .unwrap();
    if kind == "append" {
        return;
    }
    Journal::new(&mut s, Options::default())
        .unwrap()
        .step(ready.id().unwrap(), ready.cursor())
        .unwrap();
}
#[test]
fn killed_preparation_owners_roll_back_every_uncommitted_boundary_and_replay_committed_results() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for (kind, points) in [
        (
            "begin",
            &["BlobStored", "PlanStored", "Validated", "Committed"][..],
        ),
        (
            "append",
            &[
                "BlobStored",
                "StateUpdated",
                "ObjectStored",
                "Validated",
                "Committed",
            ][..],
        ),
        (
            "step",
            &[
                "BlobStored",
                "StateUpdated",
                "ObjectStored",
                "Validated",
                "Committed",
            ][..],
        ),
    ] {
        for point in points {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("killed café 🦀");
            let ready = tmp.path().join("ready");
            let mut child = Child(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "sparse_views::preparation::tests::recovery::mixed_crash_child",
                        "--nocapture",
                    ])
                    .env("S3DRIVE_MIXED_CRASH_DIRECTORY", &path)
                    .env("S3DRIVE_MIXED_CRASH_KIND", kind)
                    .env("S3DRIVE_MIXED_CRASH_POINT", point)
                    .env("S3DRIVE_MIXED_CRASH_READY", &ready)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(30);
            while !ready.exists() {
                assert!(Instant::now() < deadline, "child missed {kind}/{point}");
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child exited before {kind}/{point}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                State::open_managed(&path).is_err(),
                "live child lost its private owner lock"
            );
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            let mut s = State::open_managed(&path).unwrap();
            let snapshot = ledger::inspect(&s.connection).unwrap();
            assert_eq!(
                snapshot.views.blocks.values().collect::<Vec<_>>(),
                vec![&b"old".to_vec()]
            );
            if kind == "begin" && *point != "Committed" {
                assert!(
                    snapshot.plans.is_empty()
                        && snapshot.objects.is_empty()
                        && snapshot.blobs.is_empty()
                );
            }
            let b = base(C, BTreeMap::from([(0, pattern(0, C))]));
            let v = old_view(&mut s, &b);
            let info = Journal::new(&mut s, Options::default())
                .unwrap()
                .begin(request(&v, &b, 9))
                .unwrap();
            let plan = info.id().unwrap();
            if kind == "append" {
                assert_eq!(
                    info.cursor().offset,
                    if *point == "Committed" { C } else { 0 }
                );
            }
            if kind == "step" {
                assert_eq!(info.root().unwrap().is_some(), *point == "Committed");
            }
            let ready = if info.cursor().offset == 0 {
                Journal::new(&mut s, Options::default())
                    .unwrap()
                    .append(plan, info.cursor(), Candidate::Bytes(changed()))
                    .unwrap()
            } else {
                info
            };
            // Retrying the exact old append recovers actual progress even after
            // a later page/root commit; it cannot create another candidate.
            let retry = Journal::new(&mut s, Options::default())
                .unwrap()
                .append(
                    plan,
                    Cursor {
                        offset: 0,
                        level: -1,
                        slot: 0,
                    },
                    Candidate::Bytes(changed()),
                )
                .unwrap();
            assert_eq!(retry.cursor(), ready.cursor());
            let done = finish(&mut s, ready);
            let Dependency::New { bytes, .. } = Journal::new(&mut s, Options::default())
                .unwrap()
                .dependency(plan, 0, 0)
                .unwrap()
            else {
                panic!()
            };
            done.root().unwrap().unwrap().verify_page(&bytes).unwrap();
            let Dependency::New { bytes, .. } = Journal::new(&mut s, Options::default())
                .unwrap()
                .dependency(plan, -1, 0)
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(bytes, changed());
        }
    }
}
