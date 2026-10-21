//! Explicit sparse native writer. No old-origin promotion or deletion lifecycle.
use super::*;
use s3drive_authority::{Node as RemoteNode, NodeGuard, Reply, Request, Store};
use s3drive_managed::{
    chunks::{EditedFile, MixedRootRecovery, Reader},
    creation::{Creator, Draft},
};
use s3drive_s3::uploads::Executor;
use s3drive_state::{
    chunk_mount,
    namespace_client::Control,
    sparse_views::{
        self as views, Binding, ScopedOperation, View,
        preparation::{self as plans, publication as roots},
    },
    worker::Storage,
};
use sha2::{Digest, Sha256};
use std::sync::Weak;
mod dispatch;
const DEADLINE: Duration = Duration::from_secs(120);
fn derived(domain: &[u8], id: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(id);
    h.finalize().into()
}
fn hex(id: &[u8]) -> String {
    id.iter().map(|n| format!("{n:02x}")).collect()
}
fn fresh() -> Result<[u8; 32]> {
    let mut n = [0; 32];
    getrandom::fill(&mut n).map_err(|_| Error::Internal)?;
    Ok(n)
}
fn token(b: &Binding, op: [u8; 32]) -> Result<ScopedOperation> {
    ScopedOperation::new(b, op).map_err(error)
}
pub(crate) struct Writes {
    pub storage: Storage,
    reader: Reader,
    authority: WireClient,
    volume: String,
    default_store: String,
    local: views::Client,
    roots: roots::Client,
    stores: HashMap<String, (Store, Executor, Creator)>,
    _capability: chunk_mount::Client,
    entries: Mutex<HashMap<String, Weak<File>>>,
    known: Mutex<HashMap<String, String>>,
    open_serial: AsyncMutex<()>,
    recovery_serial: AsyncMutex<()>,
    publish_serial: AsyncMutex<()>,
    slots: Arc<Semaphore>,
    closing: AtomicBool,
}
pub(crate) enum Opened {
    Sparse(Arc<File>),
    Chunks(Box<s3drive_managed::chunks::File>),
    Legacy(Box<s3drive_managed::FileSnapshot>),
}
pub(crate) struct File {
    binding: Binding,
    reader: Reader,
    local: views::Client,
    metadata: Mutex<ObjectMetadata>,
    edited: Mutex<Option<EditedFile>>,
    serial: AsyncMutex<()>,
}
impl File {
    pub fn metadata(&self) -> Result<ObjectMetadata> {
        Ok(self.metadata.lock().map_err(|_| Error::Internal)?.clone())
    }
    async fn head(&self) -> Result<View> {
        let head = self
            .local
            .current_head(self.binding.clone())
            .await
            .map_err(error)?;
        self.metadata.lock().map_err(|_| Error::Internal)?.size = head.size();
        Ok(head)
    }
    pub async fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length > MAX_READ_BYTES {
            return Err(Error::TooLarge);
        }
        let _serial = self.serial.lock().await;
        let head = self.head().await?;
        if length == 0 || offset >= head.size() {
            return Ok(Vec::new());
        }
        let cached = self.edited.lock().map_err(|_| Error::Internal)?.clone();
        let edited = if let Some(e) = cached.filter(|e| e.view().id() == head.id()) {
            e
        } else {
            let e = self
                .reader
                .reopen_edited(self.local.clone(), head.id())
                .await
                .map_err(error)?;
            *self.edited.lock().map_err(|_| Error::Internal)? = Some(e.clone());
            e
        };
        edited.read_range(offset, length).await.map_err(error)
    }
}
impl Writes {
    pub async fn new(
        storage: Storage,
        authority: WireClient,
        volume: String,
        reader: Reader,
        executors: HashMap<String, Executor>,
        default_store: String,
    ) -> Result<Self> {
        let mut stores = HashMap::new();
        let mut controls = Vec::new();
        for (id, e) in executors {
            let Reply::Store(store) = authority
                .request(&Request::Store {
                    volume: volume.clone(),
                    store: id.clone(),
                })
                .await
                .map_err(error)?
            else {
                return Err(Error::Configuration);
            };
            if store.id != id || store.route != e.reader().storage_fingerprint() {
                return Err(Error::Configuration);
            }
            let target = e
                .recovery_target(&format!("{}binding", store.root))
                .map_err(error)?;
            controls.push(Control {
                store: store.clone(),
                source: *target.source(),
                fingerprint: target.fingerprint().into(),
            });
            let creator = Creator::bind(
                e.clone(),
                authority.clone(),
                volume.clone(),
                store.clone(),
                s3drive_state::manifest_creation::Limits::default(),
                DEADLINE,
            )
            .map_err(error)?;
            stores.insert(id, (store, e, creator));
        }
        if !stores.contains_key(&default_store) {
            return Err(Error::Configuration);
        }
        let capability = storage
            .chunk_mount(chunk_mount::Input {
                volume: volume.clone(),
                controls,
            })
            .await
            .map_err(error)?;
        if !capability.same_storage(&storage) {
            return Err(Error::Configuration);
        }
        let local = storage
            .sparse_views(views::Options::default())
            .map_err(error)?;
        local.enable_scoped().await.map_err(error)?;
        let roots = local
            .preparations(plans::Options::default())
            .map_err(error)?
            .publications(roots::Limits::default())
            .map_err(error)?;
        let writer = Self {
            storage,
            reader,
            authority,
            volume,
            default_store,
            local,
            roots,
            stores,
            _capability: capability,
            entries: Mutex::new(HashMap::new()),
            known: Mutex::new(HashMap::new()),
            open_serial: AsyncMutex::new(()),
            recovery_serial: AsyncMutex::new(()),
            publish_serial: AsyncMutex::new(()),
            slots: Arc::new(Semaphore::new(4)),
            closing: AtomicBool::new(false),
        };
        tokio::time::timeout(DEADLINE, writer.recover())
            .await
            .map_err(|_| Error::Timeout)??;
        for head in writer.local.heads().await.map_err(error)? {
            let (store, e, _) = writer
                .stores
                .get(&hex(head.binding().store()))
                .ok_or(Error::Conflict)?;
            let target = e
                .recovery_target(&format!("{}binding", store.root))
                .map_err(error)?;
            if target.source() != head.binding().source()
                || target.fingerprint() != head.binding().fingerprint()
            {
                return Err(Error::Conflict);
            }
            writer.known.lock().map_err(|_| Error::Internal)?.insert(
                head.binding().base().map_err(error)?.node().into(),
                hex(head.binding().store()),
            );
        }
        Ok(writer)
    }
    fn admit(&self) -> Result<tokio::sync::OwnedSemaphorePermit> {
        if self.closing.load(Ordering::Acquire) {
            return Err(Error::Busy);
        }
        let p = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::Busy)?;
        if self.closing.load(Ordering::Acquire) {
            return Err(Error::Busy);
        }
        Ok(p)
    }
    pub fn remember(&self, n: &RemoteNode) -> Result<()> {
        if let Some(p) = &n.payload
            && p.reservation.manifest.is_some()
            && self.stores.contains_key(&p.reservation.store.id)
        {
            let mut known = self.known.lock().map_err(|_| Error::Internal)?;
            if !known.contains_key(&n.id) && known.len() >= MAX_NODES {
                return Err(Error::ResourceLimit);
            }
            known.insert(n.id.clone(), p.reservation.store.id.clone());
        }
        Ok(())
    }
    pub fn tracks(&self, node: &str) -> bool {
        self.known.lock().ok().is_some_and(|k| k.contains_key(node))
    }
    pub fn permits(&self, node: &str) -> bool {
        self.known
            .lock()
            .ok()
            .and_then(|k| k.get(node).cloned())
            .and_then(|id| self.stores.get(&id))
            .is_some_and(|(s, e, _)| e.check_create(&format!("{}binding", s.root), 0).is_ok())
    }
    pub fn permits_directory(&self) -> bool {
        self.stores
            .get(&self.default_store)
            .is_some_and(|(s, e, _)| e.check_create(&format!("{}binding", s.root), 0).is_ok())
    }
    async fn recover(&self) -> Result<()> {
        let _serial = self.recovery_serial.lock().await;
        if let Some((_, _, creator)) = self.stores.values().next() {
            for record in creator.recovery().records().await.map_err(error)? {
                if record.receipt().is_some() || record.rejection().is_some() {
                    continue;
                }
                let i = record.input();
                if !self.stores.contains_key(&i.store.id) {
                    return Err(Error::Conflict);
                }
                let (_, _, c) = self.stores.get(&i.store.id).ok_or(Error::Conflict)?;
                if record.request().is_some() {
                    c.recovery()
                        .resume(i.operation.clone())
                        .await
                        .map_err(error)?;
                } else {
                    c.create(Draft {
                        operation: i.operation.clone(),
                        expected_sequence: i.expected_sequence,
                        parent: i.parent.clone(),
                        name: i.name.clone(),
                    })
                    .await
                    .map_err(error)?;
                }
            }
        }
        for r in self.roots.records().await.map_err(error)? {
            if r.acknowledgment().is_some() {
                continue;
            }
            let recovery = MixedRootRecovery::on_owner(
                self.roots.clone(),
                self.authority.clone(),
                self.volume.clone(),
                r.input().store().id.clone(),
            )
            .map_err(error)?;
            let r = recovery.recover(r.input().plan()).await.map_err(error)?;
            if r.receipt().is_some() {
                self.roots
                    .acknowledge_scoped(
                        token(
                            r.input().binding(),
                            derived(b"s3drive-native-chunk-ack-v1\0", &r.input().plan()),
                        )?,
                        r.input().plan(),
                        views::Options::default(),
                    )
                    .await
                    .map_err(error)?;
            }
        }
        Ok(())
    }
    async fn create_inner(&self, parent: &str, name: &str) -> Result<RemoteNode> {
        let _serial = self.open_serial.lock().await;
        self.recover().await?;
        let Reply::Info(v) = self
            .authority
            .request(&Request::Info {})
            .await
            .map_err(error)?
        else {
            return Err(Error::Conflict);
        };
        let p = self.reader.node(parent).await.map_err(error)?;
        if p.kind != s3drive_authority::Kind::Directory {
            return Err(Error::NotDirectory);
        }
        let (_, _, creator) = self
            .stores
            .get(&self.default_store)
            .ok_or(Error::ReadOnly)?;
        let receipt = creator
            .create(Draft {
                operation: hex(&fresh()?[..16]),
                expected_sequence: v.sequence,
                parent: NodeGuard {
                    id: p.id,
                    revision: p.revision,
                },
                name: name.into(),
            })
            .await
            .map_err(error)?;
        let node = receipt.metadata().node.clone();
        self.remember(&node)?;
        Ok(node)
    }
    async fn select_inner(
        &self,
        node: &str,
        writing: bool,
        legacy: s3drive_managed::Reader,
    ) -> Result<Opened> {
        let live = self.reader.node(node).await.map_err(error)?;
        self.remember(&live)?;
        if self.tracks(node) {
            if writing && !self.permits(node) {
                return Err(Error::ReadOnly);
            }
            return Box::pin(self.open_inner(node)).await.map(Opened::Sparse);
        }
        if writing {
            return Err(Error::ReadOnly);
        }
        if live
            .payload
            .as_ref()
            .is_some_and(|p| p.reservation.manifest.is_some())
        {
            return Box::pin(self.reader.open_file(node))
                .await
                .map(|file| Opened::Chunks(Box::new(file)))
                .map_err(error);
        }
        Box::pin(legacy.read(node))
            .await
            .map(|file| Opened::Legacy(Box::new(file)))
            .map_err(error)
    }
    async fn open_inner(&self, node: &str) -> Result<Arc<File>> {
        let _serial = self.open_serial.lock().await;
        self.recover().await?;
        let remote = self.reader.open_file(node).await.map_err(error)?;
        self.remember(remote.metadata())?;
        if remote
            .metadata()
            .payload
            .as_ref()
            .is_none_or(|p| p.reservation.manifest.is_none())
        {
            return Err(Error::Conflict);
        }
        let b = remote.pinned_base().sparse_binding().map_err(error)?;
        if !self.stores.contains_key(&hex(b.store())) {
            return Err(Error::ReadOnly);
        }
        let head = match self.local.current_head(b.clone()).await {
            Ok(v) => v,
            Err(e) if e.downcast_ref::<views::Failure>() == Some(&views::Failure::NotFound) => self
                .local
                .begin_scoped(
                    token(
                        &b,
                        derived(
                            b"s3drive-native-chunk-open-v1\0",
                            &b.epoch().map_err(error)?,
                        ),
                    )?,
                    b.clone(),
                    remote.size(),
                )
                .await
                .map_err(error)?,
            Err(e) => return Err(error(e)),
        };
        let head = if head.binding() != &b {
            if !head.is_clean() {
                return Err(Error::Conflict);
            }
            self.reader
                .observe_clean(
                    self.local.clone(),
                    token(head.binding(), fresh()?)?,
                    head.id(),
                )
                .await
                .map_err(error)?
                .view()
                .clone()
        } else {
            head
        };
        let held = {
            self.entries
                .lock()
                .map_err(|_| Error::Internal)?
                .get(node)
                .and_then(Weak::upgrade)
        };
        if let Some(file) = held {
            file.head().await?;
            return Ok(file);
        }
        let edited = self
            .reader
            .reopen_edited(self.local.clone(), head.id())
            .await
            .map_err(error)?;
        let mut metadata = logical_metadata(remote.metadata())?;
        metadata.size = head.size();
        let file = Arc::new(File {
            binding: head.binding().clone(),
            reader: self.reader.clone(),
            local: self.local.clone(),
            metadata: Mutex::new(metadata),
            edited: Mutex::new(Some(edited)),
            serial: AsyncMutex::new(()),
        });
        let mut entries = self.entries.lock().map_err(|_| Error::Internal)?;
        entries.retain(|_, f| f.strong_count() > 0);
        entries.insert(node.into(), Arc::downgrade(&file));
        Ok(file)
    }
    pub async fn metadata(&self, n: &RemoteNode) -> Result<Option<ObjectMetadata>> {
        self.remember(n)?;
        let heads = self.local.heads().await.map_err(error)?;
        if let Some(h) = heads
            .iter()
            .find(|h| h.binding().base().is_ok_and(|b| b.node() == n.id))
        {
            if h.is_clean()
                && s3drive_authority::payload_reference(&n.payload).as_deref()
                    != Some(h.binding().base().map_err(error)?.payload_reference())
            {
                return Ok(None);
            }
            let mut m = logical_metadata(n)?;
            m.size = h.size();
            return Ok(Some(m));
        }
        Ok(None)
    }
    async fn read_inner(&self, file: &File, offset: u64, length: usize) -> Result<Vec<u8>> {
        file.read(offset, length).await
    }
    async fn write_inner(
        &self,
        file: &File,
        offset: u64,
        input: Arc<[u8]>,
        append: bool,
        constrained: bool,
    ) -> Result<usize> {
        let _serial = file.serial.lock().await;
        let head = file.head().await?;
        let offset = if append { head.size() } else { offset };
        if constrained && offset >= head.size() {
            return Ok(0);
        }
        let count = if constrained {
            input.len().min((head.size() - offset) as usize)
        } else {
            input.len()
        };
        if count == 0 {
            return Ok(0);
        }
        if offset
            .checked_add(count as u64)
            .is_none_or(|n| n > views::MAX_FILE_BYTES)
        {
            return Err(Error::TooLarge);
        }
        let (store, e, _) = self
            .stores
            .get(&hex(head.binding().store()))
            .ok_or(Error::ReadOnly)?;
        e.check_create(&format!("{}binding", store.root), 0)
            .map_err(error)?;
        let next = self
            .local
            .write_scoped(
                token(head.binding(), fresh()?)?,
                head.id(),
                offset,
                Arc::from(&input[..count]),
            )
            .await
            .map_err(error)?;
        file.metadata.lock().map_err(|_| Error::Internal)?.size = next.size();
        Ok(count)
    }
    async fn truncate_inner(&self, file: &File, size: u64) -> Result<()> {
        let _serial = file.serial.lock().await;
        let head = file.head().await?;
        let (store, e, _) = self
            .stores
            .get(&hex(head.binding().store()))
            .ok_or(Error::ReadOnly)?;
        e.check_create(&format!("{}binding", store.root), 0)
            .map_err(error)?;
        if size == head.size() {
            return Ok(());
        }
        let next = self
            .local
            .resize_scoped(token(head.binding(), fresh()?)?, head.id(), size)
            .await
            .map_err(error)?;
        file.metadata.lock().map_err(|_| Error::Internal)?.size = next.size();
        Ok(())
    }
    async fn flush_view(&self, view: View) -> Result<()> {
        let _publishing = self.publish_serial.lock().await;
        self.recover().await?;
        if view.is_clean() {
            return Ok(());
        }
        if self
            .roots
            .records()
            .await
            .map_err(error)?
            .iter()
            .any(|r| r.input().view() == view.id() && r.acknowledgment().is_some())
        {
            return Ok(());
        }
        let head = view;
        let edited = self
            .reader
            .reopen_edited(self.local.clone(), head.id())
            .await
            .map_err(error)?;
        let id = derived(b"s3drive-native-chunk-plan-v1\0", &head.id());
        let generation = derived(b"s3drive-native-chunk-generation-v1\0", &head.id());
        let prep = edited
            .begin_preparation(
                id,
                generation[..16].try_into().map_err(|_| Error::Internal)?,
                plans::Options::default(),
            )
            .await
            .map_err(error)?;
        prep.prepare().await.map_err(error)?;
        let (_, e, _) = self
            .stores
            .get(&hex(head.binding().store()))
            .ok_or(Error::ReadOnly)?;
        prep.uploader(e.clone())
            .map_err(error)?
            .upload_tree()
            .await
            .map_err(error)?;
        let publisher = prep
            .publisher(e.clone(), roots::Limits::default())
            .map_err(error)?;
        publisher.check_semantics().await.map_err(error)?;
        publisher
            .begin(hex(&derived(
                b"s3drive-native-chunk-publish-v1\0",
                &head.id(),
            )[..16]))
            .await
            .map_err(error)?;
        publisher.resume().await.map_err(error)?;
        publisher
            .acknowledge_scoped(
                token(
                    head.binding(),
                    derived(b"s3drive-native-chunk-ack-v1\0", &publisher.plan()),
                )?,
                views::Options::default(),
            )
            .await
            .map_err(error)?;
        Ok(())
    }
    async fn flush_inner(&self, file: &File) -> Result<()> {
        let _serial = file.serial.lock().await;
        tokio::time::timeout(DEADLINE, Box::pin(self.flush_view(file.head().await?)))
            .await
            .map_err(|_| Error::Timeout)??;
        file.head().await?;
        Ok(())
    }
    async fn flush_heads(&self) -> Result<()> {
        self.recover().await?;
        for h in self.local.heads().await.map_err(error)? {
            self.flush_view(h).await?
        }
        Ok(())
    }
    async fn sync_namespace_inner(&self) -> Result<()> {
        tokio::time::timeout(DEADLINE, self.recover())
            .await
            .map_err(|_| Error::Timeout)?
    }
    async fn flush_all_inner(&self) -> Result<()> {
        tokio::time::timeout(DEADLINE, self.flush_heads())
            .await
            .map_err(|_| Error::Timeout)?
    }
    async fn stop_inner(&self) -> Result<()> {
        let _all = self
            .slots
            .clone()
            .acquire_many_owned(4)
            .await
            .map_err(|_| Error::Busy)?;
        tokio::time::timeout(DEADLINE, self.flush_heads())
            .await
            .map_err(|_| Error::Timeout)?
    }
}
