//! Real authority + SDK + private worker, no provider uploads or native writes.
use super::*;
use s3drive_managed::chunks::EditedFile;
use s3drive_state::{
    sparse_views::{Binding, Client, Options},
    worker::Storage,
};
use std::{path::Path, sync::Arc};
pub(super) async fn storage(path: &Path) -> (Storage, Client) {
    let s = Storage::open(
        path,
        s3drive_state::cache::Options::default(),
        s3drive_state::journal::Options::default(),
    )
    .await
    .unwrap();
    let c = s.sparse_views(Options::default()).unwrap();
    (s, c)
}
fn mixed() -> Vec<u8> {
    let mut expected = [pattern(127, C)[C as usize - 5..].to_vec(), pattern(128, 7)].concat();
    expected[3..9].copy_from_slice("café\0".as_bytes());
    expected
}
pub async fn flow(f: &mut Fixture, path: &Path) {
    let r = f.reader().await;
    failed(
        r.open_file(&id(10))
            .await
            .unwrap()
            .pinned_base()
            .sparse_binding(),
        ReadFailure::UnsupportedContent,
    );
    let tree = f
        .tree(
            10,
            130 * C,
            BTreeMap::from([
                (0, pattern(0, C)),
                (127, pattern(127, C)),
                (128, pattern(128, C)),
            ]),
        )
        .await;
    let base = r.open_file(&id(10)).await.unwrap().pinned_base();
    let binding = base.sparse_binding().unwrap();
    assert_eq!(binding.base_size(), 130 * C);
    let local_path = path.join("local café 🦀");
    let (s, c) = storage(&local_path).await;
    let initial = c.begin([1; 32], binding.clone(), 130 * C).await.unwrap();
    let patch = c
        .write(
            [2; 32],
            initial.id(),
            128 * C - 2,
            Arc::from("café\0".as_bytes()),
        )
        .await
        .unwrap();
    let edited = r.reopen_edited(c.clone(), patch.id()).await.unwrap();
    assert_eq!(edited.view(), &patch);
    assert_eq!(edited.read_range(128 * C - 5, 12).await.unwrap(), mixed());
    assert!(
        std::mem::size_of_val(&edited.read_range(0, 4)) <= 8192,
        "composition future exceeds callback stack budget"
    );
    // Forty interleaved base fragments in one chunk must fetch it once, not
    // reset data/traversal budgets or download forty complete chunks.
    let mut current = patch.clone();
    let mut expected = pattern(0, 80);
    for n in 0..40u8 {
        current = c
            .write(
                [n + 20; 32],
                current.id(),
                u64::from(n) * 2,
                Arc::from([255 - n].as_slice()),
            )
            .await
            .unwrap();
        expected[n as usize * 2] = 255 - n;
    }
    let fragmented = r.reopen_edited(c.clone(), current.id()).await.unwrap();
    f.s3.settle().await;
    let gets = data_gets(f);
    let served = f.s3.served.load(Ordering::SeqCst);
    assert_eq!(fragmented.read_range(0, 80).await.unwrap(), expected);
    f.s3.settle().await;
    assert_eq!(data_gets(f) - gets, 1);
    let page_bytes =
        f.s3.objects
            .lock()
            .unwrap()
            .get(&tree.pages[0])
            .unwrap()
            .bytes
            .len() as u64;
    assert_eq!(f.s3.served.load(Ordering::SeqCst) - served, C + page_bytes);
    let gets = data_gets(f);
    assert_eq!(fragmented.read_range(0, 80).await.unwrap(), expected);
    assert_eq!(
        data_gets(f) - gets,
        1,
        "request-local cache became a persistent authorization bypass"
    );
    let shrunk = c.resize([70; 32], current.id(), 0).await.unwrap();
    let zero = c.resize([71; 32], shrunk.id(), 17).await.unwrap();
    let z = r.reopen_edited(c.clone(), zero.id()).await.unwrap();
    let gets = data_gets(f);
    let requests = f.requests();
    let events = f.s3.events().len();
    assert_eq!(z.read_range(0, 17).await.unwrap(), vec![0; 17]);
    assert_eq!(data_gets(f), gets);
    assert_eq!(f.requests() - requests, 1);
    assert_eq!(
        f.s3.events().len() - events,
        1,
        "zero read did not authorize with root HEAD only"
    );
    let local = c
        .write([72; 32], zero.id(), 0, Arc::from(b"all-local".as_slice()))
        .await
        .unwrap();
    let local = c.resize([73; 32], local.id(), 9).await.unwrap();
    let l = r.reopen_edited(c.clone(), local.id()).await.unwrap();
    let gets = data_gets(f);
    assert_eq!(l.read_range(0, 9).await.unwrap(), b"all-local");
    assert_eq!(data_gets(f), gets);
    f.create(11, "maximum composed 🦀", 200, false).await;
    f.tree(11, 10 * C, data(0, 10)).await;
    let max_base = r.open_file(&id(11)).await.unwrap().pinned_base();
    let max_root = c
        .begin([80; 32], max_base.sparse_binding().unwrap(), 10 * C)
        .await
        .unwrap();
    let max_patch = c
        .write([81; 32], max_root.id(), 5, Arc::from([99u8].as_slice()))
        .await
        .unwrap();
    let max_file = r.reopen_edited(c.clone(), max_patch.id()).await.unwrap();
    f.s3.settle().await;
    let gets = data_gets(f);
    let served = f.s3.served.load(Ordering::SeqCst);
    let bytes = max_file.read_range(1, 8 * C as usize).await.unwrap();
    f.s3.settle().await;
    assert_eq!(data_gets(f) - gets, 9);
    assert_eq!(f.s3.served.load(Ordering::SeqCst) - served, 9 * C);
    assert!(bytes.iter().enumerate().all(|(i, b)| *b
        == if i == 4 {
            99
        } else {
            let at = i as u64 + 1;
            ((at / C * 17 + at % C) % 251) as u8
        }));
    let huge = c
        .resize(
            [82; 32],
            max_patch.id(),
            s3drive_state::sparse_views::MAX_FILE_BYTES,
        )
        .await
        .unwrap();
    let huge = r.reopen_edited(c.clone(), huge.id()).await.unwrap();
    let gets = data_gets(f);
    assert_eq!(
        huge.read_range(huge.size() - 4, 8).await.unwrap(),
        vec![0; 4]
    );
    assert_eq!(data_gets(f), gets);
    f.tree(10, 3, BTreeMap::from([(0, b"NEW".to_vec())])).await;
    f.change(
        180,
        Mutation::Move {
            node: id(10),
            parent: s3drive_authority::ROOT.into(),
            name: "renamed 🦀".into(),
        },
    )
    .await;
    f.change(181, Mutation::Remove { node: id(10) }).await;
    f.restart_authority(path).await;
    assert_eq!(edited.read_range(128 * C - 5, 12).await.unwrap(), mixed());
    assert_eq!(
        r.reopen_edited(c.clone(), patch.id())
            .await
            .unwrap()
            .read_range(128 * C - 5, 12)
            .await
            .unwrap(),
        mixed()
    );
    let before = f.s3.events().len();
    assert!(edited.read_range(u64::MAX, 1).await.unwrap().is_empty());
    assert!(edited.read_range(0, 0).await.unwrap().is_empty());
    failed(
        edited.read_range(0, 8 * C as usize + 1).await,
        ReadFailure::Bounds,
    );
    assert_eq!(f.s3.events().len(), before);
    let view_file = path.join("view.bin");
    std::fs::write(&view_file, patch.id()).unwrap();
    s.close().await.unwrap();
    assert!(edited.read_range(0, 1).await.is_err());
    let child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "edited_view_reopen_worker", "--nocapture"])
        .env("S3DRIVE_EDITED_WORKER_VIEW", view_file)
        .env("S3DRIVE_EDITED_WORKER_LOCAL", local_path)
        .env("S3DRIVE_EDITED_WORKER_ENDPOINT", f.endpoint())
        .env("S3DRIVE_EDITED_WORKER_VOLUME", &f.volume)
        .env(
            "S3DRIVE_EDITED_WORKER_SOURCE",
            serde_json::to_string(&f.source).unwrap(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    check_child(
        child,
        "independent edited view restart",
        Duration::from_secs(45),
    )
    .await;
}
pub async fn worker() {
    let Some(view) = std::env::var_os("S3DRIVE_EDITED_WORKER_VIEW") else {
        return;
    };
    let view: [u8; 32] = std::fs::read(view).unwrap().try_into().unwrap();
    let (s, c) = storage(Path::new(
        &std::env::var_os("S3DRIVE_EDITED_WORKER_LOCAL").unwrap(),
    ))
    .await;
    let source: s3drive_core::Source =
        serde_json::from_str(&std::env::var("S3DRIVE_EDITED_WORKER_SOURCE").unwrap()).unwrap();
    let wire = s3drive_authority::WireClient::loopback(
        &std::env::var("S3DRIVE_EDITED_WORKER_ENDPOINT").unwrap(),
        s3drive_authority::Token::new(TOKEN.into()).unwrap(),
    )
    .unwrap();
    let r = Reader::open(
        wire,
        std::env::var("S3DRIVE_EDITED_WORKER_VOLUME").unwrap(),
        vec![(id(200), S3Reader::from_source(&source).await.unwrap())],
    )
    .await
    .unwrap();
    let edited = r.reopen_edited(c, view).await.unwrap();
    assert_eq!(edited.read_range(128 * C - 5, 12).await.unwrap(), mixed());
    s.close().await.unwrap();
}
pub async fn faults(f: &Fixture, path: &Path) {
    let tree = f.tree(10, 2 * C, data(0, 2)).await;
    let r = f.reader().await;
    let base = r.open_file(&id(10)).await.unwrap().pinned_base();
    let b = base.sparse_binding().unwrap();
    for n in 0..5 {
        let wrong = Binding::new(
            b.base().unwrap(),
            if n == 0 { [99; 16] } else { *b.store() },
            if n == 1 { [99; 32] } else { *b.source() },
            if n == 2 {
                "f".repeat(64)
            } else {
                b.fingerprint().to_owned()
            },
            if n == 3 {
                "other/".into()
            } else {
                b.root().to_owned()
            },
            if n == 4 {
                b.base_size() + 1
            } else {
                b.base_size()
            },
        )
        .unwrap();
        let (s, c) = storage(&path.join(format!("wrong{n}"))).await;
        let v = c.begin([1; 32], wrong, 3).await.unwrap();
        let before = data_gets(f);
        failed(r.reopen_edited(c, v.id()).await, ReadFailure::Binding);
        assert_eq!(data_gets(f), before);
        s.close().await.unwrap();
    }
    let (s, c) = storage(&path.join("local")).await;
    let root = c.begin([1; 32], b, 2 * C).await.unwrap();
    let local = c
        .write([2; 32], root.id(), 0, Arc::from(b"XX".as_slice()))
        .await
        .unwrap();
    let zero = c.resize([3; 32], local.id(), 0).await.unwrap();
    let zero = c.resize([4; 32], zero.id(), 8).await.unwrap();
    let edited = r.reopen_edited(c.clone(), local.id()).await.unwrap();
    let z = r.reopen_edited(c.clone(), zero.id()).await.unwrap();
    for mode in 31..=37 {
        f.mode.store(mode, Ordering::SeqCst);
        let before = f.s3.events().len();
        assert!(z.read_range(0, 8).await.is_err(), "history mode{mode}");
        assert_eq!(
            f.s3.events().len(),
            before,
            "unbound history reached provider"
        );
    }
    f.mode.store(0, Ordering::SeqCst);
    for fault in [Fault::HeadDenied, Fault::Missing, Fault::BigHead] {
        f.s3.fault(Target::Exact(tree.root.clone()), fault);
        assert!(
            z.read_range(0, 8).await.is_err(),
            "zero root fault{fault:?}"
        );
        assert!(
            edited.read_range(0, 2).await.is_err(),
            "local root fault{fault:?}"
        );
        f.s3.clear();
    }
    let original =
        f.s3.objects
            .lock()
            .unwrap()
            .get(&tree.root)
            .unwrap()
            .clone();
    f.s3.objects
        .lock()
        .unwrap()
        .get_mut(&tree.root)
        .unwrap()
        .etag = "\"rival\"".into();
    failed(z.read_range(0, 8).await, ReadFailure::Integrity);
    failed(edited.read_range(0, 2).await, ReadFailure::Integrity);
    f.s3.objects
        .lock()
        .unwrap()
        .insert(tree.root.clone(), original);
    for fault in [
        Fault::GetDenied,
        Fault::BadEtag,
        Fault::Short,
        Fault::Extra,
        Fault::BadRange,
    ] {
        f.s3.fault(Target::Exact(tree.root.clone()), fault);
        assert!(
            r.reopen_edited(c.clone(), local.id()).await.is_err(),
            "root reopen fault{fault:?}"
        );
        f.s3.clear();
    }
    let original_data =
        f.s3.objects
            .lock()
            .unwrap()
            .get(&tree.data[0])
            .unwrap()
            .clone();
    *f.s3
        .objects
        .lock()
        .unwrap()
        .get_mut(&tree.data[0])
        .unwrap()
        .bytes
        .last_mut()
        .unwrap() ^= 1;
    let gets = data_gets(f);
    assert!(
        edited.read_range(0, 3).await.is_err(),
        "unread corrupt chunk tail escaped full-block verification"
    );
    assert_eq!(data_gets(f) - gets, 1);
    f.s3.objects
        .lock()
        .unwrap()
        .insert(tree.data[0].clone(), original_data);
    f.s3.fault(Target::Exact(tree.data[1].clone()), Fault::GetDenied);
    let gets = data_gets(f);
    assert!(edited.read_range(0, C as usize + 4).await.is_err());
    assert!(
        data_gets(f) >= gets + 2,
        "late data failure did not follow first verified chunk"
    );
    f.s3.clear();
    assert_eq!(c.head(root.binding().clone()).await.unwrap(), zero);
    assert_eq!(edited.read_range(0, 2).await.unwrap(), b"XX");
    let db = rusqlite::Connection::open(path.join("local/metadata.sqlite3")).unwrap();
    db.execute("UPDATE sparse_view_blocks SET data=?1", [b"YY".as_slice()])
        .unwrap();
    let before = f.s3.events().len();
    let error = edited.read_range(0, 1).await.unwrap_err();
    assert_eq!(
        error.downcast_ref::<s3drive_state::sparse_views::Failure>(),
        Some(&s3drive_state::sparse_views::Failure::Corrupt)
    );
    assert_eq!(
        f.s3.events().len(),
        before,
        "corrupt local cache caused network fallback"
    );
    let retained: Vec<u8> = db
        .query_row("SELECT data FROM sparse_view_blocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(retained, b"YY");
    db.execute("UPDATE sparse_view_blocks SET data=?1", [b"XX".as_slice()])
        .unwrap();
    drop(db);
    // A new principal may have provider access but cannot silently adopt an old
    // principal's private dirty-state binding. Existing handles pin old keys.
    std::fs::write(
        std::env::var_os("AWS_SHARED_CREDENTIALS_FILE").unwrap(),
        "[fixture]\naws_access_key_id=TESTROTATED\naws_secret_access_key=TESTROTATEDSECRET\n",
    )
    .unwrap();
    *f.s3.expected_key.lock().unwrap() = "TESTROTATED".into();
    assert!(z.read_range(0, 8).await.is_err());
    let rotated = f.reader().await;
    failed(
        rotated.reopen_edited(c.clone(), local.id()).await,
        ReadFailure::Binding,
    );
    assert_eq!(c.head(root.binding().clone()).await.unwrap(), zero);
    s.close().await.unwrap();
}
pub async fn limits(f: &Fixture, path: &Path) {
    f.tree(10, C, data(0, 1)).await;
    let r = f.reader().await;
    let base = r.open_file(&id(10)).await.unwrap().pinned_base();
    let (s, c) = storage(&path.join("local")).await;
    let root = c
        .begin([1; 32], base.sparse_binding().unwrap(), C)
        .await
        .unwrap();
    let edited = r.reopen_edited(c.clone(), root.id()).await.unwrap();
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Data, Fault::Gate);
    let a = edited.clone();
    let task = tokio::spawn(async move { a.read_range(0, 4).await });
    f.s3.wait(arrived + 1).await;
    // SQLite remains available while remote bytes are gated; the read must keep
    // its original immutable view despite this newer local head.
    let new = tokio::time::timeout(Duration::from_secs(3), c.resize([2; 32], root.id(), 8))
        .await
        .unwrap()
        .unwrap();
    f.s3.clear();
    f.s3.permits.add_permits(1);
    assert_eq!(task.await.unwrap().unwrap(), pattern(0, 4));
    assert_eq!(c.head(root.binding().clone()).await.unwrap(), new);
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Data, Fault::Gate);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let a = edited.clone();
        tasks.push(tokio::spawn(async move { a.read_range(0, 4).await }));
    }
    f.s3.wait(arrived + 4).await;
    failed(r.open_file(&id(10)).await, ReadFailure::Busy);
    failed(base.read_range(0, 1).await, ReadFailure::Busy);
    failed(
        r.reopen_edited(c.clone(), root.id()).await,
        ReadFailure::Busy,
    );
    for t in tasks {
        t.abort();
        assert!(t.await.unwrap_err().is_cancelled());
    }
    f.s3.clear();
    f.s3.permits.add_permits(4);
    f.s3.settle().await;
    assert_eq!(edited.read_range(0, 4).await.unwrap(), pattern(0, 4));
    let short = f
        .reader_options(ReadOptions::new(Duration::from_secs(3)).unwrap())
        .await;
    let e = short.reopen_edited(c.clone(), root.id()).await.unwrap();
    let arrived = f.s3.arrived.load(Ordering::SeqCst);
    f.s3.fault(Target::Data, Fault::Gate);
    let task = tokio::spawn(async move { e.read_range(0, 4).await });
    f.s3.wait(arrived + 1).await;
    failed(task.await.unwrap(), ReadFailure::Timeout);
    f.s3.clear();
    f.s3.permits.add_permits(1);
    f.s3.settle().await;
    assert_eq!(edited.read_range(0, 4).await.unwrap(), pattern(0, 4));
    let mut pins = Vec::<EditedFile>::new();
    for _ in 0..254 {
        pins.push(r.reopen_edited(c.clone(), root.id()).await.unwrap());
    }
    failed(r.open_file(&id(10)).await, ReadFailure::Busy);
    failed(
        r.reopen_edited(c.clone(), root.id()).await,
        ReadFailure::Busy,
    );
    drop(pins);
    assert!(r.reopen_edited(c.clone(), root.id()).await.is_ok());
    s.close().await.unwrap();
}
