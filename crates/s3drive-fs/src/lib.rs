//! Shared S3 namespace, journal-backed file I/O and revision-bound clean cache.
pub mod edits;
mod managed;
pub use managed::writes::lifecycle::NativeFileObject;
mod writable;
use s3drive_core::Config;
use s3drive_s3::{ListPage, MAX_READ_BYTES, ObjectMetadata, ReadHandle, S3Reader};
use s3drive_state::cache::{Cache, Key, Options, Pinned};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex as AsyncMutex, OnceCell, Semaphore};

pub const ROOT: u64 = 1;
pub const MAX_DIRECTORY_ENTRIES: usize = 10_000;
pub const MAX_NODES: usize = 32_768;
const MAX_PAGES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Directory,
    File,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: u64,
    pub parent: u64,
    pub name: String,
    pub kind: Kind,
    source: Option<usize>,
    key: String,
}

#[derive(Debug)]
pub enum Error {
    Storage(s3drive_s3::Error),
    Configuration,
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
    NotEmpty,
    DeletePending,
    InvalidOperation,
    InvalidName,
    AmbiguousName,
    InvalidListing,
    ResourceLimit,
    NoSpace,
    Timeout,
    Internal,
    Cache(String),
    ReadOnly,
    Conflict,
    TooLarge,
    Busy,
    Write(Box<dyn std::error::Error + Send + Sync>),
    Managed(Box<dyn std::error::Error + Send + Sync>),
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<s3drive_s3::Error> for Error {
    fn from(value: s3drive_s3::Error) -> Self {
        Self::Storage(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(e) => e.fmt(f),
            Self::Cache(e) => write!(f, "local cache: {e}"),
            Self::Managed(e) => write!(f, "managed filesystem: {e}"),
            Self::Write(e) => write!(f, "file mutation failed; local data retained: {e}"),
            _ => write!(f, "filesystem: {self:?}"),
        }
    }
}
impl std::error::Error for Error {}

type Identity = (usize, String, Kind);
struct Nodes {
    by_id: HashMap<u64, Arc<Node>>,
    by_key: HashMap<Identity, u64>,
    next_id: u64,
}

pub struct ReadOnlyFs {
    readers: Vec<S3Reader>,
    nodes: Mutex<Nodes>,
    aliases: Vec<Arc<Node>>,
    cache: Option<Arc<CacheBackend>>,
    writes: Option<Arc<writable::Writes>>,
    managed: Option<Arc<managed::Backend>>,
}

struct CacheBackend {
    worker: Cache,
    // Fixed stripes coalesce same-revision misses without an unbounded map.
    stripes: [AsyncMutex<()>; 16],
    slots: Semaphore,
}

pub struct OpenFile {
    pub node: Arc<Node>,
    handle: Option<ReadHandle>,
    initial_metadata: ObjectMetadata,
    managed: Option<s3drive_managed::FileSnapshot>,
    managed_write: Option<(Arc<managed::writes::Writes>, Arc<managed::writes::File>)>,
    cache: Option<(Arc<CacheBackend>, Key)>,
    snapshot: OnceCell<Pinned>,
    bypass: AtomicBool,
    writes: Option<(Arc<writable::Writes>, Arc<writable::File>)>,
    writable_open: bool,
}
impl OpenFile {
    pub fn is_unlinked(&self) -> Result<bool> {
        match &self.managed_write {
            Some((w, f)) => w.is_unlinked(f),
            None => Ok(false),
        }
    }
    pub fn current_metadata(&self) -> Result<ObjectMetadata> {
        if let Some((_, file)) = &self.managed_write {
            return file.metadata();
        }
        match &self.writes {
            Some((_, file)) => file.metadata(),
            None => Ok(self.initial_metadata.clone()),
        }
    }
    pub async fn write(
        &self,
        offset: u64,
        bytes: Arc<[u8]>,
        append: bool,
        constrained: bool,
    ) -> Result<usize> {
        if !self.writable_open {
            return Err(Error::ReadOnly);
        }
        if let Some((writer, file)) = &self.managed_write {
            return Box::pin(writer.write(file, offset, bytes, append, constrained)).await;
        }
        let (writer, file) = self.writes.as_ref().ok_or(Error::ReadOnly)?;
        Box::pin(writer.write(file, offset, bytes, append, constrained)).await
    }
    pub async fn truncate(&self, size: u64) -> Result<()> {
        if !self.writable_open {
            return Err(Error::ReadOnly);
        }
        if let Some((writer, file)) = &self.managed_write {
            return Box::pin(writer.truncate(file, size)).await;
        }
        let (writer, file) = self.writes.as_ref().ok_or(Error::ReadOnly)?;
        Box::pin(writer.truncate(file, size)).await
    }
    pub async fn flush(&self) -> Result<()> {
        if let Some((writer, file)) = &self.managed_write {
            Box::pin(writer.flush(file)).await?;
        }
        if let Some((writer, file)) = &self.writes {
            Box::pin(writer.flush(file)).await?;
        }
        Ok(())
    }
    pub async fn flush_on_close(&self) -> Result<()> {
        if self.managed_write.is_some() && !self.writable_open {
            return Ok(());
        }
        self.flush().await
    }
    pub fn metadata(&self) -> &ObjectMetadata {
        &self.initial_metadata
    }
    pub async fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if let Some((_, file)) = &self.managed_write {
            return file.read(offset, length).await;
        }
        if let Some((_, file)) = &self.writes {
            return file.read(offset, length).await;
        }
        // Keep the transport's validation/EOF behavior on cached reads too.
        if length > MAX_READ_BYTES {
            return Err(s3drive_s3::Error {
                operation: "read",
                kind: s3drive_s3::ErrorKind::InvalidInput,
                http_status: None,
            }
            .into());
        }
        if length == 0 || offset >= self.metadata().size {
            return Ok(Vec::new());
        }
        if let Some(snapshot) = &self.managed {
            return Ok(slice(snapshot.bytes(), offset, length));
        }
        if let Some(snapshot) = self.snapshot.get() {
            return Ok(slice(snapshot.bytes(), offset, length));
        }
        let Some((cache, key)) = &self.cache else {
            return Ok(Box::pin(
                self.handle
                    .as_ref()
                    .ok_or(Error::Internal)?
                    .read_range(offset, length),
            )
            .await?);
        };
        if self.bypass.load(Ordering::Relaxed) {
            return Ok(Box::pin(
                self.handle
                    .as_ref()
                    .ok_or(Error::Internal)?
                    .read_range(offset, length),
            )
            .await?);
        }
        let _stripe = cache.stripes[key.stripe()].lock().await;
        let _slot = cache.slots.acquire().await.map_err(|_| Error::Internal)?;
        if let Some(snapshot) = self.snapshot.get() {
            return Ok(slice(snapshot.bytes(), offset, length));
        }
        if self.bypass.load(Ordering::Relaxed) {
            return Ok(Box::pin(
                self.handle
                    .as_ref()
                    .ok_or(Error::Internal)?
                    .read_range(offset, length),
            )
            .await?);
        }
        if let Some(snapshot) = cache.worker.get(key.clone()).await.map_err(cache_error)? {
            let bytes = slice(snapshot.bytes(), offset, length);
            self.snapshot.set(snapshot).map_err(|_| Error::Internal)?;
            return Ok(bytes);
        }
        // No SQL transaction spans this request. At most four complete small
        // files are being downloaded/processed per mount at once.
        let bytes: Arc<[u8]> = Box::pin(
            self.handle
                .as_ref()
                .ok_or(Error::Internal)?
                .read_range(0, self.metadata().size as usize),
        )
        .await?
        .into();
        let result = slice(&bytes, offset, length);
        match cache
            .worker
            .put(key.clone(), bytes)
            .await
            .map_err(cache_error)?
        {
            Some(snapshot) => {
                self.snapshot.set(snapshot).map_err(|_| Error::Internal)?;
            }
            None => {
                self.bypass.store(true, Ordering::Relaxed);
            }
        }
        Ok(result)
    }
}
fn slice(bytes: &[u8], offset: u64, length: usize) -> Vec<u8> {
    let start = (offset.min(bytes.len() as u64)) as usize;
    bytes[start..start + length.min(bytes.len() - start)].to_vec()
}
fn cache_error(error: Box<dyn std::error::Error + Send + Sync>) -> Error {
    if s3drive_state::cache::is_disk_full(error.as_ref()) {
        Error::NoSpace
    } else {
        Error::Cache(error.to_string())
    }
}

impl ReadOnlyFs {
    pub async fn new(config: &Config) -> Result<Self> {
        Box::pin(Self::new_inner(config)).await
    }
    async fn new_inner(config: &Config) -> Result<Self> {
        config.validate().map_err(|_| Error::Configuration)?;
        if config.managed.is_some() {
            return Box::pin(managed::filesystem(config)).await;
        }
        if config.sources.len() >= MAX_NODES {
            return Err(Error::ResourceLimit);
        }
        let root = Arc::new(Node {
            id: ROOT,
            parent: ROOT,
            name: String::new(),
            kind: Kind::Directory,
            source: None,
            key: String::new(),
        });
        let mut nodes = Nodes {
            by_id: HashMap::from([(ROOT, root)]),
            by_key: HashMap::new(),
            next_id: ROOT + 1,
        };
        let mut aliases = Vec::new();
        let mut readers = Vec::new();
        let options = config
            .cache
            .as_ref()
            .map(|c| Options {
                max_bytes: c.max_bytes,
                max_file_bytes: c.max_file_bytes,
                max_entries: c.max_entries,
            })
            .unwrap_or_default();
        let storage = if let Some(writes) = &config.writes {
            Some(
                s3drive_state::worker::Storage::open(
                    &writes.directory,
                    options,
                    s3drive_state::journal::Options::default(),
                )
                .await
                .map_err(cache_error)?,
            )
        } else {
            None
        };
        let mut executors = Vec::new();
        for (source_index, source) in config.sources.iter().enumerate() {
            let mut reader = S3Reader::from_source(source).await?;
            let executor = if let Some(settings) = config
                .writes
                .as_ref()
                .and_then(|w| w.sources.iter().find(|s| s.alias == source.alias))
            {
                fn text(path: &std::path::Path) -> Result<String> {
                    use std::io::Read;
                    let file = std::fs::File::open(path).map_err(|_| Error::Configuration)?;
                    if !file.metadata().map_err(|_| Error::Configuration)?.is_file() {
                        return Err(Error::Configuration);
                    }
                    let mut text = String::new();
                    file.take(32769)
                        .read_to_string(&mut text)
                        .map_err(|_| Error::Configuration)?;
                    Ok(text)
                }
                let policy = s3drive_s3::uploads::WritePolicy::from_toml(
                    source,
                    &text(&settings.policy)?,
                    &text(&settings.plan)?,
                    &text(&settings.report)?,
                )
                .map_err(|_| Error::Configuration)?;
                let executor = s3drive_s3::uploads::Executor::open(
                    reader.clone(),
                    storage.clone().ok_or(Error::Internal)?,
                    policy,
                )
                .await
                .map_err(|_| Error::Configuration)?;
                reader = executor.reader();
                Some(executor)
            } else {
                None
            };
            executors.push(executor);
            readers.push(reader);
            let node = nodes.insert(
                source_index,
                "".into(),
                Kind::Directory,
                ROOT,
                source.alias.clone(),
            )?;
            aliases.push(node);
        }
        aliases.sort_by(|a, b| a.name.cmp(&b.name));
        let cache = if let Some(settings) = &config.cache {
            Some(Arc::new(CacheBackend {
                worker: match &storage {
                    Some(storage) => storage.cache(),
                    None => Cache::open(
                        &settings.directory,
                        Options {
                            max_bytes: settings.max_bytes,
                            max_file_bytes: settings.max_file_bytes,
                            max_entries: settings.max_entries,
                        },
                    )
                    .await
                    .map_err(cache_error)?,
                },
                stripes: std::array::from_fn(|_| AsyncMutex::new(())),
                slots: Semaphore::new(4),
            }))
        } else {
            None
        };
        Ok(Self {
            readers,
            nodes: Mutex::new(nodes),
            aliases,
            cache,
            writes: storage.map(|storage| Arc::new(writable::Writes::new(storage, executors))),
            managed: None,
        })
    }

    /// Capture before moving the filesystem into a native adapter. Poll only
    /// after callbacks stop; drain accepted I/O before closing the SQLite owner.
    pub fn shutdown_writes(
        &self,
    ) -> impl std::future::Future<Output = Result<()>> + Send + 'static {
        let writes = self.managed.as_ref().and_then(|m| m.writes.clone());
        async move {
            if let Some(w) = writes {
                w.stop().await?;
            }
            Ok(())
        }
    }
    /// Adapters close this worker only after callbacks stop and writes drain.
    pub fn cache_control(&self) -> Option<Cache> {
        self.cache
            .as_ref()
            .map(|cache| cache.worker.clone())
            .or_else(|| self.writes.as_ref().map(|w| w.storage.cache()))
            .or_else(|| {
                self.managed
                    .as_ref()
                    .and_then(|m| m.writes.as_ref())
                    .map(|w| w.storage.cache())
            })
    }
    pub fn is_read_only(&self) -> bool {
        self.writes.is_none() && self.managed.as_ref().is_none_or(|m| m.writes.is_none())
    }
    pub async fn capacity(&self) -> Result<Option<s3drive_state::journal::Capacity>> {
        if let Some(w) = self.managed.as_ref().and_then(|m| m.writes.as_ref()) {
            return Ok(Some(
                w.storage.staging().capacity().await.map_err(cache_error)?,
            ));
        }
        match &self.writes {
            Some(w) => Ok(Some(
                w.storage.staging().capacity().await.map_err(cache_error)?,
            )),
            None => Ok(None),
        }
    }
    pub fn writable(&self, node: &Node) -> bool {
        if let Some(w) = self.managed.as_ref().and_then(|m| m.writes.as_ref()) {
            return node.kind == Kind::File && w.permits(&node.key);
        }
        node.kind == Kind::File
            && node.source.is_some_and(|source| {
                self.writes
                    .as_ref()
                    .is_some_and(|w| w.permits(source, &node.key))
            })
    }
    pub fn namespace_mutation(&self) -> bool {
        self.managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .is_some_and(|w| w.mutations_enabled())
    }
    pub fn native_lifecycle_enabled(&self) -> bool {
        self.managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .is_some_and(|w| w.native_lifecycle_enabled())
    }
    pub async fn native_file_object(
        &self,
        node: u64,
        doc: bool,
    ) -> Result<Option<NativeFileObject>> {
        let node = self.node(node)?;
        match self.managed.as_ref().and_then(|m| m.writes.as_ref()) {
            Some(w) => w.native_open(&node.key, doc).await,
            None => {
                if doc {
                    Err(Error::ReadOnly)
                } else {
                    Ok(None)
                }
            }
        }
    }
    pub async fn create_native_deferred(
        &self,
        parent: u64,
        name: &str,
        directory: bool,
    ) -> Result<(Arc<Node>, NativeFileObject)> {
        validate_name(name)?;
        let parent = self.node(parent)?;
        if parent.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        if !self.writable_directory(&parent) {
            return Err(Error::ReadOnly);
        }
        let w = self
            .managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .ok_or(Error::ReadOnly)?;
        let (created, object) = w
            .native_create(
                &parent.key,
                name,
                if directory {
                    s3drive_authority::Kind::Directory
                } else {
                    s3drive_authority::Kind::EmptyFile
                },
            )
            .await?;
        let node = self
            .nodes
            .lock()
            .map_err(|_| Error::Internal)?
            .insert_managed(&created, parent.id)?;
        Ok((node, object))
    }
    pub async fn rename_native_object(
        &self,
        object: &NativeFileObject,
        parent: u64,
        name: &str,
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<Arc<Node>> {
        validate_name(name)?;
        let parent = self.node(parent)?;
        if parent.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let changed = object.rename(&parent.key, name, replace, same_name).await?;
        self.nodes
            .lock()
            .map_err(|_| Error::Internal)?
            .insert_managed(&changed, parent.id)
    }
    pub fn namespace_creation(&self) -> bool {
        self.managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .is_some_and(|w| w.namespace_enabled())
    }
    pub fn writable_directory(&self, node: &Node) -> bool {
        if let Some(w) = self.managed.as_ref().and_then(|m| m.writes.as_ref()) {
            return node.kind == Kind::Directory && w.permits_directory();
        }
        node.kind == Kind::Directory
            && node.source.is_some_and(|source| {
                self.writes
                    .as_ref()
                    .is_some_and(|w| w.permits_directory(source, &node.key))
            })
    }
    /// The adapter supplies its native comparison, under the shared create lock.
    pub async fn create_file(
        &self,
        parent: u64,
        name: &str,
        same_name: fn(&str, &str) -> bool,
        writing: bool,
    ) -> Result<OpenFile> {
        validate_name(name)?;
        let parent = self.node(parent)?;
        if parent.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        if !self.writable_directory(&parent) {
            return Err(Error::ReadOnly);
        }
        if let Some(w) = self.managed.as_ref().and_then(|m| m.writes.as_ref()) {
            let created = w
                .create_node(&parent.key, name, s3drive_authority::Kind::EmptyFile)
                .await?;
            let node = self
                .nodes
                .lock()
                .map_err(|_| Error::Internal)?
                .insert_managed(&created, parent.id)?;
            return Box::pin(self.open_mode(node.id, writing)).await;
        }
        let writer = self.writes.as_ref().ok_or(Error::ReadOnly)?;
        let _serial = writer.create_serial.lock().await;
        let source = parent.source.ok_or(Error::ReadOnly)?;
        let children = Box::pin(self.list(parent.id)).await?;
        if children.iter().any(|child| same_name(&child.name, name)) {
            return Err(Error::AlreadyExists);
        }
        if children.len() >= MAX_DIRECTORY_ENTRIES
            || self.nodes.lock().map_err(|_| Error::Internal)?.by_id.len() >= MAX_NODES
        {
            return Err(Error::ResourceLimit);
        }
        let key = format!("{}{name}", parent.key);
        match self.readers[source].head(&key).await {
            Ok(_) => return Err(Error::AlreadyExists),
            Err(error) if error.kind == s3drive_s3::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Box::pin(writer.create(source, &key)).await?;
        let node = self.nodes.lock().map_err(|_| Error::Internal)?.insert(
            source,
            key,
            Kind::File,
            parent.id,
            name.into(),
        )?;
        Box::pin(self.open_mode(node.id, writing)).await
    }
    pub async fn create_directory(&self, parent: u64, name: &str) -> Result<Arc<Node>> {
        validate_name(name)?;
        let parent = self.node(parent)?;
        if parent.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let w = self
            .managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .ok_or(Error::ReadOnly)?;
        let created = w
            .create_node(&parent.key, name, s3drive_authority::Kind::Directory)
            .await?;
        self.nodes
            .lock()
            .map_err(|_| Error::Internal)?
            .insert_managed(&created, parent.id)
    }
    pub async fn rename_entry(
        &self,
        parent: u64,
        name: &str,
        new_parent: u64,
        new_name: &str,
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<Arc<Node>> {
        Box::pin(self.rename_entry_inner(parent, name, new_parent, new_name, replace, same_name))
            .await
    }
    async fn rename_entry_inner(
        &self,
        parent: u64,
        name: &str,
        new_parent: u64,
        new_name: &str,
        replace: bool,
        same_name: fn(&str, &str) -> bool,
    ) -> Result<Arc<Node>> {
        if !self.namespace_mutation() {
            return Err(Error::ReadOnly);
        }
        validate_name(name)?;
        validate_name(new_name)?;
        let parent = self.node(parent)?;
        let target_parent = self.node(new_parent)?;
        if parent.kind != Kind::Directory || target_parent.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let source = self
            .list(parent.id)
            .await?
            .into_iter()
            .find(|n| same_name(&n.name, name))
            .ok_or(Error::NotFound)?;
        let writes = self
            .managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .ok_or(Error::ReadOnly)?;
        let changed = writes
            .rename(
                &source.key,
                &parent.key,
                name,
                (&target_parent.key, new_name),
                replace,
                same_name,
            )
            .await?;
        self.nodes
            .lock()
            .map_err(|_| Error::Internal)?
            .insert_managed(&changed, target_parent.id)
    }
    pub async fn remove_entry(&self, parent: u64, name: &str, directory: bool) -> Result<()> {
        Box::pin(self.remove_entry_inner(parent, name, directory)).await
    }
    async fn remove_entry_inner(&self, parent: u64, name: &str, directory: bool) -> Result<()> {
        if !self.namespace_mutation() {
            return Err(Error::ReadOnly);
        }
        validate_name(name)?;
        let p = self.node(parent)?;
        if p.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let source = self.lookup(parent, name).await?;
        self.managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .ok_or(Error::ReadOnly)?
            .remove(&source.key, &p.key, name, directory)
            .await
    }
    pub async fn sync_namespace(&self) -> Result<()> {
        if !self.namespace_mutation() {
            return Err(Error::ReadOnly);
        }
        self.managed
            .as_ref()
            .and_then(|m| m.writes.as_ref())
            .ok_or(Error::ReadOnly)?
            .sync_namespace()
            .await
    }
    pub async fn flush_all(&self) -> Result<()> {
        if let Some(w) = self.managed.as_ref().and_then(|m| m.writes.as_ref()) {
            w.flush_all().await?;
        }
        if let Some(writes) = &self.writes {
            writes.flush_all().await?;
        }
        Ok(())
    }

    pub fn node(&self, id: u64) -> Result<Arc<Node>> {
        self.nodes
            .lock()
            .map_err(|_| Error::Internal)?
            .by_id
            .get(&id)
            .cloned()
            .ok_or(Error::NotFound)
    }

    /// A complete bounded directory view. Pagination is not an atomic S3 snapshot.
    /// No namespace lock is held while waiting for the network.
    pub async fn list(&self, id: u64) -> Result<Vec<Arc<Node>>> {
        let parent = self.node(id)?;
        if parent.kind != Kind::Directory {
            return Err(Error::NotDirectory);
        }
        if let Some(backend) = &self.managed {
            let listing = backend
                .reader
                .list(&parent.key)
                .await
                .map_err(managed::error)?;
            let mut nodes = self.nodes.lock().map_err(|_| Error::Internal)?;
            return listing
                .children
                .into_iter()
                .map(|n| {
                    if let Some(w) = &backend.writes {
                        w.remember(&n)?;
                    }
                    nodes.insert_managed(&n, parent.id)
                })
                .collect();
        }
        let Some(source) = parent.source else {
            return Ok(self.aliases.clone());
        };
        let entries = tokio::time::timeout(Duration::from_secs(60), async {
            let mut entries = BTreeMap::new();
            let mut token = None;
            let mut seen = HashSet::new();
            for _ in 0..MAX_PAGES {
                let page = self.readers[source]
                    .list_page(&parent.key, token.as_deref(), 1000)
                    .await?;
                let next = page.next_token.clone();
                project_page(&parent.key, page, &mut entries)?;
                if entries.len() > MAX_DIRECTORY_ENTRIES {
                    return Err(Error::ResourceLimit);
                }
                match next {
                    None => {
                        if let Some(writes) = &self.writes {
                            Box::pin(writes.merge_directory(source, &parent.key, &mut entries))
                                .await?;
                        }
                        return Ok(entries);
                    }
                    Some(next) => {
                        if !seen.insert(next.clone()) {
                            return Err(Error::InvalidListing);
                        }
                        token = Some(next);
                    }
                }
            }
            Err(Error::ResourceLimit)
        })
        .await
        .map_err(|_| Error::Timeout)??;
        let mut nodes = self.nodes.lock().map_err(|_| Error::Internal)?;
        // Fail before publishing a partial directory when the mount-wide cap is hit.
        let needed = entries
            .values()
            .filter(|entry| {
                !nodes
                    .by_key
                    .contains_key(&(source, entry.key.clone(), entry.kind))
            })
            .count();
        if nodes.by_id.len() + needed > MAX_NODES {
            return Err(Error::ResourceLimit);
        }
        entries
            .into_iter()
            .map(|(name, entry)| nodes.insert(source, entry.key, entry.kind, id, name))
            .collect()
    }

    pub async fn lookup(&self, parent: u64, name: &str) -> Result<Arc<Node>> {
        validate_name(name)?;
        self.list(parent)
            .await?
            .into_iter()
            .find(|node| node.name == name)
            .ok_or(Error::NotFound)
    }

    pub async fn metadata(&self, id: u64) -> Result<Option<ObjectMetadata>> {
        let node = self.node(id)?;
        if let Some(backend) = &self.managed {
            let live = backend
                .reader
                .node(&node.key)
                .await
                .map_err(managed::error)?;
            return Ok(if live.kind == s3drive_authority::Kind::Directory {
                None
            } else {
                {
                    if let Some(w) = &backend.writes {
                        w.remember(&live)?;
                        if let Some(metadata) = w.metadata(&live).await? {
                            return Ok(Some(metadata));
                        }
                    }
                    Some(managed::metadata(&live))
                }
            });
        }
        if self.writes.is_some() && node.kind == Kind::File {
            return Ok(Some(Box::pin(self.open(id)).await?.current_metadata()?));
        }
        match node.kind {
            Kind::Directory => Ok(None),
            Kind::File => Ok(Some(
                self.readers[node.source.ok_or(Error::Internal)?]
                    .head(&node.key)
                    .await?,
            )),
        }
    }

    pub async fn open(&self, id: u64) -> Result<OpenFile> {
        Box::pin(self.open_mode(id, false)).await
    }
    pub async fn open_write(&self, id: u64) -> Result<OpenFile> {
        Box::pin(self.open_mode(id, true)).await
    }
    async fn open_mode(&self, id: u64, writing: bool) -> Result<OpenFile> {
        let node = self.node(id)?;
        if node.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        if let Some(backend) = &self.managed {
            if writing && backend.writes.is_none() {
                return Err(Error::ReadOnly);
            }
            let (snapshot, managed_write) = if let Some(w) = &backend.writes {
                let (snapshot, file) = Box::pin(w.open(&node.key, writing)).await?;
                (snapshot, file.map(|file| (w.clone(), file)))
            } else {
                (
                    Box::pin(backend.reader.read(&node.key))
                        .await
                        .map_err(managed::error)?,
                    None,
                )
            };
            let initial_metadata = managed::metadata(snapshot.metadata());
            return Ok(OpenFile {
                initial_metadata,
                node,
                handle: None,
                managed: if managed_write.is_none() {
                    Some(snapshot)
                } else {
                    None
                },
                managed_write,
                cache: None,
                snapshot: OnceCell::new(),
                bypass: AtomicBool::new(false),
                writes: None,
                writable_open: writing,
            });
        }
        let reader = &self.readers[node.source.ok_or(Error::Internal)?];
        let mut handle = reader.open(&node.key).await?;
        let writes = if let Some(writes) = &self.writes {
            let (fresh, file) =
                Box::pin(writes.open(node.source.ok_or(Error::Internal)?, &handle, writing))
                    .await?;
            if let Some(fresh) = fresh {
                handle = fresh;
            }
            file.map(|file| (writes.clone(), file))
        } else {
            if writing {
                return Err(Error::ReadOnly);
            }
            None
        };
        // Authorization above always precedes cache-key construction/acquisition.
        let cache = match &self.cache {
            Some(cache) if cache.worker.accepts(handle.metadata().size) => {
                let meta = handle.metadata();
                let key = Key::new(
                    *handle.cache_identity().as_bytes(),
                    reader.configuration_fingerprint(),
                    &meta.key,
                    meta.revision.etag.as_deref(),
                    meta.revision.version_id.as_deref(),
                    meta.size,
                )
                .map_err(cache_error)?;
                Some((cache.clone(), key))
            }
            _ => None,
        };
        Ok(OpenFile {
            node,
            initial_metadata: handle.metadata().clone(),
            handle: Some(handle),
            managed: None,
            managed_write: None,
            cache,
            snapshot: OnceCell::new(),
            bypass: AtomicBool::new(false),
            writes,
            writable_open: writing,
        })
    }
}

impl Nodes {
    fn insert(
        &mut self,
        source: usize,
        key: String,
        kind: Kind,
        parent: u64,
        name: String,
    ) -> Result<Arc<Node>> {
        let identity = (source, key.clone(), kind);
        if let Some(id) = self.by_key.get(&identity) {
            return self.by_id.get(id).cloned().ok_or(Error::Internal);
        }
        if self.by_id.len() >= MAX_NODES {
            return Err(Error::ResourceLimit);
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or(Error::ResourceLimit)?;
        let node = Arc::new(Node {
            id,
            parent,
            name,
            kind,
            source: Some(source),
            key,
        });
        self.by_id.insert(id, node.clone());
        self.by_key.insert(identity, id);
        Ok(node)
    }
}

#[derive(Debug)]
struct Entry {
    key: String,
    kind: Kind,
}
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.len() > 255
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err(Error::InvalidName);
    }
    Ok(())
}
fn project_page(prefix: &str, page: ListPage, entries: &mut BTreeMap<String, Entry>) -> Result<()> {
    for key in page.directories {
        let name = key
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix('/'))
            .ok_or(Error::InvalidListing)?;
        insert_entry(
            entries,
            name.to_owned(),
            Entry {
                key,
                kind: Kind::Directory,
            },
        )?;
    }
    for object in page.objects {
        // A zero-byte object whose key equals this directory is its folder marker.
        if object.key == prefix && object.size == 0 {
            continue;
        }
        let name = object
            .key
            .strip_prefix(prefix)
            .ok_or(Error::InvalidListing)?
            .to_owned();
        insert_entry(
            entries,
            name,
            Entry {
                key: object.key,
                kind: Kind::File,
            },
        )?;
    }
    Ok(())
}
fn insert_entry(entries: &mut BTreeMap<String, Entry>, name: String, entry: Entry) -> Result<()> {
    validate_name(&name)?;
    if let Some(previous) = entries.get(&name) {
        return Err(if previous.kind == entry.kind {
            Error::InvalidListing
        } else {
            Error::AmbiguousName
        });
    }
    entries.insert(name, entry);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn construction_future_keeps_large_sdk_state_off_caller_stacks() {
        let config =
            s3drive_core::Config::parse(include_str!("../../../examples/s3drive.toml")).unwrap();
        assert!(std::mem::size_of_val(&super::ReadOnlyFs::new(&config)) < 16 * 1024);
        let fs = super::ReadOnlyFs {
            readers: Vec::new(),
            nodes: std::sync::Mutex::new(super::Nodes {
                by_id: std::collections::HashMap::new(),
                by_key: std::collections::HashMap::new(),
                next_id: 2,
            }),
            aliases: Vec::new(),
            cache: None,
            writes: None,
            managed: None,
        };
        assert!(
            std::mem::size_of_val(&fs.rename_entry(1, "old", 1, "new", true, |a, b| a == b))
                < 16 * 1024
        );
        assert!(std::mem::size_of_val(&fs.remove_entry(1, "old", false)) < 16 * 1024);
    }

    use super::*;
    use s3drive_s3::ListedObject;
    fn page(files: &[(&str, u64)], dirs: &[&str]) -> ListPage {
        ListPage {
            objects: files
                .iter()
                .map(|(key, size)| ListedObject {
                    key: (*key).into(),
                    size: *size,
                    etag: None,
                    modified_unix_seconds: None,
                })
                .collect(),
            directories: dirs.iter().map(|s| (*s).into()).collect(),
            next_token: None,
        }
    }
    #[test]
    fn projects_exact_names_and_ignores_only_empty_folder_markers() {
        let mut entries = BTreeMap::new();
        project_page(
            "dir/",
            page(
                &[
                    ("dir/", 0),
                    ("dir/a%2Fb + café", 3),
                    ("dir/Case", 0),
                    ("dir/case", 0),
                ],
                &["dir/sub/"],
            ),
            &mut entries,
        )
        .unwrap();
        assert_eq!(
            entries.keys().map(String::as_str).collect::<Vec<_>>(),
            ["Case", "a%2Fb + café", "case", "sub"]
        );
        assert_eq!(entries["sub"].key, "dir/sub/");
        assert!(project_page("dir/", page(&[("dir/", 5)], &[]), &mut BTreeMap::new()).is_err());
    }
    #[test]
    fn fails_whole_directory_on_collisions_duplicates_and_invalid_projection() {
        for bad in [
            page(&[("a", 0)], &["a/"]),
            page(&[("a", 0), ("a", 0)], &[]),
            page(&[("../x", 0)], &[]),
            page(&[("a/b", 0)], &[]),
            page(&[("a\\b", 0)], &[]),
            page(&[("\0", 0)], &[]),
            page(&[(".", 0)], &[]),
            page(&[("", 4)], &[]),
            page(&[(&"é".repeat(128), 0)], &[]),
        ] {
            assert!(project_page("", bad, &mut BTreeMap::new()).is_err());
        }
        assert!(
            project_page(
                "prefix/",
                page(&[("outside", 0)], &[]),
                &mut BTreeMap::new()
            )
            .is_err()
        );
        let mut entries = BTreeMap::new();
        project_page("", page(&[("a", 0)], &[]), &mut entries).unwrap();
        assert!(matches!(
            project_page("", page(&[], &["a/"]), &mut entries),
            Err(Error::AmbiguousName)
        ));
    }
    #[test]
    fn stable_inodes_do_not_alias_sources_or_types() {
        let mut nodes = Nodes {
            by_id: HashMap::new(),
            by_key: HashMap::new(),
            next_id: 2,
        };
        let a = nodes
            .insert(0, "file".into(), Kind::File, 1, "file".into())
            .unwrap();
        let same = nodes
            .insert(0, "file".into(), Kind::File, 1, "file".into())
            .unwrap();
        let other = nodes
            .insert(1, "file".into(), Kind::File, 1, "file".into())
            .unwrap();
        let dir = nodes
            .insert(0, "file/".into(), Kind::Directory, 1, "file".into())
            .unwrap();
        assert_eq!(a.id, same.id);
        assert_ne!(a.id, other.id);
        assert_ne!(a.id, dir.id);
    }
}
