//! Managed immutable-file publication through the real S3 executor, private
//! local journal and authority HTTP API, plus a bounded reader used by native
//! managed mounts. Policy-bound native editing uses the same durable journal.
pub mod chunks;
pub mod creation;
pub mod disposition;
mod reader;
pub use reader::Reader;
use s3drive_authority::{Apply, Artifact, Kind, Node, Receipt, Reply, Request, Store, WireClient};
use s3drive_s3::uploads::Executor;
use s3drive_state::{
    Result, journal,
    managed::{Input, Record},
    worker::Storage,
};
use sha2::{Digest, Sha256};
use std::{fmt, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Debug)]
pub enum Failure {
    Busy,
    Binding,
    Incomplete,
    NotFound,
    IsDirectory,
    Integrity,
    Protocol,
    UnsupportedContent,
    RetainedConflict,
    CancelledTransition,
    Authority(s3drive_authority::Error),
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "managed publication {:?}; retained data and operation IDs require inspection",
            self
        )
    }
}
impl std::error::Error for Failure {}
struct Shared {
    executor: Executor,
    storage: Storage,
    authority: WireClient,
    volume: String,
    store: Store,
    slots: Semaphore,
    reader: Reader,
    disposition: tokio::sync::Mutex<()>,
}
#[derive(Clone)]
pub struct Publisher(Arc<Shared>);
pub struct FlushReceipt {
    generation: journal::GenerationId,
    metadata: Receipt,
}
impl FlushReceipt {
    pub fn generation(&self) -> journal::GenerationId {
        self.generation
    }
    pub fn metadata(&self) -> &Receipt {
        &self.metadata
    }
}
#[derive(Clone)]
pub struct FileSnapshot(Arc<Snapshot>);
struct Snapshot {
    metadata: Node,
    volume: String,
    authorization: Option<([u8; 32], String)>,
    bytes: Arc<[u8]>,
    _permit: OwnedSemaphorePermit,
}
impl FileSnapshot {
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
    pub fn metadata(&self) -> &Node {
        &self.0.metadata
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn reply(reply: Reply) -> Result<Reply> {
    if let Reply::Error(e) = reply {
        Err(Failure::Authority(e).into())
    } else {
        Ok(reply)
    }
}
impl Publisher {
    /// Bind one authority store to this executor's exact physical route. Local
    /// profile/alias names may differ between computers; credentials never leave.
    pub async fn open(
        executor: Executor,
        authority: WireClient,
        volume: String,
        store_id: String,
    ) -> Result<Self> {
        let Reply::Store(store) = reply(
            authority
                .request(&Request::Store {
                    volume: volume.clone(),
                    store: store_id,
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        if !store.valid() || store.route != executor.reader().storage_fingerprint() {
            return Err(Failure::Binding.into());
        }
        let storage = executor.storage();
        let reader = Reader::verified(
            authority.clone(),
            volume.clone(),
            vec![(store.clone(), executor.reader())],
        );
        Ok(Self(Arc::new(Shared {
            executor,
            storage,
            authority,
            volume,
            store,
            slots: Semaphore::new(4),
            reader,
            disposition: tokio::sync::Mutex::new(()),
        })))
    }
    /// Persist the complete immutable reservation request BEFORE network or
    /// staged bytes. Return only after all bytes are sealed and durably linked.
    /// Retry this exact input after a lost result; never change its operation ID.
    pub async fn stage(
        &self,
        reserve: Apply,
        publish_operation: String,
        bytes: Arc<[u8]>,
    ) -> Result<Record> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        self.stage_inner(reserve, publish_operation, bytes).await
    }
    async fn stage_inner(
        &self,
        reserve: Apply,
        publish_operation: String,
        bytes: Arc<[u8]>,
    ) -> Result<Record> {
        let target_reservation = Input {
            reserve,
            publish_operation,
            store: self.0.store.clone(),
            source: [0; 32],
            fingerprint: "0".repeat(64),
        };
        let expected = target_reservation.reservation()?;
        if expected.volume != self.0.volume
            || expected.size != bytes.len() as u64
            || hex(&Sha256::digest(&bytes)) != expected.sha256
        {
            return Err(Failure::Integrity.into());
        }
        self.0.executor.check_create(&expected.key, expected.size)?;
        let target = self.0.executor.target(&expected.key)?;
        let input = Input {
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
            ..target_reservation
        };
        let journal = self.0.storage.managed();
        if journal.begin(input.clone()).await?.rejection.is_some() {
            return Err(Failure::RetainedConflict.into());
        }
        let r = journal.recover_stage(input.id().into()).await?;
        if r.generation.is_some() {
            return Ok(r);
        }
        let staging = self.0.storage.staging();
        let generation = staging
            .begin_operation(
                input.stage_key()?,
                target,
                journal::Base::Missing,
                None,
                expected.size,
            )
            .await?;
        for (index, chunk) in bytes.chunks(journal::CHUNK_BYTES).enumerate() {
            staging
                .append_at(
                    generation,
                    (index * journal::CHUNK_BYTES) as u64,
                    Arc::from(chunk),
                )
                .await?;
        }
        staging.seal(generation).await?;
        journal.link(input.id().into(), generation).await
    }
    /// Bind the current authoritative file revision to this frozen local owner.
    /// Existing dirty/base bindings cannot be silently replaced after rotation
    /// or an external edit. This does not download or authorize a base read.
    pub async fn bind_edit_file(&self, node_id: &str) -> Result<s3drive_state::logical::File> {
        let node = self.0.reader.node(node_id).await?;
        let target = self
            .0
            .executor
            .recovery_target(&format!("{}binding", self.0.store.root))?;
        self.0
            .storage
            .logical()
            .bind(s3drive_state::logical::Binding {
                volume: self.0.volume.clone(),
                node,
                store: self.0.store.clone(),
                source: *target.source(),
                fingerprint: target.fingerprint().into(),
            })
            .await
    }
    /// Stable operational owner in the private local database, not a credential.
    pub async fn client_identity(&self) -> Result<String> {
        self.0.storage.namespace().client_identity().await
    }
    /// Inspect acknowledged ownership without adopting a foreign reservation.
    pub async fn owned_removal(
        &self,
        node: &str,
    ) -> Result<Option<s3drive_state::namespace_client::OwnedRemoval>> {
        let owned = self
            .0
            .storage
            .namespace()
            .owned_removal(self.0.volume.clone(), node.into())
            .await?;
        if let Some(owned) = &owned {
            self.check_namespace_input(&owned.input)?;
        }
        Ok(owned)
    }
    pub fn control(&self) -> Result<s3drive_state::namespace_client::Control> {
        let target = self
            .0
            .executor
            .recovery_target(&format!("{}binding", self.0.store.root))?;
        Ok(s3drive_state::namespace_client::Control {
            store: self.0.store.clone(),
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
        })
    }
    pub fn namespace_input(
        &self,
        request: Apply,
    ) -> Result<s3drive_state::namespace_client::Input> {
        if request.volume != self.0.volume {
            return Err(Failure::Binding.into());
        }
        let target = self
            .0
            .executor
            .recovery_target(&format!("{}binding", self.0.store.root))?;
        Ok(s3drive_state::namespace_client::Input {
            request,
            store: self.0.store.clone(),
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
            entries: Vec::new(),
            destination_control: None,
        })
    }
    pub fn namespace_change_input(
        &self,
        request: Apply,
        before: &[Node],
    ) -> Result<s3drive_state::namespace_client::Input> {
        let mut input = self.namespace_input(request)?;
        input.entries = before
            .iter()
            .map(s3drive_state::namespace_client::Entry::from_node)
            .collect::<std::result::Result<_, _>>()?;
        Ok(input)
    }
    /// Freeze both current physical controls for a same-owner replacement.
    pub fn namespace_change_input_with_destination(
        &self,
        request: Apply,
        before: &[Node],
        destination: &Publisher,
    ) -> Result<s3drive_state::namespace_client::Input> {
        let target = destination.namespace_input(request.clone())?.control();
        let mut input = self.namespace_change_input(request, before)?;
        if target != input.control() {
            input.destination_control = Some(Box::new(target));
        }
        Ok(input)
    }
    pub fn check_namespace_input(
        &self,
        input: &s3drive_state::namespace_client::Input,
    ) -> Result<()> {
        let mut expected = self.namespace_input(input.request.clone())?;
        expected.entries = input.entries.clone();
        expected.destination_control = input.destination_control.clone();
        if expected != *input {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub async fn transition_cancellation(
        &self,
        operation: &str,
    ) -> Result<Option<s3drive_state::namespace_client::TransitionCancellation>> {
        let proof = self
            .0
            .storage
            .namespace()
            .transition_cancellation(operation.into())
            .await?;
        if let Some(proof) = &proof {
            self.check_namespace_input(&proof.transition.input)?;
            self.check_namespace_input(&proof.cancellation.input)?;
        }
        Ok(proof)
    }
    /// Persist before RPC; commit local namespace outcome and all tracked file
    /// observations together. Retries keep exactly the original operation ID.
    pub async fn change_namespace(
        &self,
        input: s3drive_state::namespace_client::Input,
    ) -> Result<s3drive_state::namespace_client::Record> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        self.check_namespace_input(&input)?;
        let local = self.0.storage.namespace();
        let record = local.begin(input).await?;
        if record.outcome.is_some() {
            return Self::namespace_result(record);
        }
        if self
            .transition_cancellation(&record.input.request.operation)
            .await?
            .is_some()
        {
            return Err(Failure::CancelledTransition.into());
        }
        let outcome = match self
            .0
            .authority
            .request(&Request::Apply {
                request: record.input.request.clone(),
            })
            .await?
        {
            Reply::Applied(r) => Ok(r),
            Reply::Error(e @ s3drive_authority::Error::DeletePending) => {
                return Err(Failure::Authority(e).into());
            }
            Reply::Error(e) => Err(e),
            _ => return Err(Failure::Protocol.into()),
        };
        let saved = local
            .acknowledge(record.input.request.operation, outcome)
            .await?;
        Self::namespace_result(saved)
    }
    fn namespace_result(
        record: s3drive_state::namespace_client::Record,
    ) -> Result<s3drive_state::namespace_client::Record> {
        match record.outcome.as_ref() {
            Some(Err(e)) => Err(Failure::Authority(*e).into()),
            Some(Ok(_)) => Ok(record),
            None => Err(Failure::Incomplete.into()),
        }
    }
    pub fn check_edit_binding(&self, binding: &s3drive_state::logical::Binding) -> Result<()> {
        let target = self
            .0
            .executor
            .recovery_target(&format!("{}binding", self.0.store.root))?;
        if binding.volume != self.0.volume
            || binding.store != self.0.store
            || binding.source != *target.source()
            || binding.fingerprint != target.fingerprint()
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub fn can_edit(&self, node: &str, size: u64) -> bool {
        let key = format!(
            "{}v1/{}/{}/{}",
            self.0.store.root,
            self.0.volume,
            node,
            "1".repeat(32)
        );
        self.0.executor.check_create(&key, size).is_ok()
    }
    /// Adopt only metadata whose payload this exact reader authorized and verified.
    pub async fn observe_edit_file(
        &self,
        snapshot: &FileSnapshot,
    ) -> Result<s3drive_state::logical::File> {
        let target = self
            .0
            .executor
            .recovery_target(&format!("{}binding", self.0.store.root))?;
        if snapshot.0.volume != self.0.volume
            || snapshot
                .0
                .authorization
                .as_ref()
                .is_some_and(|(source, fp)| source != target.source() || fp != target.fingerprint())
        {
            return Err(Failure::Binding.into());
        }
        let binding = s3drive_state::logical::Binding {
            volume: self.0.volume.clone(),
            node: snapshot.metadata().clone(),
            store: self.0.store.clone(),
            source: *target.source(),
            fingerprint: target.fingerprint().into(),
        };
        self.0.storage.logical().observe(binding).await
    }
    /// Coalesce a complete local edit without reserving or sending any object.
    /// An exact durable intent key recovers the same immutable generation.
    pub async fn stage_local(
        &self,
        input: s3drive_state::logical::Input,
        bytes: Arc<[u8]>,
    ) -> Result<s3drive_state::logical::Record> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        if input.size != bytes.len() as u64
            || input.size > 8 * 1024 * 1024
            || input.sha256 != hex(&Sha256::digest(&bytes))
        {
            return Err(Failure::Integrity.into());
        }
        let local = self.0.storage.logical();
        let file = local.file(input.volume.clone(), input.node.clone()).await?;
        let target = self
            .0
            .executor
            .recovery_target(&format!("{}binding", self.0.store.root))?;
        if file.binding.volume != self.0.volume
            || file.binding.store != self.0.store
            || file.binding.source != *target.source()
            || file.binding.fingerprint != target.fingerprint()
        {
            return Err(Failure::Binding.into());
        }
        let r = local.begin(input.clone()).await?;
        let g = r.generation.ok_or(Failure::Incomplete)?;
        let staging = self.0.storage.staging();
        if staging.info(g).await?.phase == journal::Phase::Building {
            for (i, chunk) in bytes.chunks(journal::CHUNK_BYTES).enumerate() {
                staging
                    .append_at(g, (i * journal::CHUNK_BYTES) as u64, Arc::from(chunk))
                    .await?;
            }
        }
        local.seal(input.id).await
    }
    /// Freeze logical-generation/publication IDs before physical staging or
    /// any network request. The metadata file revision is never silently rebased.
    pub async fn stage_edit(
        &self,
        edit_id: &str,
        reserve: Apply,
        publish_operation: String,
    ) -> Result<Record> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        let mut input = Input {
            reserve,
            publish_operation,
            store: self.0.store.clone(),
            source: [0; 32],
            fingerprint: "0".repeat(64),
        };
        let expected = input.reservation()?;
        let target = self.0.executor.recovery_target(&expected.key)?;
        input.source = *target.source();
        input.fingerprint = target.fingerprint().into();
        let edit = self
            .0
            .storage
            .logical()
            .prepare(edit_id.into(), input.clone())
            .await?;
        let g = edit.generation.ok_or(Failure::Incomplete)?;
        let bytes = self
            .0
            .storage
            .staging()
            .snapshot(g)
            .await?
            .read(0, edit.input.size as usize)
            .await?;
        self.stage_inner(input.reserve, input.publish_operation, Arc::from(bytes))
            .await
    }
    /// Confirmed terminal evidence only. Keep all local bytes and operation IDs;
    /// an unknown network result cannot release namespace closing admission.
    pub async fn retain_edit_conflict(
        &self,
        edit_id: &str,
    ) -> Result<s3drive_state::logical::RetainedConflict> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        let local = self.0.storage.logical();
        let edit = local.get(edit_id.into()).await?;
        let file = local.file(edit.input.volume, edit.input.node).await?;
        self.check_edit_binding(&file.binding)?;
        local.retain_conflict(edit_id.into()).await
    }
    /// A managed file fsync requires both the physical/metadata receipt and
    /// atomic local logical settlement. Historical retries preserve newer heads.
    pub fn flush_edit<'a>(
        &'a self,
        edit_id: &'a str,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<s3drive_state::logical::Record>> + Send + 'a>,
    > {
        Box::pin(self.flush_edit_inner(edit_id))
    }
    async fn flush_edit_inner(&self, edit_id: &str) -> Result<s3drive_state::logical::Record> {
        let local = self.0.storage.logical();
        let edit = local.get(edit_id.into()).await?;
        let input = edit.publication.as_ref().ok_or(Failure::Incomplete)?;
        let expected = input.reservation()?;
        if expected.volume != self.0.volume
            || expected.store != self.0.store
            || self.0.executor.recovery_target(&expected.key)? != input.target()?
        {
            return Err(Failure::Binding.into());
        }
        if edit.receipt.is_none() {
            Box::pin(self.flush(input.id())).await?;
        }
        local.acknowledge(edit_id.into()).await
    }
    async fn bound(&self, id: &str) -> Result<Record> {
        if self
            .0
            .storage
            .managed()
            .get(id.into())
            .await?
            .rejection
            .is_some()
        {
            return Err(Failure::RetainedConflict.into());
        }
        let r = self.0.storage.managed().recover_stage(id.into()).await?;
        let expected = r.input.reservation()?;
        if expected.volume != self.0.volume
            || expected.store != self.0.store
            || self.0.executor.recovery_target(&expected.key)? != r.input.target()?
        {
            return Err(Failure::Binding.into());
        }
        Ok(r)
    }
    /// No upload: confirm the already journaled reservation at the authority.
    pub async fn reserve(&self, id: &str) -> Result<Record> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        self.reserve_inner(id).await
    }
    async fn reserve_inner(&self, id: &str) -> Result<Record> {
        let r = self.bound(id).await?;
        if r.generation.is_none() {
            return Err(Failure::Incomplete.into());
        }
        if r.reservation.is_some() {
            return Ok(r);
        }
        let receipt = self.apply_managed(id, &r.input.reserve).await?;
        let Some(Artifact::Reservation(reservation)) = receipt.artifact else {
            return Err(Failure::Protocol.into());
        };
        self.0
            .storage
            .managed()
            .reserved(id.into(), reservation)
            .await
    }
    /// S3 verification + committed metadata CAS + durable local acknowledgment.
    /// An uncertain PUT is only observed. Unknown metadata results are resolved
    /// by resubmitting the SAME request; conflicts never get a new CAS guard.
    /// This publication receipt does not settle a local logical edit. Use
    /// `flush_edit` for generations linked through the logical edit journal.
    pub fn flush<'a>(
        &'a self,
        id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FlushReceipt>> + Send + 'a>>
    {
        Box::pin(self.flush_inner(id))
    }
    async fn flush_inner(&self, id: &str) -> Result<FlushReceipt> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        let mut r = self.reserve_inner(id).await?;
        if r.receipt.is_none() {
            let generation = r.generation.ok_or(Failure::Incomplete)?;
            if let Err(error) = Box::pin(self.0.executor.flush(generation)).await {
                if let Some(info) = self.0.storage.uploads().for_generation(generation).await?
                    && info.phase == s3drive_state::remote::Phase::Rejected
                {
                    self.0
                        .storage
                        .managed()
                        .reject(
                            id.into(),
                            s3drive_state::managed::Rejection::Physical {
                                operation: *info.operation.as_bytes(),
                                status: info.rejection_status.ok_or(Failure::Integrity)?,
                            },
                        )
                        .await?;
                }
                return Err(error.into());
            }
            let Reply::Info(volume) = reply(self.0.authority.request(&Request::Info {}).await?)?
            else {
                return Err(Failure::Protocol.into());
            };
            if volume.id != self.0.volume {
                return Err(Failure::Binding.into());
            }
            r = self
                .0
                .storage
                .managed()
                .prepare(id.into(), volume.sequence)
                .await?;
            let request = r.publication.as_ref().ok_or(Failure::Incomplete)?;
            let receipt = self.apply_managed(id, request).await?;
            r = self
                .0
                .storage
                .managed()
                .acknowledge(id.into(), receipt)
                .await?;
        }
        Ok(FlushReceipt {
            generation: r.generation.ok_or(Failure::Incomplete)?,
            metadata: r.receipt.ok_or(Failure::Incomplete)?,
        })
    }
    async fn apply(&self, request: &Apply) -> Result<Receipt> {
        // The authority checks the COMPLETE original request before returning
        // a durable result; a bare operation lookup cannot prove that binding.
        let Reply::Applied(receipt) = reply(
            self.0
                .authority
                .request(&Request::Apply {
                    request: request.clone(),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        Ok(receipt)
    }
    async fn apply_managed(&self, id: &str, request: &Apply) -> Result<Receipt> {
        match self.apply(request).await {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                if matches!(
                    error.downcast_ref::<Failure>(),
                    Some(Failure::Authority(s3drive_authority::Error::Conflict))
                ) {
                    self.0
                        .storage
                        .managed()
                        .reject(
                            id.into(),
                            s3drive_state::managed::Rejection::Authority {
                                request: Box::new(request.clone()),
                                error: s3drive_authority::Error::Conflict,
                            },
                        )
                        .await?;
                }
                Err(error)
            }
        }
    }
    /// New payload opens authorize online at both services, verify the immutable S3
    /// revision and full hash, then return owned bytes. Existing snapshots pin
    /// those bytes independently of subsequent rename, publication or unlink.
    pub async fn read(&self, node_id: &str) -> Result<FileSnapshot> {
        self.0.reader.read(node_id).await
    }
    pub fn storage(&self) -> Storage {
        self.0.storage.clone()
    }
    pub fn store(&self) -> &Store {
        &self.0.store
    }
}
