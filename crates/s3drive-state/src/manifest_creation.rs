//! Durable local empty-manifest create intent. No network or mounted-file rights.
use crate::{Result, State, journal, namespace as ns, remote};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt};
mod input;
pub use input::{CreationOpen, Input};
pub(crate) mod admission;
mod journal_impl;
mod recovery;
pub use journal_impl::{Client, Journal};
pub use recovery::{RecoveryClient, RecoveryStore};
const RECORD_BYTES: usize = 65536;
const HARD_RECORDS: usize = 128;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Conflict,
    Corrupt,
    Missing,
    Quota,
    Isolated,
    Incomplete,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "empty creation {:?}; exact intent and physical outcomes retained",
            self
        )
    }
}
impl std::error::Error for Failure {}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub records: usize,
    pub metadata_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            records: 16,
            metadata_bytes: 16 * RECORD_BYTES,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self> {
        if self.records == 0
            || self.records > HARD_RECORDS
            || !(RECORD_BYTES..=HARD_RECORDS * RECORD_BYTES).contains(&self.metadata_bytes)
        {
            return Err(Failure::Invalid.into());
        }
        Ok(self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    format: u8,
    input: Input,
    generation: Option<journal::GenerationId>,
    physical: Option<Physical>,
    request: Option<ns::Apply>,
    receipt: Option<ns::Receipt>,
    rejection: Option<ns::Error>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Physical {
    manifest: [u8; 32],
    payload: [u8; 32],
    operation: String,
    etag: String,
    version: Option<String>,
}
impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EmptyCreationRecord([private; no native flush])")
    }
}
impl Record {
    pub fn input(&self) -> &Input {
        &self.input
    }
    pub fn generation(&self) -> Option<journal::GenerationId> {
        self.generation
    }
    pub fn request(&self) -> Option<&ns::Apply> {
        self.request.as_ref()
    }
    pub fn receipt(&self) -> Option<&ns::Receipt> {
        self.receipt.as_ref()
    }
    pub fn rejection(&self) -> Option<ns::Error> {
        self.rejection
    }
    fn validate(&self) -> Result<()> {
        self.input.validate()?;
        if self.format != self.input.format()
            || (self.request.is_some() && self.generation.is_none())
            || self.physical.is_some() != self.request.is_some()
            || (self.receipt.is_some() && self.rejection.is_some())
        {
            return Err(Failure::Corrupt.into());
        }
        if let Some(request) = &self.request {
            let p = self.physical.as_ref().ok_or(Failure::Corrupt)?;
            if p.payload != <[u8; 32]>::from(Sha256::digest(self.input.body()?))
                || *request
                    != self
                        .input
                        .request(p.operation.clone(), p.etag.clone(), p.version.clone())?
            {
                return Err(Failure::Corrupt.into());
            }
            self.input.payload(request, 1)?;
        }
        if let Some(receipt) = &self.receipt {
            self.input
                .check_receipt(self.request.as_ref().ok_or(Failure::Corrupt)?, receipt)?;
        }
        if let Some(error) = self.rejection
            && (self.request.is_none() || !terminal(error))
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
fn terminal(e: ns::Error) -> bool {
    matches!(
        e,
        ns::Error::Conflict
            | ns::Error::AlreadyExists
            | ns::Error::NotFound
            | ns::Error::NotDirectory
    )
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn hash<T: Serialize>(domain: &[u8], v: &T) -> Result<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(serde_json::to_vec(v)?);
    Ok(h.finalize().into())
}
fn record_hash(r: &Record) -> Result<[u8; 32]> {
    hash(b"s3drive-empty-creation-record-v1\0", r)
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT singleton,records,state_hash FROM manifest_creation_role LIMIT 0")?;
    db.prepare("SELECT operation,record,state_hash FROM manifest_creations LIMIT 0")?;
    Ok(())
}
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    let used:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM manifest_creation_role) OR EXISTS(SELECT 1 FROM manifest_creations)",[],|r|r.get(0))?;
    if used {
        return Err(Failure::Isolated.into());
    }
    Ok(())
}
fn preflight(db: &Connection) -> Result<()> {
    let triggers: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
        [],
        |r| r.get(0),
    )?;
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let fk: bool = db.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    if triggers || !(40..=crate::SCHEMA_VERSION).contains(&version) || !fk {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn isolation(db: &Connection) -> Result<()> {
    isolation_in(db, None)
}
fn isolation_in(db: &Connection, scope: Option<&crate::chunk_mount::ReadScope<'_>>) -> Result<()> {
    let combined = match scope {
        Some(_) => true,
        None => crate::chunk_mount::compatible(db)?,
    };
    for table in [
        "namespace_volume",
        "namespace_nodes",
        "namespace_operations",
        "namespace_stores",
        "namespace_uploads",
        "namespace_removals",
        "namespace_removal_events",
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
        "chunk_client_role",
        "chunk_publications",
        "chunk_source_role",
        "chunk_source_settlements",
    ] {
        if combined && crate::chunk_mount::CLIENT_TABLES.contains(&table) {
            continue;
        }
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err(Failure::Isolated.into());
        }
    }
    Ok(())
}
fn projection(rows: &BTreeMap<String, Record>) -> Result<[u8; 32]> {
    let entries: Vec<_> = rows
        .iter()
        .map(|(id, r)| Ok((id, record_hash(r)?)))
        .collect::<Result<_>>()?;
    hash(b"s3drive-empty-creation-membership-v1\0", &entries)
}
/// Original acknowledged deferred creation, independently checked against its
/// retained staging/upload metadata. Payload body bytes are not read or repaired.
pub(crate) fn verified_creation(db: &Connection, operation: &str) -> Result<Record> {
    let r = inspect_with_witnesses(db, false)?
        .remove(operation)
        .ok_or(Failure::Missing)?;
    if !(2..=3).contains(&r.format) || r.receipt.is_none() || r.input.owner.is_none() {
        return Err(Failure::Incomplete.into());
    }
    check_witness(db, &r)?;
    Ok(r)
}
fn check_witness(db: &Connection, record: &Record) -> Result<()> {
    if record.input.open.is_some() {
        crate::namespace_client::opens::creation::check_anchor(db, &record.input)?;
    }
    if let Some(generation) = record.generation {
        if let Some(proof) = &record.physical {
            if journal_impl::verified_physical(db, &record.input, generation)? != *proof {
                return Err(Failure::Corrupt.into());
            }
        } else {
            journal_impl::check_staged(db, &record.input, generation, false)?;
        }
    }
    Ok(())
}
fn inspect(db: &Connection) -> Result<BTreeMap<String, Record>> {
    inspect_with_witnesses(db, true)
}
// Keep the complete role membership/hash/format projection for a targeted
// provenance read, then validate that exact creator's physical witness once.
fn inspect_with_witnesses(
    db: &Connection,
    all_witnesses: bool,
) -> Result<BTreeMap<String, Record>> {
    let mut rows = BTreeMap::new();
    let schema: i32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let mut q=db.prepare("SELECT CASE WHEN length(CAST(operation AS BLOB))=32 THEN operation END,CASE WHEN length(record) BETWEEN 1 AND 65536 THEN record END,CASE WHEN length(state_hash)=32 THEN state_hash END FROM manifest_creations ORDER BY operation LIMIT 129")?;
    for result in q.query_map([], |r| {
        Ok((
            r.get::<_, Option<String>>(0)?,
            r.get::<_, Option<Vec<u8>>>(1)?,
            r.get::<_, Option<Vec<u8>>>(2)?,
        ))
    })? {
        let (id, raw, digest) = result?;
        let id = id.ok_or(Failure::Corrupt)?;
        let raw = raw.ok_or(Failure::Corrupt)?;
        let digest = digest.ok_or(Failure::Corrupt)?;
        let record: Record = serde_json::from_slice(&raw).map_err(|_| Failure::Corrupt)?;
        record.validate()?;
        if record.format == 2 && schema < 43 {
            return Err(Failure::Corrupt.into());
        }
        if record.format == 3 && schema < 45 {
            return Err(Failure::Corrupt.into());
        }
        if all_witnesses {
            check_witness(db, &record)?;
        }
        if record.input.operation != id
            || digest != record_hash(&record)?
            || rows.insert(id, record).is_some()
            || rows.len() > HARD_RECORDS
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut q=db.prepare("SELECT singleton,records,CASE WHEN length(state_hash)=32 THEN state_hash END FROM manifest_creation_role LIMIT 2")?;
    let roles = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match roles.as_slice() {
        [] if rows.is_empty() => {}
        [(1, count, Some(digest))]
            if !rows.is_empty() && *count == rows.len() as i64 && *digest == projection(&rows)? => {
        }
        _ => return Err(Failure::Corrupt.into()),
    }
    Ok(rows)
}

pub(crate) fn validate_all(db: &Connection) -> Result<()> {
    let rows = inspect(db)?;
    if !rows.is_empty() {
        preflight(db)?;
        isolation(db)?;
    }
    Ok(())
}
fn save(
    db: &Connection,
    rows: &mut BTreeMap<String, Record>,
    r: Record,
    fresh: bool,
) -> Result<()> {
    write(db, rows, r, fresh)?;
    if inspect(db)? != *rows {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn write(
    db: &Connection,
    rows: &mut BTreeMap<String, Record>,
    r: Record,
    fresh: bool,
) -> Result<()> {
    r.validate()?;
    let raw = serde_json::to_vec(&r)?;
    if raw.len() > RECORD_BYTES {
        return Err(Failure::Corrupt.into());
    }
    let changed = if fresh {
        db.execute(
            "INSERT INTO manifest_creations(operation,record,state_hash) VALUES(?1,?2,?3)",
            params![r.input.operation, raw, record_hash(&r)?.as_slice()],
        )?
    } else {
        db.execute(
            "UPDATE manifest_creations SET record=?2,state_hash=?3 WHERE operation=?1",
            params![r.input.operation, raw, record_hash(&r)?.as_slice()],
        )?
    };
    if changed != 1 {
        return Err(Failure::Corrupt.into());
    }
    rows.insert(r.input.operation.clone(), r);
    if db.execute("INSERT INTO manifest_creation_role(singleton,records,state_hash) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET records=excluded.records,state_hash=excluded.state_hash",params![rows.len() as i64,projection(rows)?.as_slice()])?!=1 {return Err(Failure::Corrupt.into())}
    Ok(())
}

// Test children stop the real public transaction path; production is a no-op.
fn checkpoint(label: &str) -> Result<()> {
    #[cfg(test)]
    {
        use std::io::{Read, Write};
        if std::env::var("S3DRIVE_EMPTY_CREATION_STOP").ok().as_deref() == Some(label) {
            println!("S3DRIVE_EMPTY_CREATION_BOUNDARY {label}");
            std::io::stdout().flush()?;
            std::io::stdin().read_exact(&mut [0u8; 1])?;
        }
    }
    let _ = label;
    Ok(())
}

fn acknowledge_record(r: &mut Record, request: &ns::Apply, receipt: ns::Receipt) -> Result<()> {
    if r.request.as_ref() != Some(request) || r.rejection.is_some() {
        return Err(Failure::Conflict.into());
    }
    r.input.check_receipt(request, &receipt)?;
    if r.receipt.as_ref().is_some_and(|old| old != &receipt) {
        return Err(Failure::Conflict.into());
    }
    r.receipt = Some(receipt);
    Ok(())
}
fn reject_record(r: &mut Record, request: &ns::Apply, error: ns::Error) -> Result<()> {
    if !terminal(error)
        || r.request.as_ref() != Some(request)
        || r.receipt.is_some()
        || r.rejection.is_some_and(|old| old != error)
    {
        return Err(Failure::Conflict.into());
    }
    r.rejection = Some(error);
    Ok(())
}

pub(crate) fn chunk_scope(db: &Connection, b: &crate::chunk_mount::Binding) -> Result<()> {
    for r in inspect(db)?.into_values() {
        b.volume(&r.input.volume)?;
        b.control(&crate::namespace_client::Control {
            store: r.input.store,
            source: r.input.source,
            fingerprint: r.input.fingerprint,
        })?;
    }
    Ok(())
}

// Only the already established combined-owner proof is reused. Empty stores
// still require every creator preflight, exclusion and membership check.
pub(crate) fn empty_for_mount(scope: &crate::chunk_mount::ReadScope<'_>) -> Result<bool> {
    Limits::default().validate()?;
    preflight(scope.connection())?;
    isolation_in(scope.connection(), Some(scope))?;
    Ok(inspect(scope.connection())?.is_empty())
}
