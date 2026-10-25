//! Exact local acknowledgment. No S3 access, reclamation or current-global claim.
use super::*;
use crate::sparse_views::{
    self as views,
    ledger::Ledger,
    model::{Change, Record as ViewRecord, Request as ViewRequest},
};
use s3drive_core::manifest::base::Descriptor;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acknowledgment {
    pub(super) format: u8,
    pub(super) operation: Id,
    pub(super) observed_head: Id,
    pub(super) clean_view: Option<Id>,
}
impl fmt::Debug for Acknowledgment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SparseAcknowledgment([private; historical])")
    }
}
impl Acknowledgment {
    pub fn operation(&self) -> Id {
        self.operation
    }
    pub fn observed_head(&self) -> Id {
        self.observed_head
    }
    pub fn clean_view(&self) -> Option<Id> {
        self.clean_view
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct SourceReceipt {
    record: std::sync::Arc<Record>,
}
impl fmt::Debug for SourceReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SparseSourceReceipt([exact view; no reclamation])")
    }
}
impl SourceReceipt {
    pub fn plan(&self) -> Id {
        self.record.input.plan
    }
    pub fn view(&self) -> Id {
        self.record.input.view
    }
    pub fn acknowledgment(&self) -> &Acknowledgment {
        self.record
            .acknowledgment
            .as_ref()
            .expect("validated acknowledgment")
    }
    pub fn record(&self) -> &Record {
        &self.record
    }
    pub(super) fn from_record(record: Record) -> Result<Self> {
        record.validate()?;
        record.acknowledgment.as_ref().ok_or(Failure::Missing)?;
        Ok(Self {
            record: std::sync::Arc::new(record),
        })
    }
}
pub(super) fn request(r: &Record) -> Result<ViewRequest> {
    request_with_reference(r, None)
}
pub(super) fn request_with_reference(
    r: &Record,
    prepared: Option<Option<&str>>,
) -> Result<ViewRequest> {
    let receipt = r.receipt.as_ref().ok_or(Failure::Incomplete)?;
    let payload = receipt.node.payload.as_ref().ok_or(Failure::Corrupt)?;
    let derived;
    let reference = match prepared {
        Some(reference) => reference,
        None => {
            derived = ns::payload_reference(&receipt.node.payload);
            derived.as_deref()
        }
    }
    .ok_or(Failure::Corrupt)?;
    let root = r.input.root()?;
    let old = &r.input.binding;
    let binding = Binding::new(
        Descriptor::from_hex(
            &payload.reservation.volume,
            &payload.reservation.node,
            &payload.reservation.id,
            reference,
        )?,
        *old.store(),
        *old.source(),
        old.fingerprint().into(),
        old.root().into(),
        root.length(),
    )?;
    let q = ViewRequest {
        format: 3,
        binding,
        size: root.length(),
        edit: Some(Change::Rebase {
            parent: r.input.view,
            publication: r.input.plan,
        }),
    };
    q.validate()?;
    Ok(q)
}
pub(super) fn validate_record_with_reference(
    r: &Record,
    reference: Option<Option<&str>>,
) -> Result<()> {
    let Some(a) = &r.acknowledgment else {
        return Ok(());
    };
    if a.format != 1
        || a.operation == [0; 32]
        || a.observed_head == [0; 32]
        || r.receipt.is_none()
        || r.rejection.is_some()
    {
        return Err(Failure::Corrupt.into());
    }
    match a.clean_view {
        Some(id) => {
            if a.observed_head != r.input.view
                || id
                    != views::hash(
                        b"s3drive-sparse-rebase-view-v1\0",
                        &(a.operation, request_with_reference(r, reference)?),
                    )?
            {
                return Err(Failure::Corrupt.into());
            }
        }
        None if a.observed_head == r.input.view => return Err(Failure::Corrupt.into()),
        None => (),
    }
    Ok(())
}
fn descends(s: &Ledger, older: Id, newer: Id) -> Result<bool> {
    let mut at = Some(newer);
    for _ in 0..views::HARD_VIEWS {
        let Some(id) = at else { return Ok(false) };
        if id == older {
            return Ok(true);
        }
        at = s.parent(id)?;
    }
    Err(Failure::Corrupt.into())
}
// Old supported migration fixtures predate the publication tables. A current
// schema must never forget acknowledgment identities when a table is lost.
fn legacy_without_publication(db: &Connection) -> Result<bool> {
    let exists: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='sparse_publications')", [], |q| q.get(0))?;
    if exists {
        return Ok(false);
    }
    let version: i32 = db.pragma_query_value(None, "user_version", |q| q.get(0))?;
    if (29..=32).contains(&version) {
        return Ok(true);
    }
    Err(Failure::Corrupt.into())
}
/// Called once per source snapshot. Only metadata inspection is allowed here;
/// calling root validate_source would recurse through preparation/source loading.
pub(in crate::sparse_views) fn validate_views(db: &Connection, s: &Ledger) -> Result<()> {
    if legacy_without_publication(db)? {
        if s.records
            .values()
            .any(|v| matches!(v.record.request.edit, Some(Change::Rebase { .. })))
        {
            return Err(Failure::Corrupt.into());
        }
        return Ok(());
    }
    let meta = ledger::inspect(db)?;
    validate_views_with_meta(s, &meta)
}
pub(in crate::sparse_views) fn validate_views_with_meta(
    s: &Ledger,
    meta: &ledger::Meta,
) -> Result<()> {
    for r in meta.records.values() {
        let Some(a) = &r.acknowledgment else { continue };
        let published = s.record(r.input.view)?;
        let observed = s.record(a.observed_head)?;
        if published.request.binding != r.input.binding
            || published.request.size != r.input.root()?.length()
            || !published
                .request
                .binding
                .same_scope(&observed.request.binding)?
            || !descends(s, published.id, observed.id)?
        {
            return Err(Failure::Corrupt.into());
        }
        match a.clean_view {
            Some(id) => {
                let expected = ViewRecord::child(a.operation, request(r)?, published)?;
                if s.record(id)? != &expected || s.operation(a.operation) != Some(&expected) {
                    return Err(Failure::Corrupt.into());
                }
            }
            None if s.operation(a.operation).is_some() => return Err(Failure::Corrupt.into()),
            None => (),
        }
    }
    for row in s.records.values() {
        if let Some(Change::Rebase { publication, .. }) = &row.record.request.edit {
            if let Some(anchor) = s.anchor(row.record.id) {
                if anchor.view() != &row.record || meta.records.contains_key(publication) {
                    return Err(Failure::Corrupt.into());
                }
                continue;
            }
            let r = meta.records.get(publication).ok_or(Failure::Corrupt)?;
            let a = r.acknowledgment.as_ref().ok_or(Failure::Corrupt)?;
            if a.clean_view != Some(row.record.id) || a.operation != row.record.operation {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    Ok(())
}
/// Historical acknowledgments reserve operation IDs even without a new view.
pub(in crate::sparse_views) fn guard_operation(db: &Connection, op: Id) -> Result<()> {
    if legacy_without_publication(db)? {
        return Ok(());
    }
    let meta = ledger::inspect(db)?;
    if meta
        .records
        .values()
        .any(|r| r.acknowledgment.as_ref().is_some_and(|a| a.operation == op))
    {
        return Err(views::Failure::Conflict.into());
    }
    Ok(())
}
pub(super) fn acknowledge(
    state: &mut State,
    operation: Id,
    scope: Option<views::ScopedOperation>,
    plan: Id,
    options: views::Options,
) -> Result<SourceReceipt> {
    options.validate()?;
    if operation == [0; 32] {
        return Err(Failure::Invalid.into());
    }
    let tx = state
        .connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let before = ledger::inspect(&tx)?;
    let prepared = super::super::ledger::inspect(&tx)?;
    ledger::validate_source_snapshot(&tx, &before, &prepared)?;
    let source = prepared.views;
    let r = before.records.get(&plan).ok_or(Failure::Missing)?;
    if let Some(a) = &r.acknowledgment {
        if a.operation != operation {
            return Err(Failure::Conflict.into());
        }
        let observed = source.record(a.observed_head)?;
        views::scoped::known(&tx, scope, operation, &observed.request.binding)?;
        return SourceReceipt::from_record(r.clone());
    }
    r.receipt.as_ref().ok_or(Failure::Incomplete)?;
    guard_operation(&tx, operation)?;
    if source.operation(operation).is_some() {
        return Err(Failure::Conflict.into());
    }
    let published = source.record(r.input.view)?;
    let target = r.input.binding.target()?;
    let file = source.files.get(&target).ok_or(Failure::Corrupt)?;
    views::scoped::admit(&tx, scope, operation, &file.binding)?;
    if !descends(&source, published.id, file.head)? {
        return Err(Failure::Conflict.into());
    }
    let mut expected = source.clone();
    let mut a = Acknowledgment {
        format: 1,
        operation,
        observed_head: file.head,
        clean_view: None,
    };
    if file.head == published.id {
        let view = ViewRecord::child(operation, request(r)?, published)?;
        let saved = views::ledger::Saved::new(view.clone())?;
        let raw = views::encode(&view.request.binding)?;
        let f = expected.files.get_mut(&target).ok_or(Failure::Corrupt)?;
        f.binding = view.request.binding.clone();
        f.raw = raw.clone();
        f.head = view.id;
        if expected.records.insert(view.id, saved.clone()).is_some() {
            return Err(Failure::Corrupt.into());
        }
        expected.admit(options)?;
        views::mutations::store_record(&tx, &saved, &mut |boundary| {
            let point = match boundary {
                views::mutations::Boundary::ViewStored => "ViewStored",
                views::mutations::Boundary::OperationStored => "OperationStored",
                _ => return Err(Failure::Corrupt.into()),
            };
            checkpoint("acknowledge", point);
            Ok(())
        })?;
        if tx.execute("UPDATE sparse_view_files SET binding=?1,head=?2,state_hash=?3 WHERE target=?4 AND head=?5 AND binding=?6",
            params![raw,view.id.as_slice(),views::hash(b"s3drive-sparse-binding-v1\0",&view.request.binding)?.as_slice(),target.as_slice(),file.head.as_slice(),file.raw])?!=1 {
            return Err(Failure::Corrupt.into());
        }
        checkpoint("acknowledge", "HeadUpdated");
        a.clean_view = Some(view.id);
    }
    let mut after = before.clone();
    let r = after.records.get_mut(&plan).ok_or(Failure::Missing)?;
    r.acknowledgment = Some(a);
    r.format = if r.input.owned.is_some() { 4 } else { 3 };
    r.validate()?;
    let result = SourceReceipt::from_record(r.clone())?;
    ledger::save(&tx, &before, &after, true)?;
    if views::ledger::inspect(&tx)? != expected {
        return Err(Failure::Corrupt.into());
    }
    tx.commit()?;
    checkpoint("acknowledge", "Committed");
    Ok(result)
}

/// Check only immutable source metadata when recovering an existing local result.
/// Patch/staging BLOBs and the mutable current head are deliberately not read.
pub(super) fn validate_recovery(db: &Connection, r: &Record) -> Result<()> {
    let Some(a) = &r.acknowledgment else {
        return Ok(());
    };
    fn view(db: &Connection, id: Id) -> Result<ViewRecord> {
        let (target,raw,sha):(Vec<u8>,Vec<u8>,Vec<u8>)=db.query_row(
            "SELECT target,record,state_hash FROM sparse_views WHERE view_id=?1 AND length(record) BETWEEN 1 AND 1048576",
            [id.as_slice()],|q|Ok((q.get(0)?,q.get(1)?,q.get(2)?)))?;
        let v: ViewRecord = views::decode(&raw)?;
        if v.id != id
            || views::encode(&v)? != raw
            || sha != views::hash(b"s3drive-sparse-view-v1\0", &v)?
            || target != v.request.binding.target()?
        {
            return Err(Failure::Corrupt.into());
        }
        let (key,request,sha):(Vec<u8>,Vec<u8>,Vec<u8>)=db.query_row(
            "SELECT view_id,request,state_hash FROM sparse_view_operations WHERE operation=?1 AND length(request) BETWEEN 1 AND 4096",
            [v.operation.as_slice()],|q|Ok((q.get(0)?,q.get(1)?,q.get(2)?)))?;
        if key != v.id
            || request != views::encode(&v.request)?
            || sha != views::hash(b"s3drive-sparse-request-v1\0", &(v.operation, &v.request))?
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(v)
    }
    let published = view(db, r.input.view)?;
    if published.request.binding != r.input.binding
        || published.request.size != r.input.root()?.length()
    {
        return Err(Failure::Corrupt.into());
    }
    match a.clean_view {
        Some(id) => {
            let clean = view(db, id)?;
            if clean != ViewRecord::child(a.operation, request(r)?, &published)? {
                return Err(Failure::Corrupt.into());
            }
        }
        None => {
            if db.query_row(
                "SELECT EXISTS(SELECT 1 FROM sparse_view_operations WHERE operation=?1)",
                [a.operation.as_slice()],
                |q| q.get::<_, bool>(0),
            )? {
                return Err(Failure::Corrupt.into());
            }
        }
    }
    Ok(())
}
