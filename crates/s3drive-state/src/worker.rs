//! One bounded owner for clean-cache and staged-journal commands. Network I/O
//! stays with the caller, outside this thread and all SQLite transactions.
use crate::{Result, cache, journal, remote};
use journal::{Base, Generation, GenerationId, Journal, Receipt, Target};
use std::{fmt, path::Path, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot};

const QUEUE_SIZE: usize = 8;
const DEADLINE: Duration = Duration::from_secs(30);
type Work = Box<dyn FnOnce(&mut cache::Store) + Send>;
enum Command {
    Work(Work),
    Close(oneshot::Sender<()>),
}

/// Admission errors prove the command was not submitted. Once accepted, a
/// deadline or lost reply cannot prove rollback: inspect durable state before
/// retrying. In particular, `begin` has no idempotency key; inspect drafts after
/// an unknown outcome rather than blindly allocating another builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    QueueFull,
    Closed,
    OutcomeUnknown,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::QueueFull => "local storage command queue is full; request was not submitted",
            Self::Closed => "local storage worker is closed; request was not submitted",
            Self::OutcomeUnknown => {
                "local storage reply was lost or timed out; reconcile durable state before retrying"
            }
        })
    }
}
impl std::error::Error for Failure {}

/// All handles obtained from this instance share one connection, kernel lock,
/// eight-command queue and dedicated OS thread. No second owner is opened for
/// staging. Close only after all filesystem callbacks and producers have stopped.
#[derive(Clone)]
pub struct Storage {
    owner: Owner,
    cache_options: cache::Options,
    staging_options: journal::Options,
}
impl Storage {
    pub async fn open(
        directory: &Path,
        cache_options: cache::Options,
        staging_options: journal::Options,
    ) -> Result<Self> {
        let cache_options = cache_options.validate()?;
        let staging_options = staging_options.validate()?;
        let directory = directory.to_path_buf();
        let (sender, mut receiver) = mpsc::channel(QUEUE_SIZE);
        let (ready, started) = oneshot::channel();
        std::thread::Builder::new()
            .name("s3drive-state".into())
            .spawn(move || {
                let mut store = match cache::Store::open(&directory, cache_options) {
                    Ok(store) => store,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                let mut closing = None;
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        Command::Work(work) => work(&mut store),
                        Command::Close(reply) => {
                            receiver.close();
                            closing = Some(reply);
                            break;
                        }
                    }
                }
                // Do not acknowledge close until the DB and its directory lock
                // have been released. Unwinding also releases ownership, but
                // cannot acknowledge a successful close.
                drop(store);
                if let Some(reply) = closing {
                    let _ = reply.send(());
                }
            })?;
        tokio::time::timeout(DEADLINE, started)
            .await
            .map_err(|_| "local storage initialization timed out")?
            .map_err(|_| "local storage worker stopped during initialization")??;
        Ok(Self {
            owner: Owner { sender },
            cache_options,
            staging_options,
        })
    }

    pub fn cache(&self) -> cache::Cache {
        cache::Cache {
            owner: self.owner.clone(),
            options: self.cache_options,
        }
    }
    pub fn opens(&self) -> Opens {
        Opens {
            owner: self.owner.clone(),
        }
    }
    pub fn namespace(&self) -> Namespace {
        Namespace {
            owner: self.owner.clone(),
            options: self.staging_options,
        }
    }
    pub fn logical(&self) -> Logical {
        Logical {
            owner: self.owner.clone(),
            options: self.staging_options,
        }
    }
    pub fn managed(&self) -> Managed {
        Managed {
            owner: self.owner.clone(),
            options: self.staging_options,
        }
    }
    pub fn staging(&self) -> Staging {
        Staging {
            owner: self.owner.clone(),
            options: self.staging_options,
        }
    }
    /// Offline immutable multipart plans on the same private SQLite owner.
    /// Construction and Ready records do not enable network mutations.
    pub fn multipart(
        &self,
        options: crate::multipart::Options,
    ) -> Result<crate::multipart::Planner> {
        crate::multipart::Planner::new(self.owner.clone(), options)
    }
    pub fn uploads(&self) -> Uploads {
        Uploads {
            owner: self.owner.clone(),
        }
    }
    /// This closes every clone, including cache, staging and snapshot handles.
    pub async fn close(&self) -> Result<()> {
        self.owner.close().await
    }
}

/// Durable remote-attempt records on the existing owner. These operations do
/// no network I/O and cannot establish provider capability or authorization.
#[derive(Clone)]
pub struct Uploads {
    owner: Owner,
}
impl Uploads {
    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut remote::Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.owner
            .call(move |store| {
                work(&mut remote::Journal::new(
                    &mut store.state,
                    remote::Options::default(),
                )?)
            })
            .await
    }
    pub async fn prepare(&self, id: GenerationId) -> Result<remote::Info> {
        self.call(move |j| j.prepare(id)).await
    }
    pub async fn claim(
        &self,
        id: remote::OperationId,
        binding: Target,
    ) -> Result<remote::SendPermit> {
        self.call(move |j| j.claim(id, &binding)).await
    }
    pub async fn info(&self, id: remote::OperationId) -> Result<remote::Info> {
        self.call(move |j| j.info(id)).await
    }
    pub async fn for_generation(&self, id: GenerationId) -> Result<Option<remote::Info>> {
        self.call(move |j| j.for_generation(id)).await
    }
    pub async fn list(&self, after: Option<GenerationId>, limit: u16) -> Result<Vec<remote::Info>> {
        if !(1..=1000).contains(&limit) {
            return Err(remote::Failure::InvalidInput.into());
        }
        self.call(move |j| j.list(after, limit)).await
    }
    pub async fn cancel_prepared(&self, id: remote::OperationId) -> Result<remote::Info> {
        self.call(move |j| j.cancel_prepared(id)).await
    }
    pub async fn uncertain(
        &self,
        id: remote::OperationId,
        reason: remote::Reason,
    ) -> Result<remote::Info> {
        self.call(move |j| j.uncertain(id, reason)).await
    }
    pub async fn observe_unmatched(
        &self,
        id: remote::OperationId,
        observation: remote::Observation,
    ) -> Result<remote::Info> {
        self.call(move |j| j.observe_unmatched(id, observation))
            .await
    }
    pub async fn rejected(&self, id: remote::OperationId, status: u16) -> Result<remote::Info> {
        self.call(move |j| j.rejected(id, status)).await
    }
    pub async fn verified(&self, proof: remote::Verification) -> Result<remote::Info> {
        self.call(move |j| j.verified(proof)).await
    }
    pub async fn acknowledge(&self, id: remote::OperationId) -> Result<remote::FlushReceipt> {
        self.call(move |j| j.acknowledge(id)).await
    }
    pub async fn flush_receipt(
        &self,
        generation: GenerationId,
    ) -> Result<Option<remote::FlushReceipt>> {
        self.call(move |j| j.flush_receipt(generation)).await
    }
    pub async fn settlement(&self, generation: GenerationId) -> Result<Option<remote::Settlement>> {
        self.call(move |j| j.settlement(generation)).await
    }
}

#[derive(Clone)]
pub(crate) struct Owner {
    sender: mpsc::Sender<Command>,
}
impl Owner {
    pub(crate) async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut cache::Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.call_with_deadline(work, DEADLINE).await
    }
    async fn call_with_deadline<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut cache::Store) -> Result<T> + Send + 'static,
        deadline: Duration,
    ) -> Result<T> {
        let (reply, receive) = oneshot::channel();
        let command = Command::Work(Box::new(move |store| {
            // Dropped/timed-out callers can be skipped while still queued.
            // Cancellation racing execution cannot undo a committed mutation.
            if !reply.is_closed() {
                let _ = reply.send(work(store));
            }
        }));
        self.sender.try_send(command).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => Failure::QueueFull,
            mpsc::error::TrySendError::Closed(_) => Failure::Closed,
        })?;
        tokio::time::timeout(deadline, receive)
            .await
            .map_err(|_| Failure::OutcomeUnknown)?
            .map_err(|_| Failure::OutcomeUnknown)?
    }

    pub(crate) async fn close(&self) -> Result<()> {
        let (reply, receive) = oneshot::channel();
        // Close waits for queue capacity so callers can drain accepted work.
        // Its timeout is conservatively unknown, even before admission.
        tokio::time::timeout(DEADLINE, async {
            self.sender
                .send(Command::Close(reply))
                .await
                .map_err(|_| Failure::Closed)?;
            receive.await.map_err(|_| Failure::OutcomeUnknown)
        })
        .await
        .map_err(|_| Failure::OutcomeUnknown)??;
        Ok(())
    }
}

/// Async local journal API. Each mutation is one bounded command. There are no
/// caller-supplied executable jobs and no networking on the worker. This handle
/// does not authorize cloud operations or enable writable mounts.
#[derive(Clone)]
pub struct Staging {
    owner: Owner,
    options: journal::Options,
}
impl Staging {
    pub async fn capacity(&self) -> Result<journal::Capacity> {
        self.call(|j| j.capacity()).await
    }
    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let options = self.options;
        self.owner
            .call(move |store| work(&mut Journal::new(&mut store.state, options)?))
            .await
    }

    pub async fn begin(
        &self,
        target: Target,
        base: Base,
        expected_parent: Option<GenerationId>,
        size: u64,
    ) -> Result<GenerationId> {
        base.validate()?;
        if size > i64::MAX as u64 {
            return Err("staged size exceeds supported offset range".into());
        }
        self.call(move |journal| journal.begin(&target, base, expected_parent, size))
            .await
    }
    pub async fn begin_operation(
        &self,
        key: [u8; 32],
        target: Target,
        base: Base,
        expected_parent: Option<GenerationId>,
        size: u64,
    ) -> Result<GenerationId> {
        base.validate()?;
        if size > i64::MAX as u64 {
            return Err("staged size exceeds supported offset range".into());
        }
        self.call(move |journal| {
            journal.begin_operation(&key, &target, base, expected_parent, size)
        })
        .await
    }
    pub async fn copy_next_at(
        &self,
        id: GenerationId,
        offset: u64,
        range: journal::CopyRange,
    ) -> Result<u64> {
        self.call(move |journal| journal.copy_next_at(id, offset, range))
            .await
    }
    /// Bounded input is checked before admission. Replays must match the exact
    /// previous chunk; no unguarded sequential-append variant is exposed here.
    pub async fn append_at(&self, id: GenerationId, offset: u64, bytes: Arc<[u8]>) -> Result<()> {
        if bytes.is_empty() || bytes.len() > journal::CHUNK_BYTES {
            return Err("invalid staged extent length".into());
        }
        self.call(move |journal| journal.append_at(id, offset, &bytes))
            .await
    }
    pub async fn append_zero_at(&self, id: GenerationId, offset: u64, length: u64) -> Result<()> {
        if length == 0 {
            return Err("invalid staged extent length".into());
        }
        self.call(move |journal| journal.append_zero_at(id, offset, length))
            .await
    }
    pub async fn seal(&self, id: GenerationId) -> Result<Receipt> {
        self.call(move |journal| journal.seal(id)).await
    }
    pub async fn abort(&self, id: GenerationId) -> Result<()> {
        self.call(move |journal| journal.abort(id)).await
    }
    pub async fn info(&self, id: GenerationId) -> Result<Generation> {
        self.call(move |journal| journal.info(id)).await
    }
    pub async fn head(&self, target: Target) -> Result<Option<Generation>> {
        self.call(move |journal| journal.head(&target)).await
    }
    pub async fn heads(
        &self,
        source: [u8; 32],
        fingerprint: String,
        after: Option<GenerationId>,
        limit: u16,
    ) -> Result<Vec<Generation>> {
        if !(1..=1000).contains(&limit)
            || fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid staged head catalog request".into());
        }
        self.call(move |journal| journal.heads(source, &fingerprint, after, limit))
            .await
    }
    pub async fn read(&self, id: GenerationId, offset: u64, length: usize) -> Result<Vec<u8>> {
        self.read_range(id, offset, length, false).await
    }
    pub async fn read_draft(
        &self,
        id: GenerationId,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>> {
        self.read_range(id, offset, length, true).await
    }
    async fn read_range(
        &self,
        id: GenerationId,
        offset: u64,
        length: usize,
        draft: bool,
    ) -> Result<Vec<u8>> {
        if length > journal::MAX_READ_BYTES {
            return Err("staged read exceeds bounded range".into());
        }
        self.call(move |journal| {
            if draft {
                journal.read_draft(id, offset, length)
            } else {
                journal.read(id, offset, length)
            }
        })
        .await
    }
    pub async fn pending(
        &self,
        after: Option<GenerationId>,
        limit: u16,
    ) -> Result<Vec<Generation>> {
        self.list(after, limit, false).await
    }
    pub async fn drafts(&self, after: Option<GenerationId>, limit: u16) -> Result<Vec<Generation>> {
        self.list(after, limit, true).await
    }
    async fn list(
        &self,
        after: Option<GenerationId>,
        limit: u16,
        drafts: bool,
    ) -> Result<Vec<Generation>> {
        if !(1..=1000).contains(&limit) {
            return Err("invalid staged recovery page size".into());
        }
        self.call(move |journal| {
            if drafts {
                journal.drafts(after, limit)
            } else {
                journal.pending(after, limit)
            }
        })
        .await
    }

    /// A lightweight immutable generation handle. This stores no whole-file
    /// buffer; sealed generations already cannot be reclaimed by the journal.
    /// A future GC implementation must add explicit pin accounting here.
    pub async fn snapshot(&self, id: GenerationId) -> Result<Snapshot> {
        let generation = self
            .call(move |journal| {
                let generation = journal.info(id)?;
                journal.read(id, 0, 0)?; // Validate phase and complete manifest.
                Ok(generation)
            })
            .await?;
        Ok(Snapshot {
            staging: self.clone(),
            generation,
        })
    }
}

#[derive(Clone)]
pub struct Snapshot {
    staging: Staging,
    generation: Generation,
}
impl Snapshot {
    pub fn metadata(&self) -> &Generation {
        &self.generation
    }
    pub async fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        if length > journal::MAX_READ_BYTES {
            return Err("staged read exceeds bounded range".into());
        }
        let id = self.generation.id;
        let expected = self.generation.manifest_hash;
        self.staging
            .call(move |journal| {
                if journal.info(id)?.manifest_hash != expected {
                    return Err("staged snapshot changed; data was preserved".into());
                }
                journal.read(id, offset, length)
            })
            .await
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct Managed {
    owner: Owner,
    options: journal::Options,
}
impl Managed {
    pub async fn reject(
        &self,
        id: String,
        rejection: crate::managed::Rejection,
    ) -> Result<crate::managed::Record> {
        self.call(move |j| j.reject(&id, rejection)).await
    }
    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut crate::managed::Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let options = self.options;
        self.owner
            .call(move |store| {
                work(&mut crate::managed::Journal::new(
                    &mut store.state,
                    options,
                )?)
            })
            .await
    }
    pub async fn begin(&self, input: crate::managed::Input) -> Result<crate::managed::Record> {
        self.call(move |j| j.begin(input)).await
    }
    pub async fn lookup(&self, id: String) -> Result<Option<crate::managed::Record>> {
        self.call(move |j| j.lookup(&id)).await
    }
    pub async fn get(&self, id: String) -> Result<crate::managed::Record> {
        self.call(move |j| j.get(&id)).await
    }
    pub async fn link(
        &self,
        id: String,
        generation: GenerationId,
    ) -> Result<crate::managed::Record> {
        self.call(move |j| j.link(&id, generation)).await
    }
    pub async fn recover_stage(&self, id: String) -> Result<crate::managed::Record> {
        self.call(move |j| j.recover_stage(&id)).await
    }
    pub async fn reserved(
        &self,
        id: String,
        r: crate::namespace::Reservation,
    ) -> Result<crate::managed::Record> {
        self.call(move |j| j.reserved(&id, r)).await
    }
    pub async fn prepare(&self, id: String, sequence: u64) -> Result<crate::managed::Record> {
        self.call(move |j| j.prepare(&id, sequence)).await
    }
    pub async fn acknowledge(
        &self,
        id: String,
        r: crate::namespace::Receipt,
    ) -> Result<crate::managed::Record> {
        self.call(move |j| j.acknowledge(&id, r)).await
    }
    pub async fn pending(
        &self,
        after: Option<String>,
        limit: u16,
    ) -> Result<Vec<crate::managed::Record>> {
        self.call(move |j| j.pending(after.as_deref(), limit)).await
    }
}

/// Local logical-file edits on the same bounded owner as payloads and receipts.
#[derive(Clone)]
pub struct Logical {
    owner: Owner,
    options: journal::Options,
}
impl Logical {
    pub async fn retain_conflict(&self, id: String) -> Result<crate::logical::RetainedConflict> {
        self.call(move |j| j.retain_conflict(&id)).await
    }
    pub async fn retained_conflict(
        &self,
        id: String,
    ) -> Result<Option<crate::logical::RetainedConflict>> {
        self.call(move |j| j.retained_conflict(&id)).await
    }
    pub async fn lookup(
        &self,
        volume: String,
        node: String,
    ) -> Result<Option<crate::logical::File>> {
        self.call(move |j| j.lookup(&volume, &node)).await
    }
    pub async fn observe(&self, binding: crate::logical::Binding) -> Result<crate::logical::File> {
        self.call(move |j| j.observe(binding)).await
    }
    pub async fn for_generation(&self, g: GenerationId) -> Result<crate::logical::Record> {
        self.call(move |j| j.for_generation(g)).await
    }

    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut crate::logical::Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let options = self.options;
        self.owner
            .call(move |store| {
                work(&mut crate::logical::Journal::new(
                    &mut store.state,
                    options,
                )?)
            })
            .await
    }
    pub async fn bind(&self, binding: crate::logical::Binding) -> Result<crate::logical::File> {
        self.call(move |j| j.bind(binding)).await
    }
    pub async fn file(&self, volume: String, node: String) -> Result<crate::logical::File> {
        self.call(move |j| j.file(&volume, &node)).await
    }
    pub async fn begin(&self, input: crate::logical::Input) -> Result<crate::logical::Record> {
        self.call(move |j| j.begin(input)).await
    }
    pub async fn get(&self, id: String) -> Result<crate::logical::Record> {
        self.call(move |j| j.get(&id)).await
    }
    pub async fn seal(&self, id: String) -> Result<crate::logical::Record> {
        self.call(move |j| j.seal(&id)).await
    }
    pub async fn prepare(
        &self,
        id: String,
        input: crate::managed::Input,
    ) -> Result<crate::logical::Record> {
        self.call(move |j| j.prepare(&id, input)).await
    }
    pub async fn acknowledge(&self, id: String) -> Result<crate::logical::Record> {
        self.call(move |j| j.acknowledge(&id)).await
    }
    pub async fn pending(
        &self,
        after: Option<String>,
        limit: u16,
    ) -> Result<Vec<crate::logical::Record>> {
        self.call(move |j| j.pending(after.as_deref(), limit)).await
    }
}

/// Metadata requests share the same owner/queue as file bytes and publications.
#[derive(Clone)]
pub struct Namespace {
    owner: Owner,
    options: journal::Options,
}
impl Namespace {
    pub async fn get(&self, id: String) -> Result<crate::namespace_client::Record> {
        self.call(move |j| j.get(&id)).await
    }

    pub async fn client_identity(&self) -> Result<String> {
        self.call(|j| j.client_identity()).await
    }
    pub async fn owned_removal(
        &self,
        volume: String,
        node: String,
    ) -> Result<Option<crate::namespace_client::OwnedRemoval>> {
        self.call(move |j| j.owned_removal(&volume, &node)).await
    }
    pub async fn transition_cancellation(
        &self,
        operation: String,
    ) -> Result<Option<crate::namespace_client::TransitionCancellation>> {
        self.call(move |j| j.transition_cancellation(&operation))
            .await
    }
    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut crate::namespace_client::Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let options = self.options;
        self.owner
            .call(move |store| {
                work(&mut crate::namespace_client::Journal::with_options(
                    &mut store.state,
                    options,
                )?)
            })
            .await
    }
    pub async fn begin(
        &self,
        input: crate::namespace_client::Input,
    ) -> Result<crate::namespace_client::Record> {
        self.call(move |j| j.begin(input)).await
    }
    pub async fn acknowledge(
        &self,
        id: String,
        outcome: crate::namespace::Result<crate::namespace::Receipt>,
    ) -> Result<crate::namespace_client::Record> {
        self.call(move |j| j.acknowledge(&id, outcome)).await
    }
    pub async fn pending(
        &self,
        after: Option<String>,
        limit: u16,
    ) -> Result<Vec<crate::namespace_client::Record>> {
        self.call(move |j| j.pending(after.as_deref(), limit)).await
    }
}

/// Per-open callback facts share this Storage's existing private SQLite owner.
#[derive(Clone)]
pub struct Opens {
    owner: Owner,
}
impl Opens {
    pub async fn start_session(&self, session: String, volume: String) -> Result<()> {
        self.call(move |j| j.start_session(&session, &volume)).await
    }
    pub async fn current_session(&self, session: String, volume: String) -> Result<()> {
        self.call(move |j| j.current_session(&session, &volume))
            .await
    }
    pub async fn obsolete_session(&self, session: String, volume: String) -> Result<bool> {
        self.call(move |j| j.obsolete_session(&session, &volume))
            .await
    }
    pub async fn pending_recovery(
        &self,
        volume: String,
        node: String,
    ) -> Result<Option<crate::namespace_client::opens::session::RecoveryRecord>> {
        self.call(move |j| j.pending_recovery(&volume, &node)).await
    }
    pub async fn prepare_recovery(
        &self,
        session: String,
        volume: String,
        node: String,
        cancel: Option<crate::namespace_client::Input>,
    ) -> Result<crate::namespace_client::opens::session::RecoveryRecord> {
        self.call(move |j| j.prepare_recovery(&session, &volume, &node, cancel))
            .await
    }
    pub async fn finish_recovery(
        &self,
        session: String,
        key: String,
    ) -> Result<crate::namespace_client::opens::session::RecoveryRecord> {
        self.call(move |j| j.finish_recovery(&session, &key)).await
    }
    pub async fn recovered(&self, key: String) -> Result<bool> {
        self.call(move |j| j.recovered(&key)).await
    }
    pub async fn retire(&self, id: String) -> Result<()> {
        self.call(move |j| j.retire(&id)).await
    }
    pub async fn reply(
        &self,
        id: String,
        event: String,
        callback: String,
        error: Option<crate::namespace::Error>,
    ) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.reply(&id, &event, &callback, error))
            .await
    }
    pub async fn complete(
        &self,
        id: String,
        event: String,
        callback: String,
    ) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.complete(&id, &event, &callback)).await
    }

    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut crate::namespace_client::opens::Journal<'_>) -> Result<T>
        + Send
        + 'static,
    ) -> Result<T> {
        self.owner
            .call(move |store| {
                work(&mut crate::namespace_client::opens::Journal::new(
                    &mut store.state,
                )?)
            })
            .await
    }
    pub async fn prepare(
        &self,
        input: crate::namespace_client::opens::OpenInput,
    ) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.prepare(input)).await
    }
    pub async fn get(&self, id: String) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.get(&id)).await
    }
    pub async fn records(&self) -> Result<Vec<crate::namespace_client::opens::OpenRecord>> {
        self.call(move |j| j.records()).await
    }
    pub async fn abandoned(
        &self,
        session: String,
    ) -> Result<Vec<crate::namespace_client::opens::OpenRecord>> {
        self.call(move |j| j.abandoned(&session)).await
    }
    pub async fn activate(
        &self,
        id: String,
        event: String,
    ) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.activate(&id, &event)).await
    }
    pub async fn callback(
        &self,
        id: String,
        event: String,
        kind: crate::namespace_client::opens::Callback,
    ) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.callback(&id, &event, kind)).await
    }
    pub async fn link(
        &self,
        id: String,
        event: String,
        callback: String,
        input: crate::namespace_client::Input,
    ) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.link(&id, &event, &callback, input))
            .await
    }
    pub async fn acknowledge(
        &self,
        id: String,
        event: String,
        operation: String,
    ) -> Result<crate::namespace_client::opens::OpenRecord> {
        self.call(move |j| j.acknowledge(&id, &event, &operation))
            .await
    }
}
