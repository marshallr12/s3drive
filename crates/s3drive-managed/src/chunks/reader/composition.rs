//! Authenticated sparse reads, preparation and explicit mixed-root publication.
//! Exact local acknowledgments are explicit; no reclamation or native activation.
use super::*;
use s3drive_core::manifest::{Reference, base::Descriptor};
use s3drive_state::sparse_views::{self, Binding, Id, Piece, Range, View};
mod publication;
pub use publication::{
    MixedPublisher, MixedRootCommit, MixedRootFailure, MixedRootRecovery, MixedSourceReceipt,
};
mod recovery;
pub use recovery::{MixedRecovery, MixedRecoveryFailure};
mod uploading;
pub use uploading::MixedUploader;
mod planning;
pub use planning::{Preparation, PreparedDependency};
mod preparation;
pub use preparation::{ChunkContent, PreparedChunk};

impl BaseFile {
    /// Actual pinned source and verified manifest metadata, not caller intent.
    /// This is not a fresh authorization or a remote publication receipt.
    pub fn sparse_binding(&self) -> Result<Binding> {
        let Content::Manifest { handle, store, .. } = &self.0.content else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        let payload = self.0.payload.as_ref().ok_or(ReadFailure::Binding)?;
        let descriptor = Descriptor::decode(&self.base_descriptor()?.encode())
            .map_err(|_| ReadFailure::Binding)?;
        Binding::new(
            descriptor,
            *store,
            *handle.cache_identity().as_bytes(),
            handle.configuration_fingerprint().to_owned(),
            payload.reservation.store.root.clone(),
            self.size(),
        )
    }
    async fn authorize_sparse(&self) -> Result<()> {
        let descriptor = self.base_descriptor()?;
        let Reply::ManifestPayload(payload) = reply(
            self.0
                .reader
                .0
                .authority
                .request(&Request::ManifestPayload {
                    volume: descriptor.volume().to_owned(),
                    node: descriptor.node().to_owned(),
                    reservation: descriptor.reservation().to_owned(),
                    reference: descriptor.payload_reference().to_owned(),
                })
                .await?,
        )?
        else {
            return Err(ReadFailure::Protocol.into());
        };
        if Some(&payload) != self.0.payload.as_ref() {
            return Err(ReadFailure::Binding.into());
        }
        let Content::Manifest { handle, .. } = &self.0.content else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        // Use the pinned credential generation; do not rotate/rebind this handle.
        let fresh = Box::pin(handle.open_related(&payload.reservation.key)).await?;
        let a = fresh.metadata();
        let b = handle.metadata();
        if a.size != b.size
            || a.revision != b.revision
            || fresh.cache_identity() != handle.cache_identity()
            || fresh.configuration_fingerprint() != handle.configuration_fingerprint()
        {
            return Err(ReadFailure::Integrity.into());
        }
        Ok(())
    }
}
/// Pinned local version + authenticated historical manifest. Clones share the
/// existing reader snapshot permit. No latest-head substitution or byte setter.
#[derive(Clone)]
pub struct EditedFile {
    base: BaseFile,
    local: sparse_views::Client,
    view: Arc<View>,
}
impl fmt::Debug for EditedFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EditedManifestFile([private])")
    }
}
impl Reader {
    pub async fn reopen_edited(&self, local: sparse_views::Client, id: Id) -> Result<EditedFile> {
        if !self.0.history {
            return Err(ReadFailure::UnsupportedContent.into());
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
        bounded(
            self.0.options,
            Box::pin(self.reopen_edited_inner(local, id, snapshot)),
        )
        .await
    }
    async fn reopen_edited_inner(
        &self,
        local: sparse_views::Client,
        id: Id,
        snapshot: OwnedSemaphorePermit,
    ) -> Result<EditedFile> {
        let view = local.view(id).await?;
        let descriptor = BaseDescriptor::decode(&view.binding().base()?.encode())?;
        if descriptor.volume() != self.0.volume {
            return Err(ReadFailure::Binding.into());
        }
        let base = Box::pin(self.reopen_inner(&descriptor, snapshot)).await?;
        if base.sparse_binding()? != *view.binding() {
            return Err(ReadFailure::Binding.into());
        }
        Ok(EditedFile {
            base,
            local,
            view: Arc::new(view),
        })
    }
}
impl EditedFile {
    pub fn view(&self) -> &View {
        &self.view
    }
    pub fn size(&self) -> u64 {
        self.view.size()
    }
    /// Entire verified output or an error. Nonempty local/zero reads still need
    /// fresh history authorization and a pinned-root provider HEAD. EOF/zero
    /// reads remain network-free, as in the ordinary reader.
    pub async fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length > MAX_READ_BYTES {
            return Err(ReadFailure::Bounds.into());
        }
        if length == 0 || offset >= self.size() {
            return Ok(Vec::new());
        }
        let length = (length as u64).min(self.size() - offset) as usize;
        let _active = self
            .base
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(
            self.base.0.reader.0.options,
            Box::pin(self.read_inner(offset, length)),
        )
        .await
    }
    async fn read_inner(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        // Owner releases its transaction before this future receives pieces.
        let pieces = self.local.read(self.view.id(), offset, length).await?;
        Box::pin(self.base.authorize_sparse()).await?;
        self.compose(pieces, length).await
    }
    async fn compose(&self, pieces: Vec<Piece>, length: usize) -> Result<Vec<u8>> {
        let mut output = Vec::with_capacity(length);
        let mut session = Session::new(&self.base)?;
        for piece in pieces {
            let count = match &piece {
                Piece::Base { length, .. } | Piece::Zero { length } => *length,
                Piece::Local(bytes) => bytes.len() as u64,
            };
            if count == 0 || count > length.saturating_sub(output.len()) as u64 {
                return Err(ReadFailure::Integrity.into());
            }
            match piece {
                Piece::Base { offset, length } => {
                    Box::pin(session.append(offset, length as usize, &mut output)).await?
                }
                Piece::Zero { length } => output.resize(output.len() + length as usize, 0),
                Piece::Local(bytes) => output.extend_from_slice(&bytes),
            }
        }
        if output.len() != length {
            return Err(ReadFailure::Integrity.into());
        }
        Ok(output)
    }
}
struct Session<'a> {
    base: &'a BaseFile,
    pages: HashMap<String, (Reference, Vec<u8>)>,
    data: HashMap<String, (Reference, Vec<u8>)>,
    budget: Budget,
}
impl<'a> Session<'a> {
    fn new(base: &'a BaseFile) -> Result<Self> {
        let mut budget = Budget::default();
        budget.visit()?;
        Ok(Self {
            base,
            pages: HashMap::new(),
            data: HashMap::new(),
            budget,
        })
    }
    async fn append(&mut self, mut offset: u64, length: usize, output: &mut Vec<u8>) -> Result<()> {
        let end = offset
            .checked_add(length as u64)
            .filter(|end| *end <= self.base.size())
            .ok_or(ReadFailure::Integrity)?;
        let Content::Manifest {
            root,
            handle,
            namespace,
            store,
        } = &self.base.0.content
        else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        while offset < end {
            let mut page = root.clone();
            let mut leaf = None;
            for _ in 0..=MAX_ROOT_LEVEL {
                let located = page.page().lookup(offset).ok_or(ReadFailure::Integrity)?;
                let index = page
                    .page()
                    .entries()
                    .iter()
                    .position(|e| std::ptr::eq(e, located.entry))
                    .ok_or(ReadFailure::Integrity)?;
                if let Entry::Child { object, .. } = located.entry {
                    let key = namespace.key(Role::Page, object)?;
                    let key = key.as_str().to_owned();
                    if !self.pages.contains_key(&key) {
                        self.budget.visit()?;
                        let child = Box::pin(handle.open_related(&key)).await?;
                        let size = child.metadata().size;
                        if !(64..=manifest::MAX_PAGE_BYTES as u64).contains(&size) {
                            return Err(ReadFailure::Integrity.into());
                        }
                        let bytes = Box::pin(child.read_range(0, size as usize)).await?;
                        self.pages.insert(key.clone(), (object.clone(), bytes));
                    }
                    let (reference, bytes) = self.pages.get(&key).ok_or(ReadFailure::Integrity)?;
                    if reference != object {
                        return Err(ReadFailure::Integrity.into());
                    }
                    // Recheck the parent's range/level relationship even on a
                    // cache hit. Key equality alone does not bind a digest.
                    page = page.verify_child(index, bytes)?;
                    page_scope(&page, store)?;
                } else {
                    leaf = Some((index, located.offset_in_entry));
                    break;
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
                    let key = namespace.key(Role::Data, object)?;
                    let key = key.as_str().to_owned();
                    if !self.data.contains_key(&key) {
                        self.budget.data(*length)?;
                        let data = Box::pin(handle.open_related(&key)).await?;
                        if data.metadata().size != *length {
                            return Err(ReadFailure::Integrity.into());
                        }
                        let bytes = Box::pin(data.read_range(0, *length as usize)).await?;
                        page.verify_data(index, &bytes)?;
                        self.data.insert(key.clone(), (object.clone(), bytes));
                    }
                    let (reference, bytes) = self.data.get(&key).ok_or(ReadFailure::Integrity)?;
                    if reference != object || bytes.len() as u64 != *length {
                        return Err(ReadFailure::Integrity.into());
                    }
                    let start = in_entry as usize;
                    output.extend_from_slice(
                        bytes
                            .get(start..start + take)
                            .ok_or(ReadFailure::Integrity)?,
                    );
                }
                Entry::Child { .. } => return Err(ReadFailure::Integrity.into()),
            }
            offset += take as u64;
        }
        Ok(())
    }
}
