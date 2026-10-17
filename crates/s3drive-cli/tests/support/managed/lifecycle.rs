//! Shared filesystem bridge checks. Native driver acceptance is separate.
use super::*;
pub(super) async fn portable() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new_policy(tmp.path(), &f, true).await;
    h.configure_lifecycle();
    let before = f
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.starts_with("DELETE "))
        .count();
    h.child("lifecycle-engine").await;
    assert_eq!(
        f.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("DELETE "))
            .count(),
        before
    );
    s.stop().await;
}
pub(super) async fn engine(c: &s3drive_core::Config) {
    let fs = s3drive_fs::ReadOnlyFs::new(c).await.unwrap();
    assert!(fs.native_lifecycle_enabled());
    assert!(fs.native_file_object(s3drive_fs::ROOT, true).await.is_err());
    // Early deleting cleanup, another file object, late detached I/O.
    let (n, a) = fs
        .create_native_deferred(s3drive_fs::ROOT, "early café 🦀", false)
        .await
        .unwrap();
    let file = fs.open_write(n.id).await.unwrap();
    file.write(0, Arc::from(&b"first\0\xff"[..]), false, false)
        .await
        .unwrap();
    let b = fs.native_file_object(n.id, false).await.unwrap().unwrap();
    a.cleanup(1).await.unwrap();
    a.close().await.unwrap();
    assert!(fs.lookup(s3drive_fs::ROOT, &n.name).await.is_err());
    assert!(file.is_unlinked().unwrap());
    file.truncate(0).await.unwrap();
    file.write(0, Arc::from(&b"late\0\xfe"[..]), false, false)
        .await
        .unwrap();
    file.flush().await.unwrap();
    assert_eq!(file.read(0, 30).await.unwrap(), b"late\0\xfe");
    b.cleanup(0).await.unwrap();
    b.close().await.unwrap();
    // Actual non-deleting cleanup overrides the original prospective DoC bit.
    let (n, a) = fs
        .create_native_deferred(s3drive_fs::ROOT, "cleared café 🦀", false)
        .await
        .unwrap();
    let b = fs.native_file_object(n.id, false).await.unwrap().unwrap();
    a.set_delete(false).await.unwrap();
    a.cleanup(0).await.unwrap();
    a.close().await.unwrap();
    assert!(fs.open(n.id).await.is_ok());
    b.cleanup(0).await.unwrap();
    b.close().await.unwrap();
    assert!(fs.lookup(s3drive_fs::ROOT, &n.name).await.is_ok());
    // Source and deferred target live in different S3 stores / controls.
    let source = fs
        .lookup(s3drive_fs::ROOT, "other bucket.txt")
        .await
        .unwrap();
    let data = fs.open_write(source.id).await.unwrap();
    let source_object = fs
        .native_file_object(source.id, true)
        .await
        .unwrap()
        .unwrap();
    let (target, target_object) = fs
        .create_native_deferred(s3drive_fs::ROOT, "cross-store target 日本語", false)
        .await
        .unwrap();
    let old = fs.open_write(target.id).await.unwrap();
    old.write(0, Arc::from(&b"old target"[..]), false, false)
        .await
        .unwrap();
    old.flush().await.unwrap();
    let moved = fs
        .rename_native_object(
            &source_object,
            s3drive_fs::ROOT,
            &target.name,
            true,
            |a, b| a == b,
        )
        .await
        .unwrap();
    assert_eq!(moved.id, source.id);
    assert!(old.is_unlinked().unwrap());
    old.truncate(0).await.unwrap();
    old.write(0, Arc::from(&b"old late"[..]), false, false)
        .await
        .unwrap();
    old.flush().await.unwrap();
    target_object.cleanup(1).await.unwrap();
    target_object.close().await.unwrap();
    assert_eq!(
        fs.open(moved.id).await.unwrap().read(0, 100).await.unwrap(),
        b"payload from the second bucket"
    );
    assert_eq!(
        data.read(0, 100).await.unwrap(),
        b"payload from the second bucket"
    );
    source_object.cleanup(1).await.unwrap();
    source_object.close().await.unwrap();
    assert!(fs.lookup(s3drive_fs::ROOT, &target.name).await.is_err());
    // Deferred directory rename, child creation and nonempty cleanup retention.
    let (dir, obj) = fs
        .create_native_deferred(s3drive_fs::ROOT, "directory DoC", true)
        .await
        .unwrap();
    let child = fs
        .create_file(dir.id, "child 日本語", |a, b| a == b, true)
        .await
        .unwrap();
    fs.rename_native_object(&obj, s3drive_fs::ROOT, "directory moved", false, |a, b| {
        a == b
    })
    .await
    .unwrap();
    obj.cleanup(1).await.unwrap();
    obj.close().await.unwrap();
    assert!(fs.lookup(s3drive_fs::ROOT, "directory moved").await.is_ok());
    assert!(child.read(0, 2).await.unwrap().is_empty());
    fs.flush_all().await.unwrap();
    fs.shutdown_writes().await.unwrap();
    fs.cache_control().unwrap().close().await.unwrap();
}

#[cfg(windows)]
mod windows {
    use super::*;
    use s3drive_winfsp::native_test_api::{clear_delete_on_close, pending, set_delete};
    use std::{
        fs::{File, OpenOptions},
        os::windows::fs::OpenOptionsExt,
    };
    pub(super) fn open(path: &Path, doc: bool, directory: bool, create: bool) -> File {
        OpenOptions::new()
            .read(true)
            .write(!directory)
            .create_new(create)
            .access_mode(if directory { 0x80010000 } else { 0xc0010000 })
            .share_mode(7)
            .custom_flags(
                (if doc { 0x04000000 } else { 0 }) | if directory { 0x02000000 } else { 0 },
            )
            .open(path)
            .unwrap()
    }
    fn read(file: &mut File) -> Vec<u8> {
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }
    pub(super) fn exercise(root: &Path) {
        let control = tempfile::tempdir().unwrap();
        let oracle = control.path().join("NT directory DoC oracle 🦀");
        let handle =
            s3drive_winfsp::native_test_api::create_directory_delete_on_close(&oracle).unwrap();
        assert!(oracle.is_dir());
        drop(handle);
        assert!(!oracle.exists());
        let created = root.join("atomic directory DoC 日本語");
        let handle =
            s3drive_winfsp::native_test_api::create_directory_delete_on_close(&created).unwrap();
        assert!(created.is_dir());
        drop(handle);
        assert!(!created.exists());
        let path = root.join("created DoC 日本語 🦀");
        let mut f = open(&path, true, false, true);
        f.write_all(b"created\0\xff").unwrap();
        f.sync_all().unwrap();
        assert!(!pending(&f).unwrap());
        let duplicate = f.try_clone().unwrap();
        drop(f);
        assert!(path.exists());
        drop(duplicate);
        assert!(!path.exists());
        let path = root.join("clear reassert café 🦀");
        std::fs::write(&path, b"preserved").unwrap();
        let f = open(&path, false, false, false);
        set_delete(&f, true).unwrap();
        assert!(pending(&f).unwrap());
        set_delete(&f, false).unwrap();
        assert!(!pending(&f).unwrap());
        drop(f);
        assert_eq!(std::fs::read(&path).unwrap(), b"preserved");
        let f = open(&path, false, false, false);
        set_delete(&f, true).unwrap();
        drop(f);
        assert!(!path.exists());
        let path = root.join("extended clear 🦀");
        std::fs::write(&path, b"extended").unwrap();
        let f = open(&path, true, false, false);
        clear_delete_on_close(&f).unwrap();
        drop(f);
        assert_eq!(std::fs::read(&path).unwrap(), b"extended");
        let path = root.join("multiple DoC objects 🦀");
        std::fs::write(&path, b"multiple").unwrap();
        let mut retained = open(&path, false, false, false);
        let a = open(&path, true, false, false);
        let b = open(&path, true, false, false);
        drop(a);
        assert_eq!(read(&mut retained), b"multiple");
        drop(b);
        retained.seek(SeekFrom::End(0)).unwrap();
        retained.write_all(b" late").unwrap();
        retained.sync_all().unwrap();
        drop(retained);
        assert!(!path.exists());
        let path = root.join("early POSIX cleanup 🦀");
        std::fs::write(&path, b"early").unwrap();
        let mut retained = open(&path, false, false, false);
        let deleting = open(&path, false, false, false);
        s3drive_winfsp::native_test_api::set_posix_delete(&deleting).unwrap();
        drop(deleting);
        assert!(!path.exists());
        assert_eq!(read(&mut retained), b"early");
        retained.seek(SeekFrom::End(0)).unwrap();
        retained.write_all(b" late").unwrap();
        retained.sync_all().unwrap();
        drop(retained);
        for case in 0..4 {
            let source = root.join(format!("source café {case}"));
            let target = root.join(format!("target 日本語 {case}"));
            std::fs::write(&source, b"source").unwrap();
            std::fs::write(&target, b"target").unwrap();
            let source_doc = case >= 2;
            let target_doc = case % 2 == 0;
            let mut s = open(&source, source_doc, false, false);
            let mut t = open(&target, target_doc, false, false);
            if case == 1 {
                set_delete(&t, true).unwrap();
            }
            std::fs::rename(&source, &target).unwrap();
            assert!(!source.exists());
            assert_eq!(std::fs::read(&target).unwrap(), b"source");
            assert_eq!(read(&mut t), b"target");
            t.seek(SeekFrom::End(0)).unwrap();
            t.write_all(b" late").unwrap();
            t.sync_all().unwrap();
            assert_eq!(read(&mut t), b"target late");
            drop(t);
            assert_eq!(std::fs::read(&target).unwrap(), b"source");
            s.seek(SeekFrom::End(0)).unwrap();
            s.write_all(b" next").unwrap();
            s.sync_all().unwrap();
            drop(s);
            if source_doc {
                assert!(!target.exists());
            } else {
                assert_eq!(std::fs::read(&target).unwrap(), b"source next");
            }
        }
        let old = root.join("nonempty directory DoC");
        std::fs::create_dir(&old).unwrap();
        let d = open(&old, true, true, false);
        std::fs::write(old.join("child 日本語"), b"child").unwrap();
        let renamed = root.join("renamed directory DoC");
        std::fs::rename(&old, &renamed).unwrap();
        drop(d);
        assert_eq!(
            std::fs::read(renamed.join("child 日本語")).unwrap(),
            b"child"
        );
        let empty = root.join("empty directory DoC");
        std::fs::create_dir(&empty).unwrap();
        drop(open(&empty, true, true, false));
        assert!(!empty.exists());
        let other = root.join("other bucket.txt");
        let target = root.join("cross-store target");
        std::fs::write(&target, b"first-store").unwrap();
        let s = open(&other, true, false, false);
        let mut t = open(&target, true, false, false);
        std::fs::rename(&other, &target).unwrap();
        t.seek(SeekFrom::End(0)).unwrap();
        t.write_all(b" retained").unwrap();
        t.sync_all().unwrap();
        drop(t);
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"payload from the second bucket"
        );
        drop(s);
        assert!(!target.exists());
        eprintln!(
            "PASS: native DoC create/duplicates, SetDelete clear/reassert, extended on-close clear, four replacement modes, retained late I/O, cross-store ownership and directory lifecycle"
        );
    }
}
#[cfg(windows)]
pub(super) async fn native() {
    let f = Fixture::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let (h, s) = Harness::new_policy(tmp.path(), &f, true).await;
    h.configure_lifecycle();
    let deletes = f
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.starts_with("DELETE "))
        .count();
    let m = h.mount().await;
    tokio::task::block_in_place(|| windows::exercise(&h.mount));
    m.stop().await;
    // A committed admission loses its native reply, and the mount process dies.
    let proxy = NamespaceProxy::start(&h).await;
    proxy.route(&h);
    let m = h.mount().await;
    let path = h.mount.join("lost DoC admission 🦀");
    let attempt = tokio::task::spawn_blocking({
        let path = path.clone();
        move || {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .access_mode(0xc0010000)
                .share_mode(7)
                .custom_flags(0x04000000)
                .open(path)
        }
    });
    proxy.committed().await;
    m.kill().await;
    proxy.state.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(20), attempt)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let puts_before = puts(&f);
    let m = h.mount().await;
    assert!(path.exists());
    assert!(std::fs::read(&path).unwrap().is_empty());
    assert_eq!(puts(&f), puts_before);
    m.stop().await;
    // A real SetDelete callback commits its Arm but loses the native response.
    let set_proxy = NamespaceProxy::start_kind(&h, 4).await;
    set_proxy.route(&h);
    let m = h.mount().await;
    let set_path = h.mount.join("lost SetDelete 日本語 🦀");
    std::fs::write(&set_path, b"retained native bytes").unwrap();
    let file = windows::open(&set_path, false, false, false);
    let attempt = tokio::task::spawn_blocking(move || {
        let result = s3drive_winfsp::native_test_api::set_delete(&file, true);
        drop(file);
        result
    });
    set_proxy.committed().await;
    m.kill().await;
    set_proxy.state.release.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(20), attempt)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let before = puts(&f);
    let m = h.mount().await;
    assert_eq!(std::fs::read(&set_path).unwrap(), b"retained native bytes");
    assert_eq!(puts(&f), before);
    m.stop().await;
    assert_eq!(
        f.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("DELETE "))
            .count(),
        deletes
    );
    s.stop().await;
    eprintln!(
        "PASS: native lost DoC admission/SetDelete replies and killed-owner recovery preserve name/data without fabricated callbacks or another PUT"
    );
}
