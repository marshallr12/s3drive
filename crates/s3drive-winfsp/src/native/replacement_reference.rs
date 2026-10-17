//! Explicit NTFS and selected-WinFsp legacy/POSIX reference. No S3Drive backend runs.
mod host;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_DELETE_ON_CLOSE, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_STANDARD_INFO, FileDispositionInfo, FileStandardInfo, GetFileInformationByHandleEx,
        SetFileInformationByHandle,
    },
};
fn open(path: &Path, doc: bool) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .custom_flags(if doc { FILE_FLAG_DELETE_ON_CLOSE.0 } else { 0 })
        .open(path)
        .unwrap()
}
fn pending(file: &File) -> bool {
    let mut info = FILE_STANDARD_INFO::default();
    // SAFETY: borrowed live File and correctly sized initialized output buffer.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileStandardInfo,
            (&mut info as *mut FILE_STANDARD_INFO).cast(),
            size_of::<FILE_STANDARD_INFO>() as u32,
        )
    }
    .unwrap();
    info.DeletePending
}
fn disposition(file: &File, delete: bool) {
    let info = FILE_DISPOSITION_INFO { DeleteFile: delete };
    // SAFETY: live borrowed handle; the exact repr(C) input lives for the call.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .unwrap();
}
fn contents(path: &Path) -> (Option<String>, Option<i32>) {
    match std::fs::read(path) {
        Ok(b) => (Some(String::from_utf8(b).unwrap()), None),
        Err(e) => (None, e.raw_os_error()),
    }
}
fn probe(root: &Path, backend: &str, state: Option<&host::State>, case: &str) {
    let source = root.join(format!("{case} source café 🦀"));
    let target = root.join(format!("{case} target 資料"));
    std::fs::write(&source, b"source").unwrap();
    std::fs::write(&target, b"target").unwrap();
    let target_doc = matches!(
        case,
        "deferred-target" | "both-deferred" | "cleared-deferred" | "duplicate-deferred"
    );
    let source_doc = case == "both-deferred";
    let source_handle = open(&source, source_doc);
    let mut old_target = Some(open(&target, false));
    let mut deleting = target_doc.then(|| open(&target, true));
    if case == "duplicate-deferred" {
        let duplicate = deleting.as_ref().unwrap().try_clone().unwrap();
        drop(deleting.take());
        deleting = Some(duplicate);
    }
    if case == "pending-target" {
        disposition(old_target.as_ref().unwrap(), true);
    }
    if case == "cleared-deferred" {
        disposition(deleting.as_ref().unwrap(), true);
        disposition(deleting.as_ref().unwrap(), false);
    }
    let before = old_target.as_ref().map(pending);
    if case == "closed-target" {
        drop(old_target.take());
    }
    let renames = state.map(host::State::renames);
    let renamed = std::fs::rename(&source, &target);
    let success = renamed.is_ok();
    let error = renamed.err().and_then(|e| e.raw_os_error());
    let forwarded = state.map(|s| s.renames() > renames.unwrap());
    let after_rename = old_target.as_ref().map(pending);
    let (during, during_error) = contents(&target);
    let source_exists = source.exists();
    drop(deleting);
    let after_cleanup = old_target.as_ref().map(pending);
    let (after_target_cleanup, after_target_cleanup_error) = contents(&target);
    let late_io = old_target.as_mut().map(|file| {
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut b = Vec::new();
        file.read_to_end(&mut b).unwrap();
        assert_eq!(b, b"target");
        file.write_all(b"!").unwrap();
        file.sync_all().unwrap();
        true
    });
    drop(old_target);
    drop(source_handle);
    let (final_bytes, final_error) = contents(&target);
    // A target's old cleanup must not remove the replacement source identity.
    if success && !source_doc {
        assert_eq!(final_bytes.as_deref(), Some("source"));
    }
    println!(
        "S3DRIVE_REPLACEMENT_REFERENCE {}",
        serde_json::json!({"backend":backend,"case":case,"observations":{
            "renamed":success,"rename_error":error,"rename_forwarded":forwarded,
            "target_pending_before":before,"target_pending_after_rename":after_rename,"target_pending_after_cleanup":after_cleanup,
            "source_exists_after_rename":source_exists,"destination_bytes_after_rename":during,"destination_open_error":during_error,
            "destination_bytes_after_target_cleanup":after_target_cleanup,"destination_error_after_target_cleanup":after_target_cleanup_error,
            "retained_target_late_io":late_io,"destination_bytes_after_all_close":final_bytes,"destination_error_after_all_close":final_error,
            "source_exists_after_all_close":source.exists(),"destination_exists_after_all_close":target.exists()
        }})
    );
}
#[test]
#[ignore = "explicit disposable Windows reference; not S3Drive native acceptance"]
fn measure_deferred_replacement_on_ntfs_and_selected_winfsp_profiles() {
    let tmp = tempfile::tempdir().unwrap();
    let text = tmp.path().to_str().unwrap();
    assert!(super::is_ntfs(&text[..3]).unwrap());
    let ntfs = tmp.path().join("ntfs");
    std::fs::create_dir(&ntfs).unwrap();
    let cases = [
        "closed-target",
        "ordinary-target",
        "deferred-target",
        "both-deferred",
        "pending-target",
        "cleared-deferred",
        "duplicate-deferred",
    ];
    for case in cases {
        probe(&ntfs, "ntfs", None, case);
    }
    for (backend, posix) in [("winfsp-legacy", false), ("winfsp-posix", true)] {
        let point = tmp.path().join(backend);
        let mut mounted = host::Mounted::new(&point, posix);
        for case in cases {
            probe(&point, backend, Some(&mounted.state), case);
        }
        mounted.stop();
        drop(mounted);
    }
    tmp.close().unwrap();
}
