//! Bounded authenticated manifest snapshots and exact-view mixed publication.
//! No arbitrary snapshot constructor, clean settlement or native chunk activation.
use s3drive_authority::{Kind, Listing, Node, Payload, Reply, Request, Store, WireClient};
use s3drive_core::{
    manifest::{
        self, Entry, VerifiedPage,
        objects::{Namespace, Role},
    },
    names,
};
use s3drive_s3::{ReadHandle, S3Reader};
use s3drive_state::Result;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    future::Future,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
mod base;
mod composition;
pub use base::BaseDescriptor;
pub use composition::{
    ChunkContent, CleanObservation, EditedFile, MixedPublisher, MixedRecovery,
    MixedRecoveryFailure, MixedRootCommit, MixedRootFailure, MixedRootRecovery, MixedSourceReceipt,
    MixedUploader, ObservationKind, Preparation, PreparedChunk, PreparedDependency,
};
mod provenance;
pub use provenance::{ChunkProof, MAX_CHUNK_PROOF_BYTES, MAX_PAGE_PROOF_BYTES, PageProof};

pub const MAX_READ_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ACTIVE: usize = 4;
pub const MAX_SNAPSHOTS: usize = 256;
const MAX_ROOT_LEVEL: u8 = 2;
const MAX_VISITS: usize = 27;
const MAX_DATA: usize = 9;
const DEADLINE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFailure {
    Bounds,
    Busy,
    Binding,
    Protocol,
    Integrity,
    UnsupportedContent,
    NotFound,
    IsDirectory,
    Timeout,
    Authority(s3drive_authority::Error),
}
impl fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "managed manifest read {:?}; no unverified bytes returned",
            self
        )
    }
}
impl std::error::Error for ReadFailure {}

/// A positive overall operation deadline, at most the 30-second default.
/// Byte, traversal, active-operation and retained-snapshot limits stay fixed.
#[derive(Clone, Copy, Debug)]
pub struct ReadOptions {
    deadline: Duration,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self { deadline: DEADLINE }
    }
}
impl ReadOptions {
    pub fn new(deadline: Duration) -> Result<Self> {
        if deadline.is_zero() || deadline > DEADLINE {
            return Err(ReadFailure::Bounds.into());
        }
        Ok(Self { deadline })
    }
    pub fn deadline(&self) -> Duration {
        self.deadline
    }
}
async fn bounded<T>(options: ReadOptions, future: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(options.deadline, future)
        .await
        .map_err(|_| ReadFailure::Timeout)?
}
fn reply(r: Reply) -> Result<Reply> {
    if let Reply::Error(e) = r {
        Err(ReadFailure::Authority(e).into())
    } else {
        Ok(r)
    }
}
fn identifier(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
struct Shared {
    authority: WireClient,
    volume: String,
    policy: String,
    stores: HashMap<String, (Store, S3Reader)>,
    active: Arc<Semaphore>,
    snapshots: Arc<Semaphore>,
    options: ReadOptions,
    history: bool,
}
#[derive(Clone)]
pub struct Reader(Arc<Shared>);
impl fmt::Debug for Reader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestReader([private])")
    }
}
impl Reader {
    pub async fn open(
        authority: WireClient,
        volume: String,
        bindings: Vec<(String, S3Reader)>,
    ) -> Result<Self> {
        Self::open_with_options(authority, volume, bindings, ReadOptions::default()).await
    }
    pub async fn open_with_options(
        authority: WireClient,
        volume: String,
        bindings: Vec<(String, S3Reader)>,
        options: ReadOptions,
    ) -> Result<Self> {
        if !super::id(&volume) || bindings.is_empty() || bindings.len() > 16 {
            return Err(ReadFailure::Binding.into());
        }
        let mut ids = HashSet::new();
        for (id, _) in &bindings {
            if !super::id(id) || !ids.insert(id) {
                return Err(ReadFailure::Binding.into());
            }
        }
        bounded(
            options,
            Box::pin(Self::bind(authority, volume, bindings, options)),
        )
        .await
    }
    async fn bind(
        authority: WireClient,
        volume: String,
        bindings: Vec<(String, S3Reader)>,
        options: ReadOptions,
    ) -> Result<Self> {
        let Reply::Features(features) = reply(authority.request(&Request::Features {}).await?)?
        else {
            return Err(ReadFailure::Protocol.into());
        };
        if !features.iter().any(|f| f == "manifest-root-v1") {
            return Err(ReadFailure::UnsupportedContent.into());
        }
        let Reply::Info(info) = reply(authority.request(&Request::Info {}).await?)? else {
            return Err(ReadFailure::Protocol.into());
        };
        if info.id != volume
            || !names::supported(&info.name_policy)
            || info.sequence > i64::MAX as u64
        {
            return Err(ReadFailure::Binding.into());
        }
        let mut stores = HashMap::new();
        for (id, reader) in bindings {
            let Reply::Store(store) = reply(
                authority
                    .request(&Request::Store {
                        volume: volume.clone(),
                        store: id.clone(),
                    })
                    .await?,
            )?
            else {
                return Err(ReadFailure::Protocol.into());
            };
            if store.id != id || !store.valid() || store.route != reader.storage_fingerprint() {
                return Err(ReadFailure::Binding.into());
            }
            stores.insert(id, (store, reader));
        }
        Ok(Self(Arc::new(Shared {
            authority,
            volume,
            policy: info.name_policy,
            stores,
            active: Arc::new(Semaphore::new(MAX_ACTIVE)),
            snapshots: Arc::new(Semaphore::new(MAX_SNAPSHOTS)),
            options,
            history: features.iter().any(|f| f == "manifest-history-v1"),
        })))
    }
    fn validate(&self, n: &Node) -> Result<()> {
        if !identifier(&n.id)
            || n.parent.as_ref().is_some_and(|p| !identifier(p))
            || n.revision > i64::MAX as u64
        {
            return Err(ReadFailure::Protocol.into());
        }
        if n.id == s3drive_authority::ROOT {
            if n.parent.is_some()
                || !n.name.is_empty()
                || n.kind != Kind::Directory
                || n.payload.is_some()
                || n.placement.is_some()
            {
                return Err(ReadFailure::Protocol.into());
            }
        } else if n.parent.is_none()
            || n.name.is_empty()
            || n.name.len() > 255
            || [".", ".."].contains(&n.name.as_str())
            || n.name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
            || self.0.policy == names::PORTABLE && names::collision_key(&n.name).is_none()
        {
            return Err(ReadFailure::Protocol.into());
        }
        if let Some(p) = &n.placement {
            let (mapped, _) = self.0.stores.get(&p.id).ok_or(ReadFailure::Binding)?;
            if p != mapped
                || n.kind == Kind::Directory
                || n.payload
                    .as_ref()
                    .is_some_and(|v| v.reservation.store != *p)
            {
                return Err(ReadFailure::Binding.into());
            }
        }
        match &n.payload {
            Some(p) => {
                let r = &p.reservation;
                let (mapped, _) = self.0.stores.get(&r.store.id).ok_or(ReadFailure::Binding)?;
                if n.kind != Kind::File
                    || !p.valid()
                    || r.volume != self.0.volume
                    || r.node != n.id
                    || &r.store != mapped
                {
                    return Err(ReadFailure::Binding.into());
                }
            }
            None if n.kind == Kind::File => return Err(ReadFailure::Integrity.into()),
            None => (),
        }
        Ok(())
    }
    /// Validated live namespace metadata, including legacy payloads. This does
    /// not authorize S3 bytes or consume a retained file-snapshot permit.
    pub async fn node(&self, node: &str) -> Result<Node> {
        if !identifier(node) {
            return Err(ReadFailure::Bounds.into());
        }
        let _active = self
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(self.0.options, Box::pin(self.load_node(node))).await
    }
    async fn load_node(&self, id: &str) -> Result<Node> {
        let Reply::Node(node) = reply(
            self.0
                .authority
                .request(&Request::Node {
                    volume: self.0.volume.clone(),
                    node: id.into(),
                })
                .await?,
        )?
        else {
            return Err(ReadFailure::Protocol.into());
        };
        if node.id != id {
            return Err(ReadFailure::Binding.into());
        }
        self.validate(&node)?;
        if node.deleted {
            return Err(ReadFailure::NotFound.into());
        }
        Ok(node)
    }
    /// One bounded authenticated namespace listing. Every child is validated
    /// before any listing is returned; no S3 requests or partial results.
    pub async fn list(&self, parent: &str) -> Result<Listing> {
        if !identifier(parent) {
            return Err(ReadFailure::Bounds.into());
        }
        let _active = self
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(self.0.options, Box::pin(self.list_inner(parent))).await
    }
    async fn list_inner(&self, parent: &str) -> Result<Listing> {
        let Reply::List(list) = reply(
            self.0
                .authority
                .request(&Request::List {
                    volume: self.0.volume.clone(),
                    parent: parent.into(),
                })
                .await?,
        )?
        else {
            return Err(ReadFailure::Protocol.into());
        };
        if list.sequence > i64::MAX as u64 || list.children.len() > 1000 {
            return Err(ReadFailure::Protocol.into());
        }
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for child in &list.children {
            self.validate(child)?;
            let name = if self.0.policy == names::PORTABLE {
                names::collision_key(&child.name).ok_or(ReadFailure::Protocol)?
            } else {
                child.name.clone()
            };
            if child.deleted
                || child.parent.as_deref() != Some(parent)
                || child.id == parent
                || !names.insert(name)
                || !ids.insert(&child.id)
            {
                return Err(ReadFailure::Protocol.into());
            }
        }
        Ok(list)
    }
    /// Obtain live authority metadata and independently HEAD/GET/verify the
    /// complete bounded root. No child page or file data is fetched at open.
    pub async fn open_file(&self, node: &str) -> Result<File> {
        if !identifier(node) {
            return Err(ReadFailure::Bounds.into());
        }
        let _active = self
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        let snapshot = self
            .0
            .snapshots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(self.0.options, Box::pin(self.open_inner(node, snapshot))).await
    }
    async fn open_inner(&self, id: &str, snapshot: OwnedSemaphorePermit) -> Result<File> {
        let node = self.load_node(id).await?;
        if node.kind == Kind::Directory {
            return Err(ReadFailure::IsDirectory.into());
        }
        let base = Box::pin(self.open_version(node.payload.clone(), snapshot)).await?;
        Ok(File {
            node: Arc::new(node),
            base,
        })
    }
    async fn open_version(
        &self,
        payload: Option<Payload>,
        snapshot: OwnedSemaphorePermit,
    ) -> Result<BaseFile> {
        let (size, content) = if let Some(payload) = &payload {
            let r = &payload.reservation;
            let descriptor = r
                .manifest
                .as_ref()
                .ok_or(ReadFailure::UnsupportedContent)?
                .root()
                .map_err(|_| ReadFailure::Integrity)?;
            let ns = Namespace::new(
                r.store.root.clone(),
                *descriptor.volume(),
                *descriptor.node(),
                *descriptor.page().store(),
            )?;
            let key = ns.root_key(&descriptor)?;
            if key.as_str() != r.key {
                return Err(ReadFailure::Binding.into());
            }
            let (_, reader) = self.0.stores.get(&r.store.id).ok_or(ReadFailure::Binding)?;
            let handle = Box::pin(reader.open(key.as_str())).await?;
            let m = handle.metadata();
            if m.size != r.size
                || m.revision.etag.as_deref() != Some(payload.etag.as_str())
                || payload
                    .version
                    .as_ref()
                    .is_some_and(|v| m.revision.version_id.as_ref() != Some(v))
            {
                return Err(ReadFailure::Integrity.into());
            }
            let bytes = Box::pin(handle.read_range(0, r.size as usize)).await?;
            if super::hex(&Sha256::digest(&bytes)) != r.sha256 {
                return Err(ReadFailure::Integrity.into());
            }
            let page = descriptor.verify_page(&bytes)?;
            page_scope(&page, descriptor.page().store())?;
            let level = (descriptor.page().ordinal() >> 56)
                .checked_sub(1)
                .ok_or(ReadFailure::Integrity)?;
            if level > u64::from(MAX_ROOT_LEVEL) || u64::from(page.page().level()) != level {
                return Err(ReadFailure::Integrity.into());
            }
            (
                descriptor.length(),
                Content::Manifest {
                    root: page,
                    handle: Box::new(handle),
                    namespace: ns,
                    store: *descriptor.page().store(),
                },
            )
        } else {
            (0, Content::Empty)
        };
        Ok(BaseFile(Arc::new(Version {
            payload,
            size,
            content,
            reader: self.clone(),
            _permit: snapshot,
        })))
    }
}
enum Content {
    Empty,
    Manifest {
        root: VerifiedPage,
        handle: Box<ReadHandle>,
        namespace: Namespace,
        store: [u8; 16],
    },
}
struct Version {
    payload: Option<Payload>,
    size: u64,
    content: Content,
    reader: Reader,
    _permit: OwnedSemaphorePermit,
}
/// An immutable root and metadata captured at open. Clones share one retained
/// snapshot permit; new opens authorize again. No bytes or credentials setter.
#[derive(Clone)]
pub struct File {
    node: Arc<Node>,
    base: BaseFile,
}
/// Authenticated immutable content only; no historical namespace attributes.
/// Clones share the same retained snapshot permit and operation budget.
#[derive(Clone)]
pub struct BaseFile(Arc<Version>);
impl fmt::Debug for BaseFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestBaseFile([private])")
    }
}
impl fmt::Debug for File {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ManifestFile([private])")
    }
}
impl File {
    pub fn metadata(&self) -> &Node {
        &self.node
    }
    pub fn size(&self) -> u64 {
        self.base.size()
    }
    pub fn pinned_base(&self) -> BaseFile {
        self.base.clone()
    }
    pub fn base_descriptor(&self) -> Result<BaseDescriptor> {
        self.base.base_descriptor()
    }
    pub async fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        self.base.read_range(offset, length).await
    }
    pub async fn prove_chunk(&self, chunk: u64) -> Result<ChunkProof> {
        self.base.prove_chunk(chunk).await
    }
    pub fn verify_chunk_proof(&self, encoded: &[u8]) -> Result<ChunkProof> {
        self.base.verify_chunk_proof(encoded)
    }
    pub async fn prove_root(&self) -> Result<PageProof> {
        self.base.prove_root().await
    }
    pub async fn prove_page_containing(&self, offset: u64, level: u8) -> Result<PageProof> {
        self.base.prove_page_containing(offset, level).await
    }
    pub fn verify_page_proof(&self, encoded: &[u8]) -> Result<PageProof> {
        self.base.verify_page_proof(encoded)
    }
}
impl BaseFile {
    pub fn size(&self) -> u64 {
        self.0.size
    }
    /// Return an entire bounded verified range or an error; no partial prefix
    /// escapes on a later chunk failure. EOF/zero reads need no network/permit.
    pub async fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length > MAX_READ_BYTES {
            return Err(ReadFailure::Bounds.into());
        }
        if length == 0 || offset >= self.0.size {
            return Ok(Vec::new());
        }
        let length = (length as u64).min(self.0.size - offset) as usize;
        let _active = self
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(
            self.0.reader.0.options,
            Box::pin(self.read_inner(offset, length)),
        )
        .await
    }
    async fn read_inner(&self, mut offset: u64, length: usize) -> Result<Vec<u8>> {
        let Content::Manifest {
            root,
            handle,
            namespace,
            store,
        } = &self.0.content
        else {
            return Err(ReadFailure::Integrity.into());
        };
        // Clipping above proves this addition is within the pinned logical EOF.
        let end = offset + length as u64;
        let mut output = Vec::with_capacity(length);
        let mut budget = Budget::default();
        while offset < end {
            let mut page = root.clone();
            let mut leaf = None;
            for _ in 0..=MAX_ROOT_LEVEL {
                budget.visit()?;
                let located = page.page().lookup(offset).ok_or(ReadFailure::Integrity)?;
                let index = page
                    .page()
                    .entries()
                    .iter()
                    .position(|e| std::ptr::eq(e, located.entry))
                    .ok_or(ReadFailure::Integrity)?;
                match located.entry {
                    Entry::Child { object, .. } => {
                        let key = namespace.key(Role::Page, object)?;
                        let child = Box::pin(handle.open_related(key.as_str())).await?;
                        let size = child.metadata().size;
                        if !(64..=manifest::MAX_PAGE_BYTES as u64).contains(&size) {
                            return Err(ReadFailure::Integrity.into());
                        }
                        let bytes = Box::pin(child.read_range(0, size as usize)).await?;
                        page = page.verify_child(index, &bytes)?;
                        page_scope(&page, store)?;
                    }
                    _ => {
                        leaf = Some((index, located.offset_in_entry));
                        break;
                    }
                }
            }
            let (index, in_entry) = leaf.ok_or(ReadFailure::Integrity)?;
            let entry = &page.page().entries()[index];
            let take = (entry.length() - in_entry).min(end - offset) as usize;
            if take == 0 {
                return Err(ReadFailure::Integrity.into());
            }
            match entry {
                Entry::Hole { .. } => output.resize(output.len() + take, 0),
                Entry::Data { object, length } => {
                    budget.data(*length)?;
                    let key = namespace.key(Role::Data, object)?;
                    let data = Box::pin(handle.open_related(key.as_str())).await?;
                    if data.metadata().size != *length {
                        return Err(ReadFailure::Integrity.into());
                    }
                    let bytes = Box::pin(data.read_range(0, *length as usize)).await?;
                    page.verify_data(index, &bytes)?;
                    let start = in_entry as usize;
                    output.extend_from_slice(&bytes[start..start + take]);
                }
                Entry::Child { .. } => return Err(ReadFailure::Integrity.into()),
            }
            offset += take as u64;
        }
        if output.len() != length {
            return Err(ReadFailure::Integrity.into());
        }
        Ok(output)
    }
}
fn page_scope(page: &VerifiedPage, store: &[u8; 16]) -> Result<()> {
    for entry in page.page().entries() {
        if let Entry::Child { object, .. } | Entry::Data { object, .. } = entry
            && object.store() != store
        {
            return Err(ReadFailure::Binding.into());
        }
    }
    Ok(())
}
#[derive(Default)]
struct Budget {
    visits: usize,
    objects: usize,
    data_bytes: u64,
}
impl Budget {
    fn visit(&mut self) -> Result<()> {
        self.visits += 1;
        if self.visits > MAX_VISITS {
            return Err(ReadFailure::Bounds.into());
        }
        Ok(())
    }
    fn data(&mut self, bytes: u64) -> Result<()> {
        self.objects += 1;
        self.data_bytes = self
            .data_bytes
            .checked_add(bytes)
            .ok_or(ReadFailure::Bounds)?;
        if self.objects > MAX_DATA || self.data_bytes > MAX_DATA as u64 * manifest::CHUNK_BYTES {
            return Err(ReadFailure::Bounds.into());
        }
        Ok(())
    }
}
