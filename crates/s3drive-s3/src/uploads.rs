//! Journal-driven single-PUT execution and observation.
//!
//! This type deliberately has no production constructor yet. Local SDK/TCP
//! tests exercise the complete executor, but provider acceptance and an explicit
//! application write policy must precede enabling construction. Existing
//! configurations and mounts remain read-only. A probe report is not a token
//! that can construct this executor.
use crate::{
    ErrorKind, ObjectRevision,
    transport::{self, Condition, Intent, Observation, Outcome, Session},
};
use s3drive_state::{
    journal::{GenerationId, Target},
    remote::{self, Guard, Info, OperationId, Phase, Reason, SendPermit},
    worker::Storage,
};
use sha2::{Digest, Sha256};
use std::{error::Error as StdError, fmt, sync::Arc};
use tokio::sync::Semaphore;

pub type Result<T> = std::result::Result<T, Error>;

/// Context survives even when a local reply or network result was lost. Inspect
/// this generation/operation; an error never grants permission for another PUT.
#[derive(Debug)]
pub struct Error {
    pub generation: GenerationId,
    pub operation: Option<OperationId>,
    cause: Box<dyn StdError + Send + Sync>,
}
impl Error {
    fn new(
        generation: GenerationId,
        operation: Option<OperationId>,
        cause: impl Into<Box<dyn StdError + Send + Sync>>,
    ) -> Self {
        Self {
            generation,
            operation,
            cause: cause.into(),
        }
    }
    pub fn cause(&self) -> &(dyn StdError + Send + Sync + 'static) {
        self.cause.as_ref()
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "upload journal operation: {}", self.cause)
    }
}
impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.cause.as_ref())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Busy,
    BindingMismatch,
    MissingIntent,
    Integrity,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "upload execution slots are full; request was not admitted",
            Self::BindingMismatch => "upload runtime does not match the recorded target binding",
            Self::MissingIntent => "no remote intent exists for this generation",
            Self::Integrity => {
                "upload payload or receipt integrity check failed; local data was preserved"
            }
        })
    }
}
impl StdError for Failure {}

struct Shared {
    session: Session,
    storage: Storage,
    slots: Semaphore,
}

/// One pinned credential generation, local storage owner and four bounded
/// execution slots. Exported for its contract, but construction remains private
/// until provider acceptance and application write-policy integration.
#[derive(Clone)]
pub struct Executor(Arc<Shared>);
impl Executor {
    /// Prepare/claim at most once, then observe. Only `Phase::Verified` is a
    /// verified historical remote receipt. Rejected/uncertain are not success.
    /// Repeated calls on a claimed intent only observe; they never send again.
    pub async fn submit(&self, generation: GenerationId) -> Result<Info> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| Error::new(generation, None, Failure::Busy))?;
        let selected = self
            .0
            .storage
            .staging()
            .info(generation)
            .await
            .map_err(|e| Error::new(generation, None, e))?;
        self.target_binding(generation, None, &selected.target)?;
        let info = self
            .0
            .storage
            .uploads()
            .prepare(generation)
            .await
            .map_err(|e| Error::new(generation, None, e))?;
        self.binding(&info)?;
        match info.phase {
            Phase::Verified | Phase::Rejected | Phase::Cancelled => Ok(info),
            Phase::InFlight | Phase::Uncertain => {
                let intent = self.intent(&info).await?;
                self.observe(&info, &intent, None).await
            }
            Phase::Prepared => {
                // Materialize and check bounded local bytes before issuing the
                // claim. No network operation occurs on the SQLite owner.
                let intent = self.intent(&info).await?;
                let permit = self
                    .0
                    .storage
                    .uploads()
                    .claim(info.operation, info.target.clone())
                    .await
                    .map_err(|e| Error::new(generation, Some(info.operation), e))?;
                Box::pin(self.send_once(permit, intent)).await
            }
        }
    }

    /// Observation only: even a prepared record never causes a PUT here.
    /// Absent/other observations retain uncertainty and all local bytes.
    pub async fn reconcile(&self, generation: GenerationId) -> Result<Info> {
        let _slot = self
            .0
            .slots
            .try_acquire()
            .map_err(|_| Error::new(generation, None, Failure::Busy))?;
        let info = self
            .0
            .storage
            .uploads()
            .for_generation(generation)
            .await
            .map_err(|e| Error::new(generation, None, e))?
            .ok_or_else(|| Error::new(generation, None, Failure::MissingIntent))?;
        self.binding(&info)?;
        if !matches!(info.phase, Phase::InFlight | Phase::Uncertain) {
            return Ok(info);
        }
        let intent = self.intent(&info).await?;
        self.observe(&info, &intent, None).await
    }

    fn binding(&self, info: &Info) -> Result<()> {
        self.target_binding(info.generation, Some(info.operation), &info.target)
    }
    fn target_binding(
        &self,
        generation: GenerationId,
        operation: Option<OperationId>,
        target: &Target,
    ) -> Result<()> {
        let reader = &self.0.session.reader;
        if target.source() != &self.0.session.binding_id
            || target.fingerprint() != reader.configuration_fingerprint()
        {
            return Err(Error::new(generation, operation, Failure::BindingMismatch));
        }
        reader
            .full_key(target.object_key())
            .map_err(|e| Error::new(generation, operation, e))?;
        Ok(())
    }
    async fn intent(&self, info: &Info) -> Result<Intent> {
        self.binding(info)?;
        if info.size > transport::MAX_PUT as u64 {
            return Err(Error::new(
                info.generation,
                Some(info.operation),
                Failure::Integrity,
            ));
        }
        let bytes = self
            .0
            .storage
            .staging()
            .read(info.generation, 0, info.size as usize)
            .await
            .map_err(|e| Error::new(info.generation, Some(info.operation), e))?;
        if bytes.len() as u64 != info.size
            || Sha256::digest(&bytes).as_slice() != info.payload_sha256
        {
            return Err(Error::new(
                info.generation,
                Some(info.operation),
                Failure::Integrity,
            ));
        }
        let intent = Intent {
            key: info.target.object_key().into(),
            bytes: Arc::from(bytes),
            operation: transport::hex(info.operation.as_bytes()),
            checksum: transport::hex(&info.payload_sha256),
            condition: match &info.guard {
                Guard::Missing => Condition::Missing,
                Guard::Match(tag) => Condition::Match(tag.clone()),
            },
        };
        intent
            .validate()
            .map_err(|e| Error::new(info.generation, Some(info.operation), e))?;
        Ok(intent)
    }
    async fn send_once(&self, permit: SendPermit, intent: Intent) -> Result<Info> {
        let info = permit.intent().clone();
        // Only this private function consumes send permission. It cannot take
        // an Info/operation ID in place of the single-use permit.
        self.binding(&info)?;
        let condition_matches = match (&info.guard, &intent.condition) {
            (Guard::Missing, Condition::Missing) => true,
            (Guard::Match(a), Condition::Match(b)) => a == b,
            _ => false,
        };
        if intent.operation != transport::hex(info.operation.as_bytes())
            || intent.key != info.target.object_key()
            || intent.bytes.len() as u64 != info.size
            || intent.checksum != transport::hex(&info.payload_sha256)
            || !condition_matches
        {
            return Err(Error::new(
                info.generation,
                Some(info.operation),
                Failure::Integrity,
            ));
        }
        let outcome = self.0.session.put(&intent).await;
        drop(permit);
        match outcome {
            Ok(Outcome::Conflict(error)) => {
                let Some(status @ (409 | 412)) = error.http_status else {
                    return Err(Error::new(
                        info.generation,
                        Some(info.operation),
                        Failure::Integrity,
                    ));
                };
                self.0
                    .storage
                    .uploads()
                    .rejected(info.operation, status)
                    .await
                    .map_err(|e| Error::new(info.generation, Some(info.operation), e))
            }
            Ok(Outcome::Applied(revision)) => self.observe(&info, &intent, Some(&revision)).await,
            Ok(Outcome::Unknown(error)) => {
                self.uncertain(&info, reason(error.kind)).await?;
                self.observe(&info, &intent, None).await
            }
            Err(error) => {
                self.uncertain(&info, reason(error.kind)).await?;
                Err(Error::new(info.generation, Some(info.operation), error))
            }
        }
    }
    async fn uncertain(&self, info: &Info, reason: Reason) -> Result<Info> {
        self.0
            .storage
            .uploads()
            .uncertain(info.operation, reason)
            .await
            .map_err(|e| Error::new(info.generation, Some(info.operation), e))
    }
    async fn observe(
        &self,
        info: &Info,
        intent: &Intent,
        expected: Option<&ObjectRevision>,
    ) -> Result<Info> {
        match self.0.session.observe(intent).await {
            Ok(Observation::Matched(revision)) => {
                if expected.is_some_and(|expected| *expected != revision) {
                    self.uncertain(info, Reason::Observation).await?;
                    return Err(Error::new(
                        info.generation,
                        Some(info.operation),
                        Failure::Integrity,
                    ));
                }
                let revision = remote::Revision::new(
                    revision.etag.as_deref().ok_or_else(|| {
                        Error::new(info.generation, Some(info.operation), Failure::Integrity)
                    })?,
                    revision.version_id.as_deref(),
                )
                .map_err(|e| Error::new(info.generation, Some(info.operation), e))?;
                self.0
                    .storage
                    .uploads()
                    .verified(remote::Verification {
                        operation: info.operation,
                        target: info.target.clone(),
                        payload_sha256: info.payload_sha256,
                        size: info.size,
                        revision,
                    })
                    .await
                    .map_err(|e| Error::new(info.generation, Some(info.operation), e))
            }
            Ok(observation @ (Observation::Absent | Observation::Other)) => {
                let observation = match observation {
                    Observation::Absent => remote::Observation::Absent,
                    _ => remote::Observation::Other,
                };
                self.0
                    .storage
                    .uploads()
                    .observe_unmatched(info.operation, observation)
                    .await
                    .map_err(|e| Error::new(info.generation, Some(info.operation), e))
            }
            Err(error) => {
                self.uncertain(info, reason(error.kind)).await?;
                Err(Error::new(info.generation, Some(info.operation), error))
            }
        }
    }
}
fn reason(kind: ErrorKind) -> Reason {
    match kind {
        ErrorKind::Timeout => Reason::Timeout,
        ErrorKind::Transport => Reason::Transport,
        _ => Reason::Response,
    }
}

#[cfg(test)]
mod tests;
