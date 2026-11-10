//! `FILE_FLAG_DELETE_ON_CLOSE` through `CreateFileW` on a real WinFsp mount of
//! the version 8 native writer; loopback backend only. Phase 2 of the
//! delete-on-close proposal. Not an Office GUI, physical or provider test.
use super::*;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt;

const DELETE_ON_CLOSE: u32 = 0x0400_0000;
const BODY: &[u8] = b"temporary bytes\r\n\0binary tail";
const KEPT: &[u8] = b"published before the cold remount";
const JOURNALED: &[u8] = b"journaled before the mount process died";

/// Creates a new file that carries the delete-on-close flag.
fn create_flagged(path: &Path) -> File {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .custom_flags(DELETE_ON_CLOSE)
        .open(path)
        .unwrap_or_else(|e| panic!("delete-on-close create {path:?}: {e}"))
}

/// Opens an existing file with the delete-on-close flag and read access.
fn open_flagged(path: &Path) -> std::io::Result<File> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(DELETE_ON_CLOSE)
        .open(path)
}

/// Reads a whole file through an already open handle.
fn read_all(file: &mut File) -> Vec<u8> {
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

/// Create with the flag, write, read back and close: the name is gone.
fn created_then_closed(base: &Path) {
    let path = base.join("~WRD0001 日本語.tmp");
    let mut file = create_flagged(&path);
    file.write_all(BODY).unwrap();
    assert_eq!(read_all(&mut file), BODY);
    assert!(path.exists(), "name must exist while the handle is open");
    drop(file);
    assert!(!path.exists(), "created delete-on-close file remained");
}

/// Open an existing file of this mount with the flag and close it.
fn existing_then_closed(base: &Path) {
    let path = base.join("existing.bin");
    std::fs::write(&path, BODY).unwrap();
    let mut file = open_flagged(&path).expect("delete-on-close open of an existing file");
    assert_eq!(read_all(&mut file), BODY);
    drop(file);
    assert!(!path.exists(), "existing delete-on-close file remained");
}

/// Two handles, the flagged one closing first. The other handle keeps its
/// bytes; the name is gone once it closes too. Whether the name is still
/// listed in between is driver behavior and is reported, not asserted.
fn flagged_handle_closes_first(base: &Path) -> bool {
    let path = base.join("two handles.bin");
    std::fs::write(&path, BODY).unwrap();
    let flagged = open_flagged(&path).expect("flagged open beside a reader");
    let mut reader = File::open(&path).expect("second handle");
    drop(flagged);
    let listed = path.exists();
    assert_eq!(read_all(&mut reader), BODY, "second handle lost its bytes");
    drop(reader);
    assert!(!path.exists(), "name remained after the last handle closed");
    listed
}

/// A directory with the flag stays refused on version 8 and is not created.
fn directory_refused(base: &Path) {
    let path = base.join("flagged directory");
    let created = s3drive_winfsp::native_test_api::create_directory_delete_on_close(&path);
    assert!(
        created.is_err(),
        "delete-on-close directory must be refused"
    );
    assert!(!path.exists(), "refused directory was created");
}

/// Everything that runs on the first mount. Returns the in-between listing
/// observation of the two-handle case.
fn first_mount(base: &Path) -> bool {
    created_then_closed(base);
    existing_then_closed(base);
    let listed = flagged_handle_closes_first(base);
    directory_refused(base);
    std::fs::write(base.join("kept.bin"), KEPT).unwrap();
    listed
}

/// After a cold remount none of the deleted names is back. A flagged open of
/// a file this mount has only read may be refused (it has no native object);
/// either outcome is checked for consistency and reported.
fn after_cold_remount(base: &Path) -> &'static str {
    for name in ["~WRD0001 日本語.tmp", "existing.bin", "two handles.bin"] {
        assert!(!base.join(name).exists(), "{name} returned after remount");
    }
    let kept = base.join("kept.bin");
    assert_eq!(std::fs::read(&kept).unwrap(), KEPT);
    match open_flagged(&kept) {
        Ok(file) => {
            drop(file);
            assert!(!kept.exists(), "accepted flagged open did not delete");
            "deleted"
        }
        Err(_) => {
            assert_eq!(std::fs::read(&kept).unwrap(), KEPT);
            "refused"
        }
    }
}

/// Creates a flagged file and journals its bytes; the caller keeps the handle
/// open while the mount process is killed.
fn journaled_before_kill(base: &Path) -> File {
    let mut file = create_flagged(&base.join("survivor.tmp"));
    file.write_all(JOURNALED).unwrap();
    file.sync_all().expect("FlushFileBuffers before the kill");
    file
}

/// Runs the phase 2 acceptance cases on real mounts of one drive.
pub async fn mounted(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use native_mount::{Mount, blocking};
    native_lifecycle::mount_configuration(config, root, s).await;
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    let listed = blocking(&mut mount, move || first_mount(&base)).await;
    mount.stop().await;
    drop(mount);
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    let clean = blocking(&mut mount, move || after_cold_remount(&base)).await;
    // The documented difference from a durable obligation: a mount process
    // that dies before the flagged cleanup leaves the file behind.
    let base = mount.path.clone();
    let held = blocking(&mut mount, move || journaled_before_kill(&base)).await;
    mount.interrupt().await;
    drop(held);
    drop(mount);
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    blocking(&mut mount, move || {
        let survivor = base.join("survivor.tmp");
        assert_eq!(std::fs::read(&survivor).unwrap(), JOURNALED);
    })
    .await;
    mount.stop().await;
    eprintln!(
        "DELETE_ON_CLOSE create=deleted existing=deleted two_handles=deleted listed_between={listed} directory=refused clean_published={clean} killed_before_cleanup=present_exact cold_remount=absent"
    );
}
