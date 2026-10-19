//! Private immutable initial views and exact local-operation replay. No network,
//! patch writes, clean settlement, authority capability or native activation.
use crate::{Result, State, worker::Owner};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
mod model;
pub use model::{Binding, MAX_FILE_BYTES, MAX_RANGE_BYTES, Range, View};
use model::{Record, Request};
pub type Id = [u8; 32];
const HARD_FILES: u32 = 256;
const BINDING_BYTES: usize = 4096;
const RECORD_BYTES: usize = 1_048_576;
const TABLES: [&str; 4] = [
    "sparse_view_role",
    "sparse_view_files",
    "sparse_views",
    "sparse_view_operations",
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
#[derive(Clone, Copy, Debug)]
pub struct Options {
    max_files: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self { max_files: 64 }
    }
}
impl Options {
    pub fn new(max_files: u32) -> Result<Self> {
        if !(1..=HARD_FILES).contains(&max_files) {
            return Err(Failure::Invalid.into());
        }
        Ok(Self { max_files })
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
    for table in TABLES {
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Isolated.into());
        }
    }
    Ok(())
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT singleton,format,state_hash FROM sparse_view_role LIMIT 0")?;
    db.prepare("SELECT target,binding,head,state_hash FROM sparse_view_files LIMIT 0")?;
    db.prepare("SELECT view_id,target,record,state_hash FROM sparse_views LIMIT 0")?;
    db.prepare("SELECT operation,view_id,request,state_hash FROM sparse_view_operations LIMIT 0")?;
    Ok(())
}
/// Caller holds a single read/write transaction. Count and length prechecks run
/// in that same snapshot before any untrusted BLOB is copied into a Rust Vec.
fn inspect(db: &Connection) -> Result<Vec<Record>> {
    let mut counts = [0u32; 4];
    for (i, table) in TABLES.iter().enumerate() {
        counts[i] = db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
    }
    if counts == [0; 4] {
        return Ok(Vec::new());
    }
    if counts[0] != 1
        || counts[1] == 0
        || counts[1] > HARD_FILES
        || counts[1] != counts[2]
        || counts[1] != counts[3]
    {
        return Err(Failure::Corrupt.into());
    }
    isolation(db)?;
    let bad:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sparse_view_role WHERE typeof(singleton)!='integer' OR singleton!=1 OR typeof(format)!='integer' OR format!=1 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",[],|r|r.get(0))?;
    if bad {
        return Err(Failure::Corrupt.into());
    }
    let role: Vec<u8> = db.query_row(
        "SELECT state_hash FROM sparse_view_role WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if role != role_hash()? {
        return Err(Failure::Corrupt.into());
    }
    for sql in [
        "SELECT EXISTS(SELECT 1 FROM sparse_view_files WHERE typeof(target)!='blob' OR length(target)!=32 OR typeof(binding)!='blob' OR length(binding) NOT BETWEEN 1 AND 4096 OR typeof(head)!='blob' OR length(head)!=32 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_views WHERE typeof(view_id)!='blob' OR length(view_id)!=32 OR typeof(target)!='blob' OR length(target)!=32 OR typeof(record)!='blob' OR length(record) NOT BETWEEN 1 AND 1048576 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_view_operations WHERE typeof(operation)!='blob' OR length(operation)!=32 OR typeof(view_id)!='blob' OR length(view_id)!=32 OR typeof(request)!='blob' OR length(request) NOT BETWEEN 1 AND 4096 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
    ] {
        if db.query_row(sql, [], |r| r.get::<_, bool>(0))? {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut statement =
        db.prepare("SELECT target,binding,head,state_hash FROM sparse_view_files ORDER BY target")?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (target, raw, head, binding_hash) = row?;
        let binding: Binding = decode(&raw)?;
        binding.validate().map_err(|_| Failure::Corrupt)?;
        if target != binding.target()?
            || binding_hash != hash(b"s3drive-sparse-binding-v1\0", &binding)?
        {
            return Err(Failure::Corrupt.into());
        }
        let (actual_target, raw, record_hash): (Vec<u8>, Vec<u8>, Vec<u8>) = db
            .query_row(
                "SELECT target,record,state_hash FROM sparse_views WHERE view_id=?1",
                [&head],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|_| Failure::Corrupt)?;
        let record: Record = decode(&raw)?;
        record.validate()?;
        if actual_target != target
            || record.id.as_slice() != head
            || record.request.binding != binding
            || record_hash != hash(b"s3drive-sparse-view-v1\0", &record)?
        {
            return Err(Failure::Corrupt.into());
        }
        let (operation, raw, request_hash): (Vec<u8>, Vec<u8>, Vec<u8>) = db
            .query_row(
                "SELECT operation,request,state_hash FROM sparse_view_operations WHERE view_id=?1",
                [&head],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|_| Failure::Corrupt)?;
        let request: Request = decode(&raw)?;
        if operation != record.operation
            || request != record.request
            || request_hash
                != hash(
                    b"s3drive-sparse-request-v1\0",
                    &(record.operation, &request),
                )?
        {
            return Err(Failure::Corrupt.into());
        }
        out.push(record);
    }
    Ok(out)
}
pub(crate) fn validate_all(db: &Connection) -> Result<()> {
    inspect(db).map(|_| ())
}

pub struct Journal<'a> {
    state: &'a mut State,
    options: Options,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, options: Options) -> Result<Self> {
        Options::new(options.max_files)?;
        state.verify_staging_storage()?;
        let tx = state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        inspect(&tx)?;
        tx.commit()?;
        Ok(Self { state, options })
    }
    /// Atomically persist one immutable initial local view and its exact result.
    /// Replays bypass newly narrowed admission quotas, but never binding checks.
    pub fn begin(&mut self, operation: Id, binding: Binding, size: u64) -> Result<View> {
        let request = Request {
            format: 1,
            binding,
            size,
        };
        let record = Record::new(operation, request)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        isolation(&tx)?;
        let before = inspect(&tx)?;
        if let Some(old) = before.iter().find(|r| r.operation == operation) {
            if old.request != record.request {
                return Err(Failure::Conflict.into());
            }
            let result = View(old.clone());
            tx.commit()?;
            return Ok(result);
        }
        let target = record.request.binding.target()?;
        for old in &before {
            if old.request.binding.target()? == target {
                return Err(Failure::Conflict.into());
            }
        }
        if before.len() >= self.options.max_files as usize {
            return Err(Failure::Quota.into());
        }
        let binding = &record.request.binding;
        let binding_bytes = encode(binding)?;
        let request_bytes = encode(&record.request)?;
        let record_bytes = encode(&record)?;
        if binding_bytes.len() > BINDING_BYTES
            || request_bytes.len() > BINDING_BYTES
            || record_bytes.len() > RECORD_BYTES
        {
            return Err(Failure::Bounds.into());
        }
        if before.is_empty()
            && tx.execute(
                "INSERT INTO sparse_view_role VALUES(1,1,?1)",
                [role_hash()?.as_slice()],
            )? != 1
        {
            return Err(Failure::Corrupt.into());
        }
        if tx.execute(
            "INSERT INTO sparse_view_files VALUES(?1,?2,?3,?4)",
            params![
                target.as_slice(),
                binding_bytes,
                record.id.as_slice(),
                hash(b"s3drive-sparse-binding-v1\0", binding)?.as_slice()
            ],
        )? != 1
            || tx.execute(
                "INSERT INTO sparse_views VALUES(?1,?2,?3,?4)",
                params![
                    record.id.as_slice(),
                    target.as_slice(),
                    record_bytes,
                    hash(b"s3drive-sparse-view-v1\0", &record)?.as_slice()
                ],
            )? != 1
            || tx.execute(
                "INSERT INTO sparse_view_operations VALUES(?1,?2,?3,?4)",
                params![
                    operation.as_slice(),
                    record.id.as_slice(),
                    request_bytes,
                    hash(
                        b"s3drive-sparse-request-v1\0",
                        &(operation, &record.request)
                    )?
                    .as_slice()
                ],
            )? != 1
        {
            return Err(Failure::Corrupt.into());
        }
        let mut expected = before
            .into_iter()
            .map(|r| Ok((r.request.binding.target()?, r)))
            .collect::<Result<Vec<_>>>()?;
        expected.push((target, record.clone()));
        expected.sort_by_key(|(target, _)| *target);
        let expected = expected.into_iter().map(|(_, r)| r).collect::<Vec<_>>();
        if inspect(&tx)? != expected {
            return Err(Failure::Corrupt.into());
        }
        tx.commit()?;
        Ok(View(record))
    }
    pub fn view(&mut self, id: Id) -> Result<View> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        isolation(&tx)?;
        let record = inspect(&tx)?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Failure::NotFound)?;
        tx.commit()?;
        Ok(View(record))
    }
}
#[derive(Clone)]
pub struct Client {
    owner: Owner,
    options: Options,
}
impl Client {
    pub(crate) fn new(owner: Owner, options: Options) -> Result<Self> {
        Options::new(options.max_files)?;
        Ok(Self { owner, options })
    }
    pub async fn begin(&self, operation: Id, binding: Binding, size: u64) -> Result<View> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.begin(operation, binding, size))
            .await
    }
    pub async fn view(&self, id: Id) -> Result<View> {
        let options = self.options;
        self.owner
            .call(move |s| Journal::new(&mut s.state, options)?.view(id))
            .await
    }
}
#[cfg(test)]
mod tests;
