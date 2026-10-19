use super::*;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[test]
fn epoch_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_EPOCH_CRASH_DIR") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let f = fixture();
    let r = first(&mut s, &f);
    let r = commit(&mut s, r);
    if std::env::var("S3DRIVE_EPOCH_CRASH_HISTORY").as_deref() == Ok("1") {
        vj(&mut s).write([41; 32], r.input.view, 6, &[88]).unwrap();
    }
    j(&mut s)
        .acknowledge(
            [40; 32],
            r.input.plan,
            crate::sparse_views::Options::default(),
        )
        .unwrap();
    panic!("acknowledgment did not reach requested checkpoint");
}
#[test]
fn killed_epoch_owners_keep_exact_old_state_or_complete_ack_and_replay_same_result() {
    for history in [false, true] {
        for point in [
            "ViewStored",
            "OperationStored",
            "HeadUpdated",
            "RowStored",
            "RoleStored",
            "Validated",
            "Committed",
        ] {
            if history && matches!(point, "ViewStored" | "OperationStored" | "HeadUpdated") {
                continue;
            }
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("epoch crash café 🦀");
            let ready = temp.path().join("ready");
            let mut child=Command::new(std::env::current_exe().unwrap())
                .args(["--exact","sparse_views::preparation::publication::tests::settlement::crash::epoch_crash_child","--nocapture"])
                .env("S3DRIVE_EPOCH_CRASH_DIR",&path).env("S3DRIVE_EPOCH_CRASH_HISTORY",if history {"1"}else{"0"})
                .env("S3DRIVE_MIXED_ROOT_CRASH_KIND","acknowledge").env("S3DRIVE_MIXED_ROOT_CRASH_POINT",point)
                .env("S3DRIVE_MIXED_ROOT_CRASH_READY",&ready)
                .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap();
            let start = Instant::now();
            while !ready.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    use std::io::Read;
                    let mut err = String::new();
                    child
                        .stderr
                        .take()
                        .unwrap()
                        .read_to_string(&mut err)
                        .unwrap();
                    panic!("{history}/{point} child exited {status}: {err}");
                }
                if start.elapsed() > Duration::from_secs(30) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("checkpoint timeout {history}/{point}")
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                State::open_managed(&path).is_err(),
                "kernel owner lost before kill"
            );
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut s = State::open_managed(&path).unwrap();
            let f = fixture();
            let p = pj(&mut s)
                .operation([9; 32])
                .unwrap()
                .unwrap()
                .id()
                .unwrap();
            let r = j(&mut s).get(p).unwrap();
            assert_eq!(r.acknowledgment().is_some(), point == "Committed");
            let head = vj(&mut s).current_head(&f.binding).unwrap();
            if history {
                assert_eq!(head.0.operation, [41; 32]);
                assert_eq!(head.binding(), &f.binding)
            } else if point == "Committed" {
                assert_eq!(r.acknowledgment().unwrap().clean_view(), Some(head.id()));
            } else {
                assert_eq!(head.id(), r.input.view);
                assert_eq!(head.binding(), &f.binding);
            }
            let a = j(&mut s)
                .acknowledge([40; 32], p, crate::sparse_views::Options::default())
                .unwrap();
            assert_eq!(a.acknowledgment().clean_view().is_none(), history);
            let after = vj(&mut s).current_head(&f.binding).unwrap();
            assert_eq!(
                j(&mut s)
                    .acknowledge([40; 32], p, crate::sparse_views::Options::default())
                    .unwrap(),
                a
            );
            assert_eq!(vj(&mut s).current_head(&f.binding).unwrap(), after);
            assert!(vj(&mut s).view(r.input.view).is_ok());
        }
    }
}
