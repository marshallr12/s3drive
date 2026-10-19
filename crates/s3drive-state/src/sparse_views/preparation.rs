//! Durable mixed-tree preparation intent. No authority capability, upload/source
//! receipt, provider send permission, clean settlement or payload reclamation.
use super::*;
use s3drive_core::manifest::{self, Entry, Page, Reference, VerifiedPage, objects::RootDescriptor};
use std::collections::{BTreeMap, BTreeSet};
mod model;
pub use model::{Candidate, Cursor, Info, Options, Request};
use model::{Input, Kind, Object};
mod journal;
mod ledger;
mod proof;
pub mod publication;
mod semantics;
pub use semantics::OriginalChunk;
mod transfers;
pub use journal::{Client, Dependency, Journal};
pub use transfers::{Position, Transfer, TransferFailure, TreeReceipt};
const MAX_PLANS: usize = 256;
const MAX_OBJECTS: usize = 262_144;
const MAX_METADATA: u64 = 32 * 1024 * 1024;
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RECORD: usize = 65_536;
const TABLES: [&str; 3] = [
    "sparse_preparations",
    "sparse_preparation_objects",
    "sparse_preparation_blobs",
];
fn integer(n: u64) -> Result<i64> {
    i64::try_from(n).map_err(|_| Failure::Bounds.into())
}
fn number(row: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<u64> {
    let n: i64 = row.get(at)?;
    u64::try_from(n).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(at, n))
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare(
        "SELECT plan_id,operation,view_id,input,state,state_hash FROM sparse_preparations LIMIT 0",
    )?;
    db.prepare(
        "SELECT plan_id,level,slot,record,state_hash FROM sparse_preparation_objects LIMIT 0",
    )?;
    db.prepare("SELECT blob,data FROM sparse_preparation_blobs LIMIT 0")?;
    Ok(())
}
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    publication::require_legacy(db)?;
    for table in TABLES {
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Isolated.into());
        }
    }
    Ok(())
}
pub(crate) fn validate_all(db: &Connection) -> Result<()> {
    ledger::inspect(db).map(|_| ())
}

#[cfg(test)]
mod tests;
