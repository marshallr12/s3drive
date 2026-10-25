//! Complete bounded SQLite snapshot. All graph and local intent validation is
//! offline; it never proves current provider permission or source byte health.
use super::*;
use model::{Progress, count, geometry, ordinal, root_level, span};
type Key = (Id, i8, u64);
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Saved {
    pub input: Input,
    pub input_raw: Vec<u8>,
    pub progress: Progress,
    pub state_raw: Vec<u8>,
}
impl Saved {
    pub fn new(input: Input, progress: Progress) -> Result<Self> {
        let s = Self {
            input_raw: encode(&input)?,
            state_raw: encode(&progress)?,
            input,
            progress,
        };
        if s.input_raw.len() > MAX_RECORD || s.state_raw.len() > 4096 {
            return Err(Failure::Bounds.into());
        }
        Ok(s)
    }
    pub fn state_hash(&self) -> Result<Id> {
        hash(
            b"s3drive-mixed-preparation-state-v1\0",
            &(self.input.id()?, &self.progress),
        )
    }
    pub fn info(&self, views: &super::super::ledger::Ledger) -> Result<Info> {
        Info::new(
            self.input.clone(),
            View(views.record(self.input.view)?.clone()),
            self.progress.clone(),
        )
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct SavedObject {
    pub object: Object,
    pub raw: Vec<u8>,
}
impl SavedObject {
    pub fn new(object: Object) -> Result<Self> {
        let raw = encode(&object)?;
        if raw.len() > MAX_RECORD {
            return Err(Failure::Bounds.into());
        }
        Ok(Self { object, raw })
    }
    pub fn state_hash(&self, plan: Id) -> Result<Id> {
        hash(b"s3drive-mixed-object-v1\0", &(plan, &self.object))
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub views: super::super::ledger::Ledger,
    pub plans: BTreeMap<Id, Saved>,
    pub objects: BTreeMap<Key, SavedObject>,
    pub blobs: BTreeMap<Id, Vec<u8>>,
}
fn id(raw: Vec<u8>) -> Result<Id> {
    raw.try_into().map_err(|_| Failure::Corrupt.into())
}
fn has(db: &Connection, sql: &str) -> Result<bool> {
    db.query_row(sql, [], |r| r.get(0)).map_err(Into::into)
}
fn total(db: &Connection, sql: &str) -> Result<u64> {
    db.query_row(sql, [], |r| number(r, 0)).map_err(Into::into)
}
/// Preparation tables have no application triggers. Refuse attached triggers
/// before executing writes, including ones targeting generic staging/cache data.
/// Post-snapshot equality is still required for every supported mutation.
pub(super) fn no_triggers(db: &Connection) -> Result<()> {
    if has(
        db,
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger' AND tbl_name IN ('sparse_preparations','sparse_preparation_objects','sparse_preparation_blobs'))",
    )? {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
#[cfg(test)]
thread_local! { static INSPECTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(super) fn inspection_count() -> usize {
    INSPECTIONS.get()
}
pub(super) fn inspect(db: &Connection) -> Result<Snapshot> {
    inspect_in(db, None)
}
pub(super) fn inspect_scoped(scope: &crate::chunk_mount::ReadScope<'_>) -> Result<Snapshot> {
    inspect_in(scope.connection(), Some(scope))
}
fn inspect_in(
    db: &Connection,
    scope: Option<&crate::chunk_mount::ReadScope<'_>>,
) -> Result<Snapshot> {
    #[cfg(test)]
    INSPECTIONS.set(INSPECTIONS.get() + 1);
    no_triggers(db)?;
    if !db.pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))? {
        return Err(Failure::Corrupt.into());
    }
    // Reuse only isolation within this synchronous inspection, never source,
    // mode-row, terminal, physical or native graph validation results.
    let mut isolation = super::super::IsolationCheck::new(db);
    let views = match scope {
        Some(scope) => super::super::ledger::inspect_scoped(scope)?,
        None => super::super::ledger::inspect_with_isolation(&mut isolation)?,
    };
    let plans = total(db, "SELECT count(*) FROM sparse_preparations")?;
    let objects = total(db, "SELECT count(*) FROM sparse_preparation_objects")?;
    let blobs = total(db, "SELECT count(*) FROM sparse_preparation_blobs")?;
    if plans > MAX_PLANS as u64 || objects > MAX_OBJECTS as u64 || blobs > 2 * MAX_OBJECTS as u64 {
        return Err(Failure::Corrupt.into());
    }
    for sql in [
        "SELECT EXISTS(SELECT 1 FROM sparse_preparations WHERE typeof(plan_id)!='blob' OR length(plan_id)!=32 OR typeof(operation)!='blob' OR length(operation)!=32 OR typeof(view_id)!='blob' OR length(view_id)!=32 OR typeof(input)!='blob' OR length(input) NOT BETWEEN 1 AND 65536 OR typeof(state)!='blob' OR length(state) NOT BETWEEN 1 AND 4096 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_preparation_objects WHERE typeof(plan_id)!='blob' OR length(plan_id)!=32 OR typeof(level)!='integer' OR level NOT BETWEEN -1 AND 2 OR typeof(slot)!='integer' OR slot NOT BETWEEN 0 AND 65535 OR typeof(record)!='blob' OR length(record) NOT BETWEEN 1 AND 65536 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_preparation_blobs WHERE typeof(blob)!='blob' OR length(blob)!=32 OR typeof(data)!='blob' OR length(data) NOT BETWEEN 1 AND 1048576)",
    ] {
        if has(db, sql)? {
            return Err(Failure::Corrupt.into());
        }
    }
    let metadata = total(
        db,
        "SELECT coalesce((SELECT sum(length(input)+length(state)) FROM sparse_preparations),0)+coalesce((SELECT sum(length(record)) FROM sparse_preparation_objects),0)",
    )?;
    if metadata > MAX_METADATA
        || total(
            db,
            "SELECT coalesce(sum(length(data)),0) FROM sparse_preparation_blobs",
        )? > MAX_BYTES
    {
        return Err(Failure::Corrupt.into());
    }
    let mut s = Snapshot {
        views,
        plans: BTreeMap::new(),
        objects: BTreeMap::new(),
        blobs: BTreeMap::new(),
    };
    let mut q = db.prepare("SELECT blob,data FROM sparse_preparation_blobs ORDER BY blob")?;
    for row in q.query_map([], |r| {
        Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
    })? {
        let (key, bytes) = row?;
        let key = id(key)?;
        if digest(&bytes) != key || s.blobs.insert(key, bytes).is_some() {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut q=db.prepare("SELECT plan_id,operation,view_id,input,state,state_hash FROM sparse_preparations ORDER BY plan_id")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
            r.get::<_, Vec<u8>>(4)?,
            r.get::<_, Vec<u8>>(5)?,
        ))
    })? {
        let (key, op, view, raw, state, sha) = row?;
        let key = id(key)?;
        let input: Input = decode(&raw)?;
        let progress: Progress = decode(&state)?;
        let saved = Saved::new(input, progress)?;
        if key != saved.input.id()?
            || id(op)? != saved.input.operation
            || id(view)? != saved.input.view
            || saved.input_raw != raw
            || saved.state_raw != state
            || id(sha)? != saved.state_hash()?
            || s.plans.insert(key, saved).is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut q=db.prepare("SELECT plan_id,level,slot,record,state_hash FROM sparse_preparation_objects ORDER BY plan_id,level,slot")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, i8>(1)?,
            number(r, 2)?,
            r.get::<_, Vec<u8>>(3)?,
            r.get::<_, Vec<u8>>(4)?,
        ))
    })? {
        let (plan, level, slot, raw, sha) = row?;
        let plan = id(plan)?;
        let object: Object = decode(&raw)?;
        let saved = SavedObject::new(object)?;
        if (level, slot) != (saved.object.level, saved.object.slot)
            || !s.plans.contains_key(&plan)
            || saved.raw != raw
            || saved.state_hash(plan)? != id(sha)?
            || s.objects.insert((plan, level, slot), saved).is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    s.validate()?;
    owned::validate_snapshot(db, &s)?;
    native::validate_snapshot(db, &s)?;
    match scope {
        Some(scope) => retry::validate_snapshot_scoped(scope, &s)?,
        None => retry::validate_snapshot_with_isolation(&mut isolation, &s)?,
    }
    Ok(s)
}
impl Snapshot {
    pub fn info(&self, plan: Id) -> Result<Info> {
        self.plans
            .get(&plan)
            .ok_or(Failure::NotFound)?
            .info(&self.views)
    }
    pub fn admit(&self, options: Options) -> Result<()> {
        let metadata = self
            .plans
            .values()
            .map(|p| (p.input_raw.len() + p.state_raw.len()) as u64)
            .sum::<u64>()
            + self
                .objects
                .values()
                .map(|o| o.raw.len() as u64)
                .sum::<u64>()
            + self
                .plans
                .values()
                .filter(|p| p.progress.settlement.is_none())
                .count() as u64
                * 256;
        let bytes = self.blobs.values().map(|b| b.len() as u64).sum::<u64>();
        if self.plans.len() > options.plans
            || self.objects.len() > options.objects
            || metadata > options.metadata_bytes
            || bytes > options.blob_bytes
        {
            return Err(Failure::Quota.into());
        }
        Ok(())
    }
    pub fn put_blob(&mut self, bytes: Vec<u8>) -> Result<Id> {
        if bytes.is_empty() || bytes.len() > manifest::CHUNK_BYTES as usize {
            return Err(Failure::Bounds.into());
        }
        let id = digest(&bytes);
        if let Some(old) = self.blobs.get(&id) {
            if old != &bytes {
                return Err(Failure::Corrupt.into());
            }
        } else {
            self.blobs.insert(id, bytes);
        }
        Ok(id)
    }
    pub fn object(&self, plan: Id, level: i8, slot: u64) -> Result<&Object> {
        self.objects
            .get(&(plan, level, slot))
            .map(|s| &s.object)
            .ok_or_else(|| Failure::Corrupt.into())
    }
    pub fn reference(&self, info: &Info, o: &Object) -> Result<Option<Reference>> {
        match &o.kind {
            Kind::Zero => Ok(None),
            Kind::Inherited(w) => Ok(w.resolve(info.input(), &info.view, &self.blobs)?.reference),
            Kind::Data { blob } | Kind::Page { blob } => {
                let bytes = self.blobs.get(blob).ok_or(Failure::Corrupt)?;
                if matches!(o.kind, Kind::Data { .. }) && bytes.iter().all(|b| *b == 0) {
                    return Ok(None);
                }
                Ok(Some(Reference::new(
                    *info.view.binding().store(),
                    info.input().generation,
                    ordinal(o.level, o.slot),
                    if matches!(o.kind, Kind::Page { .. }) {
                        Page::decode(bytes)?.digest()
                    } else {
                        *blob
                    },
                )?))
            }
        }
    }
    fn validate_object(&self, info: &Info, o: &Object) -> Result<()> {
        // These must precede offset/span arithmetic on untrusted persisted rows.
        if !(-1..=root_level(info.view.size())).contains(&o.level)
            || o.slot >= count(info.view.size(), o.level)
            || o.length > MAX_FILE_BYTES
        {
            return Err(Failure::Corrupt.into());
        }
        let (offset, length) = geometry(info.view.size(), o.level, o.slot)?;
        match &o.kind {
            Kind::Zero => {
                if o.level != -1
                    || o.length == 0
                    || o.end() > info.view.size()
                    || (!o.length.is_multiple_of(manifest::CHUNK_BYTES)
                        && o.end() != info.view.size())
                    || !matches!(info.view.intent_span(offset),Some(Range::Zero{length:n}) if n>=o.length)
                {
                    return Err(Failure::Corrupt.into());
                }
            }
            Kind::Data { blob } => {
                let bytes = self.blobs.get(blob).ok_or(Failure::Corrupt)?;
                if o.level != -1 || o.length != length || bytes.len() as u64 != length {
                    return Err(Failure::Corrupt.into());
                }
                self.check_local_bytes(&info.view, offset, bytes)?;
            }
            Kind::Page { blob } => {
                let p = Page::decode(self.blobs.get(blob).ok_or(Failure::Corrupt)?)?;
                let base = RootDescriptor::decode(&info.input().base_root)?;
                if o.level < 0
                    || o.length != length
                    || p.volume() != base.volume()
                    || p.node() != base.node()
                    || p.start() != offset
                    || p.length() != length
                    || p.level() != o.level as u8
                {
                    return Err(Failure::Corrupt.into());
                }
            }
            Kind::Inherited(w) => {
                let p = w.resolve(info.input(), &info.view, &self.blobs)?;
                if (p.level, p.offset, p.length) != (o.level, offset, o.length)
                    || o.length != length
                    || o.length == 0
                    || (o.level >= 0 && o.level >= root_level(info.view.size()))
                    || !matches!(info.view.intent_span(offset),Some(Range::Base{offset:at,length:n}) if at==offset && n>=o.length)
                    || p.reference
                        .as_ref()
                        .is_some_and(|r| r.generation() == &info.input().generation)
                {
                    return Err(Failure::Corrupt.into());
                }
            }
        }
        Ok(())
    }
    pub fn check_local_bytes(&self, view: &View, offset: u64, bytes: &[u8]) -> Result<()> {
        let mut at = 0;
        for range in view.range(offset, bytes.len())? {
            let length = match &range {
                Range::Base { length, .. }
                | Range::Zero { length }
                | Range::Patch { length, .. } => *length as usize,
            };
            let supplied = bytes.get(at..at + length).ok_or(Failure::Corrupt)?;
            match range {
                Range::Zero { .. } if supplied.iter().any(|b| *b != 0) => {
                    return Err(Failure::Corrupt.into());
                }
                Range::Patch { block, offset, .. } => {
                    let old = self.views.blocks.get(&block).ok_or(Failure::Corrupt)?;
                    if old.get(offset as usize..offset as usize + length) != Some(supplied) {
                        return Err(Failure::Corrupt.into());
                    }
                }
                _ => {}
            }
            at += length;
        }
        if at != bytes.len() {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
    pub fn inherited_cover(&self, plan: Id, offset: u64, level: i8) -> Option<&Object> {
        self.objects
            .range((plan, -1, 0)..=(plan, 2, u64::MAX))
            .map(|(_, s)| &s.object)
            .find(|o| {
                o.inherited()
                    && o.level >= 0
                    && o.level >= level
                    && o.offset() <= offset
                    && o.end() > offset
            })
    }
    pub(super) fn data_at(&self, plan: Id, offset: u64) -> Result<&Object> {
        if let Some(o) = self
            .objects
            .get(&(plan, -1, offset / manifest::CHUNK_BYTES))
        {
            return Ok(&o.object);
        }
        self.objects
            .range((plan, -1, 0)..=(plan, -1, offset / manifest::CHUNK_BYTES))
            .rev()
            .map(|(_, s)| &s.object)
            .find(|o| matches!(o.kind, Kind::Zero) && o.offset() <= offset && o.end() > offset)
            .ok_or_else(|| Failure::Corrupt.into())
    }
    pub fn page(&self, info: &Info, level: i8, slot: u64) -> Result<Page> {
        let plan = info.id()?;
        let (offset, length) = geometry(info.view.size(), level, slot)?;
        let mut entries = Vec::new();
        let child_level = level - 1;
        let child_span = span(child_level);
        let first = offset / child_span;
        let stop = (first + 128).min(count(info.view.size(), child_level));
        let mut child = first;
        while child < stop {
            let at = child * child_span;
            let mut n = (info.view.size() - at).min(child_span);
            let o = if child_level < 0 {
                self.data_at(plan, at)?
            } else {
                self.object(plan, child_level, child)?
            };
            let reference = self.reference(info, o)?;
            if child_level < 0 && matches!(o.kind, Kind::Zero) {
                n = o.end().min(offset + length) - at;
                child += n.div_ceil(manifest::CHUNK_BYTES);
            } else {
                child += 1;
            }
            let entry = if child_level >= 0 {
                Entry::Child {
                    length: n,
                    object: reference.ok_or(Failure::Corrupt)?,
                }
            } else if let Some(object) = reference {
                Entry::Data { length: n, object }
            } else {
                Entry::Hole { length: n }
            };
            if let (Some(Entry::Hole { length: a }), Entry::Hole { length: b }) =
                (entries.last_mut(), &entry)
            {
                *a += *b;
            } else {
                entries.push(entry);
            }
        }
        let old = RootDescriptor::decode(&info.input().base_root)?;
        let p = Page::new(*old.volume(), *old.node(), offset, level as u8, entries)?;
        if p.length() != length {
            return Err(Failure::Corrupt.into());
        }
        Ok(p)
    }
    fn validate_plan(&self, info: &Info) -> Result<()> {
        let plan = info.id()?;
        info.input().validate(&info.view, &self.blobs)?;
        let base = RootDescriptor::decode(&info.input().base_root)?;
        if &info.input().generation == base.page().generation() {
            return Err(Failure::Corrupt.into());
        }
        let cursor = info.progress.cursor;
        let size = info.view.size();
        let root_level = root_level(size);
        if !(-1..=root_level).contains(&cursor.level)
            || cursor.offset > size
            || (cursor.level == -1
                && (cursor.offset >= size || cursor.slot != 0 || info.progress.root.is_some()))
            || (cursor.level >= 0 && cursor.offset != size)
        {
            return Err(Failure::Corrupt.into());
        }
        let mut inputs = Vec::new();
        for (_, s) in self.objects.range((plan, -1, 0)..=(plan, 2, u64::MAX)) {
            self.validate_object(info, &s.object)?;
            if cursor.level < 0 && matches!(s.object.kind, Kind::Page { .. }) {
                return Err(Failure::Corrupt.into());
            }
            if !matches!(s.object.kind, Kind::Page { .. }) {
                inputs.push(&s.object);
            }
        }
        inputs.sort_by_key(|o| o.offset());
        let mut end = 0;
        let inherited: Vec<_> = inputs
            .iter()
            .copied()
            .filter(|o| o.inherited() && o.level >= 0)
            .collect();
        for o in inputs {
            if o.offset() != end || o.end() > cursor.offset {
                return Err(Failure::Corrupt.into());
            }
            end = o.end();
        }
        if end != cursor.offset {
            return Err(Failure::Corrupt.into());
        }
        if cursor.level < 0 {
            return Ok(());
        }
        if info.progress.root.is_none() && cursor.slot >= count(size, cursor.level) {
            return Err(Failure::Corrupt.into());
        }
        if info.progress.root.is_some() && (cursor.level != root_level || cursor.slot != 1) {
            return Err(Failure::Corrupt.into());
        }
        for level in 0..=root_level {
            for slot in 0..count(size, level) {
                let offset = slot * span(level);
                let covered = inherited
                    .iter()
                    .any(|o| o.level >= level && o.offset() <= offset && o.end() > offset);
                let built = info.progress.root.is_some()
                    || level < cursor.level
                    || (level == cursor.level && slot < cursor.slot);
                let row = self.objects.get(&(plan, level, slot));
                if covered {
                    if row.is_some_and(|r| matches!(r.object.kind, Kind::Page { .. })) {
                        return Err(Failure::Corrupt.into());
                    }
                    continue;
                }
                if built {
                    let o = &row.ok_or(Failure::Corrupt)?.object;
                    let Kind::Page { blob } = o.kind else {
                        return Err(Failure::Corrupt.into());
                    };
                    if self.blobs.get(&blob).ok_or(Failure::Corrupt)?
                        != &self.page(info, level, slot)?.encode()
                    {
                        return Err(Failure::Corrupt.into());
                    }
                } else if row.is_some() {
                    return Err(Failure::Corrupt.into());
                }
            }
        }
        if let Some(root) = info.root()? {
            let o = self.object(plan, root_level, 0)?;
            if root.volume() != base.volume()
                || root.node() != base.node()
                || root.length() != size
                || Some(root.page()) != self.reference(info, o)?.as_ref()
            {
                return Err(Failure::Corrupt.into());
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        let mut used = BTreeSet::new();
        let mut operations = BTreeSet::new();
        let mut generations = BTreeSet::new();
        for (plan, saved) in &self.plans {
            let info = saved.info(&self.views)?;
            let b = info.view.binding();
            let base = b.base()?;
            if *plan != saved.input.id()?
                || !operations.insert(info.input().operation)
                || !generations.insert((
                    base.volume().to_owned(),
                    base.node().to_owned(),
                    *b.store(),
                    b.root().to_owned(),
                    info.input().generation,
                ))
            {
                return Err(Failure::Corrupt.into());
            }
            self.validate_plan(&info)?;
            used.insert(info.input().base_page);
        }
        for ((plan, _, _), saved) in &self.objects {
            if !self.plans.contains_key(plan) {
                return Err(Failure::Corrupt.into());
            }
            used.extend(saved.object.blobs());
        }
        if used.len() != self.blobs.len() || used.iter().any(|id| !self.blobs.contains_key(id)) {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
