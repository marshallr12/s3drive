//! Windows desktop-application create, copy, attribute and time requests against
//! the real WinFsp mount; loopback backend only. These are the Win32 calls that
//! Office and `CopyFile` make. Not an Office GUI, physical or provider test.
use super::*;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::time::{Duration as StdDuration, SystemTime};

const MAXIMUM_ALLOWED: u32 = 0x0200_0000;
const GENERIC_ALL: u32 = 0x1000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const READONLY: u32 = 0x1;
const HIDDEN: u32 = 0x2;
const TEMPORARY: u32 = 0x100;
const DOCUMENT: &[u8] = b"first saved document\r\n\0binary tail";
const REVISED: &[u8] = b"revised document written through a temporary file";

/// Creates a new file with an explicit access mask and attributes, writes
/// `body`, and reads it back through the same handle where the mask allows.
fn create(path: &Path, access: u32, attributes: u32, body: &[u8]) {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(access)
        .attributes(attributes)
        .open(path)
        .unwrap_or_else(|e| panic!("create {path:?} access={access:#x}: {e}"));
    file.write_all(body).unwrap();
    if access != GENERIC_WRITE | WRITE_DAC | WRITE_OWNER {
        let mut read = Vec::new();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.read_to_end(&mut read).unwrap();
        assert_eq!(read, body, "dirty read through {access:#x} handle");
    }
}

/// Access masks requested by MAXIMUM_ALLOWED opens, CopyFile and Office.
fn access_masks(base: &Path) {
    create(
        &base.join("maximum allowed.bin"),
        MAXIMUM_ALLOWED,
        0,
        DOCUMENT,
    );
    create(&base.join("generic all.bin"), GENERIC_ALL, 0, DOCUMENT);
    let security = GENERIC_WRITE | WRITE_DAC | WRITE_OWNER;
    create(&base.join("write dac owner.bin"), security, 0, DOCUMENT);
    for name in [
        "maximum allowed.bin",
        "generic all.bin",
        "write dac owner.bin",
    ] {
        assert_eq!(std::fs::read(base.join(name)).unwrap(), DOCUMENT);
    }
}

/// Temporary and hidden creates: accepted and reported while mounted.
fn attributes(base: &Path) {
    let temporary = base.join("D1834730");
    create(&temporary, GENERIC_ALL, TEMPORARY, REVISED);
    let seen = std::fs::metadata(&temporary).unwrap().file_attributes();
    assert_ne!(seen & TEMPORARY, 0, "temporary attribute not reported");
    let lock = base.join("~$document 日本語.docx");
    create(&lock, GENERIC_ALL, HIDDEN, b"owner");
    let seen = std::fs::metadata(&lock).unwrap().file_attributes();
    assert_ne!(seen & HIDDEN, 0, "hidden attribute not reported");
    std::fs::remove_file(&lock).unwrap();
    assert!(!lock.exists());
}

/// CopyFileEx into and within the mount, as Explorer paste and `copy` do.
fn copies(base: &Path, outside: &Path) {
    let source = outside.join("copy-source.bin");
    std::fs::write(&source, DOCUMENT).unwrap();
    let pasted = base.join("pasted 日本語.bin");
    assert_eq!(
        std::fs::copy(&source, &pasted).expect("CopyFileEx into the mount"),
        DOCUMENT.len() as u64
    );
    assert_eq!(std::fs::read(&pasted).unwrap(), DOCUMENT);
    let duplicate = base.join("duplicate.bin");
    std::fs::copy(&pasted, &duplicate).expect("CopyFileEx within the mount");
    assert_eq!(std::fs::read(&duplicate).unwrap(), DOCUMENT);
}

/// Times are current after a write, explicit times are honored while mounted,
/// and the unsupported READONLY attribute is refused instead of ignored.
fn times(base: &Path) {
    let path = base.join("maximum allowed.bin");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let age = SystemTime::now().duration_since(modified).unwrap();
    assert!(
        age < StdDuration::from_secs(600),
        "stale write time {age:?}"
    );
    let chosen = SystemTime::UNIX_EPOCH + StdDuration::from_secs(1_700_000_000);
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(chosen).expect("SetFileTime");
    drop(file);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        chosen
    );
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    assert!(
        std::fs::set_permissions(&path, permissions).is_err(),
        "READONLY must be refused explicitly"
    );
    let seen = std::fs::metadata(&path).unwrap().file_attributes();
    assert_eq!(seen & READONLY, 0);
}

/// Office-style save: write a temporary file, then rename it over the document.
fn save_replace(base: &Path) {
    let document = base.join("document 日本語.docx");
    create(&document, GENERIC_ALL, 0, DOCUMENT);
    std::fs::rename(base.join("D1834730"), &document).expect("replace-rename");
    assert_eq!(std::fs::read(&document).unwrap(), REVISED);
    assert!(!base.join("D1834730").exists());
}

/// Runs every probe on one mount, then verifies the bytes after a cold remount.
pub async fn mounted(config: &s3drive_core::Config, root: &Path, s: &Server) {
    use native_mount::{Mount, blocking};
    native_lifecycle::mount_configuration(config, root, s).await;
    let mut mount = Mount::start(root).await;
    let (base, outside) = (mount.path.clone(), root.to_owned());
    blocking(&mut mount, move || {
        access_masks(&base);
        attributes(&base);
        copies(&base, &outside);
        times(&base);
        save_replace(&base);
    })
    .await;
    mount.stop().await;
    drop(mount);
    let mut mount = Mount::start(root).await;
    let base = mount.path.clone();
    blocking(&mut mount, move || {
        assert_eq!(
            std::fs::read(base.join("document 日本語.docx")).unwrap(),
            REVISED
        );
        for name in ["pasted 日本語.bin", "duplicate.bin", "write dac owner.bin"] {
            assert_eq!(std::fs::read(base.join(name)).unwrap(), DOCUMENT);
        }
    })
    .await;
    mount.stop().await;
    eprintln!(
        "DESKTOP_APPLICATION maximum_allowed=ok write_dac=ok temporary=ok hidden=ok copyfile=ok set_time=ok readonly=refused replace=ok cold_read=exact"
    );
}
