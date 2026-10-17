//! Disposable in-memory reference host, using the pinned legacy/POSIX profiles.
//! It deliberately permits replacement in Rename; the trace distinguishes a
//! kernel refusal before Rename from a user-mode result. Cleanup uses identity.
#![forbid(unsafe_code)]
use crate::native;
use std::{
    collections::HashMap,
    ffi::c_void,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use windows::Win32::{
    Foundation::*,
    Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL},
};
use winfsp::{
    Result, U16CStr,
    filesystem::{FileInfo, FileSecurity, FileSystemContext, OpenFileInfo, VolumeInfo},
    host::{FileSystemHost, VolumeParams},
};
struct Node {
    id: u64,
    directory: bool,
    data: Mutex<Vec<u8>>,
}
type Nodes = HashMap<Vec<u16>, Arc<Node>>;
pub(super) struct State {
    nodes: Mutex<Nodes>,
    next: AtomicU64,
    renames: AtomicU64,
}
impl State {
    pub(super) fn renames(&self) -> u64 {
        self.renames.load(Ordering::SeqCst)
    }
}
struct Host {
    state: Arc<State>,
    descriptor: Vec<u8>,
}
fn key(name: &U16CStr) -> Result<Vec<u16>> {
    Ok(native::name_key(
        &name.to_string().map_err(|_| STATUS_OBJECT_NAME_INVALID)?,
    ))
}
fn info(n: &Node) -> FileInfo {
    let size = n.data.lock().unwrap().len() as u64;
    FileInfo {
        file_attributes: if n.directory {
            FILE_ATTRIBUTE_DIRECTORY.0
        } else {
            FILE_ATTRIBUTE_NORMAL.0
        },
        file_size: size,
        allocation_size: size.div_ceil(4096) * 4096,
        index_number: n.id,
        ..Default::default()
    }
}
impl Host {
    fn node(&self, name: &U16CStr) -> Result<Arc<Node>> {
        self.state
            .nodes
            .lock()
            .unwrap()
            .get(&key(name)?)
            .cloned()
            .ok_or(STATUS_OBJECT_NAME_NOT_FOUND.into())
    }
}
impl FileSystemContext for Host {
    type FileContext = Arc<Node>;
    fn get_security_by_name(
        &self,
        name: &U16CStr,
        out: Option<&mut [c_void]>,
        _: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> Result<FileSecurity> {
        let n = self.node(name)?;
        Ok(FileSecurity {
            reparse: false,
            attributes: info(&n).file_attributes,
            sz_security_descriptor: native::copy_descriptor(&self.descriptor, out)?,
        })
    }
    fn get_security(&self, _: &Arc<Node>, out: Option<&mut [c_void]>) -> Result<u64> {
        native::copy_descriptor(&self.descriptor, out)
    }
    fn open(&self, name: &U16CStr, _: u32, _: u32, out: &mut OpenFileInfo) -> Result<Arc<Node>> {
        let n = self.node(name)?;
        *out.as_mut() = info(&n);
        Ok(n)
    }
    fn create(
        &self,
        name: &U16CStr,
        options: u32,
        _: u32,
        _: u32,
        _: Option<&[c_void]>,
        _: u64,
        _: Option<&[u8]>,
        _: bool,
        out: &mut OpenFileInfo,
    ) -> Result<Arc<Node>> {
        let k = key(name)?;
        let mut nodes = self.state.nodes.lock().unwrap();
        if nodes.contains_key(&k) {
            return Err(STATUS_OBJECT_NAME_COLLISION.into());
        }
        let n = Arc::new(Node {
            id: self.state.next.fetch_add(1, Ordering::SeqCst),
            directory: options & 1 != 0,
            data: Mutex::new(Vec::new()),
        });
        *out.as_mut() = info(&n);
        nodes.insert(k, n.clone());
        Ok(n)
    }
    fn close(&self, _: Arc<Node>) {}
    fn cleanup(&self, n: &Arc<Node>, _: Option<&U16CStr>, flags: u32) {
        if flags & 1 != 0 {
            self.state
                .nodes
                .lock()
                .unwrap()
                .retain(|_, entry| entry.id != n.id);
        }
    }
    fn set_delete(&self, _: &Arc<Node>, _: &U16CStr, _: bool) -> Result<()> {
        Ok(())
    }
    fn rename(&self, n: &Arc<Node>, old: &U16CStr, new: &U16CStr, replace: bool) -> Result<()> {
        self.state.renames.fetch_add(1, Ordering::SeqCst);
        let (old, new) = (key(old)?, key(new)?);
        let mut nodes = self.state.nodes.lock().unwrap();
        if nodes.get(&old).map(|v| v.id) != Some(n.id) {
            return Err(STATUS_FILE_INVALID.into());
        }
        if nodes.contains_key(&new) && !replace {
            return Err(STATUS_OBJECT_NAME_COLLISION.into());
        }
        nodes.remove(&old);
        nodes.insert(new, n.clone());
        Ok(())
    }
    fn get_file_info(&self, n: &Arc<Node>, out: &mut FileInfo) -> Result<()> {
        *out = info(n);
        Ok(())
    }
    fn flush(&self, n: Option<&Arc<Node>>, out: &mut FileInfo) -> Result<()> {
        if let Some(n) = n {
            *out = info(n);
        }
        Ok(())
    }
    fn read(&self, n: &Arc<Node>, out: &mut [u8], offset: u64) -> Result<u32> {
        let data = n.data.lock().unwrap();
        let start = (offset as usize).min(data.len());
        let count = out.len().min(data.len() - start);
        out[..count].copy_from_slice(&data[start..start + count]);
        Ok(count as u32)
    }
    fn write(
        &self,
        n: &Arc<Node>,
        bytes: &[u8],
        offset: u64,
        eof: bool,
        constrained: bool,
        out: &mut FileInfo,
    ) -> Result<u32> {
        let count = {
            let mut data = n.data.lock().unwrap();
            let offset = if eof {
                data.len()
            } else {
                usize::try_from(offset).map_err(|_| STATUS_FILE_TOO_LARGE)?
            };
            let count = if constrained {
                bytes.len().min(data.len().saturating_sub(offset))
            } else {
                bytes.len()
            };
            let end = offset.checked_add(count).ok_or(STATUS_FILE_TOO_LARGE)?;
            if end > 1024 * 1024 {
                return Err(STATUS_FILE_TOO_LARGE.into());
            }
            if end > data.len() {
                data.resize(end, 0);
            }
            data[offset..end].copy_from_slice(&bytes[..count]);
            count
        };
        *out = info(n);
        Ok(count as u32)
    }
    fn get_volume_info(&self, out: &mut VolumeInfo) -> Result<()> {
        out.total_size = 1024 * 1024;
        out.free_size = 1024 * 1024;
        out.set_volume_label("Reference only");
        Ok(())
    }
}
pub(super) struct Mounted {
    host: FileSystemHost<Host>,
    pub(super) state: Arc<State>,
    stopped: bool,
}
impl Mounted {
    pub(super) fn new(path: &Path, posix: bool) -> Self {
        let _init = winfsp::winfsp_init().unwrap();
        let root = Arc::new(Node {
            id: 1,
            directory: true,
            data: Mutex::new(Vec::new()),
        });
        let state = Arc::new(State {
            nodes: Mutex::new(HashMap::from([(native::name_key("\\"), root)])),
            next: AtomicU64::new(2),
            renames: AtomicU64::new(0),
        });
        let mut p = VolumeParams::new();
        p.filesystem_name("S3Reference")
            .sector_size(512)
            .sectors_per_allocation_unit(8)
            .max_component_length(255)
            .case_sensitive_search(false)
            .case_preserved_names(true)
            .unicode_on_disk(true)
            .persistent_acls(true)
            .supports_posix_unlink_rename(posix)
            .file_info_timeout(0)
            .dir_info_timeout(0)
            .flush_and_purge_on_cleanup(true);
        let mut host: FileSystemHost<Host> = FileSystemHost::new(
            p,
            Host {
                state: state.clone(),
                descriptor: native::owner_descriptor(true, true).unwrap(),
            },
        )
        .unwrap();
        host.mount(path).unwrap();
        host.start_with_threads(2).unwrap();
        Self {
            host,
            state,
            stopped: false,
        }
    }
    pub(super) fn stop(&mut self) {
        if !self.stopped {
            self.host.unmount();
            self.host.stop();
            self.stopped = true;
        }
    }
}
impl Drop for Mounted {
    fn drop(&mut self) {
        self.stop();
    }
}
