//! Explicit native NTFS reference probes; these never enable S3Drive deletion.
//! Only owned temporary paths are used. Raw FFI stays under native's audit.
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
        DELETE, FILE_DISPOSITION_FLAG_ON_CLOSE, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_DELETE_ON_CLOSE, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_STANDARD_INFO, FileDispositionInfo, FileDispositionInfoEx, FileStandardInfo,
        GetFileInformationByHandleEx, SetFileInformationByHandle,
    },
};
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_str().unwrap();
    assert!(
        path.len() > 3 && path.as_bytes()[1] == b':',
        "reference requires a drive path"
    );
    assert!(
        super::is_ntfs(&path[..3]).unwrap(),
        "NTFS reference must run on NTFS"
    );
    root
}
fn open(path: &Path, delete_on_close: bool) -> std::io::Result<File> {
    options(delete_on_close).open(path)
}
fn options(delete_on_close: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .access_mode(FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .custom_flags(
            FILE_FLAG_BACKUP_SEMANTICS.0
                | if delete_on_close {
                    FILE_FLAG_DELETE_ON_CLOSE.0
                } else {
                    0
                },
        );
    options
}
fn clear_on_close(file: &File) -> windows::core::Result<()> {
    let info = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_ON_CLOSE,
    };
    // SAFETY: a live borrowed File and a correctly sized repr(C) input remain
    // valid for this synchronous call. No pointer is retained by this helper.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfoEx,
            (&info as *const FILE_DISPOSITION_INFO_EX).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    }
}
fn reopen(path: &Path) -> (bool, Option<i32>) {
    let result = open(path, false);
    (result.is_ok(), result.err().and_then(|e| e.raw_os_error()))
}
fn disposition(file: &File, delete: bool) -> windows::core::Result<()> {
    let info = FILE_DISPOSITION_INFO { DeleteFile: delete };
    // SAFETY: the borrowed File owns a live handle throughout this synchronous
    // call. The stable repr(C) structure has the exact supplied byte length.
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            (&info as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
}
fn pending(file: &File) -> bool {
    let mut info = FILE_STANDARD_INFO::default();
    // SAFETY: live borrowed handle and an exclusive, correctly sized repr(C)
    // output buffer. Only inspect initialized fields after successful return.
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
fn trace(case: &str, values: serde_json::Value) {
    println!(
        "S3DRIVE_DISPOSITION_REFERENCE {}",
        serde_json::json!({"case":case,"backend":"ntfs","observations":values})
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn repeated_set_delete_can_cancel_without_removing_bytes() {
    let root = root();
    let path = root.path().join("cancel 資料 🦀");
    std::fs::write(&path, b"retained\0\xff").unwrap();
    let file = open(&path, false).unwrap();
    for _ in 0..2 {
        disposition(&file, true).unwrap();
        assert!(pending(&file));
        disposition(&file, false).unwrap();
        assert!(!pending(&file));
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"retained\0\xff");
    drop(file);
    assert_eq!(std::fs::read(&path).unwrap(), b"retained\0\xff");
    trace(
        "repeated-cancel",
        serde_json::json!({"live_after_close":true}),
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn deletion_finishes_on_nonarming_final_handle() {
    let root = root();
    let path = root.path().join("nonarming café");
    std::fs::write(&path, b"old bytes").unwrap();
    let armed = open(&path, false).unwrap();
    let mut last = open(&path, false).unwrap();
    disposition(&armed, true).unwrap();
    assert!(pending(&last));
    assert!(open(&path, false).is_err());
    drop(armed);
    assert!(pending(&last));
    assert!(open(&path, false).is_err());
    let mut bytes = Vec::new();
    last.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"old bytes");
    last.seek(SeekFrom::End(0)).unwrap();
    last.write_all(b"!").unwrap();
    last.sync_all().unwrap();
    drop(last);
    assert!(!path.exists());
    trace(
        "nonarming-final-handle",
        serde_json::json!({"late_io":true,"removed":true}),
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn delete_on_close_allows_another_open_before_cleanup() {
    let root = root();
    let path = root.path().join("deferred 日本語");
    std::fs::write(&path, b"data").unwrap();
    let deferred = open(&path, true).unwrap();
    let mut other = open(&path, false).unwrap();
    let before = pending(&deferred);
    let other_before = pending(&other);
    drop(deferred);
    let after = pending(&other);
    let new_open = open(&path, false).err().map(|e| e.raw_os_error());
    assert!(after);
    assert!(new_open.is_some());
    let mut bytes = Vec::new();
    other.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"data");
    drop(other);
    assert!(!path.exists());
    trace(
        "deferred-file-open",
        serde_json::json!({"pending_before_cleanup":before,"other_pending_before":other_before,"pending_after_cleanup":after,"new_open_error_after":new_open}),
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn nonempty_delete_on_close_directory_survives_then_empty_directory_is_removed() {
    let root = root();
    let path = root.path().join("nonempty directory 🦀");
    std::fs::create_dir(&path).unwrap();
    let child = path.join("資料.bin");
    std::fs::write(&child, b"child remains").unwrap();
    let directory = open(&path, true).unwrap();
    drop(directory);
    assert_eq!(std::fs::read(&child).unwrap(), b"child remains");
    assert!(path.is_dir());
    std::fs::remove_file(&child).unwrap();
    drop(open(&path, true).unwrap());
    assert!(!path.exists());
    trace(
        "nonempty-deferred-directory",
        serde_json::json!({"nonempty_open_succeeded":true,"child_preserved":true,"empty_cleanup_removed":true}),
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn ordinary_nonempty_disposition_is_refused_without_harming_children() {
    let root = root();
    let path = root.path().join("nonempty ordinary");
    std::fs::create_dir(&path).unwrap();
    let child = path.join("child");
    std::fs::write(&child, b"kept").unwrap();
    let file = open(&path, false).unwrap();
    let error = disposition(&file, true).unwrap_err();
    assert!(!pending(&file));
    drop(file);
    assert_eq!(std::fs::read(&child).unwrap(), b"kept");
    trace(
        "ordinary-nonempty-refusal",
        serde_json::json!({"hresult":error.code().0,"child_preserved":true}),
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_cross_context_cancellation_and_cleanup() {
    let root = root();
    let path = root.path().join("cross-context cancellation");
    std::fs::write(&path, b"kept").unwrap();
    let a = open(&path, false).unwrap();
    let b = open(&path, false).unwrap();
    disposition(&a, true).unwrap();
    disposition(&b, false).unwrap();
    let a_pending = pending(&a);
    let b_pending = pending(&b);
    let reopened = open(&path, false).is_ok();
    drop(a);
    let after_cleanup = pending(&b);
    let reopened_after = open(&path, false).is_ok();
    drop(b);
    trace(
        "cross-context-cancel",
        serde_json::json!({"a_pending":a_pending,"b_pending":b_pending,"reopened":reopened,"pending_after_a_cleanup":after_cleanup,"reopened_after_a_cleanup":reopened_after,"exists_after_all_close":path.exists()}),
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_legacy_cancel_of_delete_on_close() {
    let root = root();
    let path = root.path().join("legacy cancel deferred");
    std::fs::write(&path, b"kept").unwrap();
    let a = open(&path, true).unwrap();
    let b = open(&path, false).unwrap();
    disposition(&a, false).unwrap();
    let before = pending(&a);
    drop(a);
    let after = pending(&b);
    let reopened = open(&path, false).is_ok();
    drop(b);
    trace(
        "legacy-cancel-delete-on-close",
        serde_json::json!({"pending_before":before,"pending_after_cleanup":after,"reopened_after_cleanup":reopened,"exists_after_all_close":path.exists()}),
    );
}
#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_child_creation_during_deferred_directory_lifetime() {
    let root = root();
    let path = root.path().join("deferred child creation");
    std::fs::create_dir(&path).unwrap();
    let directory = open(&path, true).unwrap();
    let child = path.join("new child");
    let result = std::fs::write(&child, b"child data");
    let error = result.as_ref().err().and_then(|e| e.raw_os_error());
    let created = result.is_ok();
    drop(directory);
    if created {
        assert_eq!(std::fs::read(&child).unwrap(), b"child data");
        assert!(path.is_dir());
    }
    trace(
        "deferred-directory-child-create",
        serde_json::json!({"created":created,"error":error,"directory_exists_after_cleanup":path.exists()}),
    );
}

#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_rename_of_deferred_file_and_retained_io() {
    let root = root();
    let source = root.path().join("source café 日本語 🦀");
    let destination = root.path().join("renamed e\u{301} 🦀");
    std::fs::write(&source, b"renamed bytes\0\xff").unwrap();
    let deferred = open(&source, true).unwrap();
    let mut retained = open(&source, false).unwrap();
    let result = std::fs::rename(&source, &destination);
    let renamed = result.is_ok();
    let rename_error = result.err().and_then(|e| e.raw_os_error());
    let active = if renamed { &destination } else { &source };
    let source_exists_after_rename = source.exists();
    let destination_exists_after_rename = destination.exists();
    let pending_before_cleanup = pending(&retained);
    drop(deferred);
    let pending_after_cleanup = pending(&retained);
    let (reopened, reopen_error) = reopen(active);
    let mut bytes = Vec::new();
    retained.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"renamed bytes\0\xff");
    retained.write_all(b"!").unwrap();
    retained.sync_all().unwrap();
    drop(retained);
    trace(
        "deferred-file-rename",
        serde_json::json!({"renamed":renamed,"rename_error":rename_error,
        "source_exists_after_rename":source_exists_after_rename,"destination_exists_after_rename":destination_exists_after_rename,
        "pending_before_cleanup":pending_before_cleanup,"pending_after_cleanup":pending_after_cleanup,
        "reopened_after_cleanup":reopened,"reopen_error":reopen_error,"late_io":true,
        "source_exists_after_all_close":source.exists(),"destination_exists_after_all_close":destination.exists()}),
    );
}

#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_rename_of_nonempty_deferred_directory() {
    let root = root();
    let source = root.path().join("source directory 🦀");
    let destination = root.path().join("renamed directory 資料");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("café.bin"), b"child kept").unwrap();
    let directory = open(&source, true).unwrap();
    let result = std::fs::rename(&source, &destination);
    let renamed = result.is_ok();
    let rename_error = result.err().and_then(|e| e.raw_os_error());
    let active = if renamed { &destination } else { &source };
    assert_eq!(
        std::fs::read(active.join("café.bin")).unwrap(),
        b"child kept"
    );
    let pending_before_cleanup = pending(&directory);
    drop(directory);
    assert_eq!(
        std::fs::read(active.join("café.bin")).unwrap(),
        b"child kept"
    );
    trace(
        "deferred-directory-rename",
        serde_json::json!({"renamed":renamed,"rename_error":rename_error,
        "pending_before_cleanup":pending_before_cleanup,"child_preserved":true,
        "source_exists_after_cleanup":source.exists(),"destination_exists_after_cleanup":destination.exists()}),
    );
}

#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_exclusive_create_with_delete_on_close() {
    let root = root();
    let path = root.path().join("created 資料 🦀");
    let mut created = options(true).create_new(true).open(&path).unwrap();
    created.write_all(b"created\0\xff").unwrap();
    created.sync_all().unwrap();
    let mut retained = open(&path, false).unwrap();
    let pending_before_cleanup = pending(&retained);
    drop(created);
    let pending_after_cleanup = pending(&retained);
    let (reopened, reopen_error) = reopen(&path);
    let mut bytes = Vec::new();
    retained.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"created\0\xff");
    drop(retained);
    trace(
        "created-deferred-file",
        serde_json::json!({"exclusive_create":true,"pending_before_cleanup":pending_before_cleanup,
        "pending_after_cleanup":pending_after_cleanup,"reopened_after_cleanup":reopened,"reopen_error":reopen_error,
        "bytes_preserved":true,"exists_after_all_close":path.exists()}),
    );
}

#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_multiple_deferred_handles_and_cancellation_between_cleanups() {
    let root = root();
    let path = root.path().join("multiple deferred 🦀");
    std::fs::write(&path, b"multiple bytes").unwrap();
    let a = open(&path, true).unwrap();
    let b = open(&path, true).unwrap();
    let mut retained = open(&path, false).unwrap();
    let pending_before = pending(&retained);
    drop(a);
    let pending_after_a = pending(&retained);
    let cancellation = disposition(&b, false);
    let cancel_hresult = cancellation.err().map(|e| e.code().0);
    let pending_after_cancel = pending(&retained);
    let (reopened_after_cancel, cancel_reopen_error) = reopen(&path);
    drop(b);
    let pending_after_b = pending(&retained);
    let (reopened_after_b, b_reopen_error) = reopen(&path);
    let mut bytes = Vec::new();
    retained.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"multiple bytes");
    retained.write_all(b"!").unwrap();
    retained.sync_all().unwrap();
    drop(retained);
    trace(
        "multiple-deferred-cancel",
        serde_json::json!({"pending_before":pending_before,"pending_after_a":pending_after_a,
        "cancel_hresult":cancel_hresult,"pending_after_cancel":pending_after_cancel,"reopened_after_cancel":reopened_after_cancel,
        "cancel_reopen_error":cancel_reopen_error,"pending_after_b":pending_after_b,"reopened_after_b":reopened_after_b,
        "b_reopen_error":b_reopen_error,"late_io":true,"exists_after_all_close":path.exists()}),
    );
}

#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_extended_on_close_cancellation() {
    let root = root();
    let path = root.path().join("extended cancel 🦀");
    std::fs::write(&path, b"extended bytes").unwrap();
    let deferred = open(&path, true).unwrap();
    let retained = open(&path, false).unwrap();
    let result = clear_on_close(&deferred);
    let cancel_hresult = result.err().map(|e| e.code().0);
    let pending_before_cleanup = pending(&retained);
    drop(deferred);
    let pending_after_cleanup = pending(&retained);
    let (reopened, reopen_error) = reopen(&path);
    drop(retained);
    let exists = path.exists();
    if exists {
        assert_eq!(std::fs::read(&path).unwrap(), b"extended bytes");
    }
    trace(
        "extended-on-close-cancel",
        serde_json::json!({"cancel_hresult":cancel_hresult,"pending_before_cleanup":pending_before_cleanup,
        "pending_after_cleanup":pending_after_cleanup,"reopened_after_cleanup":reopened,"reopen_error":reopen_error,"exists_after_all_close":exists}),
    );
}

#[test]
#[ignore = "native NTFS reference; separate Windows acceptance gate"]
fn observe_duplicate_handle_lifetime_before_deferred_cleanup() {
    let root = root();
    let path = root.path().join("duplicate deferred 🦀");
    std::fs::write(&path, b"duplicate bytes").unwrap();
    let deferred = open(&path, true).unwrap();
    let duplicate = deferred.try_clone().unwrap();
    let mut retained = open(&path, false).unwrap();
    drop(deferred);
    let pending_after_first_close = pending(&retained);
    let (reopened_after_first, first_reopen_error) = reopen(&path);
    drop(duplicate);
    let pending_after_duplicate_close = pending(&retained);
    let (reopened_after_duplicate, duplicate_reopen_error) = reopen(&path);
    let mut bytes = Vec::new();
    retained.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"duplicate bytes");
    drop(retained);
    trace(
        "duplicated-deferred-handle",
        serde_json::json!({"pending_after_first_close":pending_after_first_close,
        "reopened_after_first":reopened_after_first,"first_reopen_error":first_reopen_error,
        "pending_after_duplicate_close":pending_after_duplicate_close,"reopened_after_duplicate":reopened_after_duplicate,
        "duplicate_reopen_error":duplicate_reopen_error,"bytes_preserved":true,"exists_after_all_close":path.exists()}),
    );
}
