//! Mixed-root authority requests. Distinct historical commit, never file flush.
use super::*;
use s3drive_s3::uploads::Executor;
use s3drive_state::sparse_views::preparation::{self as plans, publication as roots};
use std::path::Path;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedRootFailure {
    Binding,
    BaseChanged,
    Busy,
    NotPrepared,
    Protocol,
    UnsupportedFeature,
    RetainedConflict,
}
impl fmt::Display for MixedRootFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mixed root {:?}; exact requests and dirty data retained",
            self
        )
    }
}
impl std::error::Error for MixedRootFailure {}
pub struct MixedRootCommit {
    record: roots::Record,
}
impl fmt::Debug for MixedRootCommit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HistoricalMixedRootCommit([not file flush])")
    }
}
impl MixedRootCommit {
    pub fn plan(&self) -> Id {
        self.record.input().plan()
    }
    pub fn view_id(&self) -> Id {
        self.record.input().view()
    }
    pub fn record(&self) -> &roots::Record {
        &self.record
    }
    pub fn metadata(&self) -> &s3drive_authority::Receipt {
        self.record.receipt().expect("validated mixed root commit")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct MixedSourceReceipt(roots::SourceReceipt);
impl fmt::Debug for MixedSourceReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedSourceReceipt([exact view; no reclamation])")
    }
}
impl MixedSourceReceipt {
    pub fn plan(&self) -> Id {
        self.0.plan()
    }
    pub fn view_id(&self) -> Id {
        self.0.view()
    }
    pub fn clean_view_id(&self) -> Option<Id> {
        self.0.acknowledgment().clean_view()
    }
    pub fn observed_head(&self) -> Id {
        self.0.acknowledgment().observed_head()
    }
    pub fn record(&self) -> &roots::Record {
        self.0.record()
    }
}
fn committed(record: roots::Record) -> Result<MixedRootCommit> {
    if record.receipt().is_none() || record.publication().is_none() {
        return Err(MixedRootFailure::NotPrepared.into());
    }
    Ok(MixedRootCommit { record })
}
#[derive(Clone)]
enum Log {
    Normal(roots::Client),
    Recovery(roots::RecoveryClient),
}
impl Log {
    async fn get(&self, p: Id) -> Result<roots::Record> {
        match self {
            Self::Normal(c) => c.get(p).await,
            Self::Recovery(c) => c.get(p).await,
        }
    }
    async fn acknowledged(&self, p: Id) -> Result<Option<roots::SourceReceipt>> {
        match self {
            Self::Normal(c) => c.acknowledged(p).await,
            Self::Recovery(c) => c.acknowledged(p).await,
        }
    }
    async fn reserved(&self, p: Id, r: s3drive_authority::Receipt) -> Result<roots::Record> {
        match self {
            Self::Normal(c) => c.reserved(p, r).await,
            Self::Recovery(c) => c.reserved(p, r).await,
        }
    }
    async fn published(&self, p: Id, r: s3drive_authority::Receipt) -> Result<roots::Record> {
        match self {
            Self::Normal(c) => c.published(p, r).await,
            Self::Recovery(c) => c.published(p, r).await,
        }
    }
    async fn reject(&self, p: Id, request: s3drive_authority::Apply) -> Result<roots::Record> {
        match self {
            Self::Normal(c) => {
                c.reject(p, request, s3drive_authority::Error::Conflict)
                    .await
            }
            Self::Recovery(c) => {
                c.reject(p, request, s3drive_authority::Error::Conflict)
                    .await
            }
        }
    }
}
struct Recovery {
    log: Log,
    authority: WireClient,
    volume: String,
    store: String,
    slots: Semaphore,
}
/// Replays only requests already saved on the original private journal. No S3
/// client, Preparation, fresh feature/source probes or request-preparation API.
#[derive(Clone)]
pub struct MixedRootRecovery(Arc<Recovery>);
impl MixedRootRecovery {
    fn build(log: Log, authority: WireClient, volume: String, store: String) -> Result<Self> {
        if !identifier(&volume)
            || volume == s3drive_authority::ROOT
            || !identifier(&store)
            || store == s3drive_authority::ROOT
        {
            return Err(MixedRootFailure::Binding.into());
        }
        Ok(Self(Arc::new(Recovery {
            log,
            authority,
            volume,
            store,
            slots: Semaphore::new(1),
        })))
    }
    /// Recover saved metadata on an already open SQL owner before fresh S3 access.
    /// No source credential, upload permit or new Publish preparation is granted.
    pub fn on_owner(
        client: roots::Client,
        authority: WireClient,
        volume: String,
        store: String,
    ) -> Result<Self> {
        Self::build(Log::Normal(client), authority, volume, store)
    }
    pub async fn open_existing(
        directory: &Path,
        authority: WireClient,
        volume: String,
        store: String,
    ) -> Result<Self> {
        let c = roots::RecoveryClient::open(directory).await?;
        Self::build(Log::Recovery(c), authority, volume, store)
    }
    /// Return only an already committed local acknowledgment. No source access,
    /// new acknowledgment, head update or remote request is possible here.
    pub async fn acknowledged(&self, plan: Id) -> Result<Option<MixedSourceReceipt>> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| MixedRootFailure::Busy)?;
        let r = self.0.log.get(plan).await?;
        self.scope(&r)?;
        Ok(self.0.log.acknowledged(plan).await?.map(MixedSourceReceipt))
    }
    pub async fn close(&self) -> Result<()> {
        match &self.0.log {
            Log::Recovery(c) => c.close().await,
            Log::Normal(_) => Err(MixedRootFailure::Binding.into()),
        }
    }
    fn scope(&self, r: &roots::Record) -> Result<()> {
        if r.input().reserve().volume != self.0.volume || r.input().store().id != self.0.store {
            return Err(MixedRootFailure::Binding.into());
        }
        Ok(())
    }
    /// A saved Reserve may be recovered to a reservation only. This does not
    /// invent a Publish or imply a root commit; inspect receipt() on the record.
    pub async fn recover(&self, plan: Id) -> Result<roots::Record> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| MixedRootFailure::Busy)?;
        let r = self.0.log.get(plan).await?;
        self.scope(&r)?;
        if r.rejection().is_some() {
            return Err(MixedRootFailure::RetainedConflict.into());
        }
        if r.receipt().is_some() {
            return Ok(r);
        }
        let (request, publish) = if let Some(p) = r.publication() {
            (p.clone(), true)
        } else if r.reservation().is_none() {
            (r.input().reserve().clone(), false)
        } else {
            return Ok(r);
        };
        match self
            .0
            .authority
            .request(&Request::Apply {
                request: request.clone(),
            })
            .await?
        {
            Reply::Applied(receipt) => {
                if publish {
                    self.0.log.published(plan, receipt).await
                } else {
                    self.0.log.reserved(plan, receipt).await
                }
            }
            Reply::Error(s3drive_authority::Error::Conflict) => {
                self.0.log.reject(plan, request).await?;
                Err(MixedRootFailure::RetainedConflict.into())
            }
            Reply::Error(e) => Err(ReadFailure::Authority(e).into()),
            _ => Err(MixedRootFailure::Protocol.into()),
        }
    }
}
struct Publisher {
    preparation: Preparation,
    executor: Executor,
    client: roots::Client,
    recovery: MixedRootRecovery,
    slots: Semaphore,
}
#[derive(Clone)]
pub struct MixedPublisher(Arc<Publisher>);
impl fmt::Debug for MixedPublisher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedRootPublisher([private; explicit view acknowledgment])")
    }
}
impl Preparation {
    /// Constructor performs no fresh network admission, so saved requests can be
    /// replayed before source/policy checks. New work validates them per operation.
    pub fn publisher(&self, executor: Executor, limits: roots::Limits) -> Result<MixedPublisher> {
        if !self.client.same_storage(&executor.storage()) {
            return Err(MixedRootFailure::Binding.into());
        }
        let client = self.client.publications(limits)?;
        let reader = &self.edited.base.0.reader.0;
        let store = self
            .edited
            .view
            .binding()
            .store()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let recovery = MixedRootRecovery::build(
            Log::Normal(client.clone()),
            reader.authority.clone(),
            reader.volume.clone(),
            store,
        )?;
        Ok(MixedPublisher(Arc::new(Publisher {
            preparation: self.clone(),
            executor,
            client,
            recovery,
            slots: Semaphore::new(1),
        })))
    }
}
impl MixedPublisher {
    /// Local exact-view settlement only; no S3/authority RPC or new admission.
    pub async fn acknowledge(
        &self,
        operation: Id,
        options: s3drive_state::sparse_views::Options,
    ) -> Result<MixedSourceReceipt> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| MixedRootFailure::Busy)?;
        Ok(MixedSourceReceipt(
            self.0
                .client
                .acknowledge(operation, self.plan(), options)
                .await?,
        ))
    }
    pub async fn acknowledge_scoped(
        &self,
        token: s3drive_state::sparse_views::ScopedOperation,
        options: s3drive_state::sparse_views::Options,
    ) -> Result<MixedSourceReceipt> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| MixedRootFailure::Busy)?;
        Ok(MixedSourceReceipt(
            self.0
                .client
                .acknowledge_scoped(token, self.plan(), options)
                .await?,
        ))
    }
    pub fn plan(&self) -> Id {
        self.0.preparation.id
    }
    pub fn recovery(&self) -> MixedRootRecovery {
        self.0.recovery.clone()
    }
    async fn check_one(&self, need: roots::DataNeed) -> Result<()> {
        let prep = &self.0.preparation;
        let base = &prep.edited.base;
        let _active = base
            .0
            .reader
            .0
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(
            base.0.reader.0.options,
            Box::pin(async {
                prep.info().await?;
                Box::pin(base.authorize_sparse()).await?;
                let original = if need.uses_base {
                    let offset = need
                        .slot
                        .checked_mul(manifest::CHUNK_BYTES)
                        .ok_or(ReadFailure::Bounds)?;
                    let proof = Box::pin(base.prove_chunk_inner(need.slot, offset)).await?;
                    let bytes = if proof.reference().is_some() {
                        let mut bytes = Vec::with_capacity(proof.length() as usize);
                        let mut session = Session::new(base)?;
                        Box::pin(session.append(offset, proof.length() as usize, &mut bytes))
                            .await?;
                        Some(bytes)
                    } else {
                        None
                    };
                    Some(plans::OriginalChunk {
                        witness: proof.encode(),
                        bytes,
                    })
                } else {
                    None
                };
                self.0.client.check(self.plan(), need.slot, original).await
            }),
        )
        .await
    }
    /// Bounded per-chunk steps; structural holes/subtrees are never enumerated as
    /// absent descendant chunks. Each checked candidate is retained transactionally.
    pub async fn check_semantics(&self) -> Result<()> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| MixedRootFailure::Busy)?;
        loop {
            let needs = self.0.client.unchecked(self.plan(), None, 128).await?;
            if needs.is_empty() {
                return Ok(());
            }
            for need in needs {
                self.check_one(need).await?;
            }
        }
    }
    async fn fresh(&self) -> Result<(Node, Store, u64)> {
        let prep = &self.0.preparation;
        let base = &prep.edited.base;
        let reader = &base.0.reader.0;
        let _active = reader
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| ReadFailure::Busy)?;
        bounded(
            reader.options,
            Box::pin(async {
                let info = prep.info().await?;
                let tree = prep.client.tree_receipt(self.plan()).await?;
                if tree.plan().view() != info.view()
                    || tree.root() != &info.root()?.ok_or(MixedRootFailure::NotPrepared)?
                {
                    return Err(MixedRootFailure::Binding.into());
                }
                let target = self
                    .0
                    .executor
                    .recovery_target(tree.root_receipt().info().target.object_key())?;
                if target != tree.root_receipt().info().target {
                    return Err(MixedRootFailure::Binding.into());
                }
                self.0
                    .executor
                    .check_create(target.object_key(), tree.root_receipt().info().size)?;
                Box::pin(base.authorize_sparse()).await?;
                let authority = &reader.authority;
                let Reply::Features(features) =
                    reply(authority.request(&Request::Features {}).await?)?
                else {
                    return Err(MixedRootFailure::Protocol.into());
                };
                if !features.iter().any(|x| x == "manifest-root-v1")
                    || !features.iter().any(|x| x == "content-cas-v1")
                    || (info.ownership().is_some()
                        && !features.iter().any(|x| x == "event-fenced-manifest-v1"))
                {
                    return Err(MixedRootFailure::UnsupportedFeature.into());
                }
                let binding = info.view().binding();
                let descriptor = binding.base()?;
                let Reply::Store(store) = reply(
                    authority
                        .request(&Request::Store {
                            volume: descriptor.volume().into(),
                            store: self.0.recovery.0.store.clone(),
                        })
                        .await?,
                )?
                else {
                    return Err(MixedRootFailure::Protocol.into());
                };
                if !store.valid()
                    || store.id != self.0.recovery.0.store
                    || store.root != binding.root()
                    || store.route != self.0.executor.reader().storage_fingerprint()
                {
                    return Err(MixedRootFailure::Binding.into());
                }
                let node = if let Some(owned) = info.ownership() {
                    let Reply::RemovalEpoch(Some(epoch)) = reply(
                        authority
                            .request(&Request::RemovalEpoch {
                                volume: descriptor.volume().into(),
                                node: descriptor.node().into(),
                            })
                            .await?,
                    )?
                    else {
                        return Err(MixedRootFailure::BaseChanged.into());
                    };
                    if &epoch.claim != owned.claim() {
                        return Err(MixedRootFailure::BaseChanged.into());
                    }
                    epoch.node
                } else {
                    let Reply::Node(node) = reply(
                        authority
                            .request(&Request::Node {
                                volume: descriptor.volume().into(),
                                node: descriptor.node().into(),
                            })
                            .await?,
                    )?
                    else {
                        return Err(MixedRootFailure::Protocol.into());
                    };
                    node
                };
                if node.id != descriptor.node() {
                    return Err(MixedRootFailure::Binding.into());
                }
                if let Err(e) = self.0.client.check_base(self.plan(), node.clone()).await {
                    if e.downcast_ref::<roots::Failure>() == Some(&roots::Failure::Conflict) {
                        return Err(MixedRootFailure::BaseChanged.into());
                    }
                    return Err(e);
                }
                let Reply::Info(volume) = reply(authority.request(&Request::Info {}).await?)?
                else {
                    return Err(MixedRootFailure::Protocol.into());
                };
                if volume.id != reader.volume || volume.name_policy != reader.policy {
                    return Err(MixedRootFailure::Binding.into());
                }
                self.0
                    .executor
                    .check_create(target.object_key(), tree.root_receipt().info().size)?;
                Ok((node, store, volume.sequence))
            }),
        )
        .await
    }
    /// Derive the reserved root identity from the exact admitted scoped plan.
    /// No raw-operation fallback and no retry-mode activation are performed.
    pub async fn begin_scoped(&self, token: plans::retry::Operation) -> Result<roots::Record> {
        if self.0.preparation.info().await?.retry_operation() != Some(token) {
            return Err(MixedRootFailure::Binding.into());
        }
        self.begin(token.publish_operation()?).await
    }
    /// Save the exact reserve; never sends a mutation from this method.
    pub async fn begin(&self, publish_operation: String) -> Result<roots::Record> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| MixedRootFailure::Busy)?;
        match self.0.client.get(self.plan()).await {
            Ok(r) => {
                self.0.recovery.scope(&r)?;
                if r.input().publish_operation() != publish_operation {
                    return Err(MixedRootFailure::Binding.into());
                }
                return Ok(r);
            }
            Err(e) if e.downcast_ref::<roots::Failure>() == Some(&roots::Failure::Missing) => {}
            Err(e) => return Err(e),
        }
        if !self
            .0
            .client
            .unchecked(self.plan(), None, 1)
            .await?
            .is_empty()
        {
            return Err(MixedRootFailure::NotPrepared.into());
        }
        let (node, store, sequence) = self.fresh().await?;
        self.0
            .client
            .begin(self.plan(), node, store, sequence, publish_operation)
            .await
    }
    /// Observe/replay saved requests first. Preparing a new Publish then requires
    /// fresh source/create admission; saved Publish recovery never uses those APIs.
    pub async fn resume(&self) -> Result<MixedRootCommit> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| MixedRootFailure::Busy)?;
        let r = self.0.recovery.recover(self.plan()).await?;
        if r.receipt().is_some() {
            return committed(r);
        }
        if r.publication().is_some() {
            return Err(MixedRootFailure::NotPrepared.into());
        }
        let (_, store, sequence) = self.fresh().await?;
        if &store != r.input().store() {
            return Err(MixedRootFailure::Binding.into());
        }
        self.0.client.prepare_publish(self.plan(), sequence).await?;
        committed(self.0.recovery.recover(self.plan()).await?)
    }
}
