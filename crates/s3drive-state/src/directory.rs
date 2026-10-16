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
    _lock: File,
    // Windows handles deny delete sharing, keeping all path components stable.
    _directories: Vec<File>,
}

impl OwnedDirectory {
    #[cfg(any(target_os = "linux", windows))]
    pub(crate) fn acquire(path: &Path) -> Result<Self> {
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
        platform::create_directory(&path)?;
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
        let lock = platform::open_file(&path.join(LOCK), true)?;
        platform::verify_private(&lock, false)?;
        match lock.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock) => return Err("state directory is already in use".into()),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
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
                let file = platform::open_file(&file_path, *name == DATABASE)?;
                platform::verify_private(&file, false)?;
            }
        }
        Ok(Self {
            path,
            _lock: lock,
            _directories: directories,
        })
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub(crate) fn acquire(_: &Path) -> Result<Self> {
        Err("managed local state is supported on Linux and Windows only".into())
    }

    pub(crate) fn database_path(&self) -> PathBuf {
        self.path.join(DATABASE)
    }

    #[cfg(any(target_os = "linux", windows))]
    pub(crate) fn finish_initialization(&self) -> Result<()> {
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
            platform::verify_private(&file, false)?;
            #[cfg(target_os = "linux")]
            platform::restrict_file(&file)?;
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    pub(crate) fn finish_initialization(&self) -> Result<()> {
        Err("managed local state is supported on Linux and Windows only".into())
    }
}

#[cfg(test)]
mod tests;
