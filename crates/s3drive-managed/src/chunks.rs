//! Isolated immutable-tree publication. Root commits are not logical file
//! flushes. Explicit source acknowledgment is historical metadata only.
//! No native activation, detached/removal mode or payload reclamation.
mod reader;
pub use reader::{
    BaseDescriptor, BaseFile, ChunkContent, ChunkProof, EditedFile, File, MAX_CHUNK_PROOF_BYTES,
    MAX_PAGE_PROOF_BYTES, MixedPublisher, MixedRecovery, MixedRecoveryFailure, MixedRootCommit,
    MixedRootFailure, MixedRootRecovery, MixedUploader, PageProof, Preparation, PreparedChunk,
    PreparedDependency, ReadFailure, ReadOptions, Reader,
};

use s3drive_authority::{Apply, Error, Reply, Request, Store, WireClient};
use s3drive_s3::uploads::{Executor, chunks::Uploader};
use s3drive_state::{
    Result,
    chunk_publication::{self, Record},
    chunked,
    journal::{GenerationId, Target},
    worker::Storage,
};
use std::{fmt, sync::Arc};
use tokio::sync::Semaphore;

pub use s3drive_state::chunk_publication::sources::SourceReceipt;

#[derive(Debug)]
pub enum Failure {
    Busy,
    Binding,
    Protocol,
    UnsupportedFeature,
    NotPrepared,
    RetainedConflict,
    Authority(Error),
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "chunk root publication {:?}; data and exact requests retained",
            self
        )
    }
}
impl std::error::Error for Failure {}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn id(s: &str) -> bool {
    s != s3drive_authority::ROOT
        && s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn checked(reply: Reply) -> Result<Reply> {
    if let Reply::Error(e) = reply {
        Err(Failure::Authority(e).into())
    } else {
        Ok(reply)
    }
}
/// Durable historical authority root commit. The original source snapshot
/// remains pending; this must never be used as a mounted-file flush receipt.
pub struct RootCommit {
    record: Record,
}
impl fmt::Debug for RootCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HistoricalRootCommit([private])")
    }
}
impl RootCommit {
    pub fn plan(&self) -> [u8; 32] {
        self.record.input().plan().id
    }
    pub fn source_snapshot(&self) -> GenerationId {
        self.record.input().snapshot()
    }
    pub fn root_generation(&self) -> GenerationId {
        self.record
            .root_generation()
            .expect("validated root commit")
    }
    pub fn metadata(&self) -> &s3drive_authority::Receipt {
        self.record.receipt().expect("validated root commit")
    }
}
fn committed(record: Record) -> Result<RootCommit> {
    if record.receipt().is_none()
        || record.publication().is_none()
        || record.root_generation().is_none()
    {
        return Err(Failure::NotPrepared.into());
    }
    Ok(RootCommit { record })
}
struct RecoveryShared {
    storage: Storage,
    authority: WireClient,
    volume: String,
    store: String,
    slots: Semaphore,
}
/// Metadata-only recovery of an already-prepared CAS. It cannot reserve,
/// upload, invent IDs or prepare a new publication, and needs no S3 credentials.
#[derive(Clone)]
pub struct Recovery(Arc<RecoveryShared>);
impl Recovery {
    pub async fn open(
        storage: Storage,
        authority: WireClient,
        volume: String,
        store: String,
    ) -> Result<Self> {
        if !id(&volume) || !id(&store) {
            return Err(Failure::Binding.into());
        }
        // Local validation only, including permanent-role/lifecycle exclusion.
        // Historical replay must not depend on fresh Features/Info/Node/Store.
        storage.chunk_publications().records().await?;
        Ok(Self(Arc::new(RecoveryShared {
            storage,
            authority,
            volume,
            store,
            slots: Semaphore::new(1),
        })))
    }
    fn scope(&self, r: &Record) -> Result<()> {
        if r.input().reserve().volume != self.0.volume || r.input().store().id != self.0.store {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub fn storage(&self) -> Storage {
        self.0.storage.clone()
    }
    pub async fn resume(&self, plan: [u8; 32]) -> Result<RootCommit> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        let r = self.0.storage.chunk_publications().get(plan).await?;
        self.resume_inner(r).await
    }
    /// Recover the exact prepared root, then acknowledge its frozen source on
    /// the same SQLite owner. Known roots need no RPC; unknown roots replay only
    /// the saved Apply. No S3 client, credentials or current write policy is used.
    /// Failure retains the root/request and bytes for a later exact recovery.
    /// This does not settle ancestors/newer edits or prove current payload health.
    pub async fn resume_source(&self, plan: [u8; 32]) -> Result<SourceReceipt> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        let r = self.0.storage.chunk_publications().get(plan).await?;
        self.resume_inner(r).await?;
        self.0
            .storage
            .chunk_publications()
            .sources()
            .settle_source(plan)
            .await
    }
    async fn resume_inner(&self, r: Record) -> Result<RootCommit> {
        self.scope(&r)?;
        if r.rejection().is_some() {
            return Err(Failure::RetainedConflict.into());
        }
        if r.receipt().is_some() {
            return committed(r);
        }
        let request = r.publication().ok_or(Failure::NotPrepared)?;
        let receipt = self.apply(r.input().plan().id, request).await?;
        let r = self
            .0
            .storage
            .chunk_publications()
            .published(r.input().plan().id, receipt)
            .await?;
        committed(r)
    }
    async fn apply(&self, plan: [u8; 32], request: &Apply) -> Result<s3drive_authority::Receipt> {
        // The complete frozen request proves ownership; never bare Operation,
        // automatic retry, current-sequence rebase or a replacement UUID.
        match self
            .0
            .authority
            .request(&Request::Apply {
                request: request.clone(),
            })
            .await?
        {
            Reply::Applied(receipt) => Ok(receipt),
            Reply::Error(Error::Conflict) => {
                self.0
                    .storage
                    .chunk_publications()
                    .reject(plan, request.clone(), Error::Conflict)
                    .await?;
                Err(Failure::Authority(Error::Conflict).into())
            }
            Reply::Error(error) => Err(Failure::Authority(error).into()),
            _ => Err(Failure::Protocol.into()),
        }
    }
}
struct Shared {
    recovery: Recovery,
    executor: Executor,
    planner: chunked::Planner,
    uploader: Uploader,
    store: Store,
}
/// Policy-bound controller for new tree work. All journal/planner/uploader
/// handles come from its Executor's exact private SQLite owner.
#[derive(Clone)]
pub struct Publisher(Arc<Shared>);
impl Publisher {
    pub async fn open(
        executor: Executor,
        authority: WireClient,
        volume: String,
        store: String,
        options: chunked::Options,
    ) -> Result<Self> {
        let storage = executor.storage();
        let recovery = Recovery::open(
            storage.clone(),
            authority.clone(),
            volume.clone(),
            store.clone(),
        )
        .await?;
        let Reply::Features(features) = checked(authority.request(&Request::Features {}).await?)?
        else {
            return Err(Failure::Protocol.into());
        };
        if !features.iter().any(|f| f == "manifest-root-v1")
            || !features.iter().any(|f| f == "content-cas-v1")
        {
            return Err(Failure::UnsupportedFeature.into());
        }
        Self::volume(&recovery).await?;
        let Reply::Store(s) = checked(
            authority
                .request(&Request::Store {
                    volume,
                    store: store.clone(),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        if s.id != store || !s.valid() || s.route != executor.reader().storage_fingerprint() {
            return Err(Failure::Binding.into());
        }
        Ok(Self(Arc::new(Shared {
            recovery,
            planner: storage.chunked(options)?,
            uploader: Uploader::new(executor.clone(), options)?,
            executor,
            store: s,
        })))
    }
    async fn volume(recovery: &Recovery) -> Result<s3drive_authority::Volume> {
        let Reply::Info(v) = checked(recovery.0.authority.request(&Request::Info {}).await?)?
        else {
            return Err(Failure::Protocol.into());
        };
        if v.id != recovery.0.volume || !s3drive_core::names::supported(&v.name_policy) {
            return Err(Failure::Binding.into());
        }
        Ok(v)
    }
    fn binding(&self, r: &Record) -> Result<()> {
        self.0.recovery.scope(r)?;
        if r.input().store() != &self.0.store {
            return Err(Failure::Binding.into());
        }
        let reservation = r.input().reservation()?;
        let target = self.0.executor.recovery_target(&reservation.key)?;
        let plan = r.input().plan();
        if target != Target::new(plan.source, &plan.fingerprint, &reservation.key)? {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub fn storage(&self) -> Storage {
        self.0.recovery.storage()
    }
    pub fn store(&self) -> &Store {
        &self.0.store
    }
    /// Persist the exact reserve before any mutation request. Repetition uses
    /// the saved input; current metadata may never rebase an unknown operation.
    pub async fn begin(&self, plan: [u8; 32], publish_operation: String) -> Result<Record> {
        let _slot = self
            .0
            .recovery
            .0
            .slots
            .try_acquire()
            .map_err(|_| Failure::Busy)?;
        let journal = self.storage().chunk_publications();
        match journal.get(plan).await {
            Ok(r) => {
                self.0.recovery.scope(&r)?;
                if r.input().publish_operation() != publish_operation
                    || r.input().store() != &self.0.store
                {
                    return Err(Failure::Binding.into());
                }
                if r.publication().is_none() {
                    self.binding(&r)?;
                }
                return Ok(r);
            }
            Err(e)
                if e.downcast_ref::<chunk_publication::Failure>()
                    == Some(&chunk_publication::Failure::Missing) => {}
            Err(e) => return Err(e),
        }
        let p = self.0.planner.info(plan).await?;
        let input = p.input();
        if hex(&input.volume) != self.0.recovery.0.volume
            || hex(&input.store) != self.0.store.id
            || input.root != self.0.store.root
            || p.root().is_none()
        {
            return Err(Failure::Binding.into());
        }
        let t = self
            .0
            .planner
            .transfer(plan, p.root_level(), 0)
            .await?
            .ok_or(Failure::Binding)?;
        if self.0.executor.recovery_target(t.target().object_key())? != *t.target() {
            return Err(Failure::Binding.into());
        }
        self.0
            .executor
            .check_create(t.target().object_key(), t.size())?;
        // This deliberately bypasses the legacy Reader's manifest refusal,
        // retaining the strict new journal's own attached-base validation.
        let Reply::Node(node) = checked(
            self.0
                .recovery
                .0
                .authority
                .request(&Request::Node {
                    volume: self.0.recovery.0.volume.clone(),
                    node: hex(&input.node),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        if node.id != hex(&input.node) {
            return Err(Failure::Binding.into());
        }
        let v = Self::volume(&self.0.recovery).await?;
        journal
            .begin(
                plan,
                node,
                self.0.store.clone(),
                v.sequence,
                publish_operation,
            )
            .await
    }
    pub async fn resume(&self, plan: [u8; 32]) -> Result<RootCommit> {
        let _slot = self
            .0
            .recovery
            .0
            .slots
            .try_acquire()
            .map_err(|_| Failure::Busy)?;
        self.resume_inner(plan).await
    }
    /// Publish/recover the exact root and acknowledge just its frozen source.
    /// One family permit covers both steps. A local acknowledgment failure may
    /// leave a durable root commit; retry/recovery must use this same plan ID.
    /// No whole-file clean state, payload deletion or fresh media proof follows.
    pub async fn resume_source(&self, plan: [u8; 32]) -> Result<SourceReceipt> {
        let _slot = self
            .0
            .recovery
            .0
            .slots
            .try_acquire()
            .map_err(|_| Failure::Busy)?;
        self.resume_inner(plan).await?;
        self.storage()
            .chunk_publications()
            .sources()
            .settle_source(plan)
            .await
    }
    // Caller holds the family permit through any subsequent source settlement.
    async fn resume_inner(&self, plan: [u8; 32]) -> Result<RootCommit> {
        let journal = self.storage().chunk_publications();
        let mut r = journal.get(plan).await?;
        self.0.recovery.scope(&r)?;
        // Must precede every S3 binding/admission/upload or fresh metadata read.
        if r.publication().is_some() || r.receipt().is_some() || r.rejection().is_some() {
            return self.0.recovery.resume_inner(r).await;
        }
        self.binding(&r)?;
        if r.reservation().is_none() {
            let receipt = self.0.recovery.apply(plan, r.input().reserve()).await?;
            r = journal.reserved(plan, receipt).await?;
        }
        let tree = self.0.uploader.upload_tree(plan).await?;
        if tree.tree_receipt().plan().input() != r.input().plan()
            || tree.tree_receipt().snapshot() != r.input().snapshot()
            || tree.tree_receipt().snapshot_manifest() != r.input().snapshot_manifest()
        {
            return Err(Failure::Binding.into());
        }
        let v = Self::volume(&self.0.recovery).await?;
        let r = journal.prepare_publish(plan, v.sequence).await?;
        self.0.recovery.resume_inner(r).await
    }
}
