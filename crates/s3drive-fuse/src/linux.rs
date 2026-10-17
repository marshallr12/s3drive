use fuser::{
    BsdFileFlags, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation,
    INodeNo, LockOwner, MountOption, OpenAccMode, OpenFlags, RenameFlags, ReplyAttr, ReplyCreate,
    ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request,
    TimeOrNow, WriteFlags,
};
use s3drive_fs::{Error, Kind, Node, OpenFile, ReadOnlyFs};
use s3drive_s3::{ErrorKind, ObjectMetadata};
use std::{
    collections::HashMap,
    ffi::OsStr,
    io,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::runtime::Handle;

const TTL: Duration = Duration::ZERO;
const MAX_HANDLES: usize = 256;
const MAX_DIRECTORY_REFERENCES: usize = 100_000;

enum OpenHandle {
    File(Box<OpenFile>),
    Directory { ino: u64, entries: Vec<Arc<Node>> },
}
struct Handles {
    next: u64,
    entries: HashMap<u64, Arc<OpenHandle>>,
    directory_references: usize,
}
impl Handles {
    fn insert(&mut self, handle: OpenHandle) -> Result<u64, Errno> {
        let count = match &handle {
            OpenHandle::Directory { entries, .. } => entries.len(),
            _ => 0,
        };
        if self.entries.len() >= MAX_HANDLES
            || self.directory_references + count > MAX_DIRECTORY_REFERENCES
        {
            return Err(Errno::EMFILE);
        }
        let id = self.next;
        self.next = id.checked_add(1).ok_or(Errno::EMFILE)?;
        self.directory_references += count;
        self.entries.insert(id, Arc::new(handle));
        Ok(id)
    }
    fn remove(&mut self, id: u64) {
        if let Some(handle) = self.entries.remove(&id)
            && let OpenHandle::Directory { entries, .. } = &*handle
        {
            self.directory_references -= entries.len();
        }
    }
}
struct Adapter {
    fs: ReadOnlyFs,
    runtime: Handle,
    cancelled: tokio::sync::watch::Receiver<bool>,
    handles: Mutex<Handles>,
    uid: u32,
    gid: u32,
}
impl Adapter {
    fn wait<T>(
        &self,
        future: impl std::future::Future<Output = s3drive_fs::Result<T>>,
    ) -> Result<T, Errno> {
        let mut cancelled = self.cancelled.clone();
        self.runtime.block_on(async {
            if *cancelled.borrow() {
                return Err(Errno::ECANCELED);
            }
            tokio::select! {
                _ = cancelled.changed() => Err(Errno::ECANCELED),
                result = future => result.map_err(errno),
            }
        })
    }
    fn handle(&self, id: FileHandle) -> Result<Arc<OpenHandle>, Errno> {
        self.handles
            .lock()
            .map_err(|_| Errno::EIO)?
            .entries
            .get(&id.0)
            .cloned()
            .ok_or(Errno::EBADF)
    }
    fn add_handle(&self, handle: OpenHandle) -> Result<u64, Errno> {
        self.handles.lock().map_err(|_| Errno::EIO)?.insert(handle)
    }
    fn attr(&self, node: &Node, metadata: Option<&ObjectMetadata>) -> FileAttr {
        let size = metadata.map_or(0, |m| m.size);
        let time = metadata
            .and_then(|m| m.modified_unix_seconds)
            .and_then(|s| {
                if s >= 0 {
                    SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(s as u64))
                } else {
                    SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(s.unsigned_abs()))
                }
            })
            .unwrap_or(SystemTime::UNIX_EPOCH);
        FileAttr {
            ino: INodeNo(node.id),
            size,
            blocks: size.div_ceil(512),
            atime: time,
            mtime: time,
            ctime: time,
            crtime: time,
            kind: file_type(node.kind),
            perm: if node.kind == Kind::Directory {
                if self.fs.writable_directory(node) {
                    0o700
                } else {
                    0o500
                }
            } else {
                if self.fs.writable(node) { 0o600 } else { 0o400 }
            },
            nlink: if node.kind == Kind::Directory { 2 } else { 1 },
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        }
    }
    fn attributes(&self, ino: INodeNo, fh: Option<FileHandle>) -> Result<FileAttr, Errno> {
        if let Some(fh) = fh {
            let handle = self.handle(fh)?;
            if let OpenHandle::File(file) = &*handle {
                if file.node.id != ino.0 {
                    return Err(Errno::EBADF);
                }
                let mut attr =
                    self.attr(&file.node, Some(&file.current_metadata().map_err(errno)?));
                if file.is_unlinked().map_err(errno)? {
                    attr.nlink = 0;
                }
                return Ok(attr);
            }
        }
        // Linux may issue fstat/getattr without FUSE_GETATTR_FH even for an
        // open, unlinked inode. The name is gone, but its retained handle still
        // supplies metadata. Never reauthorize a deleted node through its path.
        let retained = self
            .handles
            .lock()
            .map_err(|_| Errno::EIO)?
            .entries
            .values()
            .find(|handle| matches!(&***handle, OpenHandle::File(file) if file.node.id == ino.0))
            .cloned();
        if let Some(handle) = retained
            && let OpenHandle::File(file) = &*handle
            && file.is_unlinked().map_err(errno)?
        {
            let mut attr = self.attr(&file.node, Some(&file.current_metadata().map_err(errno)?));
            attr.nlink = 0;
            return Ok(attr);
        }
        let node = self.fs.node(ino.0).map_err(errno)?;
        let metadata = self.wait(self.fs.metadata(ino.0))?;
        Ok(self.attr(&node, metadata.as_ref()))
    }
}
impl Filesystem for Adapter {
    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        match self.wait(self.fs.capacity()) {
            Ok(Some(capacity)) => reply.statfs(
                capacity.total.div_ceil(4096),
                capacity.free / 4096,
                capacity.free / 4096,
                0,
                0,
                4096,
                255,
                4096,
            ),
            Ok(None) => reply.statfs(0, 0, 0, 0, 0, 4096, 255, 4096),
            Err(error) => reply.error(error),
        }
    }
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let result = name
            .to_str()
            .ok_or(Errno::EINVAL)
            .and_then(|name| self.wait(self.fs.lookup(parent.0, name)))
            .and_then(|node| self.attributes(INodeNo(node.id), None));
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(e) => reply.error(e),
        }
    }
    fn getattr(&self, _req: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {
        match self.attributes(ino, fh) {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(e) => reply.error(e),
        }
    }
    fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        let result = self
            .wait(async {
                let file = if flags.acc_mode() == OpenAccMode::O_RDONLY {
                    self.fs.open(ino.0).await?
                } else {
                    self.fs.open_write(ino.0).await?
                };
                if flags.0 & nix::libc::O_TRUNC != 0 {
                    file.truncate(0).await?;
                }
                Ok(file)
            })
            .and_then(|file| self.add_handle(OpenHandle::File(Box::new(file))));
        match result {
            Ok(fh) => reply.opened(FileHandle(fh), FopenFlags::FOPEN_DIRECT_IO),
            Err(e) => reply.error(e),
        }
    }
    fn mkdir(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let result = name
            .to_str()
            .ok_or(Errno::EINVAL)
            .and_then(|name| self.wait(self.fs.create_directory(parent.0, name)))
            .and_then(|node| self.attributes(INodeNo(node.id), None));
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(e) => reply.error(e),
        }
    }
    fn rename(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        new_parent: INodeNo,
        new_name: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        if !self.fs.namespace_mutation() || !(flags & !RenameFlags::RENAME_NOREPLACE).is_empty() {
            reply.error(Errno::EOPNOTSUPP);
            return;
        }
        let result = name
            .to_str()
            .zip(new_name.to_str())
            .ok_or(Errno::EINVAL)
            .and_then(|(name, new_name)| {
                self.wait(self.fs.rename_entry(
                    parent.0,
                    name,
                    new_parent.0,
                    new_name,
                    !flags.contains(RenameFlags::RENAME_NOREPLACE),
                    |a, b| a == b,
                ))
            });
        match result {
            Ok(_) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        if !self.fs.namespace_mutation() {
            reply.error(Errno::EOPNOTSUPP);
            return;
        }
        match name
            .to_str()
            .ok_or(Errno::EINVAL)
            .and_then(|n| self.wait(self.fs.remove_entry(parent.0, n, false)))
        {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        if !self.fs.namespace_mutation() {
            reply.error(Errno::EOPNOTSUPP);
            return;
        }
        match name
            .to_str()
            .ok_or(Errno::EINVAL)
            .and_then(|n| self.wait(self.fs.remove_entry(parent.0, n, true)))
        {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn fsyncdir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        _fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        if !self.fs.namespace_mutation() {
            reply.error(Errno::EOPNOTSUPP);
            return;
        }
        match self.wait(self.fs.sync_namespace()) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn create(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        let result = name
            .to_str()
            .ok_or(Errno::EINVAL)
            .and_then(|name| {
                self.wait(Box::pin(self.fs.create_file(
                    parent.0,
                    name,
                    |a, b| a == b,
                    flags & nix::libc::O_ACCMODE != nix::libc::O_RDONLY,
                )))
            })
            .and_then(|file| {
                let attr = self.attr(&file.node, Some(&file.current_metadata().map_err(errno)?));
                self.add_handle(OpenHandle::File(Box::new(file)))
                    .map(|fh| (attr, fh))
            });
        match result {
            Ok((attr, fh)) => reply.created(
                &TTL,
                &attr,
                Generation(0),
                FileHandle(fh),
                FopenFlags::FOPEN_DIRECT_IO,
            ),
            Err(error) => reply.error(error),
        }
    }
    fn read(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        let result = self.handle(fh).and_then(|handle| match &*handle {
            OpenHandle::File(file) if file.node.id == ino.0 => {
                self.wait(file.read(offset, size as usize))
            }
            _ => Err(Errno::EBADF),
        });
        match result {
            Ok(bytes) => reply.data(&bytes),
            Err(e) => reply.error(e),
        }
    }
    fn write(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: WriteFlags,
        flags: OpenFlags,
        _owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        let result = self.handle(fh).and_then(|handle| match &*handle {
            OpenHandle::File(file) if file.node.id == ino.0 => self.wait(file.write(
                offset,
                Arc::from(data),
                flags.0 & nix::libc::O_APPEND != 0,
                false,
            )),
            _ => Err(Errno::EBADF),
        });
        match result {
            Ok(count) => reply.written(count as u32),
            Err(error) => reply.error(error),
        }
    }
    fn setattr(
        &self,
        _req: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        if mode.is_some()
            || uid.is_some()
            || gid.is_some()
            || atime.is_some()
            || (mtime.is_some() && !(size.is_some() && matches!(mtime, Some(TimeOrNow::Now))))
            || (ctime.is_some() && size.is_none())
            || crtime.is_some()
            || chgtime.is_some()
            || bkuptime.is_some()
            || flags.is_some()
        {
            reply.error(Errno::EOPNOTSUPP);
            return;
        }
        let result = (|| {
            let size = size.ok_or(Errno::EOPNOTSUPP)?;
            if let Some(fh) = fh {
                let handle = self.handle(fh)?;
                match &*handle {
                    OpenHandle::File(file) if file.node.id == ino.0 => {
                        self.wait(file.truncate(size))?
                    }
                    _ => return Err(Errno::EBADF),
                }
            } else {
                self.wait(async {
                    let file = self.fs.open_write(ino.0).await?;
                    file.truncate(size).await
                })?;
            }
            self.attributes(ino, fh)
        })();
        match result {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(error) => reply.error(error),
        }
    }
    fn flush(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        self.flush_handle(ino, fh, reply, true);
    }
    fn fsync(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        self.flush_handle(ino, fh, reply, false);
    }
    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _owner: Option<LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        self.release_handle(fh, reply);
    }
    fn opendir(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        if flags.acc_mode() != OpenAccMode::O_RDONLY {
            reply.error(Errno::EROFS);
            return;
        }
        let result = self.wait(self.fs.list(ino.0)).and_then(|children| {
            let node = self.fs.node(ino.0).map_err(errno)?;
            let parent = self.fs.node(node.parent).map_err(errno)?;
            let mut entries = Vec::with_capacity(children.len() + 2);
            entries.push(node);
            entries.push(parent);
            entries.extend(children);
            self.add_handle(OpenHandle::Directory {
                ino: ino.0,
                entries,
            })
        });
        match result {
            Ok(fh) => reply.opened(FileHandle(fh), FopenFlags::empty()),
            Err(e) => reply.error(e),
        }
    }
    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let handle = match self.handle(fh) {
            Ok(handle) => handle,
            Err(e) => {
                reply.error(e);
                return;
            }
        };
        let OpenHandle::Directory {
            ino: directory_ino,
            entries,
        } = &*handle
        else {
            reply.error(Errno::EBADF);
            return;
        };
        if *directory_ino != ino.0 {
            reply.error(Errno::EBADF);
            return;
        }
        let Ok(offset) = usize::try_from(offset) else {
            reply.error(Errno::EINVAL);
            return;
        };
        if offset > entries.len() {
            reply.error(Errno::EINVAL);
            return;
        }
        for (index, entry) in entries.iter().enumerate().skip(offset) {
            let name = match index {
                0 => ".",
                1 => "..",
                _ => &entry.name,
            };
            if reply.add(
                INodeNo(entry.id),
                (index + 1) as u64,
                file_type(entry.kind),
                name,
            ) {
                break;
            }
        }
        reply.ok();
    }
    fn releasedir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        self.release_handle(fh, reply);
    }
}
impl Adapter {
    fn release_handle(&self, fh: FileHandle, reply: ReplyEmpty) {
        match self.handles.lock() {
            Ok(mut handles) => {
                handles.remove(fh.0);
                reply.ok();
            }
            Err(_) => reply.error(Errno::EIO),
        }
    }
    fn flush_handle(&self, ino: INodeNo, fh: FileHandle, reply: ReplyEmpty, closing: bool) {
        match self.handle(fh) {
            Ok(handle) if matches!(&*handle, OpenHandle::File(file) if file.node.id == ino.0) => {
                let result = match &*handle {
                    OpenHandle::File(file) => {
                        if closing {
                            self.wait(file.flush_on_close())
                        } else {
                            self.wait(file.flush())
                        }
                    }
                    _ => Err(Errno::EBADF),
                };
                match result {
                    Ok(()) => reply.ok(),
                    Err(error) => reply.error(error),
                }
            }
            _ => reply.error(Errno::EBADF),
        }
    }
}
fn file_type(kind: Kind) -> FileType {
    match kind {
        Kind::Directory => FileType::Directory,
        Kind::File => FileType::RegularFile,
    }
}
fn errno(error: Error) -> Errno {
    if let Error::Managed(detail) = &error {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr().lock(),
            "Managed operation failed; local data retained: {detail}"
        );
    }
    match error {
        Error::NotFound => Errno::ENOENT,
        Error::AlreadyExists => Errno::EEXIST,
        Error::NotDirectory => Errno::ENOTDIR,
        Error::IsDirectory => Errno::EISDIR,
        Error::NotEmpty => Errno::ENOTEMPTY,
        Error::DeletePending => Errno::EBUSY,
        Error::InvalidOperation => Errno::EINVAL,
        Error::InvalidName => Errno::EILSEQ,
        Error::AmbiguousName => Errno::EIO,
        Error::ResourceLimit => Errno::ENOSPC,
        Error::NoSpace => Errno::ENOSPC,
        Error::Timeout => Errno::ETIMEDOUT,
        Error::ReadOnly => Errno::EROFS,
        Error::Conflict => Errno::ESTALE,
        Error::TooLarge => Errno::EFBIG,
        Error::Busy => Errno::EAGAIN,
        Error::Storage(error) => match error.kind {
            ErrorKind::PermissionDenied => Errno::EACCES,
            ErrorKind::NotFound => Errno::ENOENT,
            ErrorKind::RevisionChanged => Errno::ESTALE,
            ErrorKind::Timeout => Errno::ETIMEDOUT,
            ErrorKind::InvalidInput | ErrorKind::InvalidRange => Errno::EINVAL,
            ErrorKind::Unsupported => Errno::EOPNOTSUPP,
            _ => Errno::EIO,
        },
        _ => Errno::EIO,
    }
}

struct UnmountGuard {
    unmounter: fuser::SessionUnmounter,
    cancel: tokio::sync::watch::Sender<bool>,
}
impl UnmountGuard {
    fn unmount(&mut self) -> io::Result<()> {
        let _ = self.cancel.send(true);
        self.unmounter.unmount()
    }
}
impl Drop for UnmountGuard {
    fn drop(&mut self) {
        let _ = self.unmount();
    }
}

/// Serve until normal external unmount, Ctrl-C or SIGTERM. All callbacks run on
/// four FUSE threads; network futures use the caller's long-lived Tokio runtime.
pub async fn run(fs: ReadOnlyFs, mountpoint: &Path) -> io::Result<()> {
    let writable = !fs.is_read_only();
    let cache = fs.cache_control();
    let shutdown = fs.shutdown_writes();
    // Install signals before mounting so termination cannot leave a live mount.
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mountpoint = mountpoint.to_path_buf();
    let runtime = Handle::current();
    let (cancel, cancelled) = tokio::sync::watch::channel(false);
    let (session, unmounter) = tokio::task::spawn_blocking(move || {
        let meta = std::fs::symlink_metadata(&mountpoint)?;
        if !meta.is_dir()
            || meta.uid() != nix::unistd::geteuid().as_raw()
            || std::fs::read_dir(&mountpoint)?.next().is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mountpoint must be an existing empty directory owned by this user, not a symlink",
            ));
        }
        let read_only = fs.is_read_only();
        let adapter = Adapter {
            fs,
            runtime,
            cancelled,
            handles: Mutex::new(Handles {
                next: 1,
                entries: HashMap::new(),
                directory_references: 0,
            }),
            uid: meta.uid(),
            gid: meta.gid(),
        };
        // No allow_other, executable bits, device nodes, suid, or writeback cache.
        let mut options = fuser::Config::default();
        options.mount_options = vec![
            if read_only {
                MountOption::RO
            } else {
                MountOption::RW
            },
            MountOption::NoDev,
            MountOption::NoSuid,
            MountOption::NoExec,
            MountOption::DefaultPermissions,
            MountOption::NoAtime,
            MountOption::FSName("s3drive".into()),
        ];
        options.n_threads = Some(4);
        let mut session = fuser::Session::new(adapter, mountpoint, &options)?;
        let unmounter = session.unmount_callable();
        Ok::<_, io::Error>((session, unmounter))
    })
    .await
    .map_err(io::Error::other)??;
    let mut guard = UnmountGuard { unmounter, cancel };
    let mut running = tokio::task::spawn_blocking(move || session.run());
    eprintln!(
        "Mounted {} S3Drive. Use Ctrl-C or fusermount3 -u to unmount.",
        if writable { "read-write" } else { "read-only" }
    );
    tokio::select! {
        result = &mut running => { result.map_err(io::Error::other)??; }
        _ = interrupt.recv() => { guard.unmount()?; running.await.map_err(io::Error::other)??; }
        _ = terminate.recv() => { guard.unmount()?; running.await.map_err(io::Error::other)??; }
    }
    shutdown.await.map_err(io::Error::other)?;
    if let Some(cache) = cache {
        cache.close().await.map_err(io::Error::other)?;
    }
    Ok(())
}
