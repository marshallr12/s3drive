//! Shared mutable-file state. Native adapters never implement their own upload
//! policy or journal. Existing files only in this phase; no implicit rebase.
use super::{Error, MAX_NODES, Result, cache_error, edits};
use s3drive_s3::{ObjectMetadata, ObjectRevision, ReadHandle, uploads::Executor};
use s3drive_state::{
    journal::{Base, GenerationId, Target},
    remote::{Guard, Phase},
    worker::{Snapshot, Storage},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

pub(super) struct Writes {
    pub storage: Storage,
    pub sources: Vec<Option<Executor>>,
    coordinator: edits::Coordinator,
    entries: Mutex<HashMap<(usize, String), Arc<File>>>,
    slots: Semaphore,
}
pub(super) struct File {
    source: usize,
    target: Target,
    base: ReadHandle,
    serial: AsyncMutex<()>,
    current: Mutex<View>,
}
#[derive(Clone)]
struct View {
    snapshot: Option<Snapshot>,
    metadata: ObjectMetadata,
}
impl File {
    pub fn metadata(&self) -> Result<ObjectMetadata> {
        Ok(self
            .current
            .lock()
            .map_err(|_| Error::Internal)?
            .metadata
            .clone())
    }
    pub async fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        let view = self.current.lock().map_err(|_| Error::Internal)?.clone();
        match view.snapshot {
            Some(snapshot) => snapshot.read(offset, length).await.map_err(cache_error),
            None => Ok(Box::pin(self.base.read_range(offset, length)).await?),
        }
    }
}
impl Writes {
    pub fn new(storage: Storage, sources: Vec<Option<Executor>>) -> Self {
        Self {
            coordinator: edits::Coordinator::new(storage.staging()),
            storage,
            sources,
            entries: Mutex::new(HashMap::new()),
            slots: Semaphore::new(4),
        }
    }
    pub fn permits(&self, source: usize, key: &str) -> bool {
        self.sources
            .get(source)
            .and_then(Option::as_ref)
            .is_some_and(|e| e.check_replace(key, 0).is_ok())
    }
    // Online HEAD authorization is performed by the caller for EVERY new open.
    // A clean tracked head can be read through a fresh remote handle. Editing
    // after an external revision instead fails until explicit conflict recovery.
    pub async fn open(
        &self,
        source: usize,
        handle: &ReadHandle,
        writing: bool,
    ) -> Result<(Option<ReadHandle>, Option<Arc<File>>)> {
        let Some(executor) = self.sources.get(source).and_then(Option::as_ref) else {
            return if writing {
                Err(Error::ReadOnly)
            } else {
                Ok((None, None))
            };
        };
        let key = &handle.metadata().key;
        let target = match executor.target(key) {
            Ok(target) => target,
            Err(_) => {
                return if writing {
                    Err(Error::ReadOnly)
                } else {
                    Ok((None, None))
                };
            }
        };
        if writing {
            executor.check_replace(key, 0).map_err(policy_error)?;
        }
        let cached = self
            .entries
            .lock()
            .map_err(|_| Error::Internal)?
            .get(&(source, key.clone()))
            .cloned();
        let _serial = if let Some(file) = cached.as_ref() {
            Some(file.serial.lock().await)
        } else {
            None
        };
        let mut fresh = if cached.is_some() {
            Some(executor.reader().open(key).await?)
        } else {
            None
        };
        let head = self
            .storage
            .staging()
            .head(target.clone())
            .await
            .map_err(cache_error)?;
        if let Some(head) = &head {
            let mut selected = Some(head.clone());
            let mut expected = None;
            let mut count = 0;
            while let Some(generation) = selected {
                count += 1;
                if count > 65_536 {
                    return Err(Error::ResourceLimit);
                }
                if let Some(info) = self
                    .storage
                    .uploads()
                    .for_generation(generation.id)
                    .await
                    .map_err(cache_error)?
                {
                    match info.phase {
                        Phase::InFlight | Phase::Uncertain => {
                            let observed = executor
                                .reconcile(generation.id)
                                .await
                                .map_err(write_error)?;
                            if observed.phase != Phase::Verified {
                                return Err(Error::Conflict);
                            }
                            self.storage
                                .uploads()
                                .acknowledge(observed.operation)
                                .await
                                .map_err(cache_error)?;
                            fresh = Some(executor.reader().open(key).await?);
                            let rev = observed.revision.ok_or(Error::Internal)?;
                            expected = Some(ObjectRevision {
                                etag: Some(rev.etag().into()),
                                version_id: rev.version().map(Into::into),
                            });
                            break;
                        }
                        Phase::Rejected => return Err(Error::Conflict),
                        Phase::Verified => {
                            let rev = info.revision.ok_or(Error::Internal)?;
                            expected = Some(ObjectRevision {
                                etag: Some(rev.etag().into()),
                                version_id: rev.version().map(Into::into),
                            });
                            break;
                        }
                        Phase::Prepared => {
                            expected = Some(match info.guard {
                                Guard::Match(tag) => ObjectRevision {
                                    etag: Some(tag),
                                    version_id: None,
                                },
                                Guard::Missing => return Err(Error::Conflict),
                            });
                            break;
                        }
                        Phase::Cancelled => {}
                    }
                }
                selected = match generation.parent {
                    Some(id) => Some(self.storage.staging().info(id).await.map_err(cache_error)?),
                    None => None,
                };
            }
            let expected = expected.or_else(|| match &head.base {
                Base::Existing { etag, version } => Some(ObjectRevision {
                    etag: etag.clone(),
                    version_id: version.clone(),
                }),
                Base::Missing => None,
            });
            let handle = fresh.as_ref().unwrap_or(handle);
            if expected.as_ref().is_none_or(|e| {
                e.etag != handle.metadata().revision.etag
                    || (e.version_id.is_some()
                        && e.version_id != handle.metadata().revision.version_id)
            }) {
                if writing
                    || self
                        .storage
                        .uploads()
                        .settlement(head.id)
                        .await
                        .map_err(cache_error)?
                        .is_none()
                {
                    return Err(Error::Conflict);
                }
                return Ok((fresh, None)); // Fresh read sees external data; never edit an old local base.
            }
            if let Some(file) = cached.as_ref() {
                self.latest(file).await?;
            }
        }
        let identity = (source, key.clone());
        if let Some(file) = cached.as_ref() {
            return Ok((fresh, Some(file.clone())));
        }
        let handle = fresh.as_ref().unwrap_or(handle);
        let mut metadata = handle.metadata().clone();
        let snapshot = match head {
            Some(head) => {
                metadata.size = head.size;
                Some(
                    self.storage
                        .staging()
                        .snapshot(head.id)
                        .await
                        .map_err(cache_error)?,
                )
            }
            None => None,
        };
        let file = Arc::new(File {
            source,
            target,
            base: handle.clone(),
            serial: AsyncMutex::new(()),
            current: Mutex::new(View { snapshot, metadata }),
        });
        let mut entries = self.entries.lock().map_err(|_| Error::Internal)?;
        if entries.len() >= MAX_NODES {
            return Err(Error::ResourceLimit);
        }
        Ok((fresh, Some(entries.entry(identity).or_insert(file).clone())))
    }
    async fn latest(&self, file: &File) -> Result<View> {
        let mut view = file.current.lock().map_err(|_| Error::Internal)?.clone();
        if let Some(head) = self
            .storage
            .staging()
            .head(file.target.clone())
            .await
            .map_err(cache_error)?
            && view.snapshot.as_ref().map(|s| s.metadata().id) != Some(head.id)
        {
            view.snapshot = Some(
                self.storage
                    .staging()
                    .snapshot(head.id)
                    .await
                    .map_err(cache_error)?,
            );
            view.metadata.size = head.size;
            view.metadata.revision = ObjectRevision {
                etag: None,
                version_id: None,
            };
            *file.current.lock().map_err(|_| Error::Internal)? = view.clone();
        }
        Ok(view)
    }
    async fn editor(&self, file: &File, view: &View) -> Result<edits::Editor> {
        match &view.snapshot {
            Some(snapshot) => self
                .coordinator
                .local(snapshot.metadata().id)
                .await
                .map_err(write_error),
            None => self
                .coordinator
                .from_s3(file.base.clone())
                .map_err(write_error),
        }
    }
    async fn before_edit(&self, file: &File, view: &View) -> Result<()> {
        if let Some(snapshot) = &view.snapshot
            && let Some(info) = self
                .storage
                .uploads()
                .for_generation(snapshot.metadata().id)
                .await
                .map_err(cache_error)?
        {
            match info.phase {
                Phase::Prepared => {
                    self.storage
                        .uploads()
                        .cancel_prepared(info.operation)
                        .await
                        .map_err(cache_error)?;
                }
                Phase::InFlight | Phase::Uncertain => {
                    self.flush_generation(file, info.generation).await?
                }
                Phase::Rejected => return Err(Error::Conflict),
                _ => {}
            }
        }
        Ok(())
    }
    async fn publish(&self, file: &File, mut view: View, plan: edits::EditPlan) -> Result<()> {
        let snapshot = plan.execute().await.map_err(write_error)?;
        let id = snapshot.metadata().id;
        view.metadata.size = snapshot.metadata().size;
        view.metadata.modified_unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|t| i64::try_from(t.as_secs()).ok());
        view.metadata.revision = ObjectRevision {
            etag: None,
            version_id: None,
        };
        view.snapshot = Some(snapshot);
        *file.current.lock().map_err(|_| Error::Internal)? = view;
        self.flush_generation(file, id).await
    }
    async fn flush_generation(&self, file: &File, id: GenerationId) -> Result<()> {
        let receipt = self.sources[file.source]
            .as_ref()
            .ok_or(Error::ReadOnly)?
            .flush(id)
            .await
            .map_err(write_error)?;
        let revision = receipt.info().revision.as_ref().ok_or(Error::Internal)?;
        file.current
            .lock()
            .map_err(|_| Error::Internal)?
            .metadata
            .revision = ObjectRevision {
            etag: Some(revision.etag().into()),
            version_id: revision.version().map(Into::into),
        };
        Ok(())
    }
    pub async fn write(
        &self,
        file: &File,
        offset: u64,
        bytes: Arc<[u8]>,
        append: bool,
        constrained: bool,
    ) -> Result<usize> {
        let _slot = self.slots.try_acquire().map_err(|_| Error::Busy)?;
        let _serial = file.serial.lock().await;
        let view = self.latest(file).await?;
        let offset = if append { view.metadata.size } else { offset };
        let count = if constrained {
            bytes
                .len()
                .min(view.metadata.size.saturating_sub(offset) as usize)
        } else {
            bytes.len()
        };
        if count == 0 {
            return Ok(0);
        }
        let size = view
            .metadata
            .size
            .max(offset.checked_add(count as u64).ok_or(Error::TooLarge)?);
        self.sources[file.source]
            .as_ref()
            .ok_or(Error::ReadOnly)?
            .check_replace(file.target.object_key(), size)
            .map_err(policy_error)?;
        self.before_edit(file, &view).await?;
        let plan = self
            .editor(file, &view)
            .await?
            .write(
                offset,
                if count == bytes.len() {
                    bytes
                } else {
                    Arc::from(&bytes[..count])
                },
            )
            .map_err(write_error)?;
        self.publish(file, view, plan).await?;
        Ok(count)
    }
    pub async fn truncate(&self, file: &File, size: u64) -> Result<()> {
        let _slot = self.slots.try_acquire().map_err(|_| Error::Busy)?;
        let _serial = file.serial.lock().await;
        let view = self.latest(file).await?;
        self.sources[file.source]
            .as_ref()
            .ok_or(Error::ReadOnly)?
            .check_replace(file.target.object_key(), size)
            .map_err(policy_error)?;
        if view.metadata.size == size {
            if let Some(snapshot) = &view.snapshot {
                self.flush_generation(file, snapshot.metadata().id).await?;
            }
            return Ok(());
        }
        self.before_edit(file, &view).await?;
        let plan = self
            .editor(file, &view)
            .await?
            .truncate(size)
            .map_err(write_error)?;
        self.publish(file, view, plan).await
    }
    pub async fn flush(&self, file: &File) -> Result<()> {
        let _slot = self.slots.try_acquire().map_err(|_| Error::Busy)?;
        let _serial = file.serial.lock().await;
        if let Some(snapshot) = self.latest(file).await?.snapshot {
            self.flush_generation(file, snapshot.metadata().id).await?;
        }
        Ok(())
    }
    pub async fn flush_all(&self) -> Result<()> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| Error::Internal)?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for entry in entries {
            self.flush(&entry).await?;
        }
        Ok(())
    }
}
fn policy_error(error: s3drive_s3::uploads::Failure) -> Error {
    if error == s3drive_s3::uploads::Failure::PolicySize {
        Error::TooLarge
    } else {
        Error::ReadOnly
    }
}
fn write_error(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Error {
    let error = error.into();
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error.as_ref());
    while let Some(e) = source {
        if s3drive_state::cache::is_disk_full(e) {
            return Error::NoSpace;
        }
        if e.downcast_ref::<s3drive_state::journal::Failure>()
            == Some(&s3drive_state::journal::Failure::QuotaExceeded)
        {
            return Error::NoSpace;
        }
        if e.downcast_ref::<s3drive_state::journal::Failure>()
            == Some(&s3drive_state::journal::Failure::LocalConflict)
        {
            return Error::Conflict;
        }
        source = e.source();
    }
    Error::Write(error)
}
