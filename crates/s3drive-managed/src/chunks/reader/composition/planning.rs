//! Policy-free preparation only. Never an upload, authority root commit or clean
//! acknowledgment. The later publisher must revalidate source/write admission.
use super::*;
use s3drive_core::manifest::objects::RootDescriptor;
use s3drive_state::sparse_views::preparation as plans;
/// Exact caller intent. The state capability independently validates and freezes
/// the acknowledged event and original session-bound open before any staging.
#[derive(Clone, Debug)]
pub struct OwnedPreparation {
    pub open: String,
    pub claim: s3drive_authority::RemovalPublicationClaim,
}
#[derive(Clone)]
pub struct Preparation {
    pub(super) edited: EditedFile,
    pub(super) client: plans::Client,
    pub(super) id: Id,
}
impl fmt::Debug for Preparation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthenticatedMixedPreparation([private])")
    }
}
impl EditedFile {
    /// Owned pending/deferred/unlinked admission on the same private worker.
    /// Exact retries return historical identity before fresh source probes.
    pub async fn begin_owned_preparation(
        &self,
        operation: Id,
        generation: [u8; 16],
        options: plans::Options,
        chunks: &s3drive_state::chunk_mount::Client,
        ownership: OwnedPreparation,
    ) -> Result<Preparation> {
        let client = chunks.preparations(options)?;
        if !client.same_views(&self.local) {
            return Err(ReadFailure::Binding.into());
        }
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
            Box::pin(async {
                let (root, page) = self.original_context()?;
                let request = plans::Request {
                    operation,
                    view: self.view.id(),
                    generation,
                    base_root: root.encode().to_vec(),
                    base_page: page,
                };
                if client.operation(operation).await?.is_none() {
                    let view = self.local.view(self.view.id()).await?;
                    if view != *self.view {
                        return Err(ReadFailure::Binding.into());
                    }
                    Box::pin(self.base.authorize_sparse()).await?;
                }
                let info = client
                    .begin_owned(request, ownership.open, ownership.claim)
                    .await?;
                let result = Preparation {
                    edited: self.clone(),
                    client,
                    id: info.id()?,
                };
                result.scope(&info)?;
                Ok(result)
            }),
        )
        .await
    }
    fn original_context(&self) -> Result<(RootDescriptor, Vec<u8>)> {
        let Content::Manifest { root, .. } = &self.base.0.content else {
            return Err(ReadFailure::UnsupportedContent.into());
        };
        let hint = self
            .base
            .0
            .payload
            .as_ref()
            .and_then(|p| p.reservation.manifest.as_ref())
            .ok_or(ReadFailure::Binding)?;
        Ok((
            hint.root().map_err(|_| ReadFailure::Integrity)?,
            root.page().encode(),
        ))
    }
    /// Persist exact view/base/new-generation identity before preparing any
    /// content. One reader permit/deadline covers SQL and fresh authorization.
    pub async fn begin_preparation(
        &self,
        operation: Id,
        generation: [u8; 16],
        options: plans::Options,
    ) -> Result<Preparation> {
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
            Box::pin(async {
                let view = self.local.view(self.view.id()).await?;
                if view != *self.view {
                    return Err(ReadFailure::Binding.into());
                }
                Box::pin(self.base.authorize_sparse()).await?;
                let (root, page) = self.original_context()?;
                let client = self.local.preparations(options)?;
                let info = client
                    .begin(plans::Request {
                        operation,
                        view: self.view.id(),
                        generation,
                        base_root: root.encode().to_vec(),
                        base_page: page,
                    })
                    .await?;
                let result = Preparation {
                    edited: self.clone(),
                    client,
                    id: info.id()?,
                };
                result.scope(&info)?;
                Ok(result)
            }),
        )
        .await
    }
}
/// Freshly checked dependency intent, not a provider send permission or remote
/// receipt. New data/hole semantics are checked against the exact pinned view;
/// pages still require ordered child evidence in the future publisher.
pub struct PreparedDependency {
    plan: Id,
    view: Id,
    binding: Binding,
    content: plans::Dependency,
}
impl fmt::Debug for PreparedDependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CheckedMixedDependency([private; not a send permit])")
    }
}
impl PreparedDependency {
    pub fn plan(&self) -> Id {
        self.plan
    }
    pub fn view_id(&self) -> Id {
        self.view
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn content(&self) -> &plans::Dependency {
        &self.content
    }
}
impl Preparation {
    /// Explicit terminal resolution, not success or a clean-source receipt.
    /// NoEffect requires no staged object/root; Rejected requires the exact saved
    /// terminal authority Conflict. Unknown outcomes always remain unresolved.
    /// This local operation performs no credential, source or policy probe.
    pub async fn settle(&self, settlement: plans::owned::Settlement) -> Result<plans::Info> {
        let info = self.client.settle(self.id, settlement).await?;
        self.scope(&info)?;
        Ok(info)
    }
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn view(&self) -> &View {
        self.edited.view()
    }
    fn scope(&self, info: &plans::Info) -> Result<()> {
        if info.id()? != self.id
            || info.view() != self.edited.view()
            || info.original_root()? != self.edited.original_context()?.0
        {
            return Err(ReadFailure::Binding.into());
        }
        Ok(())
    }
    /// Offline metadata inspection only, not a current network authorization.
    pub async fn info(&self) -> Result<plans::Info> {
        let info = self.client.info(self.id).await?;
        self.scope(&info)?;
        Ok(info)
    }
    /// Revalidate semantic byte/provenance evidence before a future fresh send.
    /// A self-consistent offline plan alone cannot certify new base-backed bytes.
    pub async fn dependency(&self, level: i8, slot: u64) -> Result<PreparedDependency> {
        let _active = self
            .edited
            .base
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(self.edited.base.0.reader.0.options,Box::pin(async {
            let info=self.client.info(self.id).await?;self.scope(&info)?;
            let content=self.client.dependency(self.id,level,slot).await?;
            match &content {
                plans::Dependency::New{role:Role::Data,bytes,..}=> {
                    let offset=slot.checked_mul(manifest::CHUNK_BYTES).ok_or(ReadFailure::Bounds)?;
                    let expected=Box::pin(self.edited.prepare_inner(slot,offset,bytes.len() as u64)).await?;
                    if !matches_content(&expected,bytes) {return Err(ReadFailure::Integrity.into());}
                }
                plans::Dependency::Hole{offset,length}=> {
                    if matches!(self.edited.view.intent_span(*offset),Some(Range::Zero{length:n}) if n>=*length) {
                        Box::pin(self.edited.base.authorize_sparse()).await?;
                    } else {
                        if *length>manifest::CHUNK_BYTES {return Err(ReadFailure::Integrity.into());}
                        let expected=Box::pin(self.edited.prepare_inner(*offset/manifest::CHUNK_BYTES,*offset,*length)).await?;
                        if !matches_content(&expected,&vec![0;*length as usize]) {return Err(ReadFailure::Integrity.into());}
                    }
                }
                plans::Dependency::Inherited{role,reference,proof}=> {
                    Box::pin(self.edited.base.authorize_sparse()).await?;
                    let actual=if *role==Role::Data {self.edited.base.verify_chunk_proof(proof)?.reference().cloned()}
                        else {Some(self.edited.base.verify_page_proof(proof)?.reference().clone())};
                    if actual.as_ref()!=Some(reference) {return Err(ReadFailure::Integrity.into());}
                }
                plans::Dependency::New{role:Role::Page,..}=> {Box::pin(self.edited.base.authorize_sparse()).await?;}
            }
            Ok(PreparedDependency{plan:self.id,view:self.edited.view.id(),binding:self.edited.view.binding().clone(),content})
        })).await
    }
    pub async fn step(&self) -> Result<plans::Info> {
        let _active = self
            .edited
            .base
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(
            self.edited.base.0.reader.0.options,
            Box::pin(self.step_inner()),
        )
        .await
    }
    async fn step_inner(&self) -> Result<plans::Info> {
        let info = self.client.info(self.id).await?;
        self.scope(&info)?;
        if info.root()?.is_some() {
            return Ok(info);
        }
        let cursor = info.cursor();
        if cursor.level >= 0 {
            Box::pin(self.edited.base.authorize_sparse()).await?;
            return self.client.step(self.id, cursor).await;
        }
        // The SQL snapshot has ended. Pure view hints do not confer authority;
        // append independently revalidates coverage and retained bytes again.
        if let Some(Range::Zero { length }) = self.edited.view.intent_span(cursor.offset) {
            let length = if cursor.offset + length == self.edited.size() {
                length
            } else {
                length / manifest::CHUNK_BYTES * manifest::CHUNK_BYTES
            };
            if length > 0 {
                Box::pin(self.edited.base.authorize_sparse()).await?;
                return self
                    .client
                    .append(self.id, cursor, plans::Candidate::Zero { length })
                    .await;
            }
        }
        if let Some(Range::Base { offset, length }) = self.edited.view.intent_span(cursor.offset)
            && offset == cursor.offset
        {
            let Content::Manifest { root, .. } = &self.edited.base.0.content else {
                return Err(ReadFailure::UnsupportedContent.into());
            };
            let mut authorized = false;
            for level in (0..target_level(self.edited.size())).rev() {
                let span = manifest::CHUNK_BYTES * 128u64.pow(u32::from(level) + 1);
                let expected = (self.edited.size() - offset).min(span);
                if level > root.page().level() || !offset.is_multiple_of(span) || length < expected
                {
                    continue;
                }
                if !authorized {
                    Box::pin(self.edited.base.authorize_sparse()).await?;
                    authorized = true;
                }
                let proof = Box::pin(self.edited.base.prove_page_inner(offset, level)).await?;
                if proof.offset() == offset && proof.length() == expected {
                    return self
                        .client
                        .append(
                            self.id,
                            cursor,
                            plans::Candidate::InheritedPage(proof.encode()),
                        )
                        .await;
                }
            }
        }
        // This authenticated private helper validates SQL, reauthorizes exact
        // history/root and verifies whole old chunks before slicing. It shares
        // this operation's permit/deadline instead of acquiring a nested slot.
        let length = (self.edited.size() - cursor.offset).min(manifest::CHUNK_BYTES);
        let chunk = Box::pin(self.edited.prepare_inner(
            cursor.offset / manifest::CHUNK_BYTES,
            cursor.offset,
            length,
        ))
        .await?;
        self.client
            .append(self.id, cursor, chunk.into_candidate())
            .await
    }
    /// Cancellable resumable loop. Each individual step has the reader's overall
    /// deadline. Lost replies require inspecting this same plan ID, never a new
    /// operation, new base or unconditional replay of remote mutations.
    pub async fn prepare(&self) -> Result<plans::Info> {
        loop {
            let info = self.step().await?;
            if info.root()?.is_some() {
                return Ok(info);
            }
            tokio::task::yield_now().await;
        }
    }
}
fn matches_content(expected: &PreparedChunk, bytes: &[u8]) -> bool {
    if expected.length() != bytes.len() as u64 {
        return false;
    }
    match expected.content() {
        ChunkContent::Bytes { bytes: actual, .. } => actual == bytes,
        ChunkContent::Hole => bytes.iter().all(|b| *b == 0),
        ChunkContent::Inherited(proof) => match proof.reference() {
            Some(reference) => reference.sha256() == &<[u8; 32]>::from(Sha256::digest(bytes)),
            None => bytes.iter().all(|b| *b == 0),
        },
    }
}
fn target_level(size: u64) -> u8 {
    let mut chunks = size.div_ceil(manifest::CHUNK_BYTES);
    let mut level = 0;
    while chunks > 128 {
        chunks = chunks.div_ceil(128);
        level += 1;
    }
    level
}
impl Reader {
    /// Reopen the exact persisted historical view and root context. Current
    /// namespace heads and caller-supplied descriptors cannot rebase this plan.
    pub async fn reopen_preparation(
        &self,
        local: sparse_views::Client,
        id: Id,
        options: plans::Options,
    ) -> Result<Preparation> {
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
            Box::pin(async {
                let client = local.preparations(options)?;
                let info = client.info(id).await?;
                let edited =
                    Box::pin(self.reopen_edited_inner(local, info.view().id(), snapshot)).await?;
                let result = Preparation { edited, client, id };
                result.scope(&info)?;
                Ok(result)
            }),
        )
        .await
    }
}

/// Operation-local exact preparation authorization. No public constructor,
/// serialization, lifetime beyond this batch, or ability to select another view.
pub(super) struct DependencySession<'a> {
    preparation: &'a Preparation,
    _active: OwnedSemaphorePermit,
}
impl DependencySession<'_> {
    pub(super) fn edited(&self) -> &EditedFile {
        &self.preparation.edited
    }
    async fn check(
        &self,
        p: plans::Position,
        content: plans::Dependency,
    ) -> Result<PreparedDependency> {
        let edited = self.edited();
        match &content {
            plans::Dependency::New {
                role: Role::Data,
                bytes,
                ..
            } => {
                let offset = p
                    .slot
                    .checked_mul(manifest::CHUNK_BYTES)
                    .ok_or(ReadFailure::Bounds)?;
                let expected =
                    Box::pin(self.prepare_chunk(p.slot, offset, bytes.len() as u64)).await?;
                if !matches_content(&expected, bytes) {
                    return Err(ReadFailure::Integrity.into());
                }
            }
            plans::Dependency::Hole { offset, length } => {
                if !matches!(edited.view.intent_span(*offset), Some(Range::Zero { length:n }) if n >= *length)
                {
                    if *length > manifest::CHUNK_BYTES {
                        return Err(ReadFailure::Integrity.into());
                    }
                    let expected = Box::pin(self.prepare_chunk(
                        *offset / manifest::CHUNK_BYTES,
                        *offset,
                        *length,
                    ))
                    .await?;
                    if !matches_content(&expected, &vec![0; *length as usize]) {
                        return Err(ReadFailure::Integrity.into());
                    }
                }
            }
            plans::Dependency::Inherited {
                role,
                reference,
                proof,
            } => {
                let actual = if *role == Role::Data {
                    edited.base.verify_chunk_proof(proof)?.reference().cloned()
                } else {
                    Some(edited.base.verify_page_proof(proof)?.reference().clone())
                };
                if actual.as_ref() != Some(reference) {
                    return Err(ReadFailure::Integrity.into());
                }
            }
            plans::Dependency::New {
                role: Role::Page, ..
            } => {}
        }
        Ok(PreparedDependency {
            plan: self.preparation.id,
            view: edited.view.id(),
            binding: edited.view.binding().clone(),
            content,
        })
    }
}
impl Preparation {
    /// Called only inside upload_object's existing overall deadline. One family
    /// permit and two fresh base checks cover all immutable direct children.
    pub(super) async fn check_upload_dependencies(
        &self,
        p: plans::Position,
    ) -> Result<PreparedDependency> {
        let active = self
            .edited
            .base
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        let info = self.client.info(self.id).await?;
        self.scope(&info)?;
        let content = self.client.dependency(self.id, p.level, p.slot).await?;
        let children = self.client.children(self.id, p).await?;
        Box::pin(self.edited.base.authorize_sparse()).await?;
        let session = DependencySession {
            preparation: self,
            _active: active,
        };
        let checked = session.check(p, content).await?;
        for child in children {
            let content = self
                .client
                .dependency(self.id, child.level, child.slot)
                .await?;
            // Drop each child's material immediately; do not accumulate fanout
            // times chunk size in memory. All direct children still get checked.
            session.check(child, content).await?;
        }
        let info = self.client.info(self.id).await?;
        self.scope(&info)?;
        Box::pin(self.edited.base.authorize_sparse()).await?;
        Ok(checked)
    }
}
