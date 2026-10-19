//! Empty logical files backed by an immutable canonical manifest page.
//! Receipts describe historical creation, never native flush or current liveness.
use s3drive_authority::{Error, Kind, NodeGuard, Receipt, Reply, Request, Store, WireClient};
use s3drive_s3::uploads::Executor;
use s3drive_state::{
    Result, journal,
    manifest_creation::{self as local, Input, Record},
    remote,
};
use std::{fmt, path::Path, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Debug)]
pub enum Failure {
    Binding,
    Busy,
    Protocol,
    UnsupportedFeature,
    NotPrepared,
    RetainedConflict,
    Deadline,
    Authority(Error),
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "empty creation {:?}; exact operations and bytes retained",
            self
        )
    }
}
impl std::error::Error for Failure {}
#[derive(Clone)]
pub struct Draft {
    pub operation: String,
    pub expected_sequence: u64,
    pub parent: NodeGuard,
    pub name: String,
}
/// A receipt for the original operation, even if another operation renamed or
/// removed its node. No reader, editable handle or current pathname is returned.
pub struct HistoricalCreation {
    record: Record,
}
impl HistoricalCreation {
    pub fn metadata(&self) -> &Receipt {
        self.record.receipt().expect("validated creation receipt")
    }
}
impl fmt::Debug for HistoricalCreation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HistoricalCreation([private])")
    }
}
fn id(s: &str) -> bool {
    s != s3drive_authority::ROOT
        && s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn checked(r: Reply) -> Result<Reply> {
    match r {
        Reply::Error(e) => Err(Failure::Authority(e).into()),
        r => Ok(r),
    }
}
fn historical(r: Record) -> Result<HistoricalCreation> {
    if r.receipt().is_none() {
        return Err(Failure::NotPrepared.into());
    }
    Ok(HistoricalCreation { record: r })
}
#[derive(Clone)]
enum Log {
    Normal(local::Client),
    Restricted(local::RecoveryClient),
}
impl Log {
    async fn records(&self) -> Result<Vec<Record>> {
        match self {
            Self::Normal(c) => c.records().await,
            Self::Restricted(c) => c.records().await,
        }
    }
    async fn get(&self, op: String) -> Result<Record> {
        match self {
            Self::Normal(c) => c.get(op).await,
            Self::Restricted(c) => c.get(op).await,
        }
    }
    async fn ack(&self, r: &Record, receipt: Receipt) -> Result<Record> {
        let op = r.input().operation.clone();
        let request = r.request().ok_or(Failure::NotPrepared)?.clone();
        match self {
            Self::Normal(c) => c.acknowledge(op, request, receipt).await,
            Self::Restricted(c) => c.acknowledge(op, request, receipt).await,
        }
    }
    async fn reject(&self, r: &Record, e: Error) -> Result<()> {
        let op = r.input().operation.clone();
        let request = r.request().ok_or(Failure::NotPrepared)?.clone();
        match self {
            Self::Normal(c) => {
                c.reject(op, request, e).await?;
            }
            Self::Restricted(c) => {
                c.reject(op, request, e).await?;
            }
        }
        Ok(())
    }
}
struct RecoveryShared {
    log: Log,
    authority: WireClient,
    volume: String,
    store: Store,
    slots: Semaphore,
    deadline: Duration,
}
/// Only saved metadata requests. No S3 client, credentials, staging or rebasing.
#[derive(Clone)]
pub struct Recovery(Arc<RecoveryShared>);
impl Recovery {
    fn bind(
        log: Log,
        authority: WireClient,
        volume: String,
        store: Store,
        deadline: Duration,
    ) -> Result<Self> {
        if !id(&volume)
            || !store.valid()
            || deadline.is_zero()
            || deadline > Duration::from_secs(300)
        {
            return Err(Failure::Binding.into());
        }
        Ok(Self(Arc::new(RecoveryShared {
            log,
            authority,
            volume,
            store,
            slots: Semaphore::new(1),
            deadline,
        })))
    }
    pub async fn open_existing(
        directory: &Path,
        authority: WireClient,
        volume: String,
        store: Store,
        deadline: Duration,
    ) -> Result<Self> {
        // Validate binding before acquiring an owner, then replace its log.
        if !id(&volume)
            || !store.valid()
            || deadline.is_zero()
            || deadline > Duration::from_secs(300)
        {
            return Err(Failure::Binding.into());
        }
        let log = Log::Restricted(local::RecoveryClient::open(directory).await?);
        Self::bind(log, authority, volume, store, deadline)
    }
    fn scope(&self, r: &Record) -> Result<()> {
        if r.input().volume != self.0.volume || r.input().store != self.0.store {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        let rows = self.0.log.records().await?;
        for r in &rows {
            self.scope(r)?;
        }
        Ok(rows)
    }
    pub async fn resume(&self, operation: String) -> Result<HistoricalCreation> {
        let _slot = self.0.slots.try_acquire().map_err(|_| Failure::Busy)?;
        tokio::time::timeout(self.0.deadline, async {
            let r = self.0.log.get(operation).await?;
            self.resume_inner(r).await
        })
        .await
        .map_err(|_| Failure::Deadline)?
    }
    async fn resume_inner(&self, r: Record) -> Result<HistoricalCreation> {
        self.scope(&r)?;
        if r.rejection().is_some() {
            return Err(Failure::RetainedConflict.into());
        }
        if r.receipt().is_some() {
            return historical(r);
        }
        let request = r.request().ok_or(Failure::NotPrepared)?.clone();
        match self
            .0
            .authority
            .request(&Request::Apply { request })
            .await?
        {
            Reply::Applied(receipt) => historical(self.0.log.ack(&r, receipt).await?),
            Reply::Error(e) => {
                if matches!(
                    e,
                    Error::Conflict | Error::AlreadyExists | Error::NotFound | Error::NotDirectory
                ) {
                    self.0.log.reject(&r, e).await?;
                }
                Err(Failure::Authority(e).into())
            }
            _ => Err(Failure::Protocol.into()),
        }
    }
    /// Close the restricted database owner, including when clones still exist.
    /// A normal creator's owner must instead be closed through Storage.
    pub async fn close(&self) -> Result<()> {
        match &self.0.log {
            Log::Restricted(c) => c.close().await,
            Log::Normal(_) => Err(Failure::Binding.into()),
        }
    }
}
struct Shared {
    executor: Executor,
    journal: local::Client,
    recovery: Recovery,
}
#[derive(Clone)]
pub struct Creator(Arc<Shared>);
impl Creator {
    /// Local binding only; known receipts can replay without a feature RPC.
    pub fn bind(
        executor: Executor,
        authority: WireClient,
        volume: String,
        store: Store,
        limits: local::Limits,
        deadline: Duration,
    ) -> Result<Self> {
        if store.route != executor.reader().storage_fingerprint() {
            return Err(Failure::Binding.into());
        }
        let journal = executor.storage().manifest_creations(limits)?;
        let recovery = Recovery::bind(
            Log::Normal(journal.clone()),
            authority,
            volume,
            store,
            deadline,
        )?;
        Ok(Self(Arc::new(Shared {
            executor,
            journal,
            recovery,
        })))
    }
    pub fn recovery(&self) -> Recovery {
        self.0.recovery.clone()
    }
    fn input(&self, d: Draft) -> Result<Input> {
        let mut i = Input {
            volume: self.0.recovery.0.volume.clone(),
            operation: d.operation,
            expected_sequence: d.expected_sequence,
            parent: d.parent,
            name: d.name,
            store: self.0.recovery.0.store.clone(),
            source: [0; 32],
            fingerprint: "0".repeat(64),
        };
        let target = self.0.executor.recovery_target(&i.object_key()?)?;
        i.source = *target.source();
        i.fingerprint = target.fingerprint().into();
        i.body()?;
        Ok(i)
    }
    async fn fresh(&self, i: &Input) -> Result<()> {
        let authority = &self.0.recovery.0.authority;
        let Reply::Features(features) = checked(authority.request(&Request::Features {}).await?)?
        else {
            return Err(Failure::Protocol.into());
        };
        if ![
            "atomic-empty-manifest-v1",
            "manifest-root-v1",
            "manifest-history-v1",
        ]
        .iter()
        .all(|f| features.iter().any(|v| v == f))
        {
            return Err(Failure::UnsupportedFeature.into());
        }
        let Reply::Store(store) = checked(
            authority
                .request(&Request::Store {
                    volume: i.volume.clone(),
                    store: i.store.id.clone(),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        if store != i.store {
            return Err(Failure::Binding.into());
        }
        let Reply::Info(v) = checked(authority.request(&Request::Info {}).await?)? else {
            return Err(Failure::Protocol.into());
        };
        if v.id != i.volume
            || v.name_policy != s3drive_core::names::PORTABLE
            || v.sequence < i.expected_sequence
        {
            return Err(Failure::Binding.into());
        }
        let Reply::Node(parent) = checked(
            authority
                .request(&Request::Node {
                    volume: i.volume.clone(),
                    node: i.parent.id.clone(),
                })
                .await?,
        )?
        else {
            return Err(Failure::Protocol.into());
        };
        if parent.id != i.parent.id
            || parent.revision != i.parent.revision
            || parent.deleted
            || parent.kind != Kind::Directory
        {
            return Err(Failure::Authority(Error::Conflict).into());
        }
        Ok(())
    }
    /// Retry the identical draft after any unknown result. Never substitute a
    /// new operation, parent revision, filename or credential generation.
    pub async fn create(&self, draft: Draft) -> Result<HistoricalCreation> {
        let _slot = self
            .0
            .recovery
            .0
            .slots
            .try_acquire()
            .map_err(|_| Failure::Busy)?;
        tokio::time::timeout(self.0.recovery.0.deadline, self.create_inner(draft))
            .await
            .map_err(|_| Failure::Deadline)?
    }
    async fn create_inner(&self, draft: Draft) -> Result<HistoricalCreation> {
        let input = self.input(draft)?;
        let old = self
            .0
            .journal
            .records()
            .await?
            .into_iter()
            .find(|r| r.input().operation == input.operation);
        if let Some(r) = &old {
            if r.input() != &input {
                return Err(Failure::Binding.into());
            }
            if r.rejection().is_some() {
                return Err(Failure::RetainedConflict.into());
            }
            if r.receipt().is_some() || r.request().is_some() {
                return self.0.recovery.resume_inner(r.clone()).await;
            }
        }
        let claimed = if let Some(g) = old.as_ref().and_then(Record::generation) {
            self.0
                .executor
                .storage()
                .uploads()
                .for_generation(g)
                .await?
                .is_some_and(|p| p.phase != remote::Phase::Prepared)
        } else {
            false
        };
        if !claimed {
            self.fresh(&input).await?;
            self.0
                .executor
                .check_create(&input.object_key()?, input.body()?.len() as u64)?;
        }
        let r = self.0.journal.begin(input.clone()).await?;
        let g = if let Some(g) = r.generation() {
            g
        } else {
            let staging = self.0.executor.storage().staging();
            let body = input.body()?;
            let g = staging
                .begin_operation(
                    input.stage_key()?,
                    input.target()?,
                    journal::Base::Missing,
                    None,
                    body.len() as u64,
                )
                .await?;
            if staging.info(g).await?.phase == journal::Phase::Building {
                staging.append_at(g, 0, Arc::from(body)).await?;
            }
            staging.seal(g).await?;
            self.0.journal.link(input.operation.clone(), g).await?;
            g
        };
        self.0.executor.flush(g).await?;
        // Observation of a previously claimed upload precedes fresh probes.
        // Preparing a new metadata request still requires current authority support.
        if claimed {
            self.fresh(&input).await?;
        }
        let r = self.0.journal.prepare_request(input.operation).await?;
        self.0.recovery.resume_inner(r).await
    }
}
