use super::*;
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
#[test]
fn patch_crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_PATCH_CRASH_DIRECTORY") else {
        return;
    };
    let ready = std::env::var_os("S3DRIVE_PATCH_CRASH_READY").unwrap();
    let point = std::env::var("S3DRIVE_PATCH_CRASH_POINT").unwrap();
    let mut s = State::open_managed(Path::new(&path)).unwrap();
    let mut j = Journal::new(&mut s, Options::default()).unwrap();
    let root = j.begin([7; 32], binding(2, 0), 0).unwrap();
    let old = j.write([8; 32], root.id(), 0, b"old").unwrap();
    j.write_inner([9; 32], old.id(), 0, b"new-data", |at| {
        if format!("{at:?}") == point {
            let marker = std::path::PathBuf::from(&ready).with_extension("tmp");
            std::fs::write(&marker, b"ready")?;
            std::fs::rename(marker, &ready)?;
            let _ = std::io::stdin().read(&mut [0u8]);
        }
        Ok(())
    })
    .unwrap();
}
#[test]
fn killed_patch_owners_roll_back_each_uncommitted_boundary_and_recover_the_exact_committed_result()
{
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in [
        "BlockStored",
        "ViewStored",
        "OperationStored",
        "HeadUpdated",
        "Validated",
        "Committed",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state café 🦀");
        let ready = tmp.path().join("ready");
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "sparse_views::tests::recovery::patch_crash_child",
                    "--nocapture",
                ])
                .env("S3DRIVE_PATCH_CRASH_DIRECTORY", &path)
                .env("S3DRIVE_PATCH_CRASH_READY", &ready)
                .env("S3DRIVE_PATCH_CRASH_POINT", point)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "child did not reach {point}");
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before {point}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            State::open_managed(&path).is_err(),
            "live owner lost private lock"
        );
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let mut s = State::open_managed(&path).unwrap();
        let counts = TABLES
            .iter()
            .map(|t| {
                s.connection
                    .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| {
                        r.get::<_, u32>(0)
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            counts,
            if point == "Committed" {
                vec![1, 1, 3, 3, 2]
            } else {
                vec![1, 1, 2, 2, 1]
            },
            "{point}"
        );
        let mut j = Journal::new(&mut s, Options::default()).unwrap();
        let root = j.begin([7; 32], binding(2, 0), 0).unwrap();
        let old = j.write([8; 32], root.id(), 0, b"old").unwrap();
        assert_eq!(
            j.read(old.id(), 0, 3).unwrap(),
            vec![Piece::Local(b"old".to_vec())]
        );
        if point != "Committed" {
            assert_eq!(j.head(root.binding()).unwrap(), old);
        }
        let recovered = j.write([9; 32], old.id(), 0, b"new-data").unwrap();
        assert_eq!(j.head(root.binding()).unwrap(), recovered);
        assert_eq!(
            j.read(recovered.id(), 0, 8).unwrap(),
            vec![Piece::Local(b"new-data".to_vec())]
        );
        let newer = j.resize([10; 32], recovered.id(), 3).unwrap();
        assert_eq!(
            j.write([9; 32], old.id(), 0, b"new-data").unwrap(),
            recovered
        );
        assert_eq!(j.head(root.binding()).unwrap(), newer);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn racing_worker_mutations_keep_one_head_and_replay_old_results_after_reopen() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("worker café 🦀");
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let a = storage.sparse_views(Options::default()).unwrap();
        let b = a.clone();
        let root = a.begin([7; 32], binding(2, 0), 0).await.unwrap();
        let (left, right) = tokio::join!(
            a.write([8; 32], root.id(), 0, Arc::from(b"left".as_slice())),
            b.write([9; 32], root.id(), 0, Arc::from(b"right".as_slice()))
        );
        let (op, data, winner) = match (left, right) {
            (Ok(v), Err(e)) => {
                assert_eq!(e.downcast_ref::<Failure>(), Some(&Failure::Conflict));
                ([8; 32], b"left".as_slice(), v)
            }
            (Err(e), Ok(v)) => {
                assert_eq!(e.downcast_ref::<Failure>(), Some(&Failure::Conflict));
                ([9; 32], b"right".as_slice(), v)
            }
            _ => panic!("competing writes did not produce exactly one winner"),
        };
        let head = a.resize([10; 32], winner.id(), 9).await.unwrap();
        assert_eq!(
            b.write(op, root.id(), 0, Arc::from(data)).await.unwrap(),
            winner
        );
        assert_eq!(a.head(root.binding().clone()).await.unwrap(), head);
        storage.close().await.unwrap();
        assert!(a.read(winner.id(), 0, 8).await.is_err());
        let storage = crate::worker::Storage::open(
            &path,
            crate::cache::Options::default(),
            crate::journal::Options::default(),
        )
        .await
        .unwrap();
        let narrow = Options::new(1)
            .unwrap()
            .with_limits(Limits {
                views: 1,
                operations: 1,
                extents: 1,
                metadata_bytes: 1,
                dirty_bytes: 1,
            })
            .unwrap();
        let a = storage.sparse_views(narrow).unwrap();
        assert_eq!(
            a.write(op, root.id(), 0, Arc::from(data)).await.unwrap(),
            winner
        );
        assert_eq!(
            a.read(winner.id(), 0, 8).await.unwrap(),
            vec![Piece::Local(data.to_vec())]
        );
        assert_eq!(a.head(root.binding().clone()).await.unwrap(), head);
        storage.close().await.unwrap();
    })
    .await
    .expect("bounded worker mutation/recovery check");
}
fn original_blobs(db: &Connection, root: &View) -> Vec<Vec<u8>> {
    vec![
        db.query_row(
            "SELECT binding FROM sparse_view_files WHERE target=?1",
            [root.binding().target().unwrap().as_slice()],
            |r| r.get(0),
        )
        .unwrap(),
        db.query_row(
            "SELECT record FROM sparse_views WHERE view_id=?1",
            [root.id().as_slice()],
            |r| r.get(0),
        )
        .unwrap(),
        db.query_row(
            "SELECT request FROM sparse_view_operations WHERE operation=?1",
            [root.operation().as_slice()],
            |r| r.get(0),
        )
        .unwrap(),
    ]
}
#[test]
fn additive_schema_thirty_one_keeps_populated_thirty_literal_blobs_and_migration_collisions_roll_back()
 {
    for collision in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let root = Journal::new(&mut s, Options::default())
            .unwrap()
            .begin([7; 32], binding(2, 17), MAX_FILE_BYTES)
            .unwrap();
        for (table, column) in [
            ("sparse_view_files", "binding"),
            ("sparse_views", "record"),
            ("sparse_view_operations", "request"),
        ] {
            s.connection
                .execute_batch(&format!(
                    "UPDATE {table} SET {column}=CAST(x'20'||{column}||x'0a' AS BLOB)"
                ))
                .unwrap();
        }
        let before = original_blobs(&s.connection, &root);
        s.connection
            .execute_batch("DROP TABLE sparse_view_blocks; PRAGMA user_version=30")
            .unwrap();
        if collision {
            s.connection.execute_batch("CREATE TABLE sparse_view_blocks(keep TEXT); INSERT INTO sparse_view_blocks VALUES('untouched')").unwrap();
        }
        drop(s);
        if collision {
            assert!(State::open_managed(&path).is_err());
            let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
            assert_eq!(
                db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
                    .unwrap(),
                30
            );
            let kept: String = db
                .query_row("SELECT keep FROM sparse_view_blocks", [], |r| r.get(0))
                .unwrap();
            assert_eq!(kept, "untouched");
            assert_eq!(original_blobs(&db, &root), before);
        } else {
            let mut s = State::open_managed(&path).unwrap();
            assert_eq!(s.info().unwrap().schema_version, 31);
            assert_eq!(original_blobs(&s.connection, &root), before);
            let mut j = Journal::new(&mut s, Options::default()).unwrap();
            assert_eq!(
                j.begin([7; 32], binding(2, 17), MAX_FILE_BYTES).unwrap(),
                root
            );
            let child = j.write([8; 32], root.id(), 0, b"new").unwrap();
            assert_eq!(
                j.read(child.id(), 0, 3).unwrap(),
                vec![Piece::Local(b"new".to_vec())]
            );
            assert_eq!(original_blobs(&s.connection, &root), before);
        }
    }
}
