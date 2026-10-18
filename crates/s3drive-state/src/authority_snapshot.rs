//! Experimental private offline snapshots. Integrity/identity checks never
//! authorize restoring, serving an older history, or replacing a live volume.
use crate::namespace::Volume;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

#[derive(Clone)]
pub struct Limits {
    pub max_bytes: u64,
    pub timeout: Duration,
    pub cancelled: Arc<AtomicBool>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 512 * 1024 * 1024,
            timeout: Duration::from_secs(30),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub version: u8,
    pub application_id: i32,
    pub schema_version: i32,
    pub volume: Volume,
    pub database: String,
    pub byte_length: u64,
    pub sha256: String,
    pub validation: String,
    pub created_unix_ms: u64,
    pub writer_version: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inspection {
    pub manifest: Manifest,
    pub history_continuity_proven: bool,
    pub restore_authorized: bool,
}
/// Fixed categories only: never carries a path, SQL error, manifest source or
/// private database contents. CompletionUnknown requires inspecting this output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidLimits,
    Source,
    Destination,
    InvalidArtifact,
    Copy,
    Validation,
    Limit,
    Cancelled,
    Deadline,
    CompletionUnknown,
    Unsupported,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "authority snapshot failed ({self:?}); preserve source and inspect any partial output; restoration is not authorized"
        )
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(any(target_os = "linux", windows))]
mod native;
#[cfg(any(target_os = "linux", windows))]
pub use native::{create, inspect};
#[cfg(not(any(target_os = "linux", windows)))]
pub fn create(_: &Path, _: &str, _: &str, _: &Path, _: &Limits) -> Result<Manifest> {
    Err(Error::Unsupported)
}
#[cfg(not(any(target_os = "linux", windows)))]
pub fn inspect(_: &Path, _: &str, _: &str, _: &Limits) -> Result<Inspection> {
    Err(Error::Unsupported)
}
#[cfg(not(any(target_os = "linux", windows)))]
use std::path::Path;
