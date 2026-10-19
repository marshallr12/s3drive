use super::*;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn baseline(s: &mut State, f: &Fixture, kind: &str) -> Info {
    let info = prepare(s, f, false);
    let p = info.id().unwrap();
    if kind == "check" {
        return info;
    }
    j(s).check(p, 0, Some(&proof(f))).unwrap();
    physical(s, p);
    if kind == "begin" {
        return info;
    }
    let r = j(s)
        .begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
        .unwrap();
    if kind == "reserved" || kind == "reject" {
        return info;
    }
    let r = j(s).reserved(p, reserved(&r)).unwrap();
    if kind == "prepare" {
        return info;
    }
    j(s).prepare_publish(p, r.reservation().unwrap().sequence)
        .unwrap();
    info
}
fn action(s: &mut State, f: &Fixture, info: &Info, kind: &str) {
    let p = info.id().unwrap();
    match kind {
        "check" => j(s).check(p, 0, Some(&proof(f))).unwrap(),
        "begin" => {
            j(s).begin(p, f.node.clone(), f.store.clone(), 5, hex(&[10; 16]))
                .unwrap();
        }
        "reserved" => {
            let r = j(s).get(p).unwrap();
            j(s).reserved(p, reserved(&r)).unwrap();
        }
        "prepare" => {
            j(s).prepare_publish(p, 6).unwrap();
        }
        "published" => {
            let r = j(s).get(p).unwrap();
            j(s).published(p, published(&r)).unwrap();
        }
        "reject" => {
            let r = j(s).get(p).unwrap();
            j(s).reject(p, r.input().reserve().clone(), ns::Error::Conflict)
                .unwrap();
        }
        _ => panic!("unknown fixture action"),
    }
}
#[test]
fn root_crash_child() {
    let Ok(kind) = std::env::var("S3DRIVE_MIXED_ROOT_CRASH_KIND") else {
        return;
    };
    let path = std::path::PathBuf::from(std::env::var_os("S3DRIVE_MIXED_ROOT_CRASH_DIR").unwrap());
    let mut s = State::open_managed(&path).unwrap();
    let f = fixture();
    let info = baseline(&mut s, &f, &kind);
    action(&mut s, &f, &info, &kind);
    panic!("fixture action did not reach requested kill checkpoint");
}
#[test]
fn killed_root_owners_roll_back_every_boundary_and_replay_committed_exact_operations() {
    for kind in [
        "check",
        "begin",
        "reserved",
        "prepare",
        "published",
        "reject",
    ] {
        for point in ["RowStored", "RoleStored", "Validated", "Committed"] {
            let t = tempfile::tempdir().unwrap();
            let path = t.path().join("root crash café 🦀");
            let ready = t.path().join("ready");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "sparse_views::preparation::publication::tests::crash::root_crash_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_MIXED_ROOT_CRASH_KIND", kind)
                .env("S3DRIVE_MIXED_ROOT_CRASH_POINT", point)
                .env("S3DRIVE_MIXED_ROOT_CRASH_READY", &ready)
                .env("S3DRIVE_MIXED_ROOT_CRASH_DIR", &path)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let started = Instant::now();
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
                    panic!("{kind}/{point} child exited {status}: {err}");
                }
                if started.elapsed() > Duration::from_secs(30) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("{kind}/{point} checkpoint timed out");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                State::open_managed(&path).is_err(),
                "{kind}/{point} kernel ownership was lost before kill"
            );
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut s = State::open_managed(&path).unwrap();
            let f = fixture();
            let info = pj(&mut s).operation([9; 32]).unwrap().unwrap();
            let p = info.id().unwrap();
            let meta = ledger::inspect(&s.connection).unwrap();
            let committed = point == "Committed";
            match kind {
                "check" => assert_eq!(meta.checks.contains_key(&(p, 0)), committed),
                "begin" => assert_eq!(meta.records.contains_key(&p), committed),
                "reserved" => assert_eq!(meta.records[&p].reservation().is_some(), committed),
                "prepare" => assert_eq!(meta.records[&p].publication().is_some(), committed),
                "published" => assert_eq!(meta.records[&p].receipt().is_some(), committed),
                "reject" => assert_eq!(meta.records[&p].rejection().is_some(), committed),
                _ => unreachable!(),
            }
            action(&mut s, &f, &info, kind);
            if kind != "check" {
                let r = j(&mut s).get(p).unwrap();
                assert_eq!(r.input().plan(), p);
                assert_eq!(r.input().view(), info.view().id());
                assert_eq!(r.input().reserve().operation, hex(&[9; 16]));
                assert_eq!(r.input().publish_operation(), hex(&[10; 16]));
            }
            assert_eq!(
                crate::sparse_views::Journal::new(&mut s, crate::sparse_views::Options::default())
                    .unwrap()
                    .head(&f.binding)
                    .unwrap(),
                *info.view()
            );
        }
    }
}
