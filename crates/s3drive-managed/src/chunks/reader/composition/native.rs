//! Original-open native bridge. Local mutation and observed Close are not
//! provider receipts; completion still requires exact source acknowledgments.
use super::*;
use crate::hex;
use s3drive_state::{
    chunk_mount,
    namespace_client::opens::OpenRecord,
    sparse_views::preparation::{self as plans, native},
};

/// A nonserializable original-session capability. Clones share the synchronous
/// Close fence. Never reconstructed by cold recovery or from a saved open ID.
#[derive(Clone)]
pub struct NativeSparseFile {
    client: native::Client,
    plans: plans::Client,
    local: sparse_views::Client,
    token: native::Token,
}
impl fmt::Debug for NativeSparseFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeSparseFile([private original open])")
    }
}
impl NativeSparseFile {
    pub async fn attach(
        chunks: &chunk_mount::Client,
        local: sparse_views::Client,
        open: String,
        views: sparse_views::Options,
        options: plans::Options,
    ) -> Result<Self> {
        let plans = chunks.preparations(options)?;
        if !plans.same_views(&local) {
            return Err(ReadFailure::Binding.into());
        }
        let client = chunks.native_sparse(views, options)?;
        let token = deadline(client.bind(open)).await?;
        Ok(Self {
            client,
            plans,
            local,
            token,
        })
    }
    /// Bounded metadata for this node's original preparations in ancestry order.
    pub async fn plans(&self) -> Result<Vec<plans::Info>> {
        deadline(self.client.plans(self.token.input().node.clone())).await
    }
    /// Work selection only; original plan reopening remains mandatory.
    pub async fn flush_selection(&self, binding: Binding) -> Result<native::FlushSelection> {
        self.binding_scope(&binding)?;
        deadline(
            self.client
                .flush_selection(binding, plans::publication::Limits::default()),
        )
        .await
    }
    pub fn open(&self) -> &str {
        &self.token.input().id
    }
    fn scope(&self, edited: &EditedFile) -> Result<()> {
        if !self.plans.same_views(&edited.local) {
            return Err(ReadFailure::Binding.into());
        }
        self.binding_scope(edited.view.binding())
    }
    fn binding_scope(&self, binding: &Binding) -> Result<()> {
        let base = binding.base()?;
        let input = self.token.input();
        if base.volume() != input.volume
            || base.node() != input.node
            || binding.source() != &input.source
            || binding.fingerprint() != input.fingerprint
            || binding.root() != input.store.root
            || hex(binding.store()) != input.store.id
        {
            return Err(ReadFailure::Binding.into());
        }
        Ok(())
    }
    fn context(&self, edited: &EditedFile) -> Result<native::Context> {
        self.scope(edited)?;
        let (root, page) = edited.original_context()?;
        native::Context::new(edited.view(), root.encode().to_vec(), page)
    }
    /// Local intent only. The authenticated original root/page comes from the
    /// reader, not adapter input. State atomically owns the first dirty epoch.
    pub async fn write(
        &self,
        edited: &EditedFile,
        nonce: Id,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<View> {
        if bytes.len() > MAX_READ_BYTES {
            return Err(ReadFailure::Bounds.into());
        }
        let context = self.context(edited)?;
        let _active = permit(edited)?;
        deadline(
            self.client
                .write(self.token.clone(), context, nonce, offset, bytes),
        )
        .await
    }
    pub async fn resize(&self, edited: &EditedFile, nonce: Id, size: u64) -> Result<View> {
        let context = self.context(edited)?;
        let _active = permit(edited)?;
        deadline(self.client.resize(self.token.clone(), context, nonce, size)).await
    }
    /// Reuse a verified pinned base only for its exact immutable binding. This
    /// does not authorize reads: read_range retains fresh history/provider checks.
    pub async fn refresh(&self, edited: &EditedFile, view: &View) -> Result<EditedFile> {
        self.scope(edited)?;
        if view.binding() != edited.view.binding() {
            return Err(ReadFailure::Binding.into());
        }
        let guard = deadline(self.local.guard(view.id())).await?;
        if *guard != *view {
            return Err(ReadFailure::Binding.into());
        }
        Ok(EditedFile {
            base: edited.base.clone(),
            local: self.local.clone(),
            view: guard,
        })
    }
    /// An active handle may request work for the original live publisher, but
    /// cannot replace that publisher or adopt a closed/cold epoch.
    pub async fn prepare(&self, edited: &EditedFile) -> Result<Preparation> {
        let context = self.context(edited)?;
        let _active = permit(edited)?;
        bounded(
            edited.base.0.reader.0.options,
            Box::pin(async {
                // Known exact preparations are historical answers, before probes.
                let matches: Vec<_> = self
                    .client
                    .plans(self.token.input().node.clone())
                    .await?
                    .into_iter()
                    .filter(|p| p.view().id() == edited.view.id())
                    .collect();
                let info = match matches.as_slice() {
                    [known] => known.clone(),
                    [] => {
                        Box::pin(edited.base.authorize_sparse()).await?;
                        self.client.prepare(self.token.clone(), context).await?
                    }
                    _ => return Err(ReadFailure::Integrity.into()),
                };
                let result = Preparation {
                    edited: edited.clone(),
                    client: self.plans.clone(),
                    id: info.id()?,
                };
                result.scope(&info)?;
                Ok(result)
            }),
        )
        .await
    }
    /// Must run at actual driver Close entry, before permits, locks or queues.
    pub fn observe_close(&self, callback: &str) -> Result<()> {
        self.token.observe_close_as(callback)
    }
    /// Creating this future synchronously marks Close. Dropping or timing out
    /// the future cannot reopen the capability or invent completion.
    pub fn close(
        &self,
        callback: String,
    ) -> impl Future<Output = Result<native::Close>> + Send + '_ {
        let observed = self.observe_close(&callback);
        async move {
            observed?;
            deadline(self.client.close(self.token.clone(), callback)).await
        }
    }
    pub async fn complete(&self, callback: String, completion: String) -> Result<OpenRecord> {
        deadline(
            self.client
                .complete(self.token.clone(), callback, completion),
        )
        .await
    }
    /// Reauthenticate an exact retained view for existing plans only. State
    /// independently denies new work without the original runtime/Close freeze.
    pub async fn reopen(&self, reader: &Reader, id: Id) -> Result<Preparation> {
        deadline(async {
            let info = self.plans.info(id).await?;
            if info.native_ownership().is_none() {
                return Err(ReadFailure::Binding.into());
            }
            self.binding_scope(info.view().binding())?;
            let edited =
                Box::pin(reader.reopen_edited(self.local.clone(), info.view().id())).await?;
            self.scope(&edited)?;
            let result = Preparation {
                edited,
                client: self.plans.clone(),
                id,
            };
            result.scope(&info)?;
            Ok(result)
        })
        .await
    }
}
async fn deadline<T>(future: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(DEADLINE, future)
        .await
        .map_err(|_| ReadFailure::Timeout)?
}

fn permit(edited: &EditedFile) -> Result<OwnedSemaphorePermit> {
    edited
        .base
        .0
        .reader
        .0
        .active
        .clone()
        .try_acquire_owned()
        .map_err(|_| ReadFailure::Busy.into())
}
