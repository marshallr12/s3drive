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
mod collect;

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
        self.publication.validate()?;
        let parent = self.published.source()?;
        let ack = self.publication.acknowledgment().ok_or(Failure::Corrupt)?;
        let request = settlement::request(&self.publication)?;
        if self.format != 1
            || self.owner != owner
            || owner == [0; 32]
            || self.input.owned.is_some()
            || self.input.native.is_some()
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
        self.publication.input.root()?.verify_page(&page)?;
        if digest(&page) != remote.payload_sha256 {
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
pub(in crate::sparse_views) fn load(db: &Connection) -> Result<BTreeMap<Id, Anchor>> {
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
        return if any {
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
        || !retry::enabled(db)?
    {
        return Err(Failure::Corrupt.into());
    }
    ledger::no_triggers(db)?;
    let mut anchors = BTreeMap::new();
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
        if serde_json::to_vec(&a)? != raw
            || target != a.target()?
            || view != a.view.id
            || generation != a.generation().value()
            || sha != hash(b"s3drive-retention-anchor-v1\0", &a)?
        {
            return Err(Failure::Corrupt.into());
        }
        a.validate(db, owner)?;
        if anchors.insert(a.target()?, a).is_some() {
            return Err(Failure::Corrupt.into());
        }
    }
    if anchors.len() != entries as usize || sha != role_hash(owner, &anchors)? {
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
