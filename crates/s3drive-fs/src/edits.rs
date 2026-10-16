//! Local file-edit plans. No writable callbacks or S3 mutations are enabled.
//! Each successful plan publishes one immutable local snapshot. The original
//! remote condition is retained for a later conditional upload executor.
use s3drive_s3::ReadHandle;
use s3drive_state::{
    journal::{self, Base, CopyRange, GenerationId, Phase, Target},
    worker::{Snapshot, Staging},
};
use sha2::{Digest, Sha256};
use std::{fmt, future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

type LocalResult<T> = s3drive_state::Result<T>;
type ReadFuture<'a> = Pin<Box<dyn Future<Output = LocalResult<Vec<u8>>> + Send + 'a>>;

/// Every failure retains the known builder ID for explicit recovery. A missing
/// ID can include an uncertain begin result; retry this exact plan to recover
/// through its persisted operation key. Display never includes object names.
#[derive(Debug)]
pub struct Error {
    pub generation: Option<GenerationId>,
    cause: Box<dyn std::error::Error + Send + Sync>,
}
impl Error {
    fn new(
        generation: Option<GenerationId>,
        cause: impl Into<Box<dyn std::error::Error + Send + Sync>>,
    ) -> Self {
        Self {
            generation,
            cause: cause.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "local edit failed; staged data was preserved: {}",
            self.cause
        )
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
pub type Result<T> = std::result::Result<T, Error>;

// Private test seam, narrowly matching the already implemented pinned reader.
// The public remote constructor only accepts a real S3 ReadHandle.
trait PinnedReader: Send + Sync {
    fn read(&self, offset: u64, length: usize) -> ReadFuture<'_>;
}
struct S3Base(ReadHandle);
impl PinnedReader for S3Base {
    fn read(&self, offset: u64, length: usize) -> ReadFuture<'_> {
        Box::pin(async move { Ok(Box::pin(self.0.read_range(offset, length)).await?) })
    }
}
struct Remote {
    target: Target,
    base: Base,
    size: u64,
    reader: Arc<dyn PinnedReader>,
}
#[derive(Clone)]
enum Origin {
    Empty(Target),
    Local(Snapshot),
    Remote(Arc<Remote>),
}
impl Origin {
    fn target(&self) -> &Target {
        match self {
            Self::Empty(target) => target,
            Self::Local(snapshot) => &snapshot.metadata().target,
            Self::Remote(remote) => &remote.target,
        }
    }
    fn base(&self) -> Base {
        match self {
            Self::Empty(_) => Base::Missing,
            Self::Local(snapshot) => snapshot.metadata().base.clone(),
            Self::Remote(remote) => remote.base.clone(),
        }
    }
    fn size(&self) -> u64 {
        match self {
            Self::Empty(_) => 0,
            Self::Local(snapshot) => snapshot.metadata().size,
            Self::Remote(remote) => remote.size,
        }
    }
    fn parent(&self) -> Option<GenerationId> {
        match self {
            Self::Local(snapshot) => Some(snapshot.metadata().id),
            _ => None,
        }
    }
}

struct Shared {
    staging: Staging,
    slots: Semaphore,
    read_deadline: Duration,
}
/// Use one coordinator per mounted volume. Four plans may execute at once;
/// admission beyond that fails promptly rather than queuing file-sized inputs.
#[derive(Clone)]
pub struct Coordinator(Arc<Shared>);
impl Coordinator {
    pub fn new(staging: Staging) -> Self {
        Self(Arc::new(Shared {
            staging,
            slots: Semaphore::new(4),
            read_deadline: Duration::from_secs(30),
        }))
    }
    /// Target identity must come from an authorized source. This constructor
    /// does not establish absence or permission to create an S3 object.
    pub fn new_file(&self, target: Target) -> Editor {
        Editor {
            coordinator: self.clone(),
            origin: Origin::Empty(target),
        }
    }
    pub async fn local(&self, id: GenerationId) -> Result<Editor> {
        let snapshot = self
            .0
            .staging
            .snapshot(id)
            .await
            .map_err(|e| Error::new(Some(id), e))?;
        Ok(Editor {
            coordinator: self.clone(),
            origin: Origin::Local(snapshot),
        })
    }
    /// The handle pins the actual credentials, key, revision and size. Its own
    /// configuration fingerprint is used, never an independently supplied one.
    pub fn from_s3(&self, handle: ReadHandle) -> Result<Editor> {
        let meta = handle.metadata();
        if meta.size > i64::MAX as u64 {
            return Err(Error::new(None, "remote size exceeds local offsets"));
        }
        let target = Target::new(
            *handle.cache_identity().as_bytes(),
            handle.configuration_fingerprint(),
            &meta.key,
        )
        .map_err(|e| Error::new(None, e))?;
        let base = Base::Existing {
            etag: meta.revision.etag.clone(),
            version: meta.revision.version_id.clone(),
        };
        let size = meta.size;
        Ok(Editor {
            coordinator: self.clone(),
            origin: Origin::Remote(Arc::new(Remote {
                target,
                base,
                size,
                reader: Arc::new(S3Base(handle)),
            })),
        })
    }
}

/// A fixed input for edits. Use `Coordinator::local` on the returned generation
/// to edit again; existing editors/snapshots retain their original input.
#[derive(Clone)]
pub struct Editor {
    coordinator: Coordinator,
    origin: Origin,
}
impl Editor {
    pub fn size(&self) -> u64 {
        self.origin.size()
    }
    /// Plan a nonempty write, up to 8 MiB. Empty native writes must be handled
    /// by the future adapter without allocating a generation.
    pub fn write(&self, offset: u64, bytes: Arc<[u8]>) -> Result<EditPlan> {
        if bytes.is_empty() || bytes.len() > journal::MAX_READ_BYTES {
            return Err(Error::new(None, "invalid edit payload length"));
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|end| *end <= i64::MAX as u64)
            .ok_or_else(|| Error::new(None, "invalid edit offset"))?;
        Ok(self.plan(Change::Write { offset, bytes }, self.origin.size().max(end)))
    }
    pub fn truncate(&self, size: u64) -> Result<EditPlan> {
        if size > i64::MAX as u64 {
            return Err(Error::new(None, "invalid edit size"));
        }
        Ok(self.plan(Change::Truncate, size))
    }
    fn plan(&self, change: Change, size: u64) -> EditPlan {
        let key = operation_key(&self.origin, &change, size);
        EditPlan {
            coordinator: self.coordinator.clone(),
            origin: self.origin.clone(),
            change,
            size,
            key,
        }
    }
}
#[derive(Clone)]
enum Change {
    Write { offset: u64, bytes: Arc<[u8]> },
    Truncate,
}

/// Immutable desired edit. Retain or reconstruct this exact plan on retry.
/// The key and builder mapping are durable; the complete plan/patch input is
/// caller-owned until publication. Dropping it never discards staged bytes.
#[derive(Clone)]
pub struct EditPlan {
    coordinator: Coordinator,
    origin: Origin,
    change: Change,
    size: u64,
    key: [u8; 32],
}
impl EditPlan {
    /// Opaque local idempotency key. Not a remote operation ID or authorization.
    pub fn operation_key(&self) -> [u8; 32] {
        self.key
    }

    pub async fn execute(&self) -> Result<Snapshot> {
        let _slot = self
            .coordinator
            .0
            .slots
            .try_acquire()
            .map_err(|_| Error::new(None, "local edit concurrency limit reached"))?;
        let staging = &self.coordinator.0.staging;
        let id = staging
            .begin_operation(
                self.key,
                self.origin.target().clone(),
                self.origin.base(),
                self.origin.parent(),
                self.size,
            )
            .await
            .map_err(|e| Error::new(None, e))?;
        let result = self.fill(id).await;
        if let Err(error) = result {
            // An identical concurrent execution may already have completed.
            // The operation-bound sealed manifest must validate before success.
            if !staging
                .info(id)
                .await
                .is_ok_and(|info| info.phase == Phase::Sealed)
            {
                return Err(Error::new(Some(id), error));
            }
        }
        staging
            .seal(id)
            .await
            .map_err(|e| Error::new(Some(id), e))?;
        staging
            .snapshot(id)
            .await
            .map_err(|e| Error::new(Some(id), e))
    }
    async fn fill(&self, id: GenerationId) -> LocalResult<()> {
        let staging = &self.coordinator.0.staging;
        if staging.info(id).await?.phase == Phase::Sealed {
            return Ok(());
        }
        match &self.change {
            Change::Truncate => {
                let copy = self.origin.size().min(self.size);
                self.copy(id, 0, 0, copy).await?;
                self.zero(id, copy, self.size - copy).await?;
            }
            Change::Write { offset, bytes } => {
                let prefix = (*offset).min(self.origin.size());
                self.copy(id, 0, 0, prefix).await?;
                self.zero(id, prefix, *offset - prefix).await?;
                for (i, chunk) in bytes.chunks(journal::CHUNK_BYTES).enumerate() {
                    staging
                        .append_at(
                            id,
                            *offset + (i * journal::CHUNK_BYTES) as u64,
                            Arc::from(chunk),
                        )
                        .await?;
                }
                let end = *offset + bytes.len() as u64;
                if end < self.origin.size() {
                    self.copy(id, end, end, self.origin.size() - end).await?;
                }
            }
        }
        Ok(())
    }
    async fn zero(&self, id: GenerationId, offset: u64, length: u64) -> LocalResult<()> {
        if length > 0 {
            self.coordinator
                .0
                .staging
                .append_zero_at(id, offset, length)
                .await?;
        }
        Ok(())
    }
    async fn copy(
        &self,
        id: GenerationId,
        mut from: u64,
        mut to: u64,
        mut length: u64,
    ) -> LocalResult<()> {
        let staging = &self.coordinator.0.staging;
        while length > 0 {
            let copied = match &self.origin {
                Origin::Empty(_) => return Err("empty input has no inherited bytes".into()),
                Origin::Local(snapshot) => {
                    staging
                        .copy_next_at(
                            id,
                            to,
                            CopyRange {
                                generation: snapshot.metadata().id,
                                manifest_hash: snapshot
                                    .metadata()
                                    .manifest_hash
                                    .ok_or("missing input manifest")?,
                                offset: from,
                                length,
                            },
                        )
                        .await?
                }
                Origin::Remote(remote) => {
                    let chunk = length.min(journal::CHUNK_BYTES as u64) as usize;
                    let bytes = tokio::time::timeout(
                        self.coordinator.0.read_deadline,
                        remote.reader.read(from, chunk),
                    )
                    .await
                    .map_err(|_| "pinned base read timed out")??;
                    if bytes.len() != chunk {
                        return Err("pinned base returned an invalid range length".into());
                    }
                    staging.append_at(id, to, Arc::from(bytes)).await?;
                    chunk as u64
                }
            };
            if copied == 0 || copied > length {
                return Err("invalid local copy progress".into());
            }
            from += copied;
            to += copied;
            length -= copied;
        }
        Ok(())
    }
}

fn field(hash: &mut Sha256, value: Option<&[u8]>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
}
fn operation_key(origin: &Origin, change: &Change, size: u64) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"s3drive-local-edit-v1\0");
    hash.update(origin.target().source());
    field(&mut hash, Some(origin.target().fingerprint().as_bytes()));
    field(&mut hash, Some(origin.target().object_key().as_bytes()));
    hash.update(origin.size().to_be_bytes());
    hash.update(size.to_be_bytes());
    match origin.base() {
        Base::Missing => hash.update([0]),
        Base::Existing { etag, version } => {
            hash.update([1]);
            field(&mut hash, etag.as_deref().map(str::as_bytes));
            field(&mut hash, version.as_deref().map(str::as_bytes));
        }
    }
    match origin {
        Origin::Empty(_) => hash.update([0]),
        Origin::Local(snapshot) => {
            hash.update([1]);
            hash.update(
                snapshot
                    .metadata()
                    .manifest_hash
                    .expect("validated sealed snapshot"),
            );
        }
        Origin::Remote(_) => hash.update([2]),
    }
    match change {
        Change::Truncate => hash.update([0]),
        Change::Write { offset, bytes } => {
            hash.update([1]);
            hash.update(offset.to_be_bytes());
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(Sha256::digest(bytes));
        }
    }
    hash.finalize().into()
}

#[cfg(test)]
mod tests;
