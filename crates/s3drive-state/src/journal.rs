//! Immutable local generations and pending intent. Blocking owner-thread API;
//! no network I/O, upload execution, filesystem callbacks or remote durability.
use crate::{Result, State};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::fmt;

pub const CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_READ_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    QuotaExceeded,
    LocalConflict,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::QuotaExceeded => "staging quota is full; existing data was preserved",
            Self::LocalConflict => "staged head changed; unpublished bytes were preserved",
        })
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Non-evictable unique payload quota; DB/WAL/index overhead is additional.
    pub max_bytes: u64,
    pub max_generations: u32,
    pub max_extents_per_generation: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024 * 1024,
            max_generations: 4096,
            max_extents_per_generation: 1_000_000,
        }
    }
}
impl Options {
    fn validate(self) -> Result<Self> {
        if self.max_bytes == 0
            || self.max_bytes > i64::MAX as u64
            || self.max_generations == 0
            || self.max_extents_per_generation == 0
        {
            return Err("invalid staging limits".into());
        }
        Ok(self)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Target {
    source: [u8; 32],
    fingerprint: String,
    key: String,
}
impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StagedTarget([redacted])")
    }
}
impl Target {
    /// The caller supplies the authorized source binding. This does not itself
    /// authorize a future upload or rebind a changed credential generation.
    pub fn new(source: [u8; 32], fingerprint: &str, key: &str) -> Result<Self> {
        if key.is_empty()
            || key.len() > 1024
            || key.contains('\0')
            || fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid staged target".into());
        }
        Ok(Self {
            source,
            fingerprint: fingerprint.into(),
            key: key.into(),
        })
    }
    pub fn source(&self) -> &[u8; 32] {
        &self.source
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn object_key(&self) -> &str {
        &self.key
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base {
    Missing,
    Existing {
        etag: Option<String>,
        version: Option<String>,
    },
}
impl Base {
    fn validate(&self) -> Result<()> {
        if let Self::Existing { etag, version } = self
            && ((etag.is_none() && version.is_none())
                || [etag, version]
                    .into_iter()
                    .flatten()
                    .any(|s| s.is_empty() || s.len() > 8192 || s.contains('\0')))
        {
            return Err("staged replacement requires a valid base revision".into());
        }
        Ok(())
    }
    fn fields(&self) -> (bool, Option<&str>, Option<&str>) {
        match self {
            Self::Missing => (true, None, None),
            Self::Existing { etag, version } => (false, etag.as_deref(), version.as_deref()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GenerationId(i64);
impl GenerationId {
    pub fn value(self) -> i64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Building,
    Sealed,
}
#[derive(Clone, Debug)]
pub struct Generation {
    pub id: GenerationId,
    pub target: Target,
    pub parent: Option<GenerationId>,
    pub base: Base,
    pub size: u64,
    pub filled: u64,
    pub phase: Phase,
    pub manifest_hash: Option<[u8; 32]>,
    extents: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub generation: GenerationId,
    pub manifest_hash: [u8; 32],
}

/// Borrows the already-owned State; a future worker can combine journal/cache
/// commands on this same connection. Do not call these methods on Tokio's
/// executor or keep a transaction across external I/O.
pub struct Journal<'a> {
    state: &'a mut State,
    options: Options,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, options: Options) -> Result<Self> {
        Ok(Self {
            state,
            options: options.validate()?,
        })
    }

    pub fn info(&self, id: GenerationId) -> Result<Generation> {
        load(&self.state.connection, id)
    }

    pub fn head(&self, target: &Target) -> Result<Option<Generation>> {
        let generation = current_head(&self.state.connection, target)?
            .map(|id| load(&self.state.connection, id))
            .transpose()?;
        if generation
            .as_ref()
            .is_some_and(|generation| generation.target != *target)
        {
            return Err("staged target conflicts with registered source binding".into());
        }
        Ok(generation)
    }

    /// Create an unpublished builder. expected_parent is a local compare-and-
    /// swap guard; all descendants retain the original remote base condition.
    pub fn begin(
        &mut self,
        target: &Target,
        base: Base,
        expected_parent: Option<GenerationId>,
        size: u64,
    ) -> Result<GenerationId> {
        self.state.verify_staging_storage()?;
        base.validate()?;
        if size > i64::MAX as u64 {
            return Err("staged size exceeds supported offset range".into());
        }
        self.state
            .register_source(&target.source, &target.fingerprint)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if current_head(&tx, target)? != expected_parent {
            return Err(Failure::LocalConflict.into());
        }
        if let Some(parent) = expected_parent {
            let parent = load(&tx, parent)?;
            if parent.target != *target || parent.base != base || parent.phase != Phase::Sealed {
                return Err("invalid staged parent or base revision".into());
            }
            validate_manifest(&tx, &parent, false)?;
        }
        let count: i64 =
            tx.query_row("SELECT count(*) FROM staged_generations", [], |r| r.get(0))?;
        if count >= i64::from(self.options.max_generations) {
            return Err(Failure::QuotaExceeded.into());
        }
        let (absent, etag, version) = base.fields();
        tx.execute("INSERT INTO staged_generations (source_id,object_key,parent_id,base_absent,base_etag,base_version,size_bytes) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![target.source.as_slice(),target.key,expected_parent.map(|id|id.0),absent,etag,version,size as i64])?;
        let id = GenerationId(tx.last_insert_rowid());
        tx.commit()?;
        Ok(id)
    }

    /// Append one bounded immutable chunk. Builder progress is durable but is
    /// not a published filesystem write or a local durability receipt.
    pub fn append(&mut self, id: GenerationId, bytes: &[u8]) -> Result<()> {
        self.append_inner(id, Content::Bytes(bytes), None, |_| Ok(()))
    }
    /// Idempotent append for a worker/RPC retry: require the expected cursor,
    /// or verify the exact previously committed extent without appending twice.
    pub fn append_at(&mut self, id: GenerationId, offset: u64, bytes: &[u8]) -> Result<()> {
        self.append_inner(id, Content::Bytes(bytes), Some(offset), |_| Ok(()))
    }
    /// Explicit sparse zero extent. Unwritten/missing data is never inferred
    /// to be zero. This can span large logical holes without allocating them.
    pub fn append_zero(&mut self, id: GenerationId, length: u64) -> Result<()> {
        self.append_inner(id, Content::Zero(length), None, |_| Ok(()))
    }
    pub fn append_zero_at(&mut self, id: GenerationId, offset: u64, length: u64) -> Result<()> {
        self.append_inner(id, Content::Zero(length), Some(offset), |_| Ok(()))
    }
    fn append_inner(
        &mut self,
        id: GenerationId,
        content: Content<'_>,
        expected_offset: Option<u64>,
        mut hook: impl FnMut(AppendPoint) -> Result<()>,
    ) -> Result<()> {
        self.state.verify_staging_storage()?;
        let length = match content {
            Content::Bytes(bytes) if !bytes.is_empty() && bytes.len() <= CHUNK_BYTES => {
                bytes.len() as u64
            }
            Content::Zero(length) if length > 0 => length,
            _ => return Err("invalid staged extent length".into()),
        };
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let generation = load(&tx, id)?;
        if generation.phase != Phase::Building {
            return Err("sealed staged generations are immutable".into());
        }
        if let Some(offset) = expected_offset.filter(|offset| *offset != generation.filled) {
            if offset
                .checked_add(length)
                .is_none_or(|end| end > generation.filled)
            {
                return Err(Failure::LocalConflict.into());
            }
            let previous:Option<(i64,Option<Vec<u8>>)>=tx.query_row("SELECT length_bytes,chunk_hash FROM staged_extents WHERE generation_id=?1 AND offset_bytes=?2",params![id.0,offset as i64],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let matches = match (previous, content) {
                (Some((stored, None)), Content::Zero(_)) => stored as u64 == length,
                (Some((stored, Some(digest))), Content::Bytes(bytes))
                    if stored as u64 == length =>
                {
                    read_chunk(&tx, &digest32(digest)?)? == bytes
                }
                _ => false,
            };
            return if matches {
                Ok(())
            } else {
                Err(Failure::LocalConflict.into())
            };
        }
        let end = generation
            .filled
            .checked_add(length)
            .filter(|end| *end <= generation.size)
            .ok_or("staged extent exceeds declared size")?;
        if generation.extents >= self.options.max_extents_per_generation {
            return Err(Failure::QuotaExceeded.into());
        }
        let digest = match content {
            Content::Zero(_) => None,
            Content::Bytes(bytes) => {
                let digest: [u8; 32] = Sha256::digest(bytes).into();
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM staged_chunks WHERE chunk_hash=?1)",
                    [digest.as_slice()],
                    |r| r.get(0),
                )?;
                if exists {
                    if read_chunk(&tx, &digest)? != bytes {
                        return Err(
                            "staged chunk collision or corruption; data was preserved".into()
                        );
                    }
                } else {
                    let used: i64 = tx.query_row(
                        "SELECT coalesce(sum(length(data)),0) FROM staged_chunks",
                        [],
                        |r| r.get(0),
                    )?;
                    let used =
                        u64::try_from(used).map_err(|_| "invalid staging quota accounting")?;
                    if used
                        .checked_add(length)
                        .is_none_or(|total| total > self.options.max_bytes)
                    {
                        return Err(Failure::QuotaExceeded.into());
                    }
                    tx.execute(
                        "INSERT INTO staged_chunks (chunk_hash,data,created_by) VALUES (?1,?2,?3)",
                        params![digest.as_slice(), bytes, id.0],
                    )?;
                }
                Some(digest)
            }
        };
        hook(AppendPoint::ChunkStored)?;
        tx.execute(
            "INSERT INTO staged_extents VALUES (?1,?2,?3,?4)",
            params![
                id.0,
                generation.filled as i64,
                length as i64,
                digest.as_ref().map(|d| d.as_slice())
            ],
        )?;
        hook(AppendPoint::ExtentStored)?;
        tx.execute("UPDATE staged_generations SET filled_bytes=?1,extent_count=extent_count+1 WHERE generation_id=?2",params![end as i64,id.0])?;
        hook(AppendPoint::CursorAdvanced)?;
        tx.commit()?;
        hook(AppendPoint::Committed)?;
        Ok(())
    }

    /// Atomically seal the complete generation, retain its chunks, append
    /// pending intent and advance the head. Replaying an acknowledged seal
    /// returns its receipt without rewinding a newer local head.
    pub fn seal(&mut self, id: GenerationId) -> Result<Receipt> {
        self.seal_inner(id, |_| Ok(()))
    }
    fn seal_inner(
        &mut self,
        id: GenerationId,
        mut hook: impl FnMut(SealPoint) -> Result<()>,
    ) -> Result<Receipt> {
        self.state.verify_staging_storage()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let generation = load(&tx, id)?;
        let digest = validate_manifest(&tx, &generation, true)?;
        let receipt = Receipt {
            generation: id,
            manifest_hash: digest,
        };
        if generation.phase == Phase::Sealed {
            current_head(&tx, &generation.target)?
                .ok_or("missing staged head; data was preserved")?;
            return Ok(receipt);
        }
        if generation.filled != generation.size {
            return Err("cannot seal incomplete staged generation".into());
        }
        if current_head(&tx, &generation.target)? != generation.parent {
            return Err(Failure::LocalConflict.into());
        }
        hook(SealPoint::Validated)?;
        tx.execute("INSERT OR IGNORE INTO retained_staged_chunks SELECT chunk_hash FROM staged_extents WHERE generation_id=?1 AND chunk_hash IS NOT NULL",[id.0])?;
        hook(SealPoint::Retained)?;
        tx.execute(
            "UPDATE staged_generations SET phase='sealed',manifest_hash=?1 WHERE generation_id=?2",
            params![digest.as_slice(), id.0],
        )?;
        tx.execute("INSERT INTO write_journal VALUES (?1,'pending')", [id.0])?;
        hook(SealPoint::Journaled)?;
        tx.execute("INSERT INTO staged_heads VALUES (?1,?2,?3) ON CONFLICT(source_id,object_key) DO UPDATE SET generation_id=excluded.generation_id",params![generation.target.source.as_slice(),generation.target.key,id.0])?;
        hook(SealPoint::HeadUpdated)?;
        tx.commit()?;
        hook(SealPoint::Committed)?;
        Ok(receipt)
    }

    pub fn read(&self, id: GenerationId, offset: u64, length: usize) -> Result<Vec<u8>> {
        self.read_inner(id, offset, length, false)
    }
    /// Inspect explicitly unpublished recovery bytes, clipped to filled length.
    pub fn read_draft(&self, id: GenerationId, offset: u64, length: usize) -> Result<Vec<u8>> {
        self.read_inner(id, offset, length, true)
    }
    fn read_inner(
        &self,
        id: GenerationId,
        offset: u64,
        length: usize,
        draft: bool,
    ) -> Result<Vec<u8>> {
        if length > MAX_READ_BYTES {
            return Err("staged read exceeds bounded range".into());
        }
        let connection = &self.state.connection;
        let generation = load(connection, id)?;
        if (generation.phase == Phase::Building) != draft {
            return Err("staged generation is not in the requested phase".into());
        }
        validate_manifest(connection, &generation, false)?;
        if offset >= generation.filled || length == 0 {
            return Ok(Vec::new());
        }
        let end = offset + ((length as u64).min(generation.filled - offset));
        let mut result = Vec::with_capacity((end - offset) as usize);
        let mut statement=connection.prepare("SELECT offset_bytes,length_bytes,chunk_hash FROM staged_extents WHERE generation_id=?1 AND offset_bytes<?2 AND offset_bytes+length_bytes>?3 ORDER BY offset_bytes")?;
        let mut rows = statement.query(params![id.0, end as i64, offset as i64])?;
        while let Some(row) = rows.next()? {
            let start: u64 = number(row.get(0)?)?;
            let len: u64 = number(row.get(1)?)?;
            let from = offset.saturating_sub(start);
            let to = (end - start).min(len);
            let digest: Option<Vec<u8>> = row.get(2)?;
            if let Some(digest) = digest {
                let digest = digest32(digest)?;
                let bytes = read_chunk(connection, &digest)?;
                result.extend_from_slice(
                    bytes
                        .get(from as usize..to as usize)
                        .ok_or("corrupt staged extent range")?,
                );
            } else {
                result.resize(result.len() + (to - from) as usize, 0);
            }
        }
        if result.len() as u64 != end - offset {
            return Err("missing staged bytes; data was preserved".into());
        }
        Ok(result)
    }

    pub fn pending(&self, after: Option<GenerationId>, limit: u16) -> Result<Vec<Generation>> {
        self.list(after, limit, false)
    }
    pub fn drafts(&self, after: Option<GenerationId>, limit: u16) -> Result<Vec<Generation>> {
        self.list(after, limit, true)
    }
    fn list(
        &self,
        after: Option<GenerationId>,
        limit: u16,
        drafts: bool,
    ) -> Result<Vec<Generation>> {
        if !(1..=1000).contains(&limit) {
            return Err("invalid staged recovery page size".into());
        }
        let sql = if drafts {
            "SELECT generation_id FROM staged_generations WHERE phase='building' AND generation_id>?1 ORDER BY generation_id LIMIT ?2"
        } else {
            "SELECT generation_id FROM write_journal WHERE status='pending' AND generation_id>?1 ORDER BY generation_id LIMIT ?2"
        };
        let mut statement = self.state.connection.prepare(sql)?;
        let ids = statement
            .query_map(params![after.map_or(0, |id| id.0), i64::from(limit)], |r| {
                r.get::<_, i64>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .map(|id| load(&self.state.connection, GenerationId(id)))
            .collect()
    }

    /// Explicit abort of an unpublished builder only. Never discard a sealed
    /// generation, pending intent, or a chunk that was ever published.
    pub fn abort(&mut self, id: GenerationId) -> Result<()> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let generation = load(&tx, id)?;
        if generation.phase != Phase::Building {
            return Err("cannot discard sealed staged data".into());
        }
        // Chunk FKs are deferred only until this transaction removes the
        // aborted builder's extents. Other generations' references block deletion.
        tx.execute("DELETE FROM staged_chunks WHERE NOT EXISTS(SELECT 1 FROM retained_staged_chunks r WHERE r.chunk_hash=staged_chunks.chunk_hash) AND chunk_hash IN (SELECT chunk_hash FROM staged_extents WHERE generation_id=?1) AND NOT EXISTS(SELECT 1 FROM staged_extents e WHERE e.chunk_hash=staged_chunks.chunk_hash AND e.generation_id<>?1)",[id.0])?;
        tx.execute(
            "DELETE FROM staged_generations WHERE generation_id=?1",
            [id.0],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Content<'a> {
    Bytes(&'a [u8]),
    Zero(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppendPoint {
    ChunkStored,
    ExtentStored,
    CursorAdvanced,
    Committed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SealPoint {
    Validated,
    Retained,
    Journaled,
    HeadUpdated,
    Committed,
}
fn number(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| "invalid negative staging value".into())
}
fn digest32(value: Vec<u8>) -> Result<[u8; 32]> {
    value
        .try_into()
        .map_err(|_| "invalid staged checksum shape".into())
}
fn load(connection: &Connection, id: GenerationId) -> Result<Generation> {
    // Bound metadata allocation even when a damaged DB bypasses its CHECKs.
    let shape:bool=connection.query_row("SELECT length(g.source_id)=32 AND length(b.config_fingerprint)=64 AND length(CAST(g.object_key AS BLOB)) BETWEEN 1 AND 1024 AND (g.base_etag IS NULL OR length(CAST(g.base_etag AS BLOB)) BETWEEN 1 AND 8192) AND (g.base_version IS NULL OR length(CAST(g.base_version AS BLOB)) BETWEEN 1 AND 8192) AND (g.manifest_hash IS NULL OR length(g.manifest_hash)=32) FROM staged_generations g JOIN source_bindings b ON b.source_id=g.source_id WHERE g.generation_id=?1",[id.0],|r|r.get(0))?;
    if !shape {
        return Err("invalid staged metadata shape; data was preserved".into());
    }
    let value=connection.query_row("SELECT g.source_id,b.config_fingerprint,g.object_key,g.parent_id,g.base_absent,g.base_etag,g.base_version,g.size_bytes,g.filled_bytes,g.extent_count,g.phase,g.manifest_hash FROM staged_generations g JOIN source_bindings b ON b.source_id=g.source_id WHERE g.generation_id=?1",[id.0],|r|{
        Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,bool>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,i64>(7)?,r.get::<_,i64>(8)?,r.get::<_,i64>(9)?,r.get::<_,String>(10)?,r.get::<_,Option<Vec<u8>>>(11)?))
    })?;
    let (source, fingerprint, key, parent, absent, etag, version, size, filled, count, phase, hash) =
        value;
    let target = Target::new(digest32(source)?, &fingerprint, &key)?;
    let base = if absent {
        if etag.is_some() || version.is_some() {
            return Err("invalid staged base".into());
        }
        Base::Missing
    } else {
        Base::Existing { etag, version }
    };
    base.validate()?;
    let phase = match phase.as_str() {
        "building" => Phase::Building,
        "sealed" => Phase::Sealed,
        _ => return Err("invalid staged phase".into()),
    };
    let size = number(size)?;
    let filled = number(filled)?;
    let extents = u32::try_from(count).map_err(|_| "invalid staged extent count")?;
    let hash = hash.map(digest32).transpose()?;
    if id.0 <= 0
        || parent.is_some_and(|parent| parent <= 0 || parent >= id.0)
        || filled > size
        || (phase == Phase::Sealed && (hash.is_none() || filled != size))
        || (phase == Phase::Building && hash.is_some())
    {
        return Err("corrupt staged generation metadata".into());
    }
    Ok(Generation {
        id,
        target,
        parent: parent.map(GenerationId),
        base,
        size,
        filled,
        phase,
        manifest_hash: hash,
        extents,
    })
}
fn current_head(connection: &Connection, target: &Target) -> Result<Option<GenerationId>> {
    let head: Option<i64> = connection
        .query_row(
            "SELECT generation_id FROM staged_heads WHERE source_id=?1 AND object_key=?2",
            params![target.source.as_slice(), target.key],
            |r| r.get(0),
        )
        .optional()?;
    let latest:Option<i64>=connection.query_row("SELECT max(generation_id) FROM staged_generations WHERE source_id=?1 AND object_key=?2 AND phase='sealed'",params![target.source.as_slice(),target.key],|r|r.get(0))?;
    if head != latest {
        return Err("inconsistent staged head; data was preserved".into());
    }
    Ok(head.map(GenerationId))
}
fn field(hash: &mut Sha256, value: Option<&[u8]>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
}
fn validate_manifest(
    connection: &Connection,
    generation: &Generation,
    verify_payloads: bool,
) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(b"s3drive-staged-manifest-v1\0");
    hash.update(generation.id.0.to_be_bytes());
    hash.update(generation.target.source);
    field(&mut hash, Some(generation.target.fingerprint.as_bytes()));
    field(&mut hash, Some(generation.target.key.as_bytes()));
    hash.update(generation.parent.map_or(0, |id| id.0).to_be_bytes());
    let (absent, etag, version) = generation.base.fields();
    hash.update([u8::from(absent)]);
    field(&mut hash, etag.map(str::as_bytes));
    field(&mut hash, version.map(str::as_bytes));
    hash.update(generation.size.to_be_bytes());
    let mut statement=connection.prepare("SELECT e.offset_bytes,e.length_bytes,e.chunk_hash,length(c.data),r.chunk_hash IS NOT NULL FROM staged_extents e LEFT JOIN staged_chunks c ON c.chunk_hash=e.chunk_hash LEFT JOIN retained_staged_chunks r ON r.chunk_hash=e.chunk_hash WHERE e.generation_id=?1 ORDER BY e.offset_bytes")?;
    let mut rows = statement.query([generation.id.0])?;
    let mut cursor = 0u64;
    let mut count = 0u64;
    while let Some(row) = rows.next()? {
        let offset = number(row.get(0)?)?;
        let length = number(row.get(1)?)?;
        if offset != cursor || length == 0 {
            return Err("staged manifest has a gap or overlap; data was preserved".into());
        }
        cursor = cursor
            .checked_add(length)
            .filter(|end| *end <= generation.filled)
            .ok_or("staged manifest exceeds filled size")?;
        count += 1;
        if count > u64::from(generation.extents) {
            return Err("staged manifest extent count mismatch".into());
        }
        let digest: Option<Vec<u8>> = row.get(2)?;
        hash.update(offset.to_be_bytes());
        hash.update(length.to_be_bytes());
        if let Some(digest) = digest {
            let digest = digest32(digest)?;
            let stored: Option<i64> = row.get(3)?;
            let retained: Option<bool> = row.get(4)?;
            if length > CHUNK_BYTES as u64
                || stored != Some(length as i64)
                || (generation.phase == Phase::Sealed && retained != Some(true))
            {
                return Err("missing or invalid staged chunk; data was preserved".into());
            }
            if verify_payloads {
                read_chunk(connection, &digest)?;
            }
            field(&mut hash, Some(&digest));
        } else {
            field(&mut hash, None);
        }
    }
    if cursor != generation.filled || count != u64::from(generation.extents) {
        return Err("incomplete staged manifest; data was preserved".into());
    }
    let digest: [u8; 32] = hash.finalize().into();
    if generation.phase == Phase::Sealed {
        let pending:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM write_journal WHERE generation_id=?1 AND status='pending')",[generation.id.0],|r|r.get(0))?;
        if generation.manifest_hash != Some(digest) || !pending {
            return Err("staged manifest or journal is corrupt; data was preserved".into());
        }
    }
    Ok(digest)
}
fn read_chunk(connection: &Connection, digest: &[u8; 32]) -> Result<Vec<u8>> {
    let length: Option<i64> = connection
        .query_row(
            "SELECT length(data) FROM staged_chunks WHERE chunk_hash=?1",
            [digest.as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    if !length.is_some_and(|length| (1..=CHUNK_BYTES as i64).contains(&length)) {
        return Err("missing or oversized staged chunk; data was preserved".into());
    }
    let bytes: Vec<u8> = connection.query_row(
        "SELECT data FROM staged_chunks WHERE chunk_hash=?1",
        [digest.as_slice()],
        |r| r.get(0),
    )?;
    if Sha256::digest(&bytes).as_slice() != digest {
        return Err("staged chunk checksum mismatch; data was preserved".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
