#![forbid(unsafe_code)]
use crate::{names::valid_component, native};
use s3drive_fs::{Error, Kind, NativeFileObject, Node, OpenFile, ROOT, ReadOnlyFs};
use s3drive_s3::{ErrorKind, MAX_READ_BYTES, ObjectMetadata};
use std::{
    collections::BTreeMap,
    ffi::c_void,
    future::Future,
    os::windows::fs::MetadataExt,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use windows::Win32::{Foundation::*, Storage::FileSystem::*};
use winfsp::{
    FspError, Result, U16CStr,
    filesystem::{
        DirInfo, DirMarker, FileInfo, FileSecurity, FileSystemContext, OpenFileInfo, VolumeInfo,
        WideNameInfo,
    },
    host::{FileSystemHost, VolumeParams},
};

struct Entry {
    name: Vec<u16>,
    info: FileInfo,
}
struct Snapshot {
    entries: Vec<Entry>,
    _permit: OwnedSemaphorePermit,
}
struct FileContext {
    node: Arc<Node>,
    file: Option<Box<OpenFile>>,
    native: Option<NativeFileObject>,
    info: FileInfo,
    directory: Mutex<Option<Snapshot>>,
    _permit: OwnedSemaphorePermit,
}
struct Adapter {
    fs: ReadOnlyFs,
    runtime: tokio::runtime::Handle,
    cancelled: watch::Receiver<bool>,
    descriptor: Vec<u8>,
    handles: Arc<Semaphore>,
    entries: Arc<Semaphore>,
}
fn rename_error(error: Error) -> FspError {
    use std::io::Write;
    let _ = writeln!(std::io::stderr().lock(), "Namespace rename failed: {error}");
    storage_error(error)
}
fn storage_error(error: Error) -> FspError {
    if let Error::Managed(detail) = &error {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr().lock(),
            "Managed operation failed; local data retained: {detail}"
        );
    }
    match error {
        Error::Storage(e) => match e.kind {
            ErrorKind::PermissionDenied => STATUS_ACCESS_DENIED,
            ErrorKind::NotFound => STATUS_OBJECT_NAME_NOT_FOUND,
            ErrorKind::RevisionChanged => STATUS_FILE_INVALID,
            ErrorKind::Timeout => STATUS_IO_TIMEOUT,
            ErrorKind::InvalidInput | ErrorKind::InvalidRange => STATUS_INVALID_PARAMETER,
            _ => STATUS_UNEXPECTED_IO_ERROR,
        },
        Error::NotFound => STATUS_OBJECT_NAME_NOT_FOUND,
        Error::AlreadyExists => STATUS_OBJECT_NAME_COLLISION,
        Error::NotDirectory => STATUS_NOT_A_DIRECTORY,
        Error::IsDirectory => STATUS_FILE_IS_A_DIRECTORY,
        Error::NotEmpty => STATUS_DIRECTORY_NOT_EMPTY,
        Error::DeletePending => STATUS_DELETE_PENDING,
        Error::InvalidOperation => STATUS_INVALID_PARAMETER,
        Error::InvalidName => STATUS_OBJECT_NAME_INVALID,
        Error::AmbiguousName => STATUS_OBJECT_NAME_COLLISION,
        Error::ResourceLimit => STATUS_INSUFFICIENT_RESOURCES,
        Error::NoSpace => STATUS_DISK_FULL,
        Error::Timeout => STATUS_IO_TIMEOUT,
        Error::ReadOnly => STATUS_MEDIA_WRITE_PROTECTED,
        Error::Conflict => STATUS_FILE_INVALID,
        Error::TooLarge => STATUS_FILE_TOO_LARGE,
        Error::Busy => STATUS_DEVICE_BUSY,
        _ => STATUS_UNEXPECTED_IO_ERROR,
    }
    .into()
}
fn info(node: &Node, metadata: Option<&ObjectMetadata>) -> FileInfo {
    let size = metadata.map_or(0, |m| m.size);
    let time = metadata
        .and_then(|m| m.modified_unix_seconds)
        .and_then(|s| s.checked_add(11_644_473_600))
        .and_then(|s| u64::try_from(s).ok())
        .and_then(|s| s.checked_mul(10_000_000))
        .unwrap_or(116_444_736_000_000_000);
    FileInfo {
        file_attributes: FILE_ATTRIBUTE_READONLY.0
            | if node.kind == Kind::Directory {
                FILE_ATTRIBUTE_DIRECTORY.0
            } else {
                0
            },
        file_size: size,
        allocation_size: size.saturating_add(4095) / 4096 * 4096,
        index_number: node.id,
        creation_time: time,
        last_access_time: time,
        last_write_time: time,
        change_time: time,
        ..Default::default()
    }
}
impl Adapter {
    fn file_info(&self, node: &Node, metadata: Option<&ObjectMetadata>) -> FileInfo {
        let mut result = info(node, metadata);
        if self.fs.writable(node) || self.fs.writable_directory(node) {
            result.file_attributes &= !FILE_ATTRIBUTE_READONLY.0;
            if result.file_attributes == 0 {
                result.file_attributes = FILE_ATTRIBUTE_NORMAL.0;
            }
        }
        result
    }
    fn current_info(&self, context: &FileContext) -> Result<FileInfo> {
        match &context.file {
            Some(file) => Ok(self.file_info(
                &context.node,
                Some(&file.current_metadata().map_err(storage_error)?),
            )),
            None => Ok(context.info.clone()),
        }
    }
    fn wait<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        let mut cancelled = self.cancelled.clone();
        self.runtime.block_on(async {
            if *cancelled.borrow() { return Err(STATUS_CANCELLED.into()); }
            tokio::select! {
                biased;
                _ = cancelled.changed() => Err(STATUS_CANCELLED.into()),
                result = tokio::time::timeout(Duration::from_secs(60), future) => result.map_err(|_| FspError::from(STATUS_IO_TIMEOUT))?,
            }
        })
    }
    async fn children(&self, id: u64) -> Result<BTreeMap<Vec<u16>, Arc<Node>>> {
        let mut names = BTreeMap::new();
        for node in self.fs.list(id).await.map_err(storage_error)? {
            if !valid_component(&node.name) {
                return Err(STATUS_OBJECT_NAME_INVALID.into());
            }
            if names.insert(native::name_key(&node.name), node).is_some() {
                return Err(STATUS_OBJECT_NAME_COLLISION.into());
            }
        }
        Ok(names)
    }
    async fn resolve(&self, path: &U16CStr) -> Result<(Arc<Node>, Vec<u16>)> {
        let path = path.to_string().map_err(|_| STATUS_OBJECT_NAME_INVALID)?;
        self.resolve_text(&path).await
    }
    async fn resolve_text(&self, path: &str) -> Result<(Arc<Node>, Vec<u16>)> {
        let suffix = path.strip_prefix('\\').ok_or(STATUS_OBJECT_NAME_INVALID)?;
        let mut node = self.fs.node(ROOT).map_err(storage_error)?;
        let mut normalized = String::new();
        if !suffix.is_empty() {
            for part in suffix.split('\\') {
                if !valid_component(part) {
                    return Err(STATUS_OBJECT_NAME_INVALID.into());
                }
                node = self
                    .children(node.id)
                    .await?
                    .remove(&native::name_key(part))
                    .ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
                normalized.push('\\');
                normalized.push_str(&node.name);
            }
        } else {
            normalized.push('\\');
        }
        Ok((node, normalized.encode_utf16().collect()))
    }
    async fn snapshot(&self, node: &Node) -> Result<Snapshot> {
        let current = self.fs.node(node.id).map_err(storage_error)?;
        let node = current.as_ref();
        let children = self.children(node.id).await?;
        let count = children.len() + if node.id == ROOT { 0 } else { 2 };
        let permit = self
            .entries
            .clone()
            .try_acquire_many_owned(count as u32)
            .map_err(|_| STATUS_INSUFFICIENT_RESOURCES)?;
        let mut entries = Vec::with_capacity(count);
        if node.id != ROOT {
            entries.push(Entry {
                name: vec![46],
                info: self.file_info(node, None),
            });
            let parent = self.fs.node(node.parent).map_err(storage_error)?;
            entries.push(Entry {
                name: vec![46, 46],
                info: self.file_info(&parent, None),
            });
        }
        for child in children.into_values() {
            let metadata = self.fs.metadata(child.id).await.map_err(storage_error)?;
            entries.push(Entry {
                name: child.name.encode_utf16().collect(),
                info: self.file_info(&child, metadata.as_ref()),
            });
        }
        Ok(Snapshot {
            entries,
            _permit: permit,
        })
    }
}
impl FileSystemContext for Adapter {
    type FileContext = FileContext;
    fn get_security_by_name(
        &self,
        name: &U16CStr,
        output: Option<&mut [c_void]>,
        _: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> Result<FileSecurity> {
        let (node, _) = self.wait(self.resolve(name))?;
        Ok(FileSecurity {
            reparse: false,
            attributes: self.file_info(&node, None).file_attributes,
            sz_security_descriptor: native::copy_descriptor(&self.descriptor, output)?,
        })
    }
    fn get_security(&self, _: &FileContext, output: Option<&mut [c_void]>) -> Result<u64> {
        native::copy_descriptor(&self.descriptor, output)
    }
    fn open(
        &self,
        name: &U16CStr,
        options: u32,
        access: u32,
        output: &mut OpenFileInfo,
    ) -> Result<FileContext> {
        let writes = WRITE_DAC.0
            | WRITE_OWNER.0
            | if self.fs.namespace_mutation() {
                0
            } else {
                DELETE.0
            };
        // FILE_DELETE_ON_CLOSE = 0x1000, FILE_DIRECTORY_FILE = 1,
        // FILE_NON_DIRECTORY_FILE = 0x40 (NT create options).
        if access & writes != 0
            || (options & 0x1000 != 0 && !self.fs.native_lifecycle_enabled())
            || (self.fs.is_read_only()
                && access
                    & (FILE_WRITE_DATA.0
                        | FILE_APPEND_DATA.0
                        | FILE_WRITE_EA.0
                        | FILE_WRITE_ATTRIBUTES.0)
                    != 0)
        {
            return Err(STATUS_MEDIA_WRITE_PROTECTED.into());
        }
        let permit = self
            .handles
            .clone()
            .try_acquire_owned()
            .map_err(|_| STATUS_TOO_MANY_OPENED_FILES)?;
        self.wait(async {
            let (node, normalized) = self.resolve(name).await?;
            if access & DELETE.0 != 0
                && !(self.fs.writable(&node) || self.fs.writable_directory(&node))
            {
                return Err(STATUS_MEDIA_WRITE_PROTECTED.into());
            }
            if node.kind == Kind::Directory && options & 0x40 != 0 {
                return Err(STATUS_FILE_IS_A_DIRECTORY.into());
            }
            if node.kind == Kind::File && options & 1 != 0 {
                return Err(STATUS_NOT_A_DIRECTORY.into());
            }
            if node.kind == Kind::Directory {
                // Resolution uses the parent's listing. Reauthorize the node
                // itself so an authority-reserved empty directory cannot open.
                self.fs.metadata(node.id).await.map_err(storage_error)?;
            }
            let file = if node.kind == Kind::File {
                Some(Box::new(
                    if access & (FILE_WRITE_DATA.0 | FILE_APPEND_DATA.0) != 0 {
                        self.fs.open_write(node.id).await.map_err(storage_error)?
                    } else {
                        self.fs.open(node.id).await.map_err(storage_error)?
                    },
                ))
            } else {
                None
            };
            let metadata = file
                .as_ref()
                .map(|file| file.current_metadata())
                .transpose()
                .map_err(storage_error)?;
            let info = self.file_info(&node, metadata.as_ref());
            // The wrapper copies to a caller-sized buffer. Check the real size,
            // not only its much larger transaction-size upper bound.
            if normalized.len() * 2 > usize::from(output.normalized_name_size()) {
                return Err(STATUS_NAME_TOO_LONG.into());
            }
            let native = match file.as_ref().and_then(|f| f.native_object()) {
                Some(object) => Some(object),
                None => self
                    .fs
                    .native_file_object(node.id, options & 0x1000 != 0)
                    .await
                    .map_err(storage_error)?,
            };
            output.set_normalized_name(&normalized, None);
            *output.as_mut() = info.clone();
            Ok(FileContext {
                node,
                file,
                native,
                info,
                directory: Mutex::new(None),
                _permit: permit,
            })
        })
    }
    fn create(
        &self,
        name: &U16CStr,
        options: u32,
        access: u32,
        attributes: u32,
        _security: Option<&[c_void]>,
        _allocation: u64,
        extra: Option<&[u8]>,
        reparse: bool,
        output: &mut OpenFileInfo,
    ) -> Result<FileContext> {
        if self.fs.is_read_only()
            || (options & 0x1000 != 0 && !self.fs.native_lifecycle_enabled())
            || access
                & (WRITE_DAC.0
                    | WRITE_OWNER.0
                    | if self.fs.namespace_mutation() {
                        0
                    } else {
                        DELETE.0
                    })
                != 0
        {
            return Err(STATUS_MEDIA_WRITE_PROTECTED.into());
        }
        let directory = options & 1 != 0;
        if (directory && !self.fs.namespace_creation())
            || reparse
            || extra.is_some_and(|b| !b.is_empty())
            || attributes
                & !(FILE_ATTRIBUTE_NORMAL.0
                    | FILE_ATTRIBUTE_ARCHIVE.0
                    | if directory {
                        FILE_ATTRIBUTE_DIRECTORY.0
                    } else {
                        0
                    })
                != 0
        {
            return Err(STATUS_NOT_SUPPORTED.into());
        }
        // New files use the mount's fixed owner-only security descriptor.
        // Per-file ACLs, alternate streams and mutable attributes are unsupported.
        let permit = self
            .handles
            .clone()
            .try_acquire_owned()
            .map_err(|_| STATUS_TOO_MANY_OPENED_FILES)?;
        self.wait(async {
            let path = name.to_string().map_err(|_| STATUS_OBJECT_NAME_INVALID)?;
            let (parent, leaf) = path.rsplit_once('\\').ok_or(STATUS_OBJECT_NAME_INVALID)?;
            if !valid_component(leaf) {
                return Err(STATUS_OBJECT_NAME_INVALID.into());
            }
            let (parent, mut normalized) = self
                .resolve_text(if parent.is_empty() { "\\" } else { parent })
                .await?;
            if normalized.last() != Some(&92) {
                normalized.push(92);
            }
            normalized.extend(leaf.encode_utf16());
            if normalized.len() * 2 > usize::from(output.normalized_name_size()) {
                return Err(STATUS_NAME_TOO_LONG.into());
            }
            let doc = options & 0x1000 != 0;
            let writing = access & (FILE_WRITE_DATA.0 | FILE_APPEND_DATA.0) != 0;
            let (node, file, object) = if doc {
                let (node, object) = self
                    .fs
                    .create_native_deferred(parent.id, leaf, directory)
                    .await
                    .map_err(storage_error)?;
                let file = if directory {
                    None
                } else {
                    Some(Box::new(if writing {
                        self.fs.open_write(node.id).await.map_err(storage_error)?
                    } else {
                        self.fs.open(node.id).await.map_err(storage_error)?
                    }))
                };
                (node, file, Some(object))
            } else if directory {
                let node = self
                    .fs
                    .create_directory(parent.id, leaf)
                    .await
                    .map_err(storage_error)?;
                (node, None, None)
            } else {
                let writing = access & (FILE_WRITE_DATA.0 | FILE_APPEND_DATA.0) != 0;
                let file = Box::new(
                    Box::pin(self.fs.create_file(
                        parent.id,
                        leaf,
                        |a, b| native::name_key(a) == native::name_key(b),
                        writing,
                    ))
                    .await
                    .map_err(storage_error)?,
                );
                (file.node.clone(), Some(file), None)
            };
            let metadata = file
                .as_ref()
                .map(|f| f.current_metadata())
                .transpose()
                .map_err(storage_error)?;
            let info = self.file_info(&node, metadata.as_ref());
            let native = match object {
                Some(object) => Some(object),
                None => match file.as_ref().and_then(|f| f.native_object()) {
                    Some(object) => Some(object),
                    None => self
                        .fs
                        .native_file_object(node.id, false)
                        .await
                        .map_err(storage_error)?,
                },
            };
            output.set_normalized_name(&normalized, None);
            *output.as_mut() = info.clone();
            Ok(FileContext {
                node,
                file,
                native,
                info,
                directory: Mutex::new(None),
                _permit: permit,
            })
        })
    }
    fn rename(
        &self,
        context: &FileContext,
        file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace: bool,
    ) -> Result<()> {
        if !self.fs.namespace_mutation() {
            return Err(STATUS_NOT_SUPPORTED.into());
        }
        self.wait(async {
            if let Some(object) = &context.native {
                let new = new_file_name
                    .to_string()
                    .map_err(|_| STATUS_OBJECT_NAME_INVALID)?;
                let (parent, leaf) = new.rsplit_once('\\').ok_or(STATUS_OBJECT_NAME_INVALID)?;
                if !valid_component(leaf) {
                    return Err(STATUS_OBJECT_NAME_INVALID.into());
                }
                let (parent, _) = self
                    .resolve_text(if parent.is_empty() { "\\" } else { parent })
                    .await?;
                self.fs
                    .rename_native_object(object, parent.id, leaf, replace, |a, b| {
                        native::name_key(a) == native::name_key(b)
                    })
                    .await
                    .map_err(rename_error)?;
                return Ok(());
            }
            let old = file_name
                .to_string()
                .map_err(|_| STATUS_OBJECT_NAME_INVALID)?;
            let new = new_file_name
                .to_string()
                .map_err(|_| STATUS_OBJECT_NAME_INVALID)?;
            let (old_parent, old_leaf) = old.rsplit_once('\\').ok_or(STATUS_OBJECT_NAME_INVALID)?;
            let (new_parent, new_leaf) = new.rsplit_once('\\').ok_or(STATUS_OBJECT_NAME_INVALID)?;
            if !valid_component(new_leaf) {
                return Err(STATUS_OBJECT_NAME_INVALID.into());
            }
            let (source, _) = self.resolve_text(&old).await?;
            if source.id != context.node.id {
                return Err(STATUS_FILE_INVALID.into());
            }
            let (old_parent, _) = self
                .resolve_text(if old_parent.is_empty() {
                    "\\"
                } else {
                    old_parent
                })
                .await?;
            let (new_parent, _) = self
                .resolve_text(if new_parent.is_empty() {
                    "\\"
                } else {
                    new_parent
                })
                .await?;
            self.fs
                .rename_entry(
                    old_parent.id,
                    old_leaf,
                    new_parent.id,
                    new_leaf,
                    replace,
                    |a, b| native::name_key(a) == native::name_key(b),
                )
                .await
                .map_err(rename_error)?;
            Ok(())
        })
    }
    fn set_delete(&self, context: &FileContext, _name: &U16CStr, delete: bool) -> Result<()> {
        let object = context.native.as_ref().ok_or(STATUS_NOT_SUPPORTED)?;
        self.wait(async { object.set_delete(delete).await.map_err(storage_error) })
    }
    fn cleanup(&self, context: &FileContext, _name: Option<&U16CStr>, flags: u32) {
        // Actual void observations are persisted even during cancellation.
        if let Some(object) = &context.native
            && let Err(error) = self.runtime.block_on(object.cleanup(flags))
        {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr().lock(),
                "Native cleanup retained for recovery: {error}"
            );
        }
    }
    fn close(&self, context: FileContext) {
        if let Some(object) = &context.native
            && let Err(error) = self.runtime.block_on(object.close())
        {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr().lock(),
                "Native close retained for recovery: {error}"
            );
        }
        drop(context);
    }
    fn get_file_info(&self, context: &FileContext, output: &mut FileInfo) -> Result<()> {
        *output = self.current_info(context)?;
        Ok(())
    }
    fn flush(&self, context: Option<&FileContext>, output: &mut FileInfo) -> Result<()> {
        if let Some(context) = context {
            self.wait(async {
                if let Some(file) = &context.file {
                    file.flush_on_close().await.map_err(storage_error)?;
                }
                Ok(())
            })?;
            *output = self.current_info(context)?;
        } else {
            self.wait(async { self.fs.flush_all().await.map_err(storage_error) })?;
        }
        Ok(())
    }
    fn read(&self, context: &FileContext, output: &mut [u8], offset: u64) -> Result<u32> {
        let file = context.file.as_ref().ok_or(STATUS_FILE_IS_A_DIRECTORY)?;
        if output.is_empty() {
            return Ok(0);
        }
        if offset >= file.current_metadata().map_err(storage_error)?.size {
            return Err(STATUS_END_OF_FILE.into());
        }
        self.wait(async {
            let bytes = file
                .read(offset, output.len().min(MAX_READ_BYTES))
                .await
                .map_err(storage_error)?;
            output[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len() as u32)
        })
    }
    fn write(
        &self,
        context: &FileContext,
        buffer: &[u8],
        offset: u64,
        write_to_eof: bool,
        constrained_io: bool,
        output: &mut FileInfo,
    ) -> Result<u32> {
        let file = context.file.as_ref().ok_or(STATUS_FILE_IS_A_DIRECTORY)?;
        let count = self.wait(async {
            file.write(offset, Arc::from(buffer), write_to_eof, constrained_io)
                .await
                .map_err(storage_error)
        })?;
        *output = self.current_info(context)?;
        Ok(count as u32)
    }
    fn set_file_size(
        &self,
        context: &FileContext,
        size: u64,
        allocation: bool,
        output: &mut FileInfo,
    ) -> Result<()> {
        let file = context.file.as_ref().ok_or(STATUS_FILE_IS_A_DIRECTORY)?;
        if !allocation || size < file.current_metadata().map_err(storage_error)?.size {
            self.wait(async { file.truncate(size).await.map_err(storage_error) })?;
        }
        *output = self.current_info(context)?;
        Ok(())
    }
    fn overwrite(
        &self,
        context: &FileContext,
        attributes: u32,
        replace_attributes: bool,
        _allocation: u64,
        extra: Option<&[u8]>,
        output: &mut FileInfo,
    ) -> Result<()> {
        if extra.is_some_and(|b| !b.is_empty())
            || (replace_attributes
                && attributes & !(FILE_ATTRIBUTE_ARCHIVE.0 | FILE_ATTRIBUTE_NORMAL.0) != 0)
        {
            return Err(STATUS_NOT_SUPPORTED.into());
        }
        let file = context.file.as_ref().ok_or(STATUS_FILE_IS_A_DIRECTORY)?;
        self.wait(async { file.truncate(0).await.map_err(storage_error) })?;
        *output = self.current_info(context)?;
        Ok(())
    }
    fn read_directory(
        &self,
        context: &FileContext,
        _: Option<&U16CStr>,
        marker: DirMarker,
        buffer: &mut [u8],
    ) -> Result<u32> {
        if context.node.kind != Kind::Directory {
            return Err(STATUS_NOT_A_DIRECTORY.into());
        }
        let mut snapshot = context
            .directory
            .lock()
            .map_err(|_| STATUS_INTERNAL_ERROR)?;
        if snapshot.is_none() {
            *snapshot = Some(self.wait(self.snapshot(&context.node))?);
        }
        let snapshot = snapshot.as_ref().ok_or(STATUS_INTERNAL_ERROR)?;
        let start = match marker.inner() {
            None => 0,
            Some(name) => snapshot
                .entries
                .iter()
                .position(|entry| entry.name == name)
                .map(|i| i + 1)
                .ok_or(STATUS_INVALID_PARAMETER)?,
        };
        let mut cursor = 0;
        for entry in &snapshot.entries[start..] {
            let mut directory = DirInfo::<255>::new();
            directory.set_name_raw(entry.name.as_slice())?;
            *directory.file_info_mut() = entry.info.clone();
            if !directory.append_to_buffer(buffer, &mut cursor) {
                return Ok(cursor);
            }
        }
        DirInfo::<255>::finalize_buffer(buffer, &mut cursor);
        Ok(cursor)
    }
    fn get_volume_info(&self, output: &mut VolumeInfo) -> Result<()> {
        // Report only the local logical staging budget, never a remote quota.
        let capacity = self.wait(async { self.fs.capacity().await.map_err(storage_error) })?;
        output.total_size = capacity.as_ref().map_or(0, |c| c.total);
        output.free_size = capacity.as_ref().map_or(0, |c| c.free);
        output.set_volume_label("S3Drive");
        Ok(())
    }
}
struct Mounted {
    host: FileSystemHost<Adapter>,
    cancel: watch::Sender<bool>,
}
impl Drop for Mounted {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        self.host.unmount();
        self.host.stop();
    }
}

fn mount_point(
    path: &Path,
) -> std::result::Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let point = path
        .to_str()
        .ok_or("mountpoint must be valid Unicode")?
        .replace('/', "\\");
    let bytes = point.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Ok(point);
    }
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || &bytes[1..3] != b":\\"
        || point[3..].split('\\').any(|part| !valid_component(part))
    {
        return Err("Windows mountpoint must be an unused drive letter or an absolute local directory path with normal components".into());
    }
    let path = Path::new(&point);
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => return Err(
            "Windows directory mountpoint must not already exist; WinFsp creates and removes it"
                .into(),
        ),
    }
    for parent in path.ancestors().skip(1) {
        let info = std::fs::symlink_metadata(parent)?;
        if !info.is_dir() || info.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err(
                "Mountpoint ancestors must be existing directories without junctions or symlinks"
                    .into(),
            );
        }
    }
    if !native::is_ntfs(&point[..3])? {
        return Err("Directory mountpoints require a local NTFS volume".into());
    }
    Ok(point)
}

/// Foreground, per-user drive letter or NTFS directory mount. EOF supervision is opt-in; otherwise
/// Ctrl+C/Ctrl+Break trigger cancellation and graceful teardown.
pub async fn run(
    fs: ReadOnlyFs,
    mountpoint: &Path,
    unmount_on_stdin_close: bool,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let writable = !fs.is_read_only();
    let cache = fs.cache_control();
    let shutdown = fs.shutdown_writes();
    let point = mount_point(mountpoint)?;
    let mut ctrl_c = tokio::signal::windows::ctrl_c()?;
    let mut ctrl_break = tokio::signal::windows::ctrl_break()?;
    let runtime = tokio::runtime::Handle::current();
    let mounted = tokio::task::spawn_blocking(move || -> Result<Mounted> {
        let _init = winfsp::winfsp_init()?;
        if fs.namespace_creation() && !native::portable_mapping_compatible(){return Err(STATUS_NOT_SUPPORTED.into())}
        let read_only=fs.is_read_only();
        let descriptor = native::owner_descriptor(!read_only,fs.namespace_mutation())?;
        let (cancel, cancelled) = watch::channel(false);
        let posix=fs.native_posix_unlink_rename_enabled();
        let adapter = Adapter { fs, runtime, cancelled, descriptor, handles: Arc::new(Semaphore::new(256)), entries: Arc::new(Semaphore::new(100_000)) };
        let mut params = VolumeParams::new();
        params.filesystem_name("S3Drive").sector_size(512).sectors_per_allocation_unit(8).max_component_length(255)
            .case_sensitive_search(false).case_preserved_names(true).unicode_on_disk(true)
            .persistent_acls(true).read_only_volume(read_only).file_info_timeout(0).dir_info_timeout(0)
            .flush_and_purge_on_cleanup(true).irp_timeout(120_000).supports_posix_unlink_rename(posix);
        let mut host: FileSystemHost<Adapter> = FileSystemHost::new(params, adapter)?;
        host.mount(&point)?;
        host.start_with_threads(4)?;
        Ok(Mounted { host, cancel })
    }).await?.map_err(|e| format!("WinFsp mount failed ({e:?}); check WinFsp 2.1 installation and use an unused drive letter or new NTFS directory path"))?;
    let (stdin_closed, mut stdin_shutdown) = tokio::sync::oneshot::channel();
    if unmount_on_stdin_close {
        // A detached reader is deliberate: a blocked console read must not keep
        // Tokio shutdown alive after Ctrl+C. Process exit releases this thread.
        std::thread::spawn(move || {
            use std::io::Read;
            let mut byte = [0u8; 1];
            let mut input = std::io::stdin().lock();
            while matches!(input.read(&mut byte), Ok(n) if n > 0) {}
            let _ = stdin_closed.send(());
        });
    }
    eprintln!(
        "Mounted {} WinFsp drive; Ctrl+C to unmount.",
        if writable { "read-write" } else { "read-only" }
    );
    tokio::select! {
        _ = ctrl_c.recv() => {},
        _ = ctrl_break.recv() => {},
        _ = &mut stdin_shutdown, if unmount_on_stdin_close => {},
    }
    tokio::task::spawn_blocking(move || drop(mounted)).await?;
    shutdown.await?;
    if let Some(cache) = cache {
        cache.close().await?;
    }
    Ok(())
}
