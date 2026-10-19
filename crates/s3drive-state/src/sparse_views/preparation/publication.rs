//! Durable mixed-root requests and historical semantic coverage. No S3 rights,
//! source authentication, payload reclamation or mounted-file flush capability.
use super::{Binding, Id, Info, OriginalChunk, RootDescriptor, digest};
use crate::{Result, State, journal::GenerationId, namespace as ns};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
mod model;
use model::Check;
pub use model::{Input, Physical, Record, Rejection};
mod journal;
mod ledger;
mod lineage;
pub use journal::{Client, DataNeed, Journal};
pub(in crate::sparse_views) mod settlement;
pub use settlement::{Acknowledgment, SourceReceipt};
mod recovery;
pub use recovery::{RecoveryClient, RecoveryStore};
const MAX_RECORDS: usize = 256;
const MAX_CHECKS: usize = 262144;
const MAX_METADATA: u64 = 32 * 1024 * 1024;
const MAX_RECORD: usize = 65536;
const TABLES: [&str; 3] = [
    "sparse_publication_role",
    "sparse_publications",
    "sparse_publication_checks",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Conflict,
    Corrupt,
    Missing,
    Quota,
    Incomplete,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mixed root publication {:?}; exact requests and dirty data retained",
            self
        )
    }
}
impl std::error::Error for Failure {}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub plans: usize,
    pub checks: usize,
    pub metadata_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            plans: 16,
            checks: 32768,
            metadata_bytes: 8 * 1024 * 1024,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self> {
        if self.plans == 0
            || self.plans > MAX_RECORDS
            || self.checks == 0
            || self.checks > MAX_CHECKS
            || self.metadata_bytes == 0
            || self.metadata_bytes > MAX_METADATA
        {
            return Err(Failure::Invalid.into());
        }
        Ok(self)
    }
}
fn hash<T: Serialize>(domain: &[u8], value: &T) -> Result<Id> {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(serde_json::to_vec(value)?);
    Ok(h.finalize().into())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn id(s: &str) -> bool {
    s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && s != "."
        && s != ".."
        && !s.chars().any(|c| c == '/' || c == '\\' || c.is_control())
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare(
        "SELECT singleton,admitted,checked,state_hash FROM sparse_publication_role LIMIT 0",
    )?;
    db.prepare(
        "SELECT plan_id,reserve_id,publish_id,record,state_hash FROM sparse_publications LIMIT 0",
    )?;
    db.prepare("SELECT plan_id,slot,record,state_hash FROM sparse_publication_checks LIMIT 0")?;
    Ok(())
}
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    for t in TABLES {
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {t})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Conflict.into());
        }
    }
    Ok(())
}
pub(crate) fn require_pre35(db: &Connection) -> Result<()> {
    if ledger::inspect(db)?
        .records
        .values()
        .any(|r| r.acknowledgment.is_some())
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
pub(crate) fn validate_all(db: &Connection) -> Result<()> {
    let meta = ledger::inspect(db)?;
    if meta.empty() {
        return Ok(());
    }
    ledger::validate_source(db, &meta)?;
    Ok(())
}
#[cfg(test)]
mod tests;
#[cfg(not(test))]
fn checkpoint(_kind: &str, _point: &str) {}
#[cfg(test)]
fn checkpoint(kind: &str, point: &str) {
    if std::env::var("S3DRIVE_MIXED_ROOT_CRASH_KIND").as_deref() != Ok(kind)
        || std::env::var("S3DRIVE_MIXED_ROOT_CRASH_POINT").as_deref() != Ok(point)
    {
        return;
    }
    let Some(path) = std::env::var_os("S3DRIVE_MIXED_ROOT_CRASH_READY") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, b"ready").unwrap();
    std::fs::rename(temp, path).unwrap();
    use std::io::Read;
    let _ = std::io::stdin().read(&mut [0u8]);
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct AdoptionFence(ledger::Meta);
/// Full offline preparation/physical/publication validation; never remote auth.
pub(super) fn adoption_fence(
    db: &Connection,
    source: &super::ledger::Snapshot,
    target: Id,
) -> Result<AdoptionFence> {
    let meta = ledger::inspect(db)?;
    ledger::validate_source(db, &meta)?;
    for (plan, saved) in &source.plans {
        let view = source.views.record(saved.input.view)?;
        if view.request.binding.target()? == target
            && meta
                .records
                .get(plan)
                .and_then(|r| r.acknowledgment.as_ref())
                .is_none()
        {
            return Err(super::super::Failure::Conflict.into());
        }
    }
    Ok(AdoptionFence(meta))
}
