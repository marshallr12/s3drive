//! Explicit local composition. No S3 permission, native activation or body repair.
use crate::{
    Result, State, journal, namespace as ns,
    namespace_client::{self as names, Control},
    worker::{Owner, Storage},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
const MAX_CONTROLS: usize = 64;
const MAX_RECORD: usize = 65536;
pub(crate) const CLIENT_TABLES: &[&str] = &[
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
];
#[derive(Clone, PartialEq, Eq)]
pub struct Input {
    pub volume: String,
    pub controls: Vec<Control>,
}
impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChunkMountInput([private])")
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    format: u8,
    pub(crate) volume: String,
    pub(crate) owner: String,
    controls: Vec<Control>,
}
impl fmt::Debug for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChunkMountBinding([private])")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Key {
    volume: String,
    owner: String,
}
impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChunkMountKey([owner-bound])")
    }
}
impl Key {
    pub fn owner(&self) -> &str {
        &self.owner
    }
    pub fn volume(&self) -> &str {
        &self.volume
    }
}
fn id(s: &str) -> bool {
    s != ns::ROOT
        && s.len() == 32
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn hash<T: Serialize>(label: &[u8], value: &T) -> Result<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(label);
    h.update(serde_json::to_vec(value)?);
    Ok(h.finalize().into())
}
fn sorted(mut controls: Vec<Control>) -> Result<Vec<Control>> {
    if controls.is_empty() || controls.len() > MAX_CONTROLS {
        return Err("chunk mount control limit".into());
    }
    controls.sort_by(|a, b| {
        (&a.store.id, &a.source, &a.fingerprint).cmp(&(&b.store.id, &b.source, &b.fingerprint))
    });
    for (n, c) in controls.iter().enumerate() {
        if !c.store.valid() || c.source == [0; 32] || !ns::publication::digest(&c.fingerprint) {
            return Err("invalid chunk mount control".into());
        }
        for old in &controls[..n] {
            if old == c
                || (old.store.id == c.store.id && old.store != c.store)
                || (old.source == c.source && old.fingerprint != c.fingerprint)
            {
                return Err("conflicting chunk mount controls".into());
            }
        }
    }
    Ok(controls)
}
impl Binding {
    fn validate(&self) -> Result<()> {
        if self.format != 1
            || !id(&self.volume)
            || !id(&self.owner)
            || sorted(self.controls.clone())? != self.controls
            || serde_json::to_vec(self)?.len() > MAX_RECORD
        {
            return Err("invalid chunk mount binding".into());
        }
        Ok(())
    }
    pub(crate) fn control(&self, c: &Control) -> Result<()> {
        if self.controls.contains(c) {
            Ok(())
        } else {
            Err("source/store outside chunk mount binding".into())
        }
    }
    pub(crate) fn volume(&self, volume: &str) -> Result<()> {
        if volume == self.volume {
            Ok(())
        } else {
            Err("volume outside chunk mount binding".into())
        }
    }
    pub(crate) fn store_for_sparse(
        &self,
        store: &[u8; 16],
        root: &str,
        source: &[u8; 32],
        fingerprint: &str,
    ) -> Result<ns::Store> {
        let id: String = store.iter().map(|b| format!("{b:02x}")).collect();
        self.controls
            .iter()
            .find(|c| {
                c.store.id == id
                    && c.store.root == root
                    && c.source == *source
                    && c.fingerprint == fingerprint
            })
            .map(|c| c.store.clone())
            .ok_or_else(|| "sparse scope outside chunk mount binding".into())
    }
    pub(crate) fn open(&self, input: &names::opens::OpenInput) -> Result<()> {
        self.volume(&input.volume)?;
        self.control(&Control {
            store: input.store.clone(),
            source: input.source,
            fingerprint: input.fingerprint.clone(),
        })?;
        if let Some(admission) = &input.admission {
            self.namespace(admission)?;
        }
        Ok(())
    }
    pub(crate) fn namespace(&self, input: &names::Input) -> Result<()> {
        self.volume(&input.request.volume)?;
        self.control(&input.control())?;
        if let Some(c) = &input.destination_control {
            self.control(c)?;
        }
        if matches!(&input.request.mutation,ns::Mutation::CreateEntry{node_kind,..}|ns::Mutation::CreateDeferredEntry{node_kind,..} if *node_kind!=ns::Kind::Directory)
        {
            return Err("chunk files require canonical manifest creation".into());
        }
        Ok(())
    }
}
pub(crate) fn identity_hash(owner: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(b"s3drive-chunk-client-identity-v1\0");
    h.update(owner.as_bytes());
    h.finalize().to_vec()
}
// Independent identity witness: no recursion through identity::read/load.
pub(crate) fn identity_marker(db: &Connection, owner: &str) -> Result<bool> {
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version != crate::SCHEMA_VERSION || !exists(db)? {
        return Ok(false);
    }
    let mut q=db.prepare("SELECT singleton,CASE WHEN typeof(record)='blob' AND length(record) BETWEEN 1 AND 65536 THEN record END,CASE WHEN length(state_hash)=32 THEN state_hash END FROM chunk_mount_role LIMIT 2")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<Vec<u8>>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let [(1, Some(raw), Some(digest))] = rows.as_slice() else {
        return Ok(false);
    };
    let binding: Binding = serde_json::from_slice(raw)?;
    binding.validate()?;
    Ok(binding.owner == owner
        && digest.as_slice() == hash(b"s3drive-chunk-mount-binding-v1\0", &binding)?)
}
fn exists(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='chunk_mount_role')",
        [],
        |r| r.get(0),
    )?)
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT singleton,record,state_hash FROM chunk_mount_role LIMIT 0")?;
    Ok(())
}
fn preflight(db: &Connection) -> Result<()> {
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let fk: bool = db.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
    let bad: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
        [],
        |r| r.get(0),
    )?;
    if version != crate::SCHEMA_VERSION || !fk || bad {
        return Err("chunk mount schema/trigger guard failed".into());
    }
    Ok(())
}
fn foreign(db: &Connection) -> Result<()> {
    for table in [
        "managed_publications",
        "managed_rejections",
        "logical_files",
        "logical_edits",
        "logical_publication_links",
        "logical_conflicts",
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
    ] {
        if db.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
            r.get::<_, bool>(0)
        })? {
            return Err("chunk mount refuses foreign/old writer roles".into());
        }
    }
    Ok(())
}
pub(crate) fn load(db: &Connection) -> Result<Option<Binding>> {
    if !exists(db)? {
        if names::identity::has_chunk_witness(db)? {
            return Err("chunk mount marker missing beside retained identity".into());
        }
        let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        return if version < 41 {
            Ok(None)
        } else {
            Err("current chunk mount table missing".into())
        };
    }
    let mut q=db.prepare("SELECT singleton,CASE WHEN typeof(record)='blob' AND length(record) BETWEEN 1 AND 65536 THEN record END,CASE WHEN typeof(state_hash)='blob' AND length(state_hash)=32 THEN state_hash END FROM chunk_mount_role LIMIT 2")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<Vec<u8>>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        if names::identity::has_chunk_witness(db)? {
            return Err("chunk mount marker missing beside retained identity".into());
        }
        return Ok(None);
    }
    preflight(db)?;
    foreign(db)?;
    let [(1, Some(raw), Some(digest))] = rows.as_slice() else {
        return Err("chunk mount role shape changed".into());
    };
    let value: Binding = serde_json::from_slice(raw)?;
    value.validate()?;
    if digest.as_slice() != hash(b"s3drive-chunk-mount-binding-v1\0", &value)? {
        return Err("chunk mount role checksum changed".into());
    }
    if names::chunk_identity(db)?.as_ref() != Some(&value.owner) {
        return Err("chunk mount permanent identity changed".into());
    }
    for c in &value.controls {
        let fp:Option<String>=db.query_row("SELECT CASE WHEN length(config_fingerprint)=64 THEN config_fingerprint END FROM source_bindings WHERE source_id=?",[c.source.as_slice()],|r|r.get(0)).optional()?.flatten();
        if fp.as_ref() != Some(&c.fingerprint) {
            return Err("chunk mount source binding changed".into());
        }
    }
    Ok(Some(value))
}
fn scopes(db: &Connection, b: &Binding) -> Result<()> {
    names::chunk_scope(db, b)?;
    crate::sparse_views::chunk_scope(db, b)?;
    crate::manifest_creation::chunk_scope(db, b)?;
    Ok(())
}
pub(crate) fn validate_all(db: &Connection) -> Result<()> {
    if let Some(b) = load(db)? {
        scopes(db, &b)?;
    }
    Ok(())
}
pub(crate) fn compatible(db: &Connection) -> Result<bool> {
    if let Some(b) = load(db)? {
        scopes(db, &b)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
pub(crate) fn verify_key(db: &Connection, key: &Key) -> Result<Binding> {
    let b = load(db)?.ok_or("chunk mount capability missing")?;
    if b.volume != key.volume || b.owner != key.owner {
        return Err("chunk mount capability belongs to another owner".into());
    }
    Ok(b)
}
pub(crate) fn require(db: &Connection, key: &Key) -> Result<Binding> {
    let b = load(db)?.ok_or("chunk mount capability missing")?;
    if b.volume != key.volume || b.owner != key.owner {
        return Err("chunk mount capability belongs to another owner".into());
    }
    scopes(db, &b)?;
    Ok(b)
}
pub(crate) fn require_legacy(db: &Connection) -> Result<()> {
    if names::identity::has_chunk_witness(db)? || load(db)?.is_some() {
        Err("legacy writer refuses chunk mount role".into())
    } else {
        Ok(())
    }
}
/// Durable admission; no migration, network, file-body reads or namespace effects.
pub fn admit(state: &mut State, input: Input) -> Result<Key> {
    state.verify_staging_storage()?;
    preflight(&state.connection)?;
    if !id(&input.volume) {
        return Err("invalid chunk mount volume".into());
    }
    let controls = sorted(input.controls)?;
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    preflight(&tx)?;
    foreign(&tx)?;
    let old = load(&tx)?;
    let before = old.clone();
    let mut b = if let Some(old) = old {
        old
    } else {
        for table in CLIENT_TABLES {
            if tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
                r.get::<_, bool>(0)
            })? {
                return Err("chunk mount cannot adopt legacy client history".into());
            }
        }
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| "chunk identity generation failed")?;
        let owner: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        if !id(&owner) {
            return Err("chunk identity generation failed".into());
        }
        names::install_chunk_identity(&tx, &owner)?;
        checkpoint("IdentityStored")?;
        Binding {
            format: 1,
            volume: input.volume.clone(),
            owner,
            controls: Vec::new(),
        }
    };
    if b.volume != input.volume {
        return Err("chunk mount volume cannot change".into());
    }
    for c in controls {
        if !b.controls.contains(&c) {
            b.controls.push(c);
        }
    }
    b.controls = sorted(b.controls)?;
    b.validate()?;
    if before.as_ref() == Some(&b) {
        let key = Key {
            volume: b.volume.clone(),
            owner: b.owner.clone(),
        };
        require(&tx, &key)?;
        tx.commit()?;
        return Ok(key);
    }
    for c in &b.controls {
        tx.execute("INSERT INTO source_bindings(source_id,config_fingerprint) VALUES(?1,?2) ON CONFLICT(source_id) DO NOTHING",params![c.source.as_slice(),c.fingerprint])?;
    }
    checkpoint("SourcesStored")?;
    tx.execute("INSERT INTO chunk_mount_role(singleton,record,state_hash) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET record=excluded.record,state_hash=excluded.state_hash",params![serde_json::to_vec(&b)?,hash(b"s3drive-chunk-mount-binding-v1\0",&b)?.as_slice()])?;
    let key = Key {
        volume: b.volume.clone(),
        owner: b.owner.clone(),
    };
    require(&tx, &key)?;
    checkpoint("RoleStored")?;
    tx.commit()?;
    checkpoint("RoleCommitted")?;
    Ok(key)
}
#[derive(Clone)]
pub struct Client {
    owner: Owner,
    options: journal::Options,
    key: Key,
}
impl Client {
    pub(crate) fn new(owner: Owner, options: journal::Options, key: Key) -> Self {
        Self {
            owner,
            options,
            key,
        }
    }
    pub fn key(&self) -> &Key {
        &self.key
    }
    /// Retained local store control only; grants no provider permission or
    /// current remote ownership. Safe for metadata-only recovery construction.
    pub async fn require_store(&self, store: ns::Store) -> Result<()> {
        let key = self.key.clone();
        self.owner
            .call(move |worker| {
                let binding = require(&worker.state.connection, &key)?;
                if !binding.controls.iter().any(|c| c.store == store) {
                    return Err("store is outside the retained chunk capability".into());
                }
                Ok(())
            })
            .await
    }
    pub fn namespace(&self) -> crate::worker::Namespace {
        crate::worker::Namespace::chunk(self.owner.clone(), self.options, self.key.clone())
    }
    pub fn opens(&self) -> crate::worker::Opens {
        crate::worker::Opens::chunk(self.owner.clone(), self.key.clone())
    }
    pub fn creations(
        &self,
        limits: crate::manifest_creation::Limits,
    ) -> Result<crate::manifest_creation::Client> {
        crate::manifest_creation::Client::chunk(self.owner.clone(), limits, self.key.clone())
    }
    pub fn same_storage(&self, storage: &Storage) -> bool {
        storage.same_owner(&self.owner)
    }
}

// Owned test children stop the actual public transaction path. Production no-op.
fn checkpoint(label: &str) -> Result<()> {
    #[cfg(test)]
    {
        use std::io::{Read, Write};
        if std::env::var("S3DRIVE_CHUNK_MOUNT_STOP").ok().as_deref() == Some(label) {
            println!("S3DRIVE_CHUNK_MOUNT_BOUNDARY {label}");
            std::io::stdout().flush()?;
            std::io::stdin().read_exact(&mut [0u8; 1])?;
        }
    }
    let _ = label;
    Ok(())
}

#[cfg(test)]
mod tests;
