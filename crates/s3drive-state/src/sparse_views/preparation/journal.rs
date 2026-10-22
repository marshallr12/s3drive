//! Private-owner transactions. No network I/O or new upload/clean receipts.
use super::*;
use ledger::{Saved, SavedObject, Snapshot};
use model::{Progress, count, geometry, ordinal, root_level, span};
pub struct Journal<'a> {
    pub(super) state: &'a mut State,
    options: Options,
}
impl<'a> Journal<'a> {
    pub fn new(state: &'a mut State, options: Options) -> Result<Self> {
        state.verify_staging_storage()?;
        Ok(Self {
            state,
            options: options.validate()?,
        })
    }
    pub fn info(&mut self, plan: Id) -> Result<Info> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let result = ledger::inspect(&tx)?.info(plan)?;
        tx.commit()?;
        Ok(result)
    }
    /// Private local lookup for a lost begin reply; no shared-authority
    /// operation adoption or network permission follows from this metadata.
    pub fn operation(&mut self, operation: Id) -> Result<Option<Info>> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let saved = ledger::inspect(&tx)?;
        let result = saved
            .plans
            .values()
            .find(|p| p.input.operation == operation)
            .map(|p| p.info(&saved.views))
            .transpose()?;
        tx.commit()?;
        Ok(result)
    }
    pub fn begin(&mut self, request: Request) -> Result<Info> {
        self.begin_inner(request, None)
    }
    pub fn begin_owned(
        &mut self,
        request: Request,
        key: &crate::chunk_mount::Key,
        open: &str,
        claim: crate::namespace::RemovalPublicationClaim,
    ) -> Result<Info> {
        self.begin_inner(request, Some((key.clone(), open.into(), claim)))
    }
    fn begin_inner(
        &mut self,
        request: Request,
        ownership: Option<(
            crate::chunk_mount::Key,
            String,
            crate::namespace::RemovalPublicationClaim,
        )>,
    ) -> Result<Info> {
        request_bounds(&request)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (info, changed) = begin_in_transaction(
            &tx,
            self.options,
            self.state.incarnation.as_deref(),
            &self.state.native_sparse,
            request,
            Admission {
                ownership,
                ..Admission::default()
            },
        )?;
        if changed {
            tx.commit()?;
            checkpoint("begin", "Committed");
        }
        Ok(info)
    }
    /// Exact candidate retry after progress/newer heads returns actual progress.
    /// A stale cursor with different content is a conflict, never an acknowledgment.
    pub fn append(&mut self, plan: Id, expected: Cursor, candidate: Candidate) -> Result<Info> {
        candidate_bounds(&candidate)?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = ledger::inspect(&tx)?;
        let info = before.info(plan)?;
        owned::require_unsettled(&info)?;
        if expected.level != -1
            || expected.slot != 0
            || expected.offset >= info.view.size()
            || !expected.offset.is_multiple_of(manifest::CHUNK_BYTES)
        {
            return Err(Failure::Bounds.into());
        }
        let mut after = before.clone();
        let object = candidate_object(&mut after, &info, expected.offset, candidate)?;
        let key = (plan, object.level, object.slot);
        if info.cursor() != expected {
            if before.objects.get(&key).is_some_and(|s| s.object == object) {
                return Ok(info);
            }
            return Err(Failure::Conflict.into());
        }
        if info.progress.root.is_some() {
            return Err(Failure::Conflict.into());
        }
        if info.native_ownership().is_some() {
            owned::fresh(
                &tx,
                self.state.incarnation.as_deref(),
                &self.state.native_sparse,
                &info,
            )?;
        }
        let end = object.end();
        if after
            .objects
            .insert(key, SavedObject::new(object)?)
            .is_some()
        {
            return Err(Failure::Corrupt.into());
        }
        let mut progress = info.progress.clone();
        progress.cursor.offset = end;
        if end == info.view.size() {
            progress.cursor.level = 0;
            progress.cursor.slot = 0;
        }
        after.plans.insert(plan, Saved::new(info.input, progress)?);
        after.validate()?;
        after.admit(self.options)?;
        write_delta(&tx, &before, &after, "append")?;
        let result = after.info(plan)?;
        tx.commit()?;
        checkpoint("append", "Committed");
        Ok(result)
    }
    /// One bounded page or subtree-skip step; input is computed in the owner.
    /// Stale cursors return actual progress without accepting caller content.
    pub fn step(&mut self, plan: Id, expected: Cursor) -> Result<Info> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let before = ledger::inspect(&tx)?;
        let info = before.info(plan)?;
        owned::require_unsettled(&info)?;
        if info.cursor() != expected || info.progress.root.is_some() {
            return Ok(info);
        }
        if expected.level < 0 {
            return Err(Failure::Invalid.into());
        }
        if info.native_ownership().is_some() {
            owned::fresh(
                &tx,
                self.state.incarnation.as_deref(),
                &self.state.native_sparse,
                &info,
            )?;
        }
        let mut after = before.clone();
        let mut progress = info.progress.clone();
        if let Some(cover) =
            before.inherited_cover(plan, expected.slot * span(expected.level), expected.level)
        {
            progress.cursor.slot = cover
                .end()
                .div_ceil(span(expected.level))
                .min(count(info.view.size(), expected.level));
        } else {
            let page = before.page(&info, expected.level, expected.slot)?;
            let blob = after.put_blob(page.encode())?;
            let object = Object {
                level: expected.level,
                slot: expected.slot,
                length: page.length(),
                kind: Kind::Page { blob },
            };
            let key = (plan, object.level, object.slot);
            if after
                .objects
                .insert(key, SavedObject::new(object)?)
                .is_some()
            {
                return Err(Failure::Corrupt.into());
            }
            progress.cursor.slot += 1;
            if expected.level == root_level(info.view.size()) {
                let old = RootDescriptor::decode(&info.input.base_root)?;
                let reference = Reference::new(
                    *info.view.binding().store(),
                    info.input.generation,
                    ordinal(expected.level, expected.slot),
                    page.digest(),
                )?;
                progress.root = Some(
                    RootDescriptor::new(*old.volume(), *old.node(), info.view.size(), reference)?
                        .encode()
                        .to_vec(),
                );
            }
        }
        if progress.root.is_none()
            && progress.cursor.slot == count(info.view.size(), expected.level)
        {
            progress.cursor.level += 1;
            progress.cursor.slot = 0;
        }
        after.plans.insert(plan, Saved::new(info.input, progress)?);
        after.validate()?;
        after.admit(self.options)?;
        write_delta(&tx, &before, &after, "step")?;
        let result = after.info(plan)?;
        tx.commit()?;
        checkpoint("step", "Committed");
        Ok(result)
    }
    /// Inspect one retained dependency as offline intent, never a send permit.
    pub fn dependency(&mut self, plan: Id, level: i8, slot: u64) -> Result<Dependency> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let s = ledger::inspect(&tx)?;
        let info = s.info(plan)?;
        let o = s.object(plan, level, slot)?;
        let role = if level < 0 {
            s3drive_core::manifest::objects::Role::Data
        } else {
            s3drive_core::manifest::objects::Role::Page
        };
        let result = match (&o.kind, s.reference(&info, o)?) {
            (_, None) => Dependency::Hole {
                offset: o.offset(),
                length: o.length,
            },
            (Kind::Inherited(w), Some(reference)) => Dependency::Inherited {
                role,
                reference,
                proof: w.encode(&s.blobs)?,
            },
            (Kind::Data { blob } | Kind::Page { blob }, Some(reference)) => Dependency::New {
                role,
                reference,
                bytes: s.blobs.get(blob).ok_or(Failure::Corrupt)?.clone(),
            },
            _ => return Err(Failure::Corrupt.into()),
        };
        tx.commit()?;
        Ok(result)
    }
}
/// Tagged local preparation intent. No historical remote receipt is invented.
pub enum Dependency {
    New {
        role: s3drive_core::manifest::objects::Role,
        reference: Reference,
        bytes: Vec<u8>,
    },
    Inherited {
        role: s3drive_core::manifest::objects::Role,
        reference: Reference,
        proof: Vec<u8>,
    },
    Hole {
        offset: u64,
        length: u64,
    },
}
impl fmt::Debug for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedDependency([offline private intent])")
    }
}
fn request_bounds(r: &Request) -> Result<()> {
    if r.operation == [0; 32]
        || r.generation == [0; 16]
        || r.base_root.len() != s3drive_core::manifest::objects::ROOT_BYTES
        || !(64..=manifest::MAX_PAGE_BYTES).contains(&r.base_page.len())
    {
        return Err(Failure::Invalid.into());
    }
    Ok(())
}
fn candidate_bounds(c: &Candidate) -> Result<()> {
    let valid = match c {
        Candidate::Bytes(b) => !b.is_empty() && b.len() <= manifest::CHUNK_BYTES as usize,
        Candidate::Zero { length } => *length > 0 && *length <= MAX_FILE_BYTES,
        Candidate::InheritedChunk(b) | Candidate::InheritedPage(b) => {
            b.len() >= 49 && b.len() <= 50 + 2 * (2 + manifest::MAX_PAGE_BYTES)
        }
    };
    if !valid {
        return Err(Failure::Bounds.into());
    }
    Ok(())
}
fn candidate_object(
    s: &mut Snapshot,
    info: &Info,
    offset: u64,
    candidate: Candidate,
) -> Result<Object> {
    let slot = offset / manifest::CHUNK_BYTES;
    let (_, length) = geometry(info.view.size(), -1, slot)?;
    Ok(match candidate {
        Candidate::Bytes(bytes) => {
            if bytes.len() as u64 != length {
                return Err(Failure::Bounds.into());
            }
            s.check_local_bytes(&info.view, offset, &bytes)?;
            Object {
                level: -1,
                slot,
                length,
                kind: Kind::Data {
                    blob: s.put_blob(bytes)?,
                },
            }
        }
        Candidate::Zero { length } => Object {
            level: -1,
            slot,
            length,
            kind: Kind::Zero,
        },
        Candidate::InheritedChunk(raw) => inherited_object(s, info, offset, &raw, false)?,
        Candidate::InheritedPage(raw) => inherited_object(s, info, offset, &raw, true)?,
    })
}
fn inherited_object(
    s: &mut Snapshot,
    info: &Info,
    offset: u64,
    raw: &[u8],
    page: bool,
) -> Result<Object> {
    let witness = proof::Witness::admit(raw, page, &mut s.blobs)?;
    let p = witness.resolve(&info.input, &info.view, &s.blobs)?;
    if p.offset != offset {
        return Err(Failure::Conflict.into());
    }
    Ok(Object {
        level: p.level,
        slot: offset / span(p.level),
        length: p.length,
        kind: Kind::Inherited(witness),
    })
}
pub(super) fn write_delta(
    db: &Connection,
    before: &Snapshot,
    after: &Snapshot,
    kind: &str,
) -> Result<()> {
    ledger::no_triggers(db)?;
    for (id, bytes) in &after.blobs {
        if let Some(old) = before.blobs.get(id) {
            if old != bytes {
                return Err(Failure::Corrupt.into());
            }
            continue;
        }
        if db.execute(
            "INSERT INTO sparse_preparation_blobs VALUES(?1,?2)",
            params![id.as_slice(), bytes],
        )? != 1
        {
            return Err(Failure::Corrupt.into());
        }
        checkpoint(kind, "BlobStored");
    }
    for (id, s) in &after.plans {
        let changed = if let Some(old) = before.plans.get(id) {
            if old.input_raw != s.input_raw {
                return Err(Failure::Corrupt.into());
            }
            if old == s {
                continue;
            }
            db.execute(
                "UPDATE sparse_preparations SET state=?2,state_hash=?3 WHERE plan_id=?1",
                params![id.as_slice(), &s.state_raw, s.state_hash()?.as_slice()],
            )?
        } else {
            db.execute(
                "INSERT INTO sparse_preparations VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    id.as_slice(),
                    s.input.operation.as_slice(),
                    s.input.view.as_slice(),
                    &s.input_raw,
                    &s.state_raw,
                    s.state_hash()?.as_slice()
                ],
            )?
        };
        if changed != 1 {
            return Err(Failure::Corrupt.into());
        }
        checkpoint(
            kind,
            if before.plans.contains_key(id) {
                "StateUpdated"
            } else {
                "PlanStored"
            },
        );
    }
    for ((plan, level, slot), o) in &after.objects {
        if let Some(old) = before.objects.get(&(*plan, *level, *slot)) {
            if old != o {
                return Err(Failure::Corrupt.into());
            }
            continue;
        }
        if db.execute(
            "INSERT INTO sparse_preparation_objects VALUES(?1,?2,?3,?4,?5)",
            params![
                plan.as_slice(),
                level,
                integer(*slot)?,
                &o.raw,
                o.state_hash(*plan)?.as_slice()
            ],
        )? != 1
        {
            return Err(Failure::Corrupt.into());
        }
        checkpoint(kind, "ObjectStored");
    }
    if &ledger::inspect(db)? != after {
        return Err(Failure::Corrupt.into());
    }
    checkpoint(kind, "Validated");
    Ok(())
}
#[derive(Clone)]
pub struct Client {
    pub(super) owner: Owner,
    options: Options,
    chunk: Option<crate::chunk_mount::Key>,
}
impl Client {
    /// Identity of the live private worker, not matching paths or stored IDs.
    pub fn same_views(&self, views: &super::super::Client) -> bool {
        self.owner.same_owner(&views.owner)
    }
    pub(crate) fn for_chunks(
        owner: Owner,
        options: Options,
        key: crate::chunk_mount::Key,
    ) -> Result<Self> {
        Ok(Self {
            owner,
            options: options.validate()?,
            chunk: Some(key),
        })
    }
    pub async fn begin_owned(
        &self,
        request: Request,
        open: String,
        claim: crate::namespace::RemovalPublicationClaim,
    ) -> Result<Info> {
        let key = self.chunk.clone().ok_or(Failure::Conflict)?;
        self.call(move |j| j.begin_owned(request, &key, &open, claim))
            .await
    }
    pub async fn settle(&self, plan: Id, settlement: owned::Settlement) -> Result<Info> {
        self.call(move |j| j.settle(plan, settlement)).await
    }
    pub(crate) fn new(owner: Owner, options: Options) -> Result<Self> {
        Ok(Self {
            owner,
            options: options.validate()?,
            chunk: None,
        })
    }
    pub(super) async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Journal<'_>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let options = self.options;
        self.owner
            .call(move |s| f(&mut Journal::new(&mut s.state, options)?))
            .await
    }
    pub async fn operation(&self, operation: Id) -> Result<Option<Info>> {
        self.call(move |j| j.operation(operation)).await
    }
    pub async fn begin(&self, request: Request) -> Result<Info> {
        request_bounds(&request)?;
        self.call(move |j| j.begin(request)).await
    }
    pub async fn info(&self, plan: Id) -> Result<Info> {
        self.call(move |j| j.info(plan)).await
    }
    pub async fn append(&self, plan: Id, expected: Cursor, candidate: Candidate) -> Result<Info> {
        candidate_bounds(&candidate)?;
        self.call(move |j| j.append(plan, expected, candidate))
            .await
    }
    pub async fn step(&self, plan: Id, expected: Cursor) -> Result<Info> {
        self.call(move |j| j.step(plan, expected)).await
    }
    pub async fn dependency(&self, plan: Id, level: i8, slot: u64) -> Result<Dependency> {
        self.call(move |j| j.dependency(plan, level, slot)).await
    }
}

#[cfg(not(test))]
pub(super) fn checkpoint(_kind: &str, _point: &str) {}
#[cfg(test)]
pub(super) fn checkpoint(kind: &str, point: &str) {
    if std::env::var("S3DRIVE_MIXED_CRASH_KIND").as_deref() != Ok(kind)
        || std::env::var("S3DRIVE_MIXED_CRASH_POINT").as_deref() != Ok(point)
    {
        return;
    }
    let Some(path) = std::env::var_os("S3DRIVE_MIXED_CRASH_READY") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, b"ready").unwrap();
    std::fs::rename(temporary, path).unwrap();
    use std::io::Read;
    let _ = std::io::stdin().read(&mut [0u8]);
}

#[derive(Default)]
pub(super) struct Admission {
    pub ownership: Option<(
        crate::chunk_mount::Key,
        String,
        crate::namespace::RemovalPublicationClaim,
    )>,
    pub native: Option<native::Provenance>,
    pub finalizing: bool,
}
/// The caller must commit all source/open records with this preparation atomically.
pub(super) fn begin_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    options: Options,
    incarnation: Option<&str>,
    native_runtime: &native::Runtime,
    request: Request,
    admission: Admission,
) -> Result<(Info, bool)> {
    let before = ledger::inspect(tx)?;
    let owned = admission
        .ownership
        .as_ref()
        .map(|(key, open, claim)| owned::capture(tx, key, open, claim))
        .transpose()?;
    let input = Input {
        operation: request.operation,
        view: request.view,
        generation: request.generation,
        base_root: request.base_root,
        base_page: digest(&request.base_page),
        owned,
        native: admission.native,
    };
    if let Some(old) = before
        .plans
        .values()
        .find(|p| p.input.operation == input.operation)
    {
        if old.input != input || before.blobs.get(&input.base_page) != Some(&request.base_page) {
            return Err(Failure::Conflict.into());
        }
        return Ok((old.info(&before.views)?, false));
    }
    let view = View(before.views.record(input.view)?.clone());
    let base = view.binding().base()?;
    native::admit(
        tx,
        incarnation,
        native_runtime,
        &view,
        input.native.as_ref(),
        admission.finalizing,
    )?;
    match (&admission.ownership, &input.owned) {
        (Some((key, _, _)), Some(provenance)) => owned::admit(
            tx,
            incarnation.ok_or(Failure::Conflict)?,
            key,
            provenance,
            view.binding(),
        )?,
        (None, None) => {
            crate::namespace_client::authorize_sparse_preparation(tx, base.volume(), base.node())?
        }
        _ => return Err(Failure::Corrupt.into()),
    }
    for old in before.plans.values() {
        if old.input.generation == input.generation
            && before
                .views
                .record(old.input.view)?
                .request
                .binding
                .target()?
                == view.binding().target()?
        {
            return Err(Failure::Conflict.into());
        }
    }
    let plan = input.id()?;
    let mut after = before.clone();
    after.put_blob(request.base_page)?;
    let progress = Progress {
        cursor: Cursor {
            offset: 0,
            level: if view.size() == 0 { 0 } else { -1 },
            slot: 0,
        },
        root: None,
        settlement: None,
    };
    if after
        .plans
        .insert(plan, Saved::new(input, progress)?)
        .is_some()
    {
        return Err(Failure::Conflict.into());
    }
    after.validate()?;
    after.admit(options)?;
    write_delta(tx, &before, &after, "begin")?;
    if after
        .plans
        .get(&plan)
        .is_some_and(|p| p.input.native.is_some())
    {
        crate::namespace_client::opens::sparse_capacity(tx)?;
    }
    let result = after.info(plan)?;
    Ok((result, true))
}
