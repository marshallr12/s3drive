//! Exclusive ownership of a private local directory. Never unlink volume.lock:
//! lock identity must remain stable across contenders and process crashes.
#![forbid(unsafe_code)]

use crate::Result;
use std::{
    fs::{self, File, TryLockError},
    path::{Component, Path, PathBuf},
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
use crate::directory_windows as platform;
#[cfg(target_os = "linux")]
use linux as platform;

const DATABASE: &str = "metadata.sqlite3";
const LOCK: &str = "volume.lock";
const FILES: &[&str] = &[
    LOCK,
    DATABASE,
    "metadata.sqlite3-wal",
    "metadata.sqlite3-shm",
    "metadata.sqlite3-journal",
];

pub(crate) struct OwnedDirectory {
    path: PathBuf,
    // State drops SQLite first. These must then close BEFORE _lock releases
    // ownership, so a new same-process owner cannot lose its fresh POSIX locks.
    #[cfg(target_os = "linux")]
    _verified_files: Vec<File>,
    _lock: DirectoryLock,
    // Windows handles deny delete sharing, keeping all path components stable.
    _directories: Vec<File>,
}

struct DirectoryLock(File);
impl Drop for DirectoryLock {
    fn drop(&mut self) {
        // State closes SQLite before dropping OwnedDirectory. Also cover
        // errors after acquisition: inherited descriptors must not prolong an
        // owner that has closed or failed initialization. File close remains
        // the fail-closed fallback if explicit unlock fails.
        let _ = self.0.unlock();
    }
}

impl OwnedDirectory {
    pub(crate) fn verify_staging_storage(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        linux::verify_persistent(
            self._directories
                .last()
                .ok_or("missing owned state directory")?,
        )?;
        // Windows acquisition already requires a local fixed NTFS volume.
        #[cfg(not(any(target_os = "linux", windows)))]
        return Err("staging is supported on Linux and Windows only".into());
        #[cfg(any(target_os = "linux", windows))]
        Ok(())
    }
    #[cfg(any(target_os = "linux", windows))]
    pub(crate) fn acquire(path: &Path) -> Result<Self> {
        Self::acquire_mode(path, true)
    }

    #[cfg(any(target_os = "linux", windows))]
    pub(crate) fn acquire_existing(path: &Path) -> Result<Self> {
        Self::acquire_mode(path, false)
    }

    #[cfg(any(target_os = "linux", windows))]
    fn acquire_mode(path: &Path, create: bool) -> Result<Self> {
        // Native Windows creation APIs use NUL-terminated strings; reject a
        // public API caller's embedded NUL before it can create a prefix path.
        if path.as_os_str().as_encoded_bytes().contains(&0) {
            return Err("state path must not contain NUL".into());
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err("state path must not contain parent traversal".into());
        }
        let path = std::path::absolute(path)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or("state directory must have an existing parent")?;
        let mut directories = platform::hold_parents(parent)?;
        platform::verify_local(
            directories.last().ok_or("missing parent directory")?,
            parent,
        )?;
        if create {
            platform::create_directory(&path)?;
        }
        let root = platform::open_directory(&path)?;
        platform::verify_private(&root, true)?;
        platform::verify_local(&root, &path)?;
        directories.push(root);

        // Validate the directory's purpose before adding any managed file.
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            if !FILES.iter().any(|name| entry.file_name() == *name) {
                return Err("state directory contains an unmanaged entry".into());
            }
        }
        let lock = platform::open_file(&path.join(LOCK), create)?;
        platform::verify_private(&lock, false)?;
        match lock.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => return Err("state directory is already in use".into()),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        let lock = DirectoryLock(lock);
        // Acquire ownership before touching SQLite files. Check sidecars too;
        // SQLite may create/open them without going through Rust's file APIs.
        for name in &FILES[1..] {
            let file_path = path.join(name);
            let exists = match fs::symlink_metadata(&file_path) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                Err(e) => return Err(e.into()),
            };
            if *name == DATABASE || exists {
                let file = platform::open_file(&file_path, create && *name == DATABASE)?;
                platform::verify_private(&file, false)?;
            }
        }
        Ok(Self {
            path,
            _lock: lock,
            _directories: directories,
            #[cfg(target_os = "linux")]
            _verified_files: Vec::new(),
        })
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub(crate) fn acquire(_: &Path) -> Result<Self> {
        Err("managed local state is supported on Linux and Windows only".into())
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub(crate) fn acquire_existing(_: &Path) -> Result<Self> {
        Err("managed local state is supported on Linux and Windows only".into())
    }

    pub(crate) fn database_path(&self) -> PathBuf {
        self.path.join(DATABASE)
    }

    #[cfg(any(target_os = "linux", windows))]
    fn root_file(&self) -> Result<&File> {
        self._directories
            .last()
            .ok_or_else(|| "missing owned root".into())
    }

    #[cfg(any(target_os = "linux", windows))]
    pub(crate) fn finish_initialization(&mut self) -> Result<()> {
        // Recheck SQLite-created sidecars as well as preexisting files. The
        // connection has now established application/schema ownership.
        for name in FILES {
            let path = self.path.join(name);
            match fs::symlink_metadata(&path) {
                Ok(_) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            }
            let file = platform::open_file(&path, false)?;
            #[cfg(target_os = "linux")]
            {
                // Retain BEFORE validation: an initialization error must also
                // drop SQLite before these handles release any of its locks.
                self._verified_files.push(file);
                let file = self._verified_files.last().ok_or("missing verified file")?;
                platform::verify_private(file, false)?;
                platform::restrict_file(file)?;
            }
            #[cfg(windows)]
            platform::verify_private(&file, false)?;
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub(crate) fn finish_initialization(&mut self) -> Result<()> {
        Err("managed local state is supported on Linux and Windows only".into())
    }
}

/// A separate, non-bootable artifact role. Creation NEVER adopts an existing
/// directory; inspection requires its existing marker and completed manifest.
#[cfg(any(target_os = "linux", windows))]
pub(crate) struct SnapshotDirectory {
    path: PathBuf,
    pub(crate) database: File,
    _lock: DirectoryLock,
    _directories: Vec<File>,
}
#[cfg(any(target_os = "linux", windows))]
impl SnapshotDirectory {
    pub(crate) const DATABASE: &'static str = "snapshot.sqlite3";
    const LOCK: &'static str = "snapshot.lock";
    const MANIFEST: &'static str = "snapshot.json";

    pub(crate) fn create(path: &Path, source: &OwnedDirectory) -> Result<Self> {
        Self::acquire(path, Some(source))
    }
    pub(crate) fn inspect(path: &Path) -> Result<Self> {
        Self::acquire(path, None)
    }
    fn acquire(path: &Path, source: Option<&OwnedDirectory>) -> Result<Self> {
        if path.as_os_str().as_encoded_bytes().contains(&0)
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err("invalid snapshot path".into());
        }
        let path = std::path::absolute(path)?;
        let parent = path.parent().ok_or("missing snapshot parent")?;
        let mut directories = platform::hold_parents(parent)?;
        platform::verify_local(directories.last().ok_or("missing snapshot parent")?, parent)?;
        if let Some(source) = source {
            // Compare actual handles, including Windows case/short-name aliases.
            for ancestor in &directories {
                if platform::same_file(ancestor, source.root_file()?)? {
                    return Err("snapshot output must be outside authority state".into());
                }
            }
            platform::create_new_directory(&path)?;
        }
        let root = platform::open_directory(&path)?;
        platform::verify_private(&root, true)?;
        platform::verify_local(&root, &path)?;
        #[cfg(target_os = "linux")]
        platform::verify_persistent(&root)?;
        directories.push(root);
        for entry in fs::read_dir(&path)? {
            let name = entry?.file_name();
            if ![Self::LOCK, Self::DATABASE, Self::MANIFEST]
                .iter()
                .any(|n| name == *n)
            {
                return Err("snapshot contains an unexpected or recovery entry".into());
            }
        }
        let lock = if source.is_some() {
            platform::open_new_file(&path.join(Self::LOCK))?
        } else {
            platform::open_file(&path.join(Self::LOCK), false)?
        };
        Self::strict_file(&lock)?;
        lock.try_lock()?;
        let lock = DirectoryLock(lock);
        let database = if source.is_some() {
            platform::open_new_file(&path.join(Self::DATABASE))?
        } else {
            let manifest = platform::open_read_file(&path.join(Self::MANIFEST))?;
            Self::strict_file(&manifest)?;
            platform::open_read_file(&path.join(Self::DATABASE))?
        };
        Self::strict_file(&database)?;
        Ok(Self {
            path,
            database,
            _lock: lock,
            _directories: directories,
        })
    }
    fn strict_file(file: &File) -> Result<()> {
        platform::verify_private(file, false)?;
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            if file.metadata()?.mode() & 0o077 != 0 {
                return Err("snapshot file permits access by another account".into());
            }
        }
        Ok(())
    }
    pub(crate) fn database_path(&self) -> PathBuf {
        self.path.join(Self::DATABASE)
    }
    pub(crate) fn recheck_database(&self) -> Result<()> {
        let file = platform::open_read_file(&self.database_path())?;
        Self::strict_file(&file)?;
        if !platform::same_file(&file, &self.database)? {
            return Err("snapshot database identity changed".into());
        }
        Ok(())
    }
    pub(crate) fn read_manifest(&self) -> Result<Vec<u8>> {
        use std::io::Read;
        let mut file = platform::open_read_file(&self.path.join(Self::MANIFEST))?;
        Self::strict_file(&file)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(65_537)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 65_536 {
            return Err("snapshot manifest exceeds limit".into());
        }
        Ok(bytes)
    }
    pub(crate) fn write_manifest(
        &self,
        bytes: &[u8],
        written: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        use std::io::Write;
        if bytes.len() > 65_536 {
            return Err("snapshot manifest exceeds limit".into());
        }
        let mut file = platform::open_new_file(&self.path.join(Self::MANIFEST))?;
        Self::strict_file(&file)?;
        file.write_all(bytes)?;
        written()?;
        file.sync_all()?;
        #[cfg(target_os = "linux")]
        {
            for directory in self._directories.iter().rev().take(2) {
                directory.sync_all()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

/// Read a bounded owner-private file without following links. This never creates
/// or repairs it. Contents must not appear in caller errors or diagnostics.
#[cfg(any(target_os = "linux", windows))]
pub fn read_private_secret(path: &Path, limit: usize) -> crate::Result<Vec<u8>> {
    use std::io::Read;
    if !path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir))
        || path.as_os_str().as_encoded_bytes().contains(&0)
    {
        return Err("private secret requires an absolute local path".into());
    }
    if !(1..=65_536).contains(&limit) {
        return Err("invalid private secret size limit".into());
    }
    let parent = path.parent().ok_or("missing secret parent")?;
    let _parents = platform::hold_parents(parent)?;
    let mut file = platform::open_file(path, false)?;
    platform::verify_private(&file, false)?;
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        if file.metadata()?.mode() & 0o077 != 0 {
            return Err("private secret permits access by another account".into());
        }
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("private secret exceeds its size limit".into());
    }
    Ok(bytes)
}
/// Create one new token in a private directory. Never overwrite or tighten an
/// existing secret. The directory is private before any token bytes are written.
#[cfg(any(target_os = "linux", windows))]
pub fn create_private_secret(directory: &Path, bytes: &[u8]) -> crate::Result<PathBuf> {
    if bytes.len() > 66 {
        return Err("invalid private secret length".into());
    }
    create_private_file(directory, "authority.token", bytes, false)
}

/// Write one new bounded owner-private document; never overwrite an existing
/// file. The filename is one normal component and the parent must exist.
#[cfg(any(target_os = "linux", windows))]
pub fn create_private_document(
    directory: &Path,
    filename: &str,
    bytes: &[u8],
) -> crate::Result<PathBuf> {
    create_private_file(directory, filename, bytes, true)
}
#[cfg(any(target_os = "linux", windows))]
fn create_private_file(
    directory: &Path,
    filename: &str,
    bytes: &[u8],
    persistent: bool,
) -> crate::Result<PathBuf> {
    use std::io::Write;
    if filename.is_empty()
        || filename.contains(['/', '\\', '\0', ':'])
        || filename == "."
        || filename == ".."
        || !directory.is_absolute()
        || directory
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        || directory.as_os_str().as_encoded_bytes().contains(&0)
        || bytes.len() > 65_536
    {
        return Err("invalid private secret destination or length".into());
    }
    let _parents = platform::hold_parents(directory.parent().ok_or("missing secret parent")?)?;
    platform::create_directory(directory)?;
    let held = platform::open_directory(directory)?;
    platform::verify_private(&held, true)?;
    platform::verify_local(&held, directory)?;
    #[cfg(target_os = "linux")]
    if persistent {
        platform::verify_persistent(&held)?;
    }
    #[cfg(windows)]
    let _ = persistent; // verify_local already requires a local fixed NTFS volume.
    let path = directory.join(filename);
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let mut file = options.open(&path)?;
    platform::verify_private(&file, false)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(target_os = "linux")]
    {
        held.sync_all()?;
        if let Some(parent) = _parents.last() {
            parent.sync_all()?;
        }
    }
    Ok(path)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn read_private_secret(_: &Path, _: usize) -> crate::Result<Vec<u8>> {
    Err("private secrets require Linux or Windows".into())
}
#[cfg(not(any(target_os = "linux", windows)))]
pub fn create_private_secret(_: &Path, _: &[u8]) -> crate::Result<PathBuf> {
    Err("private secrets require Linux or Windows".into())
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn create_private_document(_: &Path, _: &str, _: &[u8]) -> crate::Result<PathBuf> {
    Err("private documents require Linux or Windows".into())
}

#[cfg(test)]
mod secret_tests {
    use super::*;
    #[test]
    fn private_documents_are_bounded_create_new_and_refuse_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let directory = tmp.path().join("plans");
        for name in ["", ".", "..", "../plan", "a/b", "a\\b", "a:b", "a\0b"] {
            assert!(create_private_document(&directory, name, b"test").is_err());
        }
        assert!(!directory.exists());
        assert!(create_private_document(&directory, "plan.toml", &vec![0; 65_537]).is_err());
        let bytes = vec![b'x'; 65_536];
        let path = create_private_document(&directory, "plan.toml", &bytes).unwrap();
        assert_eq!(read_private_secret(&path, 65_536).unwrap(), bytes);
        assert!(create_private_document(&directory, "plan.toml", b"replacement").is_err());
        assert_eq!(read_private_secret(&path, 65_536).unwrap(), bytes);
        assert!(create_private_secret(&directory, &[b'x'; 67]).is_err());
    }
    #[test]
    fn private_secret_round_trip_does_not_overwrite_or_print_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path =
            create_private_secret(&dir.path().join("secrets"), b"private fixture bytes").unwrap();
        assert_eq!(
            read_private_secret(&path, 66).unwrap(),
            b"private fixture bytes"
        );
        assert!(create_private_secret(path.parent().unwrap(), b"replacement").is_err());
        assert_eq!(
            read_private_secret(&path, 66).unwrap(),
            b"private fixture bytes"
        );
        assert!(read_private_secret(&path, 3).is_err());
    }
    #[test]
    fn private_tls_material_has_a_finite_read_bound() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_private_secret(&dir.path().join("tls"), b"fixture").unwrap();
        let bytes = vec![b'x'; 8192];
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(read_private_secret(&path, 65_536).unwrap(), bytes);
        assert!(read_private_secret(&path, 8191).is_err());
        assert!(read_private_secret(&path, 65_537).is_err());
        assert!(read_private_secret(&path, 0).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn private_secret_refuses_permissions_symlinks_and_hard_links() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let path = create_private_secret(&dir.path().join("secrets"), b"private").unwrap();
        let link = dir.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(read_private_secret(&link, 66).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_private_secret(&path, 66).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let hard = dir.path().join("hard");
        std::fs::hard_link(&path, hard).unwrap();
        assert!(read_private_secret(&path, 66).is_err());
    }
}
