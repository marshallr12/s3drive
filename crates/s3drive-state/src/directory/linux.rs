use crate::Result;
use nix::{
    sys::statfs::{self, fstatfs},
    unistd::geteuid,
};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub(super) fn hold_parents(path: &Path) -> Result<Vec<File>> {
    let mut handles = Vec::new();
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        let file = open_directory(ancestor)?;
        let metadata = file.metadata()?;
        if metadata.uid() != 0 && metadata.uid() != geteuid().as_raw() {
            return Err("state path ancestor is owned by another account".into());
        }
        // A root-owned sticky directory such as /tmp is safe for an owned,
        // private child. Other group/world-writable ancestors are refused.
        let sticky_root = metadata.uid() == 0 && metadata.mode() & 0o1000 != 0;
        if metadata.mode() & 0o022 != 0 && !sticky_root {
            return Err("state path ancestor permits other accounts to replace entries".into());
        }
        handles.push(file);
    }
    Ok(handles)
}

pub(super) fn create_directory(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub(super) fn open_directory(path: &Path) -> Result<File> {
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW)
        .open(path)?)
}

pub(super) fn open_file(path: &Path, create: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    if create {
        match options.clone().create_new(true).open(path) {
            Ok(file) => return Ok(file),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(options.open(path)?)
}

pub(super) fn verify_private(file: &File, directory: bool) -> Result<()> {
    let metadata = file.metadata()?;
    if metadata.uid() != geteuid().as_raw()
        || (directory && !metadata.is_dir())
        || (!directory && (!metadata.is_file() || metadata.nlink() != 1))
    {
        return Err("state entry must be owned by this account and have the expected type; file hard links are refused".into());
    }
    if directory {
        if metadata.mode() & 0o777 != 0o700 {
            return Err("state directory must have mode 0700".into());
        }
    } else if metadata.mode() & 0o777 != 0o600 {
        // Legacy SQLite bootstrap used the user's umask. These are known files
        // inside an already-private directory, owned by us and not hard-linked.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub(super) fn verify_local(file: &File, _: &Path) -> Result<()> {
    let kind = fstatfs(file)?.filesystem_type();
    if ![
        statfs::EXT4_SUPER_MAGIC,
        statfs::BTRFS_SUPER_MAGIC,
        statfs::XFS_SUPER_MAGIC,
        statfs::F2FS_SUPER_MAGIC,
        statfs::TMPFS_MAGIC,
    ]
    .contains(&kind)
    {
        return Err("state requires a supported local filesystem (ext, btrfs, xfs, f2fs or tmpfs); network, overlay and FUSE filesystems are refused".into());
    }
    Ok(())
}
