//! S3 reads, explicitly scoped conformance probes and a policy/evidence-bound
//! library upload executor. Existing configurations and mounts are read-only.

mod error;
pub mod probe;
mod reader;
mod transport;
pub mod uploads;

pub use error::{Error, ErrorKind, Result};
pub use reader::{
    CacheIdentity, ListPage, ListedObject, MAX_READ_BYTES, ObjectMetadata, ReadHandle, S3Reader,
};

/// Unknown is distinct from unsupported and must never enable a mutation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Support {
    #[default]
    Unknown,
    Unsupported,
    Verified,
}

/// Evidence must eventually be bound to an endpoint, bucket and probe version.
/// A value here is not sufficient to enable writes; see the implementation plan.
#[derive(Debug, Default)]
pub struct Capabilities {
    pub conditional_create: Support,
    pub conditional_replace: Support,
    pub conditional_multipart_complete: Support,
    pub conditional_delete: Support,
    pub versioned_reads: Support,
    pub atomic_rename: Support,
}

/// Opaque validators, never assumed to be content hashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRevision {
    pub etag: Option<String>,
    pub version_id: Option<String>,
}

pub const IMPLEMENTATION_STATUS: &str = "S3 reads, scoped write probe and explicit policy/evidence-bound single-PUT executor; no ordinary CLI upload command";
