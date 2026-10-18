//! Durable OFFLINE full-snapshot chunk/page preparation. No sends, object
//! adoption, remote receipts, authority capability or mounted-limit changes.
use crate::{Result, State, journal, worker::Owner};
use journal::{GenerationId, Target};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use s3drive_core::manifest::{
    self, Entry, Page, Reference,
    objects::{Namespace, Role, RootDescriptor},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
mod transfers;
pub use transfers::Transfer;

const MAX_CHUNKS: u64 = 65_536;
const MAX_OBJECTS: u64 = 262_144;
const MAX_PLANS: u32 = 256;
const MAX_EXTENTS: u32 = 65_536;
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub max_plans: u32,
    pub max_objects: u64,
    pub max_scan_extents: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_plans: 16,
            max_objects: 32768,
            max_scan_extents: 4096,
        }
    }
}
impl Options {
    fn validate(self) -> Result<Self> {
        if self.max_plans == 0
            || self.max_plans > MAX_PLANS
            || self.max_objects == 0
            || self.max_objects > MAX_OBJECTS
            || self.max_scan_extents == 0
            || self.max_scan_extents > MAX_EXTENTS
        {
            return Err(Failure::Invalid.into());
        }
        Ok(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Conflict,
    Corrupt,
    Quota,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid offline chunk plan",
            Self::Conflict => "chunk plan identity conflicts; data preserved",
            Self::Corrupt => "chunk plan integrity check failed; data preserved",
            Self::Quota => "offline chunk plan quota exceeded; data preserved",
        })
    }
}
impl std::error::Error for Failure {}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: [u8; 32],
    pub volume: [u8; 16],
    pub node: [u8; 16],
    pub store: [u8; 16],
    /// Object identity, distinct from a local SQLite GenerationId.
    pub generation: [u8; 16],
    pub root: String,
    pub source: [u8; 32],
    pub fingerprint: String,
    pub parent: Option<GenerationId>,
    pub size: u64,
    /// Frozen original content CAS guard; not authenticated by this module.
    pub expected_content: Option<[u8; 32]>,
}
impl fmt::Debug for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChunkPlanInput([private])")
    }
}
fn hash<T: Serialize>(domain: &[u8], value: &T) -> Result<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(domain);
    h.update(serde_json::to_vec(value)?);
    Ok(h.finalize().into())
}
impl Input {
    fn namespace(&self) -> Result<Namespace> {
        Ok(Namespace::new(
            self.root.clone(),
            self.volume,
            self.node,
            self.store,
        )?)
    }
    fn validate(&self) -> Result<()> {
        self.namespace()?;
        if self.id == [0; 32] || self.generation == [0; 16] || chunks(self.size) > MAX_CHUNKS {
            return Err(Failure::Invalid.into());
        }
        self.local_target()?;
        Ok(())
    }
    /// Local-only staging address; it is outside the physical v2 object namespace.
    pub fn local_target(&self) -> Result<Target> {
        let name = hash(
            b"s3drive-chunk-local-target-v1\0",
            &(self.volume, self.node, self.store, &self.root),
        )?;
        let hex: String = name.iter().map(|b| format!("{b:02x}")).collect();
        Target::new(
            self.source,
            &self.fingerprint,
            &format!("chunked-local/{hex}"),
        )
    }
    pub fn staging_operation(&self) -> Result<[u8; 32]> {
        self.validate()?;
        hash(b"s3drive-chunk-staging-v1\0", self)
    }
    fn input_hash(&self) -> Result<[u8; 32]> {
        hash(b"s3drive-chunk-input-v1\0", self)
    }
    fn scope_hash(&self) -> Result<[u8; 32]> {
        hash(
            b"s3drive-chunk-scope-v1\0",
            &(self.volume, self.node, self.store, self.generation),
        )
    }
}
fn chunks(size: u64) -> u64 {
    size.div_ceil(manifest::CHUNK_BYTES)
}
fn level_count(size: u64, level: i8) -> u64 {
    let mut n = chunks(size);
    for _ in 0..=level {
        n = n.div_ceil(128).max(1);
    }
    n
}
fn root_level(size: u64) -> i8 {
    let mut l = 0;
    while level_count(size, l) > 1 {
        l += 1;
    }
    l
}
fn reserved(size: u64) -> u64 {
    chunks(size)
        + (0..=root_level(size))
            .map(|l| level_count(size, l))
            .sum::<u64>()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Cursor {
    pub next_chunk: u64,
    pub level: i8,
    pub next_slot: u64,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Info {
    input: Input,
    generation: Option<GenerationId>,
    manifest: Option<[u8; 32]>,
    cursor: Cursor,
    root: Option<RootDescriptor>,
}
impl fmt::Debug for Info {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OfflineChunkPlan")
            .field("cursor", &self.cursor)
            .field("ready", &self.root.is_some())
            .finish()
    }
}
impl Info {
    pub fn input(&self) -> &Input {
        &self.input
    }
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }
    pub fn root(&self) -> Option<&RootDescriptor> {
        self.root.as_ref()
    }
    pub fn generation(&self) -> Option<GenerationId> {
        self.generation
    }
    /// Offline geometry only; callers must still require a Ready root.
    pub fn object_slots(&self, level: i8) -> Result<u64> {
        if !(-1..=root_level(self.input.size)).contains(&level) {
            return Err(Failure::Invalid.into());
        }
        Ok(if level == -1 {
            chunks(self.input.size)
        } else {
            level_count(self.input.size, level)
        })
    }
    pub fn root_level(&self) -> i8 {
        root_level(self.input.size)
    }
    fn state_hash(&self) -> Result<[u8; 32]> {
        hash(
            b"s3drive-chunk-state-v1\0",
            &(
                self.input.input_hash()?,
                self.generation,
                self.manifest,
                self.cursor,
                self.root.as_ref().map(|r| r.encode().to_vec()),
            ),
        )
    }
}
fn read_number(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}
fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Failure::Invalid.into())
}
fn digest(bytes: Vec<u8>) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|_| Failure::Corrupt.into())
}
fn load(db: &Connection, id: [u8; 32]) -> Result<Info> {
    let (raw,ih,res,snapshot_id,mh,chunk,level,slot,root,sh)=db.query_row("SELECT CASE WHEN length(input)<=4096 THEN input END,CASE WHEN length(input_hash)=32 THEN input_hash END,reserved_objects,generation_id,CASE WHEN manifest_hash IS NULL OR length(manifest_hash)=32 THEN manifest_hash ELSE zeroblob(0) END,next_chunk,build_level,next_slot,CASE WHEN root IS NULL OR length(root)=120 THEN root ELSE zeroblob(0) END,CASE WHEN length(state_hash)=32 THEN state_hash END FROM chunk_plans WHERE plan_id=?1",[id.as_slice()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?,read_number(r,2)?,r.get::<_,Option<i64>>(3)?,r.get::<_,Option<Vec<u8>>>(4)?,read_number(r,5)?,r.get::<_,i8>(6)?,read_number(r,7)?,r.get::<_,Option<Vec<u8>>>(8)?,r.get::<_,Vec<u8>>(9)?)))?;
    let input: Input = serde_json::from_slice(&raw).map_err(|_| Failure::Corrupt)?;
    input.validate()?;
    let scope:Vec<u8>=db.query_row("SELECT CASE WHEN length(scope_hash)=32 THEN scope_hash END FROM chunk_plans WHERE plan_id=?1",[id.as_slice()],|r|r.get(0))?;
    if digest(scope)? != input.scope_hash()? {
        return Err(Failure::Corrupt.into());
    }
    if input.id != id
        || serde_json::to_vec(&input)? != raw
        || input.input_hash()? != digest(ih)?
        || reserved(input.size) != res
    {
        return Err(Failure::Corrupt.into());
    }
    let info = Info {
        input,
        generation: snapshot_id.map(GenerationId::from_database).transpose()?,
        manifest: mh.map(digest).transpose()?,
        cursor: Cursor {
            next_chunk: chunk,
            level,
            next_slot: slot,
        },
        root: root.map(|r| RootDescriptor::decode(&r)).transpose()?,
    };
    if info.state_hash()? != digest(sh)?
        || chunk > chunks(info.input.size)
        || !(-1..=root_level(info.input.size)).contains(&level)
        || (level == -1 && slot != 0)
        || (level >= 0
            && (chunk != chunks(info.input.size) || slot > level_count(info.input.size, level)))
    {
        return Err(Failure::Corrupt.into());
    }
    if info.generation.is_none()
        && (info.manifest.is_some()
            || info.root.is_some()
            || info.cursor
                != (Cursor {
                    next_chunk: 0,
                    level: -1,
                    next_slot: 0,
                }))
    {
        return Err(Failure::Corrupt.into());
    }
    if let Some(g) = info.generation {
        let snapshot = journal::load(db, g)?;
        let operation: Option<i64> = db
            .query_row(
                "SELECT generation_id FROM staged_operations WHERE operation_key=?1",
                [info.input.staging_operation()?.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if snapshot.phase != journal::Phase::Sealed
            || snapshot.size != info.input.size
            || snapshot.parent != info.input.parent
            || snapshot.base != journal::Base::Missing
            || snapshot.target != info.input.local_target()?
            || snapshot.manifest_hash != info.manifest
            || snapshot.extent_count() > MAX_EXTENTS
            || operation != Some(g.value())
        {
            return Err(Failure::Corrupt.into());
        }
        journal::validate_manifest(db, &snapshot, false)?;
    }
    if let Some(root) = &info.root
        && (info.generation.is_none()
            || level != root_level(info.input.size)
            || slot != 1
            || root.volume() != &info.input.volume
            || root.node() != &info.input.node
            || root.length() != info.input.size)
    {
        return Err(Failure::Corrupt.into());
    }
    if info.root.is_none() && level >= 0 && slot == level_count(info.input.size, level) {
        return Err(Failure::Corrupt.into());
    }
    if info.generation.is_some() && level == -1 && chunk > 0 && chunk == chunks(info.input.size) {
        return Err(Failure::Corrupt.into());
    }
    Ok(info)
}
#[derive(Clone, PartialEq, Eq, Serialize)]
struct Object {
    level: i8,
    slot: u64,
    ordinal: u64,
    offset: u64,
    length: u64,
    hole: bool,
    sha: [u8; 32],
    body: Option<Vec<u8>>,
}
impl Object {
    fn reference(&self, input: &Input) -> Result<Reference> {
        Ok(Reference::new(
            input.store,
            input.generation,
            self.ordinal,
            self.sha,
        )?)
    }
    fn hash(&self, input: &Input) -> Result<[u8; 32]> {
        hash(b"s3drive-chunk-object-v1\0", &(input.input_hash()?, self))
    }
}
fn geometry(input: &Input, level: i8, slot: u64) -> Result<(u64, u64, u64)> {
    if level < -1 || level > root_level(input.size) || slot >= level_count(input.size, level) {
        return Err(Failure::Corrupt.into());
    }
    let span = manifest::CHUNK_BYTES * 128u64.pow((level + 1) as u32);
    let offset = slot * span;
    let length = input.size.saturating_sub(offset).min(span);
    let ordinal = if level == -1 {
        slot
    } else {
        ((level as u64 + 1) << 56) | slot
    };
    Ok((offset, length, ordinal))
}
fn object(db: &Connection, input: &Input, level: i8, slot: u64) -> Result<Object> {
    let (ordinal,offset,length,hole,sha,body,rh)=db.query_row("SELECT CASE WHEN length(ordinal)=8 THEN ordinal END,offset_bytes,length_bytes,hole,CASE WHEN length(sha256)=32 THEN sha256 END,CASE WHEN body IS NULL OR length(body)<=10432 THEN body ELSE zeroblob(0) END,CASE WHEN length(record_hash)=32 THEN record_hash END FROM chunk_objects WHERE plan_id=?1 AND level=?2 AND slot=?3",params![input.id.as_slice(),level,integer(slot)?],|r|Ok((r.get::<_,Vec<u8>>(0)?,read_number(r,1)?,read_number(r,2)?,r.get::<_,bool>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,Option<Vec<u8>>>(5)?,r.get::<_,Vec<u8>>(6)?)))?;
    let ordinal = u64::from_le_bytes(ordinal.try_into().map_err(|_| Failure::Corrupt)?);
    let o = Object {
        level,
        slot,
        ordinal,
        offset,
        length,
        hole,
        sha: digest(sha)?,
        body,
    };
    if (offset, length, ordinal) != geometry(input, level, slot)? || o.hash(input)? != digest(rh)? {
        return Err(Failure::Corrupt.into());
    }
    if level == -1 {
        if o.body.is_some() || length == 0 {
            return Err(Failure::Corrupt.into());
        }
    } else {
        let page = Page::decode(o.body.as_deref().ok_or(Failure::Corrupt)?)?;
        if hole
            || page.volume() != &input.volume
            || page.node() != &input.node
            || page.start() != offset
            || page.length() != length
            || page.level() != level as u8
            || page.digest() != o.sha
        {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(o)
}
fn page(db: &Connection, input: &Input, level: i8, slot: u64) -> Result<Page> {
    let mut entries = Vec::new();
    let count = level_count(input.size, level - 1);
    for child in slot * 128..((slot + 1) * 128).min(count) {
        let o = object(db, input, level - 1, child)?;
        let entry = if level > 0 {
            Entry::Child {
                length: o.length,
                object: o.reference(input)?,
            }
        } else if o.hole {
            Entry::Hole { length: o.length }
        } else {
            Entry::Data {
                length: o.length,
                object: o.reference(input)?,
            }
        };
        if let (Some(Entry::Hole { length: a }), Entry::Hole { length: b }) =
            (entries.last_mut(), &entry)
        {
            *a += *b;
        } else {
            entries.push(entry);
        }
    }
    let (start, _, _) = geometry(input, level, slot)?;
    Ok(Page::new(
        input.volume,
        input.node,
        start,
        level as u8,
        entries,
    )?)
}
fn validate_objects(db: &Connection, info: &Info) -> Result<()> {
    let expected = info.cursor.next_chunk
        + (0..=info.cursor.level)
            .map(|l| {
                if info.root.is_some() || l < info.cursor.level {
                    level_count(info.input.size, l)
                } else {
                    info.cursor.next_slot
                }
            })
            .sum::<u64>();
    let count: u64 = db.query_row(
        "SELECT count(*) FROM chunk_objects WHERE plan_id=?1",
        [info.input.id.as_slice()],
        |r| read_number(r, 0),
    )?;
    if count != expected {
        return Err(Failure::Corrupt.into());
    }
    for l in -1..=info.cursor.level {
        let n = if l == -1 {
            info.cursor.next_chunk
        } else if info.root.is_some() || l < info.cursor.level {
            level_count(info.input.size, l)
        } else {
            info.cursor.next_slot
        };
        for slot in 0..n {
            let o = object(db, &info.input, l, slot)?;
            if l >= 0
                && o.body.as_deref() != Some(page(db, &info.input, l, slot)?.encode().as_slice())
            {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    if let Some(root) = &info.root {
        let o = object(db, &info.input, root_level(info.input.size), 0)?;
        if root.page() != &o.reference(&info.input)? {
            return Err(Failure::Corrupt.into());
        }
        root.verify_page(o.body.as_deref().ok_or(Failure::Corrupt)?)?;
    }
    Ok(())
}
pub(crate) fn check_schema(db: &Connection) -> Result<()> {
    db.prepare("SELECT plan_id,scope_hash,input,input_hash,reserved_objects,generation_id,manifest_hash,next_chunk,build_level,next_slot,root,state_hash FROM chunk_plans LIMIT 0")?;
    db.prepare("SELECT plan_id,level,slot,ordinal,offset_bytes,length_bytes,hole,sha256,body,record_hash FROM chunk_objects LIMIT 0")?;
    Ok(())
}
pub(crate) fn validate_all(db: &Connection) -> Result<(u32, u64)> {
    let (count, res): (u32, u64) = db.query_row(
        "SELECT count(*),coalesce(sum(reserved_objects),0) FROM chunk_plans",
        [],
        |r| Ok((r.get(0)?, read_number(r, 1)?)),
    )?;
    if count > MAX_PLANS || res > MAX_OBJECTS {
        return Err(Failure::Corrupt.into());
    }
    let orphan:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM chunk_objects o LEFT JOIN chunk_plans p ON p.plan_id=o.plan_id WHERE p.plan_id IS NULL)",[],|r|r.get(0))?;
    if orphan {
        return Err(Failure::Corrupt.into());
    }
    let ids = db
        .prepare("SELECT CASE WHEN length(plan_id)=32 THEN plan_id END FROM chunk_plans LIMIT 257")?
        .query_map([], |r| r.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in ids {
        let i = load(db, digest(id)?)?;
        validate_objects(db, &i)?;
    }
    Ok((count, res))
}
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
    pub fn info(&self, id: [u8; 32]) -> Result<Info> {
        let i = load(&self.state.connection, id)?;
        validate_objects(&self.state.connection, &i)?;
        Ok(i)
    }
    pub fn freeze(&mut self, input: Input) -> Result<Info> {
        self.state.verify_staging_storage()?;
        input.validate()?;
        self.state
            .register_source(&input.source, &input.fingerprint)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM chunk_plans WHERE plan_id=?1)",
            [input.id.as_slice()],
            |r| r.get(0),
        )?;
        if exists {
            let i = load(&tx, input.id)?;
            if i.input != input {
                return Err(Failure::Conflict.into());
            }
            validate_objects(&tx, &i)?;
            return Ok(i);
        }
        let (count, res) = validate_all(&tx)?;
        if count >= self.options.max_plans || res + reserved(input.size) > self.options.max_objects
        {
            return Err(Failure::Quota.into());
        }
        let reused: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM chunk_plans WHERE scope_hash=?1)",
            [input.scope_hash()?.as_slice()],
            |r| r.get(0),
        )?;
        if reused {
            return Err(Failure::Conflict.into());
        }
        let i = Info {
            input,
            generation: None,
            manifest: None,
            cursor: Cursor {
                next_chunk: 0,
                level: -1,
                next_slot: 0,
            },
            root: None,
        };
        if tx.execute(
            "INSERT INTO chunk_plans VALUES (?1,?2,?3,?4,?5,NULL,NULL,0,-1,0,NULL,?6)",
            params![
                i.input.id.as_slice(),
                i.input.scope_hash()?.as_slice(),
                serde_json::to_vec(&i.input)?,
                i.input.input_hash()?.as_slice(),
                integer(reserved(i.input.size))?,
                i.state_hash()?.as_slice()
            ],
        )? != 1
            || load(&tx, i.input.id)? != i
        {
            return Err(Failure::Corrupt.into());
        }
        tx.commit()?;
        Ok(i)
    }
    pub fn attach(&mut self, id: [u8; 32], generation: GenerationId) -> Result<Info> {
        self.state.verify_staging_storage()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut i = load(&tx, id)?;
        if let Some(old) = i.generation {
            if old != generation {
                return Err(Failure::Conflict.into());
            }
            validate_objects(&tx, &i)?;
            return Ok(i);
        }
        let g = journal::load(&tx, generation)?;
        if g.extent_count() > self.options.max_scan_extents {
            return Err(Failure::Quota.into());
        }
        i.generation = Some(generation);
        i.manifest = g.manifest_hash;
        store(&tx, &i)?;
        let checked = load(&tx, id)?;
        if checked != i {
            return Err(Failure::Corrupt.into());
        }
        tx.commit()?;
        Ok(i)
    }
    /// A step hashes <=1MiB or creates one page from <=128 children. Finishing
    /// additionally validates the admitted graph; staged metadata reads scan the
    /// admitted extents. These bounds do not establish constant command latency.
    /// A stale cursor returns current progress without doing another step.
    pub fn step(&mut self, id: [u8; 32], expected: Cursor) -> Result<Info> {
        self.state.verify_staging_storage()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut i = load(&tx, id)?;
        let snapshot_id = i.generation.ok_or(Failure::Invalid)?;
        if i.root.is_some() || i.cursor != expected {
            validate_objects(&tx, &i)?;
            return Ok(i);
        }
        let o = if i.cursor.level == -1 && i.cursor.next_chunk < chunks(i.input.size) {
            let slot = i.cursor.next_chunk;
            let (offset, length, ordinal) = geometry(&i.input, -1, slot)?;
            let bytes = journal::read_data(&tx, snapshot_id, offset, length as usize, false)?;
            if bytes.len() as u64 != length {
                return Err(Failure::Corrupt.into());
            }
            i.cursor.next_chunk += 1;
            Object {
                level: -1,
                slot,
                ordinal,
                offset,
                length,
                hole: bytes.iter().all(|b| *b == 0),
                sha: Sha256::digest(&bytes).into(),
                body: None,
            }
        } else {
            if i.cursor.level == -1 {
                i.cursor.level = 0;
            }
            let l = i.cursor.level;
            let slot = i.cursor.next_slot;
            let p = page(&tx, &i.input, l, slot)?;
            let (offset, length, ordinal) = geometry(&i.input, l, slot)?;
            i.cursor.next_slot += 1;
            let o = Object {
                level: l,
                slot,
                ordinal,
                offset,
                length,
                hole: false,
                sha: p.digest(),
                body: Some(p.encode()),
            };
            if level_count(i.input.size, l) == 1 {
                i.root = Some(RootDescriptor::new(
                    i.input.volume,
                    i.input.node,
                    i.input.size,
                    o.reference(&i.input)?,
                )?);
            } else if i.cursor.next_slot == level_count(i.input.size, l) {
                i.cursor.level += 1;
                i.cursor.next_slot = 0;
            }
            o
        };
        if tx.execute(
            "INSERT INTO chunk_objects VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                id.as_slice(),
                o.level,
                integer(o.slot)?,
                o.ordinal.to_le_bytes().as_slice(),
                integer(o.offset)?,
                integer(o.length)?,
                o.hole,
                o.sha.as_slice(),
                o.body,
                o.hash(&i.input)?.as_slice()
            ],
        )? != 1
            || object(&tx, &i.input, o.level, o.slot)? != o
        {
            return Err(Failure::Corrupt.into());
        }
        if i.cursor.level == -1 && i.cursor.next_chunk == chunks(i.input.size) {
            i.cursor.level = 0;
        }
        store(&tx, &i)?;
        if load(&tx, id)? != i || object(&tx, &i.input, o.level, o.slot)? != o {
            return Err(Failure::Corrupt.into());
        }
        let count = tx.query_row(
            "SELECT count(*) FROM chunk_objects WHERE plan_id=?1",
            [id.as_slice()],
            |r| read_number(r, 0),
        )?;
        let expected = i.cursor.next_chunk
            + (0..=i.cursor.level)
                .map(|level| {
                    if i.root.is_some() || level < i.cursor.level {
                        level_count(i.input.size, level)
                    } else {
                        i.cursor.next_slot
                    }
                })
                .sum::<u64>();
        if count != expected {
            return Err(Failure::Corrupt.into());
        }
        if i.root.is_some() {
            validate_objects(&tx, &i)?;
        }
        tx.commit()?;
        Ok(i)
    }
    /// Fresh verified bytes for a future executor, NOT a send permit. Hole rows
    /// explicitly return None; a missing row or missing media is an error.
    pub fn bytes(&self, id: [u8; 32], level: i8, slot: u64) -> Result<Option<PreparedObject>> {
        self.state.verify_staging_storage()?;
        let i = load(&self.state.connection, id)?;
        let o = object(&self.state.connection, &i.input, level, slot)?;
        let bytes = if let Some(body) = &o.body {
            if *body != page(&self.state.connection, &i.input, level, slot)?.encode() {
                return Err(Failure::Corrupt.into());
            }
            body.clone()
        } else {
            let bytes = journal::read_data(
                &self.state.connection,
                i.generation.ok_or(Failure::Corrupt)?,
                o.offset,
                o.length as usize,
                false,
            )?;
            if bytes.len() as u64 != o.length
                || <[u8; 32]>::from(Sha256::digest(&bytes)) != o.sha
                || bytes.iter().all(|b| *b == 0) != o.hole
            {
                return Err(Failure::Corrupt.into());
            }
            bytes
        };
        if o.hole {
            return Ok(None);
        }
        let reference = o.reference(&i.input)?;
        let role = if level == -1 { Role::Data } else { Role::Page };
        let key = i.input.namespace()?.key(role, &reference)?;
        Ok(Some(PreparedObject {
            target: Target::new(i.input.source, &i.input.fingerprint, key.as_str())?,
            reference,
            role,
            bytes,
        }))
    }
}
fn store(db: &Connection, i: &Info) -> Result<()> {
    if db.execute("UPDATE chunk_plans SET generation_id=?2,manifest_hash=?3,next_chunk=?4,build_level=?5,next_slot=?6,root=?7,state_hash=?8 WHERE plan_id=?1",params![i.input.id.as_slice(),i.generation.map(|g|g.value()),i.manifest.as_ref().map(|h|h.as_slice()),integer(i.cursor.next_chunk)?,i.cursor.level,integer(i.cursor.next_slot)?,i.root.as_ref().map(|r|r.encode().to_vec()),i.state_hash()?.as_slice()])?!=1 {return Err(Failure::Corrupt.into());}
    Ok(())
}
pub struct PreparedObject {
    pub target: Target,
    pub reference: Reference,
    pub role: Role,
    pub bytes: Vec<u8>,
}
impl fmt::Debug for PreparedObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OfflinePreparedChunkObject([private])")
    }
}
#[derive(Clone)]
pub struct Planner {
    owner: Owner,
    options: Options,
    staging_options: journal::Options,
}
impl Planner {
    pub(crate) fn new(
        owner: Owner,
        options: Options,
        staging_options: journal::Options,
    ) -> Result<Self> {
        Ok(Self {
            owner,
            options: options.validate()?,
            staging_options,
        })
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let options = self.options;
        self.owner
            .call(move |s| f(&mut Journal::new(&mut s.state, options)?))
            .await
    }
    pub async fn freeze(&self, input: Input) -> Result<Info> {
        self.call(move |j| j.freeze(input)).await
    }
    pub async fn info(&self, id: [u8; 32]) -> Result<Info> {
        self.call(move |j| j.info(id)).await
    }
    pub async fn attach(&self, id: [u8; 32], snapshot_id: GenerationId) -> Result<Info> {
        self.call(move |j| j.attach(id, snapshot_id)).await
    }
    pub async fn prepare(&self, id: [u8; 32]) -> Result<Info> {
        let mut i = self.info(id).await?;
        while i.root.is_none() {
            let cursor = i.cursor;
            i = self.call(move |j| j.step(id, cursor)).await?;
            tokio::task::yield_now().await;
        }
        Ok(i)
    }
    pub async fn bytes(
        &self,
        id: [u8; 32],
        level: i8,
        slot: u64,
    ) -> Result<Option<PreparedObject>> {
        self.call(move |j| j.bytes(id, level, slot)).await
    }
}
#[cfg(test)]
mod tests;
