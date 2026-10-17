//! Managed immutable-file publication through the real S3 executor, private
//! local journal and authority HTTP API, plus a bounded reader used by native
//! managed read-only mounts. Native managed writes remain a later phase.
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
        journal.begin(input.clone()).await?;
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
    async fn bound(&self, id: &str) -> Result<Record> {
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
        let receipt = self.apply(&r.input.reserve).await?;
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
    pub async fn flush(&self, id: &str) -> Result<FlushReceipt> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        let mut r = self.reserve_inner(id).await?;
        if r.receipt.is_none() {
            let generation = r.generation.ok_or(Failure::Incomplete)?;
            Box::pin(self.0.executor.flush(generation)).await?;
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
            let receipt = self.apply(request).await?;
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
