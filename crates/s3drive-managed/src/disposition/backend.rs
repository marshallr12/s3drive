//! Role-correct shared namespace policy. Never construct a whole-file Publisher
//! against a chunk owner, and never let a content hook mint namespace authority.
use super::*;
use s3drive_state::{chunk_mount, namespace_client as names, worker};

/// Chunk content barriers supplied by the owning native adapter. Implementations
/// must retain original-open capabilities, not rebind them from historical IDs.
pub trait NativeContent: Send + Sync {
    fn flush<'a>(&'a self, node: &'a str) -> Task<'a, ()>;
    fn complete<'a>(&'a self, open: &'a str, callback: &'a str, completion: String)
    -> Task<'a, ()>;
}
#[derive(Clone)]
pub(super) enum Backend {
    Whole(Publisher),
    Native(Arc<NativeBackend>),
}
pub(super) struct NativeBackend {
    chunks: chunk_mount::Client,
    executor: Executor,
    authority: WireClient,
    store: Store,
    serial: Arc<tokio::sync::Mutex<()>>,
    slots: Semaphore,
    pub content: Arc<dyn NativeContent>,
}
impl Backend {
    pub async fn native(
        chunks: chunk_mount::Client,
        executor: Executor,
        authority: WireClient,
        store: Store,
        content: Arc<dyn NativeContent>,
        serial: Arc<tokio::sync::Mutex<()>>,
    ) -> Result<Self> {
        if !chunks.same_storage(&executor.storage()) {
            return Err(Failure::Binding.into());
        }
        chunks.require_store(store.clone()).await?;
        Ok(Self::Native(Arc::new(NativeBackend {
            chunks,
            executor,
            authority,
            store,
            content,
            serial,
            slots: Semaphore::new(4),
        })))
    }
    pub fn volume(&self) -> String {
        match self {
            Self::Whole(p) => p.0.volume.clone(),
            Self::Native(n) => n.chunks.key().volume().into(),
        }
    }
    pub fn store(&self) -> Store {
        match self {
            Self::Whole(p) => p.0.store.clone(),
            Self::Native(n) => n.store.clone(),
        }
    }
    pub fn executor(&self) -> &Executor {
        match self {
            Self::Whole(p) => &p.0.executor,
            Self::Native(n) => &n.executor,
        }
    }
    pub fn authority(&self) -> &WireClient {
        match self {
            Self::Whole(p) => &p.0.authority,
            Self::Native(n) => &n.authority,
        }
    }
    pub fn serial(&self) -> &tokio::sync::Mutex<()> {
        match self {
            Self::Whole(p) => &p.0.disposition,
            Self::Native(n) => &n.serial,
        }
    }
    pub fn opens(&self) -> worker::Opens {
        match self {
            Self::Whole(p) => p.storage().opens(),
            Self::Native(n) => n.chunks.opens(),
        }
    }
    pub fn namespace(&self) -> worker::Namespace {
        match self {
            Self::Whole(p) => p.storage().namespace(),
            Self::Native(n) => n.chunks.namespace(),
        }
    }
    pub fn native_record(&self, record: &OpenRecord) -> bool {
        matches!(self, Self::Native(_))
            && record
                .events
                .iter()
                .any(|e| matches!(e.action, Action::BindSparse))
    }
    pub fn namespace_change_input(
        &self,
        request: Apply,
        before: &[Node],
    ) -> Result<NamespaceInput> {
        match self {
            Self::Whole(p) => p.namespace_change_input(request, before),
            Self::Native(_) => {
                if request.volume != self.volume() {
                    return Err(Failure::Binding.into());
                }
                let store = self.store();
                let target = self
                    .executor()
                    .recovery_target(&format!("{}binding", store.root))?;
                Ok(NamespaceInput {
                    request,
                    store,
                    source: *target.source(),
                    fingerprint: target.fingerprint().into(),
                    entries: before
                        .iter()
                        .map(names::Entry::from_node)
                        .collect::<std::result::Result<_, _>>()?,
                    destination_control: None,
                })
            }
        }
    }
    pub fn check_namespace_input(&self, input: &NamespaceInput) -> Result<()> {
        let mut expected = self.namespace_change_input(input.request.clone(), &[])?;
        expected.entries = input.entries.clone();
        expected.destination_control = input.destination_control.clone();
        if expected != *input {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    pub async fn owned_removal(&self, node: &str) -> Result<Option<names::OwnedRemoval>> {
        let result = self
            .namespace()
            .owned_removal(self.volume(), node.into())
            .await?;
        if let Some(owned) = &result {
            self.check_namespace_input(&owned.input)?;
        }
        Ok(result)
    }
    pub async fn transition_cancellation(
        &self,
        operation: &str,
    ) -> Result<Option<names::TransitionCancellation>> {
        let result = self
            .namespace()
            .transition_cancellation(operation.into())
            .await?;
        if let Some(proof) = &result {
            self.check_namespace_input(&proof.transition.input)?;
            self.check_namespace_input(&proof.cancellation.input)?;
        }
        Ok(result)
    }
    pub async fn change_namespace(&self, input: NamespaceInput) -> Result<names::Record> {
        let n = match self {
            Self::Whole(p) => return p.change_namespace(input).await,
            Self::Native(n) => n,
        };
        let _slot = n.slots.try_acquire().map_err(|_| Failure::Busy)?;
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            self.check_namespace_input(&input)?;
            let local = n.chunks.namespace();
            let record = local.begin(input).await?;
            if record.outcome.is_some() {
                return Publisher::namespace_result(record);
            }
            if self
                .transition_cancellation(&record.input.request.operation)
                .await?
                .is_some()
            {
                return Err(Failure::CancelledTransition.into());
            }
            let outcome = match n
                .authority
                .request(&Request::Apply {
                    request: record.input.request.clone(),
                })
                .await?
            {
                Reply::Applied(receipt) => Ok(receipt),
                Reply::Error(e @ Error::DeletePending) => return Err(Failure::Authority(e).into()),
                Reply::Error(e) => Err(e),
                _ => return Err(Failure::Protocol.into()),
            };
            Publisher::namespace_result(
                local
                    .acknowledge(record.input.request.operation, outcome)
                    .await?,
            )
        })
        .await
        .map_err(|_| Failure::Incomplete)?
    }
    pub async fn complete(
        &self,
        record: &OpenRecord,
        callback: &str,
        completion: String,
    ) -> Result<()> {
        if self.native_record(record) && record.close().is_some_and(|e| e.id == callback) {
            let Self::Native(n) = self else {
                return Err(Failure::Binding.into());
            };
            return n
                .content
                .complete(&record.input.id, callback, completion)
                .await;
        }
        self.opens()
            .complete(record.input.id.clone(), completion, callback.into())
            .await?;
        Ok(())
    }
}
