use super::*;
#[derive(Clone, Default, PartialEq, Eq)]
pub(in crate::sparse_views) struct Meta {
    pub records: BTreeMap<Id, Record>,
    pub(super) checks: BTreeMap<(Id, u64), Check>,
}
impl Meta {
    pub fn empty(&self) -> bool {
        self.records.is_empty() && self.checks.is_empty()
    }
    pub(super) fn role(&self) -> Result<Id> {
        hash(
            b"s3drive-mixed-root-role-v1\0",
            &(
                &self.records.iter().collect::<Vec<_>>(),
                &self
                    .checks
                    .iter()
                    .map(|(k, v)| (k.0, k.1, v))
                    .collect::<Vec<_>>(),
            ),
        )
    }
    pub fn semantic_hash(&self, plan: Id) -> Result<Id> {
        hash(
            b"s3drive-mixed-root-semantics-v1\0",
            &(
                plan,
                self.checks
                    .range((plan, 0)..=(plan, u64::MAX))
                    .map(|(_, c)| c)
                    .collect::<Vec<_>>(),
            ),
        )
    }
    pub fn validate(&self) -> Result<()> {
        if self.records.len() > MAX_RECORDS || self.checks.len() > MAX_CHECKS {
            return Err(Failure::Corrupt.into());
        }
        let mut operations = BTreeSet::new();
        let mut acknowledgments = BTreeSet::new();
        for (plan, r) in &self.records {
            r.validate()?;
            lineage::validate(self, r)?;
            if let Some(a) = &r.acknowledgment
                && !acknowledgments.insert(a.operation())
            {
                return Err(Failure::Corrupt.into());
            }
            if *plan != r.input.plan
                || !operations.insert(&r.input.reserve.operation)
                || !operations.insert(&r.input.publish_operation)
                || r.semantics != self.semantic_hash(*plan)?
            {
                return Err(Failure::Corrupt.into());
            }
        }
        for ((plan, slot), c) in &self.checks {
            if *plan != c.plan
                || *slot != c.slot
                || c.plan == [0; 32]
                || c.slot >= super::super::MAX_FILE_BYTES / s3drive_core::manifest::CHUNK_BYTES
                || c.original.is_some() && c.witness.is_none()
            {
                return Err(Failure::Corrupt.into());
            }
        }
        Ok(())
    }
    pub fn admit(&self, limits: Limits) -> Result<()> {
        // Reserve each admitted request's full maximum record footprint. Later
        // authenticated replies may grow it; recovery must not need a fresh
        // metadata-admission quota after the original request was saved.
        let mut bytes = (self.records.len() as u64) * (MAX_RECORD as u64);
        for check in self.checks.values() {
            bytes = bytes
                .checked_add(serde_json::to_vec(check)?.len() as u64)
                .ok_or(Failure::Corrupt)?;
        }
        if self.records.len() > limits.plans
            || self.checks.len() > limits.checks
            || bytes > limits.metadata_bytes
        {
            return Err(Failure::Quota.into());
        }
        Ok(())
    }
}
pub(super) fn no_triggers(db: &Connection) -> Result<()> {
    if db
        .prepare_cached("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')")?
        .query_row([], |r| r.get::<_, bool>(0))?
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn id(raw: Vec<u8>) -> Result<Id> {
    raw.try_into().map_err(|_| Failure::Corrupt.into())
}
fn count(db: &Connection, sql: &str) -> Result<u64> {
    let n: i64 = db.prepare_cached(sql)?.query_row([], |r| r.get(0))?;
    u64::try_from(n).map_err(|_| Failure::Corrupt.into())
}
#[cfg(test)]
thread_local! { static INSPECTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
pub(in crate::sparse_views) fn inspection_count() -> usize {
    INSPECTIONS.get()
}
pub(in crate::sparse_views) fn inspect(db: &Connection) -> Result<Meta> {
    #[cfg(test)]
    INSPECTIONS.set(INSPECTIONS.get() + 1);
    let records = count(db, "SELECT count(*) FROM sparse_publications")?;
    let checks = count(db, "SELECT count(*) FROM sparse_publication_checks")?;
    let roles = count(db, "SELECT count(*) FROM sparse_publication_role")?;
    if records > MAX_RECORDS as u64 || checks > MAX_CHECKS as u64 || roles > 1 {
        return Err(Failure::Corrupt.into());
    }
    if roles == 0 && records == 0 && checks == 0 {
        return Ok(Meta::default());
    }
    no_triggers(db)?;
    if roles != 1
        || records == 0 && checks == 0
        || !db.pragma_query_value(None, "foreign_keys", |r| r.get::<_, bool>(0))?
    {
        return Err(Failure::Corrupt.into());
    }
    for sql in [
        "SELECT EXISTS(SELECT 1 FROM sparse_publications WHERE typeof(plan_id)!='blob' OR length(plan_id)!=32 OR typeof(reserve_id)!='text' OR length(reserve_id)!=32 OR typeof(publish_id)!='text' OR length(publish_id)!=32 OR typeof(record)!='blob' OR length(record) NOT BETWEEN 1 AND 65536 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
        "SELECT EXISTS(SELECT 1 FROM sparse_publication_checks WHERE typeof(plan_id)!='blob' OR length(plan_id)!=32 OR typeof(slot)!='integer' OR slot NOT BETWEEN 0 AND 65535 OR typeof(record)!='blob' OR length(record) NOT BETWEEN 1 AND 2048 OR typeof(state_hash)!='blob' OR length(state_hash)!=32)",
    ] {
        if db
            .prepare_cached(sql)?
            .query_row([], |r| r.get::<_, bool>(0))?
        {
            return Err(Failure::Corrupt.into());
        }
    }
    if count(
        db,
        "SELECT coalesce((SELECT sum(length(record)) FROM sparse_publications),0)+coalesce((SELECT sum(length(record)) FROM sparse_publication_checks),0)",
    )? > MAX_METADATA
    {
        return Err(Failure::Corrupt.into());
    }
    let mut meta = Meta::default();
    let mut q=db.prepare_cached("SELECT plan_id,reserve_id,publish_id,record,state_hash FROM sparse_publications ORDER BY plan_id")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Vec<u8>>(3)?,
            r.get::<_, Vec<u8>>(4)?,
        ))
    })? {
        let (plan, reserve, publish, raw, sha) = row?;
        let plan = id(plan)?;
        let r: Record = serde_json::from_slice(&raw)?;
        if serde_json::to_vec(&r)? != raw
            || hash(b"s3drive-mixed-root-record-v1\0", &r)? != id(sha)?
            || r.input.reserve.operation != reserve
            || r.input.publish_operation != publish
            || meta.records.insert(plan, r).is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut q=db.prepare_cached("SELECT plan_id,slot,record,state_hash FROM sparse_publication_checks ORDER BY plan_id,slot")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })? {
        let (plan, slot, raw, sha) = row?;
        let plan = id(plan)?;
        let slot = u64::try_from(slot).map_err(|_| Failure::Corrupt)?;
        let c: Check = serde_json::from_slice(&raw)?;
        if serde_json::to_vec(&c)? != raw
            || hash(b"s3drive-mixed-root-check-v1\0", &c)? != id(sha)?
            || meta.checks.insert((plan, slot), c).is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    meta.validate()?;
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for r in meta.records.values() {
        if let Some(p) = &r.input.owned {
            if version < 47 {
                return Err(Failure::Corrupt.into());
            }
            p.historical(db, &r.input.binding)?;
            let raw: Vec<u8> = db.prepare_cached("SELECT CASE WHEN typeof(input)='blob' AND length(input) BETWEEN 1 AND 65536 THEN input END FROM sparse_preparations WHERE plan_id=?1")?.query_row( [r.input.plan.as_slice()], |row|row.get(0))?;
            let source: super::super::model::Input = serde_json::from_slice(&raw)?;
            if serde_json::to_vec(&source)? != raw
                || source.id()? != r.input.plan
                || source.owned.as_ref() != Some(p)
                || source.operation != r.input.operation
                || source.view != r.input.view
                || source.generation != r.input.generation
                || source.base_root != r.input.original_root
            {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    let (singleton, admitted, checked, sha): (i64, i64, i64, Vec<u8>) = db
        .prepare_cached(
            "SELECT singleton,admitted,checked,state_hash FROM sparse_publication_role",
        )?
        .query_row([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    if singleton != 1
        || admitted != records as i64
        || checked != checks as i64
        || id(sha)? != meta.role()?
    {
        return Err(Failure::Corrupt.into());
    }
    Ok(meta)
}
pub(super) fn coverage(meta: &Meta, s: &super::super::ledger::Snapshot, plan: Id) -> Result<()> {
    let info = s.info(plan)?;
    let root = digest(&info.root()?.ok_or(Failure::Incomplete)?.encode());
    let mut count = 0;
    for ((_, level, slot), o) in s.objects.range((plan, -1, 0)..=(plan, 2, u64::MAX)) {
        if let super::super::model::Kind::Data { blob } = o.object.kind {
            if *level != -1 {
                return Err(Failure::Corrupt.into());
            }
            let c = meta.checks.get(&(plan, *slot)).ok_or(Failure::Incomplete)?;
            if c.candidate != blob || c.root != root {
                return Err(Failure::Corrupt.into());
            }
            count += 1;
        }
    }
    if meta.checks.range((plan, 0)..=(plan, u64::MAX)).count() != count {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
pub(super) fn validate_checks(meta: &Meta, s: &super::super::ledger::Snapshot) -> Result<()> {
    for ((plan, slot), c) in &meta.checks {
        let info = s.info(*plan)?;
        let o = s.object(*plan, -1, *slot)?;
        if !matches!(o.kind,super::super::model::Kind::Data{blob} if blob==c.candidate)
            || c.root != digest(&info.root()?.ok_or(Failure::Corrupt)?.encode())
        {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(())
}
pub(super) fn validate_source(db: &Connection, meta: &Meta) -> Result<()> {
    let s = super::super::ledger::inspect(db)?;
    validate_source_snapshot(db, meta, &s)
}
pub(in crate::sparse_views::preparation) fn validate_source_snapshot(
    db: &Connection,
    meta: &Meta,
    s: &super::super::ledger::Snapshot,
) -> Result<()> {
    validate_checks(meta, s)?;
    if meta.records.is_empty() {
        return Ok(());
    }
    // `s` was fully validated in this same read transaction. Reuse it and one
    // whole physical-ledger proof for all roots, never across mutation/await.
    let trees = super::super::transfers::checked_trees(db, s)?;
    for (plan, r) in &meta.records {
        if !r.input.matches(&s.info(*plan)?)? {
            return Err(Failure::Corrupt.into());
        }
        if let Some(p) = r.input.predecessor {
            let prior = meta.records.get(&p).ok_or(Failure::Corrupt)?;
            if !lineage::ancestor(s, prior.input.view, r.input.view)? {
                return Err(Failure::Corrupt.into());
            }
        }
        coverage(meta, s, *plan)?;
        let tree = trees.tree(*plan)?;
        if Physical::from_tree(&tree)? != r.physical {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(())
}
pub(super) fn kind(before: &Meta, after: &Meta) -> &'static str {
    if after.checks.len() > before.checks.len() {
        return "check";
    }
    if after.records.len() > before.records.len() {
        return if after
            .records
            .iter()
            .any(|(p, r)| !before.records.contains_key(p) && r.input.predecessor.is_some())
        {
            "lineage_begin"
        } else {
            "begin"
        };
    }
    for (p, r) in &after.records {
        if let Some(old) = before.records.get(p) {
            if r.acknowledgment != old.acknowledgment {
                return "acknowledge";
            }
            if r.rejection != old.rejection {
                return "reject";
            }
            if r.receipt != old.receipt {
                return "published";
            }
            if r.publication != old.publication {
                return "prepare";
            }
            if r.reservation != old.reservation {
                return "reserved";
            }
        }
    }
    "replay"
}
pub(super) fn save(db: &Connection, before: &Meta, after: &Meta, source: bool) -> Result<()> {
    no_triggers(db)?;
    let kind = kind(before, after);
    after.validate()?;
    after.admit(Limits {
        plans: MAX_RECORDS,
        checks: MAX_CHECKS,
        metadata_bytes: MAX_METADATA,
    })?;
    if !before.records.keys().all(|k| after.records.contains_key(k))
        || !before.checks.keys().all(|k| after.checks.contains_key(k))
    {
        return Err(Failure::Corrupt.into());
    }
    for (plan, r) in &after.records {
        if before.records.get(plan) == Some(r) {
            continue;
        }
        let raw = serde_json::to_vec(r)?;
        if raw.len() > MAX_RECORD {
            return Err(Failure::Corrupt.into());
        }
        if db.execute("INSERT INTO sparse_publications(plan_id,reserve_id,publish_id,record,state_hash) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(plan_id) DO UPDATE SET record=excluded.record,state_hash=excluded.state_hash",params![plan.as_slice(),r.input.reserve.operation,r.input.publish_operation,raw,hash(b"s3drive-mixed-root-record-v1\0",r)?.as_slice()])?!=1{return Err(Failure::Corrupt.into());}
        checkpoint(kind, "RowStored");
    }
    for ((plan, slot), c) in &after.checks {
        if before.checks.get(&(*plan, *slot)) == Some(c) {
            continue;
        }
        if before.checks.contains_key(&(*plan, *slot)) {
            return Err(Failure::Conflict.into());
        }
        let raw = serde_json::to_vec(c)?;
        if raw.len() > 2048 {
            return Err(Failure::Corrupt.into());
        }
        if db.execute("INSERT INTO sparse_publication_checks(plan_id,slot,record,state_hash) VALUES(?1,?2,?3,?4)",params![plan.as_slice(),i64::try_from(*slot)?,raw,hash(b"s3drive-mixed-root-check-v1\0",c)?.as_slice()])?!=1{return Err(Failure::Corrupt.into());}
        checkpoint(kind, "RowStored");
    }
    if db.execute("INSERT INTO sparse_publication_role(singleton,admitted,checked,state_hash) VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET admitted=excluded.admitted,checked=excluded.checked,state_hash=excluded.state_hash",params![i64::try_from(after.records.len())?,i64::try_from(after.checks.len())?,after.role()?.as_slice()])?!=1{return Err(Failure::Corrupt.into());}
    checkpoint(kind, "RoleStored");
    if &inspect(db)? != after {
        return Err(Failure::Corrupt.into());
    }
    if source {
        validate_source(db, after)?;
    }
    checkpoint(kind, "Validated");
    Ok(())
}
