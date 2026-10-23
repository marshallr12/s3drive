//! One bounded SQLite snapshot, including immutable bytes and exact old BLOBs.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, PartialEq, Eq)]
pub(super) struct File {
    pub binding: Binding,
    pub raw: Vec<u8>,
    pub head: Id,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Saved {
    pub record: Record,
    pub raw: Vec<u8>,
    pub request: Vec<u8>,
    pub target: Id,
}
impl Saved {
    pub fn new(record: Record) -> Result<Self> {
        let value = Self {
            raw: encode(&record)?,
            request: encode(&record.request)?,
            target: record.request.binding.target()?,
            record,
        };
        if value.raw.len() > RECORD_BYTES || value.request.len() > BINDING_BYTES {
            return Err(Failure::Bounds.into());
        }
        Ok(value)
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Ledger {
    pub read_pins: pins::Ledger,
    pub files: BTreeMap<Id, File>,
    pub records: BTreeMap<Id, Saved>,
    pub blocks: BTreeMap<Id, Vec<u8>>,
}
impl Ledger {
    pub fn record(&self, id: Id) -> Result<&Record> {
        self.records
            .get(&id)
            .map(|r| &r.record)
            .ok_or_else(|| Failure::NotFound.into())
    }
    pub fn operation(&self, operation: Id) -> Option<&Record> {
        self.records
            .values()
            .find(|s| s.record.operation == operation)
            .map(|s| &s.record)
    }
    pub fn admit(&self, options: Options) -> Result<()> {
        let metadata = self.files.values().map(|f| f.raw.len() as u64).sum::<u64>()
            + self
                .records
                .values()
                .map(|r| (r.raw.len() + r.request.len()) as u64)
                .sum::<u64>();
        let extents = self
            .records
            .values()
            .map(|r| r.record.spans.len() as u64)
            .sum::<u64>();
        let dirty = self.blocks.values().map(|b| b.len() as u64).sum::<u64>();
        if self.files.len() > options.max_files as usize
            || self.records.len() > options.limits.views as usize
            || self.records.len() > options.limits.operations as usize
            || extents > options.limits.extents as u64
            || metadata > options.limits.metadata_bytes
            || dirty > options.limits.dirty_bytes
            || self.blocks.len() > HARD_BLOCKS
        {
            return Err(Failure::Quota.into());
        }
        Ok(())
    }
}
fn id(raw: Vec<u8>) -> Result<Id> {
    raw.try_into().map_err(|_| Failure::Corrupt.into())
}
pub(super) fn inspect_initial(db: &Connection) -> Result<Ledger> {
    load(db, false, false)
}
pub(super) fn inspect(db: &Connection) -> Result<Ledger> {
    load(db, true, true)
}
fn count(db: &Connection, sql: &str) -> Result<u64> {
    let n: i64 = db.query_row(sql, [], |r| r.get(0))?;
    n.try_into().map_err(|_| Failure::Corrupt.into())
}
/// Structural history only. This distinct type cannot supply source bytes.
pub(in crate::sparse_views) struct Metadata {
    pub files: BTreeMap<Id, File>,
    pub records: BTreeMap<Id, Saved>,
}
pub(in crate::sparse_views) fn inspect_metadata(db: &Connection) -> Result<Metadata> {
    let data = load(db, true, false)?;
    Ok(Metadata {
        files: data.files,
        records: data.records,
    })
}
fn load(db: &Connection, patches: bool, bodies: bool) -> Result<Ledger> {
    let tables = if patches && bodies {
        &TABLES[..]
    } else {
        &TABLES[..4]
    };
    let mut counts = [0u64; 5];
    for (i, table) in tables.iter().enumerate() {
        counts[i] = count(db, &format!("SELECT count(*) FROM {table}"))?;
    }
    let mut scope = IsolationCheck::new(db);
    if patches {
        scoped::enabled_in(&mut scope)?;
    }
    let mut out = Ledger::default();
    if patches {
        out.read_pins = pins::load_in(&mut scope)?;
    }
    if counts == [0; 5] {
        return Ok(out);
    }
    if counts[0] != 1
        || counts[1] == 0
        || counts[1] > HARD_FILES as u64
        || counts[2] < counts[1]
        || counts[2] > HARD_VIEWS as u64
        || counts[3] != counts[2]
        || counts[4] > HARD_BLOCKS as u64
        || (!patches && counts[1] != counts[2])
    {
        return Err(Failure::Corrupt.into());
    }
    scope.require()?;
    for sql in [
        "SELECT EXISTS(SELECT 1 FROM sparse_view_role WHERE typeof(singleton)!='integer' OR singleton!=1 OR typeof(format)!='integer' OR format!=1 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_view_files WHERE typeof(target)!='blob' OR length(target)!=32 OR typeof(binding)!='blob' OR length(binding) NOT BETWEEN 1 AND 4096 OR typeof(head)!='blob' OR length(head)!=32 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_views WHERE typeof(view_id)!='blob' OR length(view_id)!=32 OR typeof(target)!='blob' OR length(target)!=32 OR typeof(record)!='blob' OR length(record) NOT BETWEEN 1 AND 1048576 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_view_operations WHERE typeof(operation)!='blob' OR length(operation)!=32 OR typeof(view_id)!='blob' OR length(view_id)!=32 OR typeof(request)!='blob' OR length(request) NOT BETWEEN 1 AND 4096 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
    ] {
        if db.query_row(sql, [], |r| r.get::<_, bool>(0))? {
            return Err(Failure::Corrupt.into());
        }
    }
    let metadata = count(
        db,
        "SELECT (SELECT coalesce(sum(length(binding)),0) FROM sparse_view_files)+(SELECT coalesce(sum(length(record)),0) FROM sparse_views)+(SELECT coalesce(sum(length(request)),0) FROM sparse_view_operations)",
    )?;
    if metadata > HARD_METADATA {
        return Err(Failure::Corrupt.into());
    }
    if bodies {
        let bad:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM sparse_view_blocks WHERE typeof(block)!='blob' OR length(block)!=32 OR typeof(data)!='blob' OR length(data) NOT BETWEEN 1 AND 1048576)",[],|r|r.get(0))?;
        let bytes = count(
            db,
            "SELECT coalesce(sum(length(data)),0) FROM sparse_view_blocks",
        )?;
        if bad || bytes > HARD_DIRTY {
            return Err(Failure::Corrupt.into());
        }
        let mut q = db.prepare("SELECT block,data FROM sparse_view_blocks ORDER BY block")?;
        let rows = q.query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (key, data) = row?;
            let key = id(key)?;
            if digest(&data) != key || out.blocks.insert(key, data).is_some() {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    let role: Vec<u8> = db.query_row(
        "SELECT state_hash FROM sparse_view_role WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    if role != role_hash()? {
        return Err(Failure::Corrupt.into());
    }
    let mut q =
        db.prepare("SELECT target,binding,head,state_hash FROM sparse_view_files ORDER BY target")?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    for row in rows {
        let (target, raw, head, h) = row?;
        let target = id(target)?;
        let binding: Binding = decode(&raw)?;
        binding.validate().map_err(|_| Failure::Corrupt)?;
        if target != binding.target()?
            || h != hash(b"s3drive-sparse-binding-v1\0", &binding)?
            || out
                .files
                .insert(
                    target,
                    File {
                        binding,
                        raw,
                        head: id(head)?,
                    },
                )
                .is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut operations = BTreeMap::new();
    let mut operation_ids = BTreeSet::new();
    let mut q=db.prepare("SELECT operation,view_id,request,state_hash FROM sparse_view_operations ORDER BY operation")?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    for row in rows {
        let (op, view, raw, h) = row?;
        let op = id(op)?;
        let request: Request = decode(&raw)?;
        if h != hash(b"s3drive-sparse-request-v1\0", &(op, &request))?
            || !operation_ids.insert(op)
            || operations.insert(id(view)?, (op, request, raw)).is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut extents = 0usize;
    let mut used = BTreeSet::new();
    let mut q =
        db.prepare("SELECT view_id,target,record,state_hash FROM sparse_views ORDER BY view_id")?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    for row in rows {
        let (key, target, raw, h) = row?;
        let key = id(key)?;
        let target = id(target)?;
        let record: Record = decode(&raw)?;
        if record.format == 4
            && db.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))? < 38
        {
            return Err(Failure::Corrupt.into());
        }
        extents += record.spans.len();
        if record.spans.len() > MAX_VIEW_EXTENTS
            || extents > HARD_EXTENTS as usize
            || record.id != key
            || h != hash(b"s3drive-sparse-view-v1\0", &record)?
        {
            return Err(Failure::Corrupt.into());
        }
        let (op, request, request_raw) = operations.remove(&key).ok_or(Failure::Corrupt)?;
        let file = out.files.get(&target).ok_or(Failure::Corrupt)?;
        if record.operation != op
            || record.request != request
            || !record.request.binding.same_scope(&file.binding)?
        {
            return Err(Failure::Corrupt.into());
        }
        if let Some(change) = &record.request.edit {
            if !patches {
                return Err(Failure::Corrupt.into());
            }
            if let Change::Write { blocks, .. } = change {
                for b in blocks {
                    if bodies
                        && out
                            .blocks
                            .get(&b.block)
                            .is_none_or(|data| data.len() as u64 != b.length)
                    {
                        return Err(Failure::Corrupt.into());
                    }
                    used.insert(b.block);
                }
            }
        }
        if out
            .records
            .insert(
                key,
                Saved {
                    record,
                    raw,
                    request: request_raw,
                    target,
                },
            )
            .is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    if !operations.is_empty() || (bodies && used.len() != out.blocks.len()) {
        return Err(Failure::Corrupt.into());
    }
    let mut visited = BTreeSet::new();
    for (target, file) in &out.files {
        if out.record(file.head)?.request.binding != file.binding {
            return Err(Failure::Corrupt.into());
        }
        let mut chain = Vec::new();
        let mut next = Some(file.head);
        while let Some(key) = next {
            if !visited.insert(key) {
                return Err(Failure::Corrupt.into());
            }
            let row = out.records.get(&key).ok_or(Failure::Corrupt)?;
            if &row.target != target {
                return Err(Failure::Corrupt.into());
            }
            chain.push(key);
            next = row.record.parent;
        }
        let mut parent = None;
        for key in chain.into_iter().rev() {
            let record = &out.records.get(&key).ok_or(Failure::Corrupt)?.record;
            record.validate(parent)?;
            parent = Some(record);
        }
    }
    if visited.len() != out.records.len() {
        return Err(Failure::Corrupt.into());
    }
    if patches {
        preparation::publication::settlement::validate_views(db, &out)?;
    }
    Ok(out)
}
