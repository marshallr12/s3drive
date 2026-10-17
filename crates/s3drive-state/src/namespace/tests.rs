use super::*;
use std::{
    io::{BufRead, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn request(v: &Volume, id: u64) -> Apply {
    Apply {
        volume: v.id.clone(),
        operation: format!("{id:032x}"),
        expected_sequence: v.sequence,
        mutation: Mutation::Create {
            parent: ROOT.into(),
            name: format!("file-{id} 🦀"),
            node_kind: Kind::EmptyFile,
        },
    }
}
#[test]
fn migration_preserves_local_rows_and_refuses_role_mixing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    s.register_source(&[1; 32], &"a".repeat(64)).unwrap();
    s.connection.execute_batch("DROP TABLE namespace_operations; DROP TABLE namespace_nodes; DROP TABLE namespace_volume; PRAGMA user_version=7;").unwrap();
    drop(s);
    let mut s = State::open_managed(&path).unwrap();
    assert_eq!(s.info().unwrap().schema_version, 8);
    assert_eq!(s.namespace_initialize(), Err(Error::Invalid));
    let count: i64 = s
        .connection
        .query_row("SELECT count(*) FROM source_bindings", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    drop(s);
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let v = s.namespace_initialize().unwrap();
    assert!(s.register_source(&[1; 32], &"a".repeat(64)).is_err());
    assert!(crate::journal::Journal::new(&mut s, crate::journal::Options::default()).is_err());
    assert_eq!(s.namespace_volume().unwrap(), v);
}
#[test]
fn migration_failure_keeps_schema_seven_and_original_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state");
    let mut s = State::open_managed(&path).unwrap();
    s.register_source(&[1; 32], &"a".repeat(64)).unwrap();
    s.connection.execute_batch("DROP TABLE namespace_operations; DROP TABLE namespace_nodes; DROP TABLE namespace_volume; CREATE TABLE namespace_nodes(keep TEXT); INSERT INTO namespace_nodes VALUES('untouched'); PRAGMA user_version=7;").unwrap();
    drop(s);
    assert!(State::open_managed(&path).is_err());
    let db = Connection::open(path.join("metadata.sqlite3")).unwrap();
    assert_eq!(
        db.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        7
    );
    let original: String = db
        .query_row("SELECT keep FROM namespace_nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(original, "untouched");
    assert!(db.prepare("SELECT * FROM namespace_volume").is_err());
}
#[test]
fn failed_result_insert_rolls_back_name_and_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let v = s.namespace_initialize().unwrap();
    let r = request(&v, 1);
    s.connection.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON namespace_operations BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END;").unwrap();
    assert_eq!(s.namespace_apply(&r), Err(Error::Internal));
    assert_eq!(s.namespace_volume().unwrap(), v);
    assert!(s.namespace_list(&v.id, ROOT).unwrap().children.is_empty());
    assert_eq!(s.namespace_operation(&v.id, &r.operation), Ok(None));
    s.connection
        .execute_batch("DROP TRIGGER fail_receipt")
        .unwrap();
    let receipt = s.namespace_apply(&r).unwrap();
    assert_eq!(receipt.sequence, 1);
    // Refused deletion leaves the exact prior node and all names intact.
    let dir_request = Apply {
        volume: v.id.clone(),
        expected_sequence: 1,
        operation: format!("{:032x}", 2),
        mutation: Mutation::Create {
            parent: ROOT.into(),
            name: "folder".into(),
            node_kind: Kind::Directory,
        },
    };
    let folder = s.namespace_apply(&dir_request).unwrap();
    let mv = Apply {
        volume: v.id.clone(),
        expected_sequence: 2,
        operation: format!("{:032x}", 3),
        mutation: Mutation::Move {
            node: receipt.node.id.clone(),
            parent: folder.node.id.clone(),
            name: "moved".into(),
        },
    };
    s.connection.execute_batch("CREATE TRIGGER fail_sequence BEFORE UPDATE ON namespace_volume BEGIN SELECT RAISE(ABORT,'injected after move'); END;").unwrap();
    assert_eq!(s.namespace_apply(&mv), Err(Error::Internal));
    assert_eq!(
        s.namespace_node(&v.id, &receipt.node.id).unwrap(),
        receipt.node
    );
    assert_eq!(s.namespace_volume().unwrap().sequence, 2);
    assert!(
        s.namespace_list(&v.id, &folder.node.id)
            .unwrap()
            .children
            .is_empty()
    );
    assert_eq!(s.namespace_operation(&v.id, &mv.operation), Ok(None));
}
#[test]
fn sqlite_full_never_commits_partial_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let mut v = s.namespace_initialize().unwrap();
    s.connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let pages: i64 = s
        .connection
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    s.connection
        .pragma_update(None, "max_page_count", pages)
        .unwrap();
    let mut full = false;
    for id in 1..200 {
        let r = request(&v, id);
        match s.namespace_apply(&r) {
            Ok(receipt) => v.sequence = receipt.sequence,
            Err(Error::Internal) => {
                full = true;
                assert_eq!(s.namespace_volume().unwrap(), v);
                assert_eq!(s.namespace_operation(&v.id, &r.operation), Ok(None));
                assert_eq!(s.namespace_node(&v.id, &r.operation), Err(Error::NotFound));
                assert_eq!(
                    s.namespace_list(&v.id, ROOT).unwrap().children.len() as u64,
                    v.sequence
                );
                break;
            }
            e => panic!("unexpected: {e:?}"),
        }
    }
    assert!(full, "test must reach real SQLite full condition");
}
#[test]
fn retained_limits_and_listing_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = State::open_managed(&dir.path().join("authority")).unwrap();
    let v = s.namespace_initialize().unwrap();
    {
        let tx = s.connection.transaction().unwrap();
        for id in 1..MAX_NODES {
            tx.execute(
                "INSERT INTO namespace_nodes VALUES(?1,?2,?3,'empty-file',0,0)",
                params![format!("{id:032x}"), ROOT, format!("name{id}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    assert_eq!(s.namespace_list(&v.id, ROOT), Err(Error::Limit));
    let r = request(&v, 99999);
    assert_eq!(s.namespace_apply(&r), Err(Error::Limit));
    assert_eq!(
        s.namespace_operation(&v.id, &r.operation),
        Ok(Some(Err(Error::Limit)))
    );
    assert_eq!(s.namespace_volume().unwrap(), v);
    {
        let tx = s.connection.transaction().unwrap();
        for id in 1..MAX_OPERATIONS {
            tx.execute(
                "INSERT INTO namespace_operations VALUES(?1,x'00',x'00')",
                [format!("{:032x}", 100_000 + id)],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let r = request(&v, 99998);
    assert_eq!(s.namespace_apply(&r), Err(Error::Limit));
    assert_eq!(s.namespace_operation(&v.id, &r.operation), Ok(None));
}
#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("S3DRIVE_AUTHORITY_CRASH_CHILD") else {
        return;
    };
    let mut s = State::open_managed(std::path::Path::new(&path)).unwrap();
    let v = s.namespace_initialize().unwrap();
    s.namespace_apply(&request(&v, 77)).unwrap();
    println!("namespace-committed");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    panic!("parent should terminate child before graceful close");
}
#[test]
fn forced_process_death_retains_metadata_and_operation_result() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("authority");
    let v = {
        let mut s = State::open_managed(&path).unwrap();
        s.namespace_initialize().unwrap()
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "namespace::tests::crash_child", "--nocapture"])
        .env("S3DRIVE_AUTHORITY_CRASH_CHILD", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            let line = line.unwrap();
            if line == "namespace-committed" {
                send.send(()).unwrap();
                return;
            }
        }
    });
    let signaled = receive.recv_timeout(Duration::from_secs(10));
    child.kill().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "child termination timed out");
        std::thread::yield_now();
    }
    reader.join().unwrap();
    signaled.unwrap();
    let mut s = State::open_managed(&path).unwrap();
    let r = request(&v, 77);
    let receipt = s.namespace_apply(&r).unwrap();
    assert_eq!(receipt.sequence, 1);
    assert_eq!(
        s.namespace_operation(&v.id, &r.operation),
        Ok(Some(Ok(receipt)))
    );
    assert_eq!(s.namespace_list(&v.id, ROOT).unwrap().children.len(), 1);
}
