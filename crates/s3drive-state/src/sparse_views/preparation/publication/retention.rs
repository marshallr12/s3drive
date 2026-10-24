//! Explicit ordinary-owner local history retirement. Never deletes S3 objects.
use super::super::{
    model::{Input as PlanInput, Progress},
    retry,
};
use super::*;
use crate::sparse_views::{
    self as views,
    model::{Change, Record as Source, Request as SourceRequest},
};
use rusqlite::OptionalExtension;
mod collect;
mod native;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    format: u8,
    id: Id,
    operation: Id,
    request: SourceRequest,
    parent: Option<Id>,
}
impl Identity {
    fn capture(source: &Source) -> Self {
        Self {
            format: source.format,
            id: source.id,
            operation: source.operation,
            request: source.request.clone(),
            parent: source.parent,
        }
    }
    fn source(&self) -> Result<Source> {
        self.request.validate()?;
        let domain: &[u8] = match (self.format, &self.request.edit) {
            (1, None) if self.parent.is_none() => b"s3drive-sparse-initial-view-v1\0",
            (2, Some(Change::Write { .. } | Change::Resize { .. })) => {
                b"s3drive-sparse-child-view-v1\0"
            }
            (3, Some(Change::Rebase { .. })) => b"s3drive-sparse-rebase-view-v1\0",
            (4, Some(Change::Observe { .. })) => b"s3drive-sparse-observed-view-v1\0",
            _ => return Err(Failure::Corrupt.into()),
        };
        if self.operation == [0; 32]
            || self.id != views::hash(domain, &(self.operation, &self.request))?
            || self.parent != self.request.edit.as_ref().map(Change::parent)
        {
            return Err(Failure::Corrupt.into());
        }
        // Only a rebase constructor consumes this header. It derives full clean
        // coverage independently and never treats these missing dirty spans as
        // source bytes, an ordinary view or a source-authentication capability.
        Ok(Source {
            format: self.format,
            id: self.id,
            operation: self.operation,
            request: self.request.clone(),
            parent: self.parent,
            spans: Vec::new(),
        })
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::sparse_views) struct Anchor {
    format: u8,
    owner: Id,
    view: Source,
    published: Identity,
    input: PlanInput,
    progress: Progress,
    original_page: Vec<u8>,
    publication: Record,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native: Option<native::Terminal>,
}
impl Anchor {
    pub(in crate::sparse_views) fn view(&self) -> &Source {
        &self.view
    }
    pub(in crate::sparse_views) fn generation(&self) -> GenerationId {
        self.publication.physical.generation
    }
    pub(in crate::sparse_views) fn bytes(&self) -> Result<usize> {
        Ok(serde_json::to_vec(self)?.len())
    }
    fn target(&self) -> Result<Id> {
        self.view.request.binding.target()
    }
    fn validate(&self, db: &Connection, owner: Id) -> Result<()> {
        self.validate_in(&mut views::IsolationCheck::new(db), owner, None)
    }
    fn validate_in(
        &self,
        isolation: &mut views::IsolationCheck<'_>,
        owner: Id,
        scope: Option<&crate::chunk_mount::ReadScope<'_>>,
    ) -> Result<()> {
        let db = isolation.db;
        self.publication.validate()?;
        let parent = self.published.source()?;
        let ack = self.publication.acknowledgment().ok_or(Failure::Corrupt)?;
        let request = settlement::request(&self.publication)?;
        if !matches!(self.format, 1 | 2)
            || (self.format == 2) != self.native.is_some()
            || self.owner != owner
            || owner == [0; 32]
            || self.input.owned.is_some()
            || self.input.native.is_some() != self.native.is_some()
            || self.progress.settlement.is_some()
            || self.publication.input.owned.is_some()
            || self.publication.receipt.is_none()
            || self.view != Source::child(ack.operation(), request, &parent)?
            || ack.clean_view() != Some(self.view.id)
            || parent.id != self.publication.input.view
            || parent.request.binding != self.publication.input.binding
            || parent.request.size != self.publication.input.root()?.length()
            || parent.request.binding.epoch()? == self.view.request.binding.epoch()?
        {
            return Err(Failure::Corrupt.into());
        }
        let info = Info {
            input: self.input.clone(),
            view: views::View(parent),
            progress: self.progress.clone(),
        };
        if let Some(proof) = &self.native {
            let mount = match scope {
                Some(scope) => proof.validate_scoped(scope, &info, self.view.id)?,
                None => proof.validate_with_isolation(isolation, &info, self.view.id)?,
            };
            proof.verify_witness(&mount, &self.target()?)?;
        }
        self.input.validate(
            info.view(),
            &BTreeMap::from([(digest(&self.original_page), self.original_page.clone())]),
        )?;
        if !self.publication.input.matches(&info)? {
            return Err(Failure::Corrupt.into());
        }
        let receipt = crate::remote::checked_flushes(db)?
            .receipt(db, self.generation())?
            .ok_or(Failure::Corrupt)?;
        let remote = receipt.info();
        let reservation = self.publication.input.reservation()?;
        let revision = remote.revision.as_ref().ok_or(Failure::Corrupt)?;
        let physical = Physical {
            generation: remote.generation,
            operation: *remote.operation.as_bytes(),
            checksum: *receipt.checksum(),
            size: remote.size,
            sha256: remote.payload_sha256,
            etag: revision.etag().into(),
            version: revision.version().map(Into::into),
        };
        if physical != self.publication.physical
            || receipt.covered_generations() != 1
            || remote.guard != crate::remote::Guard::Missing
            || remote.phase != crate::remote::Phase::Verified
            || remote.target.source() != info.view().binding().source()
            || remote.target.fingerprint() != info.view().binding().fingerprint()
            || remote.target.object_key() != reservation.key
        {
            return Err(Failure::Corrupt.into());
        }
        let page = crate::journal::read_data(
            db,
            self.generation(),
            0,
            usize::try_from(remote.size)?,
            false,
        )?;
        let checked_page = self.publication.input.root()?.verify_page(&page)?;
        // Independently bind the compact plan to its original physical operation.
        // Rehashing a rewritten anchor must not substitute a different plan while
        // keeping a genuine receipt for this same root payload.
        let object = super::super::model::Object {
            level: checked_page.page().level() as i8,
            slot: 0,
            length: self.publication.input.root()?.length(),
            kind: super::super::model::Kind::Page {
                blob: digest(&page),
            },
        };
        let operation = super::super::transfers::object_operation(&info, &object, digest(&page))?;
        let mapped: Option<i64> = db
            .query_row(
                "SELECT generation_id FROM staged_operations WHERE operation_key=?1",
                [operation.as_slice()],
                |r| r.get(0),
            )
            .optional()?;
        if mapped != Some(self.generation().value()) || digest(&page) != remote.payload_sha256 {
            return Err(Failure::Corrupt.into());
        }
        Ok(())
    }
}
fn role_hash(owner: Id, anchors: &BTreeMap<Id, Anchor>) -> Result<Id> {
    hash(
        b"s3drive-retention-role-v1\0",
        &(owner, anchors.iter().collect::<Vec<_>>()),
    )
}
// Opaque proof retained solely by one private immutable ReadScope. The guard
// detects ordinary DML/DDL/PRAGMA misuse; it is NOT an arbitrary-SQL fence.
// Correctness still requires no writes, transaction-control SQL or awaits in
// that audited scope. In particular, deliberately rewriting schema cookies or
// raw COMMIT/BEGIN is forbidden, not made safe by these supplementary counters.
pub(crate) struct CheckedAnchors {
    anchors: BTreeMap<Id, Anchor>,
    guard: AnchorReadGuard,
}
#[derive(PartialEq, Eq)]
struct AnchorReadGuard {
    changes: u64,
    main_schema: i64,
    temp_schema: i64,
    user_version: i64,
    foreign_keys: bool,
}
impl AnchorReadGuard {
    fn capture(db: &Connection) -> Result<Self> {
        if db.is_autocommit() {
            return Err(Failure::Corrupt.into());
        }
        Ok(Self {
            changes: db.total_changes(),
            main_schema: db.pragma_query_value(Some("main"), "schema_version", |r| r.get(0))?,
            temp_schema: db.pragma_query_value(Some("temp"), "schema_version", |r| r.get(0))?,
            user_version: db.pragma_query_value(None, "user_version", |r| r.get(0))?,
            foreign_keys: db.pragma_query_value(None, "foreign_keys", |r| r.get(0))?,
        })
    }
}
// Always performs the full check, including during ReadScope construction.
// Never consult a previously installed set here.
pub(crate) fn check_anchors_scoped(
    scope: &crate::chunk_mount::ReadScope<'_>,
) -> Result<CheckedAnchors> {
    let anchors = load_in(
        &mut views::IsolationCheck::from_mount_scope(scope),
        Some(scope),
    )?;
    Ok(CheckedAnchors {
        anchors,
        guard: AnchorReadGuard::capture(scope.connection())?,
    })
}
fn canonical_anchor_hash(bytes: &[u8]) -> Id {
    let mut hash = Sha256::new();
    hash.update(b"s3drive-retention-anchor-v1\0");
    hash.update(bytes);
    hash.finalize().into()
}
// Exact streaming encoding of the existing role_hash tuple, not a new format.
// Anchor bytes may enter only after the caller proves canonical equality. Keep
// role_hash's independent serde implementation for writes and differential tests.
struct CanonicalRoleHash {
    hash: Sha256,
    previous: Option<Id>,
}
impl CanonicalRoleHash {
    fn new(owner: Id) -> Result<Self> {
        let mut hash = Sha256::new();
        hash.update(b"s3drive-retention-role-v1\0");
        hash.update(b"[");
        hash.update(serde_json::to_vec(&owner)?);
        hash.update(b",[");
        Ok(Self {
            hash,
            previous: None,
        })
    }
    fn push(&mut self, target: Id, canonical: &[u8]) -> Result<()> {
        if self.previous.is_some_and(|previous| previous >= target) {
            return Err(Failure::Corrupt.into());
        }
        if self.previous.is_some() {
            self.hash.update(b",");
        }
        self.hash.update(b"[");
        self.hash.update(serde_json::to_vec(&target)?);
        self.hash.update(b",");
        self.hash.update(canonical);
        self.hash.update(b"]");
        self.previous = Some(target);
        Ok(())
    }
    fn finish(mut self) -> Id {
        self.hash.update(b"]]");
        self.hash.finalize().into()
    }
}
#[cfg(test)]
thread_local! {
    static ANCHOR_LOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ANCHOR_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(in crate::sparse_views) fn inspection_counts() -> (usize, usize) {
    (ANCHOR_LOADS.get(), ANCHOR_CHECKS.get())
}
pub(in crate::sparse_views) fn load(db: &Connection) -> Result<BTreeMap<Id, Anchor>> {
    load_with_isolation(&mut views::IsolationCheck::new(db))
}
pub(in crate::sparse_views) fn load_scoped(
    scope: &crate::chunk_mount::ReadScope<'_>,
) -> Result<BTreeMap<Id, Anchor>> {
    if let Some(checked) = scope.anchors() {
        if !scope.retry_enabled() || checked.guard != AnchorReadGuard::capture(scope.connection())?
        {
            return Err(Failure::Corrupt.into());
        }
        return Ok(checked.anchors.clone());
    }
    load_in(
        &mut views::IsolationCheck::from_mount_scope(scope),
        Some(scope),
    )
}
pub(in crate::sparse_views) fn load_with_isolation(
    isolation: &mut views::IsolationCheck<'_>,
) -> Result<BTreeMap<Id, Anchor>> {
    load_in(isolation, None)
}
fn load_in(
    isolation: &mut views::IsolationCheck<'_>,
    scope: Option<&crate::chunk_mount::ReadScope<'_>>,
) -> Result<BTreeMap<Id, Anchor>> {
    let db = isolation.db;
    #[cfg(test)]
    ANCHOR_LOADS.set(ANCHOR_LOADS.get() + 1);
    let mut q = match db
        .prepare("SELECT singleton,owner,entries,state_hash FROM sparse_retention_role LIMIT 2")
    {
        Ok(q) => q,
        Err(error) => {
            let exists: i64 = db.query_row("SELECT count(*) FROM sqlite_schema WHERE type='table' AND name IN ('sparse_retention_role','sparse_retention_anchors')", [], |r|r.get(0))?;
            let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if exists == 0 && version < 50 {
                return Ok(BTreeMap::new());
            }
            return Err(error.into());
        }
    };
    let mut rows = q.query([])?;
    let Some(row) = rows.next()? else {
        let any: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sparse_retention_anchors)",
            [],
            |r| r.get(0),
        )?;
        return if any || !crate::chunk_mount::admission::witnesses(db)?.is_empty() {
            Err(Failure::Corrupt.into())
        } else {
            Ok(BTreeMap::new())
        };
    };
    let (singleton, owner, entries, sha): (i64, Vec<u8>, i64, Vec<u8>) =
        (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?);
    let owner: Id = owner.try_into().map_err(|_| Failure::Corrupt)?;
    let version: i32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if rows.next()?.is_some()
        || singleton != 1
        || owner == [0; 32]
        || !(1..=i64::from(views::HARD_FILES)).contains(&entries)
        || version < 50
        || !retry::enabled_with_isolation(isolation)?
    {
        return Err(Failure::Corrupt.into());
    }
    ledger::no_triggers(db)?;
    let mut anchors = BTreeMap::new();
    let mut role = CanonicalRoleHash::new(owner)?;
    let mut bytes = 0usize;
    let mut q=db.prepare("SELECT target,view_id,generation_id,record,state_hash FROM sparse_retention_anchors ORDER BY target LIMIT 257")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Vec<u8>>(3)?,
            r.get::<_, Vec<u8>>(4)?,
        ))
    })? {
        let (target, view, generation, raw, sha) = row?;
        bytes = bytes.checked_add(raw.len()).ok_or(Failure::Corrupt)?;
        if raw.len() > 2_097_152 || bytes > views::HARD_METADATA as usize {
            return Err(Failure::Corrupt.into());
        }
        let a: Anchor = serde_json::from_slice(&raw)?;
        // Canonical equality is still mandatory. Reuse only those exact bytes
        // for the identical domain-separated row hash, not unverified JSON.
        let canonical = serde_json::to_vec(&a)?;
        if canonical != raw
            || target != a.target()?
            || view != a.view.id
            || generation != a.generation().value()
            || sha != canonical_anchor_hash(&canonical)
        {
            return Err(Failure::Corrupt.into());
        }
        #[cfg(test)]
        ANCHOR_CHECKS.set(ANCHOR_CHECKS.get() + 1);
        a.validate_in(isolation, owner, scope)?;
        let target = a.target()?;
        role.push(target, &canonical)?;
        if anchors.insert(target, a).is_some() {
            return Err(Failure::Corrupt.into());
        }
    }
    let witnesses = anchors
        .iter()
        .filter_map(|(target, anchor)| anchor.native.as_ref().map(|proof| (target, proof)))
        .map(|(target, proof)| {
            Ok((
                target
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>(),
                proof.digest()?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    if witnesses != crate::chunk_mount::admission::witnesses(db)? {
        return Err(Failure::Corrupt.into());
    }
    if anchors.len() != entries as usize || sha != role.finish() {
        return Err(Failure::Corrupt.into());
    }
    Ok(anchors)
}
fn save(db: &Connection, anchors: &BTreeMap<Id, Anchor>, owner: Id, target: Id) -> Result<()> {
    let a = anchors.get(&target).ok_or(Failure::Corrupt)?;
    db.execute(
        "DELETE FROM sparse_retention_anchors WHERE target=?1",
        [target.as_slice()],
    )?;
    db.execute(
        "INSERT INTO sparse_retention_anchors VALUES(?1,?2,?3,?4,?5)",
        params![
            target.as_slice(),
            a.view.id.as_slice(),
            a.generation().value(),
            serde_json::to_vec(a)?,
            hash(b"s3drive-retention-anchor-v1\0", a)?.as_slice()
        ],
    )?;
    db.execute("INSERT INTO sparse_retention_role VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET owner=excluded.owner,entries=excluded.entries,state_hash=excluded.state_hash",params![owner.as_slice(),anchors.len() as i64,role_hash(owner,anchors)?.as_slice()])?;
    Ok(())
}

pub(crate) fn validate_native_anchors(db: &Connection) -> Result<()> {
    load(db).map(|_| ())
}

#[cfg(test)]
mod canonical_encoding_tests {
    use super::*;
    #[test]
    fn native_retry_validation_work_canonical_hash_bytes_match_independent_serde() {
        for count in [0usize, 1, 2, 256] {
            let owner = [254; 32];
            let rows = (0..count).map(|n| {
                let mut target = [255; 32]; target[0] = n as u8;
                let value = serde_json::json!({"unicode": "café 日本語 🦀", "escaped": "\"\\\n\t", "nested": [n, true, null, {"z": [0,255]}]});
                (target, value)
            }).collect::<BTreeMap<_, _>>();
            let mut encoded = CanonicalRoleHash::new(owner).unwrap();
            for (target, value) in &rows {
                let canonical = serde_json::to_vec(value).unwrap();
                assert_eq!(
                    canonical_anchor_hash(&canonical),
                    hash(b"s3drive-retention-anchor-v1\0", value).unwrap()
                );
                encoded.push(*target, &canonical).unwrap();
            }
            assert_eq!(
                encoded.finish(),
                hash(
                    b"s3drive-retention-role-v1\0",
                    &(owner, rows.iter().collect::<Vec<_>>())
                )
                .unwrap()
            );
        }
        for invalid in [[1; 32], [0; 32]] {
            let mut encoded = CanonicalRoleHash::new([9; 32]).unwrap();
            encoded.push([1; 32], b"{}").unwrap();
            assert!(encoded.push(invalid, b"{}").is_err());
        }
    }
}
