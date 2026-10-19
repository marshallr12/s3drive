//! Private immutable base/patch views and exact local-operation replay.
//! Exact acknowledgments live in preparation::publication. No network or native activation.
use crate::{Result, State, worker::Owner};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
mod model;
pub use model::{Binding, MAX_FILE_BYTES, MAX_RANGE_BYTES, Range, View};
use model::{BlockRef, Change, Record, Request};
mod ledger;
use ledger::{File, Saved, inspect};
mod mutations;
pub mod preparation;
pub use mutations::{Client, Journal, Piece};
pub type Id = [u8; 32];
const HARD_FILES: u32 = 256;
const HARD_VIEWS: u32 = 4096;
const HARD_EXTENTS: u32 = 32768;
const HARD_METADATA: u64 = 32 * 1024 * 1024;
const HARD_DIRTY: u64 = 256 * 1024 * 1024;
const HARD_BLOCKS: usize = 32768;
pub const PATCH_BYTES: usize = 1024 * 1024;
const MAX_VIEW_EXTENTS: usize = 4096;
fn digest(bytes: &[u8]) -> Id {
    Sha256::digest(bytes).into()
}
const BINDING_BYTES: usize = 4096;
const RECORD_BYTES: usize = 1_048_576;
const TABLES: [&str; 5] = [
    "sparse_view_role",
    "sparse_view_files",
    "sparse_views",
    "sparse_view_operations",
    "sparse_view_blocks",
];
const EXCLUDED: &[&str] = &[
    "managed_publications",
    "managed_rejections",
    "logical_files",
    "logical_edits",
    "logical_publication_links",
    "logical_conflicts",
    "namespace_intents",
    "client_identity",
    "retained_client_identity",
    "windows_open_journal",
    "retained_windows_opens",
    "windows_open_events",
    "retired_windows_opens",
    "windows_mount_sessions",
    "windows_session_recoveries",
    "recovered_windows_opens",
    "namespace_volume",
    "namespace_nodes",
    "namespace_operations",
    "namespace_stores",
    "namespace_uploads",
    "namespace_removals",
    "namespace_removal_events",
    "chunk_client_role",
    "chunk_publications",
    "chunk_source_role",
    "chunk_source_settlements",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Bounds,
    Conflict,
    Corrupt,
    Quota,
    Isolated,
    NotFound,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "local sparse-view {:?}; no remote authorization or clean acknowledgment",
            self
        )
    }
}
impl std::error::Error for Failure {}
/// Admission quotas only. Narrowing them never discards or hides old work.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub views: u32,
    pub operations: u32,
    pub extents: u32,
    pub metadata_bytes: u64,
    pub dirty_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            views: 512,
            operations: 512,
            extents: 8192,
            metadata_bytes: 8 * 1024 * 1024,
            dirty_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Options {
    max_files: u32,
    limits: Limits,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_files: 64,
            limits: Limits::default(),
        }
    }
}
impl Options {
    pub fn new(max_files: u32) -> Result<Self> {
        Self {
            max_files,
            ..Self::default()
        }
        .validate()
    }
    pub fn with_limits(self, limits: Limits) -> Result<Self> {
        Self { limits, ..self }.validate()
    }
    fn validate(self) -> Result<Self> {
        let l = self.limits;
        if !(1..=HARD_FILES).contains(&self.max_files)
            || !(1..=HARD_VIEWS).contains(&l.views)
            || !(1..=HARD_VIEWS).contains(&l.operations)
            || !(1..=HARD_EXTENTS).contains(&l.extents)
            || !(1..=HARD_METADATA).contains(&l.metadata_bytes)
            || !(1..=HARD_DIRTY).contains(&l.dirty_bytes)
        {
            return Err(Failure::Invalid.into());
        }
        Ok(self)
    }
}
fn encode<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|_| Failure::Corrupt.into())
}
fn decode<T: serde::de::DeserializeOwned>(raw: &[u8]) -> Result<T> {
    serde_json::from_slice(raw).map_err(|_| Failure::Corrupt.into())
}
fn hash<T: Serialize>(domain: &[u8], v: &T) -> Result<Id> {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(encode(v)?);
    Ok(h.finalize().into())
}
fn role_hash() -> Result<Id> {
    hash(b"s3drive-sparse-role-v1\0", &1u8)
}
fn isolation(db: &Connection) -> Result<()> {
    for table in EXCLUDED {
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Isolated.into());
        }
    }
    Ok(())
}
/// Constructor guard only; generic cache/staging remains available for future
/// mixed uploads. No old complete-snapshot publication may adopt these views.
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    preparation::require_legacy(db)?;
    for table in TABLES {
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Isolated.into());
        }
    }
    Ok(())
}
fn check_legacy_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT singleton,format,state_hash FROM sparse_view_role LIMIT 0")?;
    db.prepare("SELECT target,binding,head,state_hash FROM sparse_view_files LIMIT 0")?;
    db.prepare("SELECT view_id,target,record,state_hash FROM sparse_views LIMIT 0")?;
    db.prepare("SELECT operation,view_id,request,state_hash FROM sparse_view_operations LIMIT 0")?;
    Ok(())
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    check_legacy_schema(db)?;
    db.prepare("SELECT block,data FROM sparse_view_blocks LIMIT 0")?;
    Ok(())
}
/// Rebuild only after validating the old rows. Do not execute user-attached
/// DELETE/INSERT triggers while moving cyclic references through an empty state.
/// The caller owns the opening transaction; every failure rolls back the schema,
/// copied bytes and user_version together. Foreign-key enforcement stays on.
pub(crate) fn migrate_thirty(db: &Connection) -> Result<()> {
    let foreign_keys: bool = db.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    if !foreign_keys {
        return Err(Failure::Corrupt.into());
    }
    check_legacy_schema(db)?;
    let before = ledger::inspect_initial(db)?;
    let triggers: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name IN ('sparse_view_role','sparse_view_files','sparse_views','sparse_view_operations'))",
        [], |r| r.get(0),
    )?;
    if triggers {
        return Err(Failure::Corrupt.into());
    }
    db.execute_batch(include_str!("../migrations/030.sql"))?;
    for table in [
        "sparse_view_files",
        "sparse_views",
        "sparse_view_operations",
    ] {
        let bad: bool = db.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM pragma_foreign_key_check('{table}'))"),
            [],
            |r| r.get(0),
        )?;
        if bad {
            return Err(Failure::Corrupt.into());
        }
    }
    if ledger::inspect_initial(db)? != before {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}

pub(crate) fn validate_all(db: &Connection) -> Result<()> {
    inspect(db).map(|_| ())
}

#[cfg(test)]
mod tests;
