//! One durable local head per global file; physical publication is explicit.
use super::*;
use s3drive_authority::{Apply, Mutation, Reply, Request, WireClient, payload_reference};
use s3drive_managed::{FileSnapshot, Publisher};
use s3drive_state::{
    logical,
    worker::{Snapshot, Storage},
};
use std::sync::Weak;
const MAX_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) struct Writes {
    pub storage: Storage,
    volume: String,
    default_store: String,
    authority: WireClient,
    publishers: HashMap<String, Publisher>,
    stores: Mutex<HashMap<String, String>>,
    entries: Mutex<HashMap<String, Weak<File>>>,
    open_serial: AsyncMutex<()>,
    edit_serial: AsyncMutex<()>,
    // Also serializes recovery of files whose last local handle was closed.
    publish_serial: AsyncMutex<()>,
    slots: Semaphore,
}
pub(crate) struct File {
    id: String,
    publisher: Publisher,
    // Pins the authorization and bounded original buffer until all handles close.
    original: FileSnapshot,
    serial: AsyncMutex<()>,
    view: Mutex<View>,
}
#[derive(Clone)]
struct View {
    local: Option<Snapshot>,
    metadata: ObjectMetadata,
    base: Option<String>,
}
impl File {
    pub fn metadata(&self) -> Result<ObjectMetadata> {
        Ok(self
            .view
            .lock()
            .map_err(|_| Error::Internal)?
            .metadata
            .clone())
    }
    pub async fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length > MAX_READ_BYTES {
            return Err(Error::TooLarge);
        }
        let view = self.view.lock().map_err(|_| Error::Internal)?.clone();
        match view.local {
            Some(s) => s.read(offset, length).await.map_err(error),
            None => Ok(slice(self.original.bytes(), offset, length)),
        }
    }
}
fn random_id() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Internal)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
impl Writes {
    pub fn new(
        storage: Storage,
        volume: String,
        authority: WireClient,
        publishers: HashMap<String, Publisher>,
        default_store: String,
    ) -> Self {
        Self {
            storage,
            volume,
            default_store,
            authority,
            publishers,
            stores: Mutex::new(HashMap::new()),
            entries: Mutex::new(HashMap::new()),
            open_serial: AsyncMutex::new(()),
            edit_serial: AsyncMutex::new(()),
            publish_serial: AsyncMutex::new(()),
            slots: Semaphore::new(4),
        }
    }
    pub fn remember(&self, node: &s3drive_authority::Node) -> Result<()> {
        if node.kind != s3drive_authority::Kind::Directory {
            let store = node
                .payload
                .as_ref()
                .map(|p| p.reservation.store.id.clone())
                .unwrap_or_else(|| self.default_store.clone());
            self.stores
                .lock()
                .map_err(|_| Error::Internal)?
                .insert(node.id.clone(), store);
        }
        Ok(())
    }
    pub fn permits(&self, node: &str) -> bool {
        self.stores
            .lock()
            .ok()
            .and_then(|s| s.get(node).cloned())
            .and_then(|s| self.publishers.get(&s))
            .is_some_and(|p| p.can_edit(node, 0))
    }
    pub fn metadata(&self, node: &str) -> Result<Option<ObjectMetadata>> {
        self.entries
            .lock()
            .map_err(|_| Error::Internal)?
            .get(node)
            .and_then(Weak::upgrade)
            .map(|f| f.metadata())
            .transpose()
    }
    pub async fn open(&self, snapshot: FileSnapshot, writing: bool) -> Result<Option<Arc<File>>> {
        self.remember(snapshot.metadata())?;
        let store = snapshot
            .metadata()
            .payload
            .as_ref()
            .map(|p| p.reservation.store.id.as_str())
            .unwrap_or(&self.default_store);
        let Some(publisher) = self.publishers.get(store) else {
            if writing {
                return Err(Error::ReadOnly);
            }
            return Ok(None);
        };
        if writing && !publisher.can_edit(&snapshot.metadata().id, 0) {
            return Err(Error::ReadOnly);
        }
        let _open = self.open_serial.lock().await;
        let _publication = self.publish_serial.lock().await;
        let _mutation = self.edit_serial.lock().await;
        let node = &snapshot.metadata().id;
        // A crash can follow metadata commit but precede logical acknowledgment.
        // Resolve only the exact known operation before comparing remote bases.
        for r in self.pending().await? {
            if r.input.node == *node
                && r.publication.as_ref().is_some_and(|p| {
                    snapshot
                        .metadata()
                        .payload
                        .as_ref()
                        .is_some_and(|remote| remote.reservation.id == p.id())
                })
            {
                publisher.flush_edit(&r.input.id).await.map_err(error)?;
            }
        }
        let state = publisher
            .observe_edit_file(&snapshot)
            .await
            .map_err(error)?;
        let previous = self
            .entries
            .lock()
            .map_err(|_| Error::Internal)?
            .get(node)
            .and_then(Weak::upgrade);
        if let Some(file) = previous {
            let old = file.view.lock().map_err(|_| Error::Internal)?.base.clone();
            if old == payload_reference(&snapshot.metadata().payload) {
                let _edit = file.serial.lock().await;
                let state = self.state(node).await?;
                self.refresh(&file, &state).await?;
                drop(_edit);
                return Ok(Some(file));
            }
        }
        let mut view = View {
            local: None,
            metadata: metadata(snapshot.metadata()),
            base: payload_reference(&state.base.payload),
        };
        if state.head != state.settled {
            let g = state.head.ok_or(Error::Conflict)?;
            let local = self.storage.staging().snapshot(g).await.map_err(error)?;
            view.metadata.size = local.metadata().size;
            view.local = Some(local);
        }
        let file = Arc::new(File {
            id: node.clone(),
            publisher: publisher.clone(),
            original: snapshot,
            serial: AsyncMutex::new(()),
            view: Mutex::new(view),
        });
        let mut entries = self.entries.lock().map_err(|_| Error::Internal)?;
        entries.retain(|_, w| w.strong_count() > 0);
        entries.insert(file.id.clone(), Arc::downgrade(&file));
        Ok(Some(file))
    }
    async fn state(&self, node: &str) -> Result<logical::File> {
        self.storage
            .logical()
            .file(self.volume.clone(), node.into())
            .await
            .map_err(error)
    }
    fn check_base(&self, file: &File, state: &logical::File) -> Result<()> {
        let mut view = file.view.lock().map_err(|_| Error::Internal)?;
        let reference = payload_reference(&state.base.payload);
        if view.base != reference {
            // A lost local acknowledgment may already have settled this exact
            // view. Only the matching own generation can recover in place;
            // an externally observed base never authorizes stale handle bytes.
            if state.base_observed
                || state.head != state.settled
                || view.local.as_ref().map(|s| s.metadata().id) != state.settled
                || state.settled.is_none()
            {
                return Err(Error::Conflict);
            }
            view.base = reference;
        }
        Ok(())
    }
    async fn refresh(&self, file: &File, state: &logical::File) -> Result<()> {
        self.check_base(file, state)?;
        if state.head != state.settled {
            let local = self
                .storage
                .staging()
                .snapshot(state.head.ok_or(Error::Conflict)?)
                .await
                .map_err(error)?;
            let mut view = file.view.lock().map_err(|_| Error::Internal)?;
            view.metadata.size = local.metadata().size;
            view.local = Some(local);
        }
        Ok(())
    }
    pub async fn write(
        &self,
        file: &File,
        offset: u64,
        input: Arc<[u8]>,
        append: bool,
        constrained: bool,
    ) -> Result<usize> {
        let _slot = self.slots.try_acquire().map_err(|_| Error::Busy)?;
        let _mutation = self.edit_serial.lock().await;
        let _serial = file.serial.lock().await;
        let state = self.state(&file.id).await?;
        self.refresh(file, &state).await?;
        let size = file.metadata()?.size;
        let offset = if append { size } else { offset };
        if constrained && offset >= size {
            return Ok(0);
        }
        let count = if constrained {
            input.len().min((size - offset) as usize)
        } else {
            input.len()
        };
        if count == 0 {
            return Ok(0);
        }
        let end = offset.checked_add(count as u64).ok_or(Error::TooLarge)?;
        let new_size = size.max(end);
        if new_size > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        if !file.publisher.can_edit(&file.id, new_size) {
            return Err(Error::ReadOnly);
        }
        let mut bytes = file.read(0, size as usize).await?;
        bytes.resize(new_size as usize, 0);
        bytes[offset as usize..end as usize].copy_from_slice(&input[..count]);
        self.stage(file, &state, bytes).await?;
        Ok(count)
    }
    pub async fn truncate(&self, file: &File, size: u64) -> Result<()> {
        let _slot = self.slots.try_acquire().map_err(|_| Error::Busy)?;
        let _mutation = self.edit_serial.lock().await;
        let _serial = file.serial.lock().await;
        let state = self.state(&file.id).await?;
        self.refresh(file, &state).await?;
        if size > MAX_BYTES {
            return Err(Error::TooLarge);
        }
        if !file.publisher.can_edit(&file.id, size) {
            return Err(Error::ReadOnly);
        }
        if size == file.metadata()?.size {
            return Ok(());
        }
        let mut bytes = file.read(0, file.metadata()?.size as usize).await?;
        bytes.resize(size as usize, 0);
        self.stage(file, &state, bytes).await
    }
    async fn stage(&self, file: &File, state: &logical::File, bytes: Vec<u8>) -> Result<()> {
        let input = logical::Input::from_bytes(random_id()?, &state.binding, state.head, &bytes);
        let r = file
            .publisher
            .stage_local(input, Arc::from(bytes))
            .await
            .map_err(error)?;
        let snapshot = self
            .storage
            .staging()
            .snapshot(r.generation.ok_or(Error::Conflict)?)
            .await
            .map_err(error)?;
        let mut view = file.view.lock().map_err(|_| Error::Internal)?;
        view.metadata.size = snapshot.metadata().size;
        view.local = Some(snapshot);
        Ok(())
    }
    async fn pending(&self) -> Result<Vec<logical::Record>> {
        let mut out = Vec::new();
        let mut after = None;
        loop {
            let page = self
                .storage
                .logical()
                .pending(after, 1000)
                .await
                .map_err(error)?;
            if page.is_empty() {
                break;
            }
            after = page.last().map(|r| r.input.id.clone());
            out.extend(page);
        }
        Ok(out)
    }
    async fn finish(&self, publisher: &Publisher, r: logical::Record) -> Result<()> {
        if let Some(p) = &r.publication {
            let physical = self
                .storage
                .managed()
                .lookup(p.id().into())
                .await
                .map_err(error)?;
            if physical.as_ref().is_none_or(|r| r.generation.is_none()) {
                publisher
                    .stage_edit(&r.input.id, p.reserve.clone(), p.publish_operation.clone())
                    .await
                    .map_err(error)?;
            }
        } else {
            let state = self.state(&r.input.node).await?;
            let Reply::Info(info) = self
                .authority
                .request(&Request::Info {})
                .await
                .map_err(error)?
            else {
                return Err(Error::Conflict);
            };
            if info.id != self.volume {
                return Err(Error::Conflict);
            }
            let reserve = Apply {
                volume: self.volume.clone(),
                operation: random_id()?,
                expected_sequence: info.sequence,
                mutation: Mutation::ReserveContent {
                    node: r.input.node.clone(),
                    expected_revision: state.base.revision,
                    store: publisher.store().id.clone(),
                    size: r.input.size,
                    sha256: r.input.sha256.clone(),
                    expected_payload: payload_reference(&state.base.payload),
                },
            };
            publisher
                .stage_edit(&r.input.id, reserve, random_id()?)
                .await
                .map_err(error)?;
        }
        publisher.flush_edit(&r.input.id).await.map_err(error)?;
        Ok(())
    }
    async fn flush_node(&self, node: &str) -> Result<()> {
        let state = self.state(node).await?;
        let publisher = self
            .publishers
            .get(&state.binding.store.id)
            .ok_or(Error::ReadOnly)?;
        let mut retained = self
            .pending()
            .await?
            .into_iter()
            .filter(|r| r.input.node == node && r.input.volume == self.volume)
            .collect::<Vec<_>>();
        retained.sort_by_key(|r| r.generation);
        // Recover attempted ancestors first. Never abandon unknown PUT/metadata
        // results in favor of a newly generated identity for the latest head.
        for r in &retained {
            if r.generation.is_none()
                || self
                    .storage
                    .staging()
                    .info(r.generation.ok_or(Error::Conflict)?)
                    .await
                    .map_err(error)?
                    .phase
                    != s3drive_state::journal::Phase::Sealed
            {
                return Err(Error::Conflict);
            }
            if r.publication.is_some() {
                self.finish(publisher, r.clone()).await?;
            }
        }
        let state = self.state(node).await?;
        if state.head != state.settled {
            let r = self
                .storage
                .logical()
                .for_generation(state.head.ok_or(Error::Conflict)?)
                .await
                .map_err(error)?;
            self.finish(publisher, r).await?;
        }
        Ok(())
    }
    pub async fn flush(&self, file: &File) -> Result<()> {
        let _publication = self.publish_serial.lock().await;
        let _mutation = self.edit_serial.lock().await;
        let _edit = file.serial.lock().await;
        let before = self.state(&file.id).await?;
        self.check_base(file, &before)?;
        self.flush_node(&file.id).await?;
        let state = self.state(&file.id).await?;
        // Publication may have settled an ancestor while a newer edit arrived.
        let mut view = file.view.lock().map_err(|_| Error::Internal)?;
        view.base = payload_reference(&state.base.payload);
        Ok(())
    }
    pub async fn flush_all(&self) -> Result<()> {
        let _publication = self.publish_serial.lock().await;
        let _mutation = self.edit_serial.lock().await;
        let mut nodes = HashSet::new();
        for r in self.pending().await? {
            if r.input.volume != self.volume {
                return Err(Error::Conflict);
            }
            nodes.insert(r.input.node);
        }
        for node in nodes {
            let file = {
                self.entries
                    .lock()
                    .map_err(|_| Error::Internal)?
                    .get(&node)
                    .and_then(Weak::upgrade)
            };
            let _edit = if let Some(file) = &file {
                Some(file.serial.lock().await)
            } else {
                None
            };
            self.flush_node(&node).await?;
            if let Some(file) = &file {
                let state = self.state(&node).await?;
                file.view.lock().map_err(|_| Error::Internal)?.base =
                    payload_reference(&state.base.payload);
            }
        }
        Ok(())
    }
}
