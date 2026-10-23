//! Exact mixed-plan physical handoff. All evidence here remains offline and
//! historical. Only the managed controller checks source semantics/admission.
use super::*;
use crate::{journal as staging, remote, worker::Storage};
use rusqlite::OptionalExtension;
use s3drive_core::manifest::objects::{Namespace, Role};
use staging::{GenerationId, Target};

/// Missing physical completion is distinct from corruption or an ID collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferFailure {
    Incomplete,
}
impl fmt::Display for TransferFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("mixed tree has incomplete physical uploads; data retained")
    }
}
impl std::error::Error for TransferFailure {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub level: i8,
    pub slot: u64,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Transfer {
    position: Position,
    target: Target,
    reference: Reference,
    size: u64,
    payload_sha256: Id,
    pub(super) operation: Id,
    generation: Option<GenerationId>,
}
impl fmt::Debug for Transfer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MixedObjectTransfer([offline private intent])")
    }
}
impl Transfer {
    pub fn position(&self) -> Position {
        self.position
    }
    pub fn target(&self) -> &Target {
        &self.target
    }
    pub fn reference(&self) -> &Reference {
        &self.reference
    }
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn payload_sha256(&self) -> &Id {
        &self.payload_sha256
    }
    pub fn generation(&self) -> Option<GenerationId> {
        self.generation
    }
    pub fn role(&self) -> Role {
        if self.position.level < 0 {
            Role::Data
        } else {
            Role::Page
        }
    }
    pub fn receipt_matches(&self, receipt: &remote::FlushReceipt) -> bool {
        let i = receipt.info();
        self.generation == Some(i.generation)
            && i.target == self.target
            && i.size == self.size
            && i.payload_sha256 == self.payload_sha256
            && i.guard == remote::Guard::Missing
            && i.phase == remote::Phase::Verified
            && receipt.covered_generations() == 1
    }
}
/// Distinct from the full-snapshot tree receipt. Does not authenticate source
/// bytes, publish a root, settle an edit view or establish a fresh send permit.
pub struct TreeReceipt {
    plan: Info,
    root: RootDescriptor,
    root_receipt: remote::FlushReceipt,
    new_objects: u32,
    inherited_objects: u32,
    hole_records: u32,
}
impl fmt::Debug for TreeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HistoricalMixedTreeReceipt([private; not file flush])")
    }
}
impl TreeReceipt {
    pub fn plan(&self) -> &Info {
        &self.plan
    }
    pub fn root(&self) -> &RootDescriptor {
        &self.root
    }
    pub fn root_receipt(&self) -> &remote::FlushReceipt {
        &self.root_receipt
    }
    pub fn new_objects(&self) -> u32 {
        self.new_objects
    }
    pub fn inherited_objects(&self) -> u32 {
        self.inherited_objects
    }
    pub fn hole_records(&self) -> u32 {
        self.hole_records
    }
}
fn no_triggers(db: &Connection) -> Result<()> {
    // Generic staging commands commit separately. Do not allow attached SQL to
    // change source/plan rows between these command boundaries.
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='trigger')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
fn inspect(db: &Connection) -> Result<ledger::Snapshot> {
    no_triggers(db)?;
    ledger::inspect(db)
}
fn ready(s: &ledger::Snapshot, plan: Id) -> Result<Info> {
    let i = s.info(plan)?;
    i.root()?.ok_or(Failure::Invalid)?;
    Ok(i)
}
pub(super) fn object_operation(i: &Info, o: &model::Object, blob: Id) -> Result<Id> {
    hash(
        b"s3drive-mixed-object-staging-v1\0",
        &(
            i.id()?,
            i.root()?.ok_or(Failure::Invalid)?.encode().to_vec(),
            o.level,
            o.slot,
            o,
            blob,
        ),
    )
}
pub(super) fn transfer(
    db: &Connection,
    s: &ledger::Snapshot,
    i: &Info,
    p: Position,
) -> Result<Option<Transfer>> {
    // `object` refuses non-admitted positions, including descendants skipped by
    // whole-subtree reuse and interior chunks of a structural zero run.
    let o = s.object(i.id()?, p.level, p.slot)?;
    let blob = match o.kind {
        Kind::Data { blob } | Kind::Page { blob } => blob,
        _ => return Ok(None),
    };
    let Some(reference) = s.reference(i, o)? else {
        return Ok(None);
    };
    let root = i.original_root()?;
    let ns = Namespace::new(
        i.view.binding().root().to_owned(),
        *root.volume(),
        *root.node(),
        *i.view.binding().store(),
    )?;
    let role = if p.level < 0 { Role::Data } else { Role::Page };
    let key = ns.key(role, &reference)?;
    let bytes = s.blobs.get(&blob).ok_or(Failure::Corrupt)?;
    let operation = object_operation(i, o, blob)?;
    let mapped: Option<i64> = db
        .query_row(
            "SELECT generation_id FROM staged_operations WHERE operation_key=?1",
            [operation.as_slice()],
            |r| r.get(0),
        )
        .optional()?;
    let t = Transfer {
        position: p,
        target: Target::new(
            *i.view.binding().source(),
            i.view.binding().fingerprint(),
            key.as_str(),
        )?,
        reference,
        size: bytes.len() as u64,
        payload_sha256: blob,
        operation,
        generation: mapped.map(GenerationId::from_database).transpose()?,
    };
    if let Some(g) = t.generation {
        let g = staging::load(db, g)?;
        if g.target != t.target
            || g.size != t.size
            || g.base != staging::Base::Missing
            || g.parent.is_some()
        {
            return Err(Failure::Conflict.into());
        }
        staging::validate_manifest(db, &g, false)?;
    }
    Ok(Some(t))
}
fn check_receipt(
    db: &Connection,
    checked: &remote::CheckedFlushes,
    t: &Transfer,
) -> Result<remote::FlushReceipt> {
    let g = t.generation.ok_or(TransferFailure::Incomplete)?;
    let r = checked.receipt(db, g)?.ok_or(TransferFailure::Incomplete)?;
    if !t.receipt_matches(&r) {
        return Err(Failure::Corrupt.into());
    }
    Ok(r)
}
fn children(s: &ledger::Snapshot, i: &Info, p: Position) -> Result<Vec<Position>> {
    let o = s.object(i.id()?, p.level, p.slot)?;
    if p.level < 0 || !matches!(o.kind, Kind::Page { .. }) {
        return Ok(Vec::new());
    }
    let level = p.level - 1;
    let span = model::span(level);
    let mut at = o.offset();
    let mut found = BTreeSet::new();
    while at < o.end() {
        let child = if level < 0 {
            s.data_at(i.id()?, at)?
        } else {
            s.object(i.id()?, level, at / span)?
        };
        found.insert(Position {
            level: child.level,
            slot: child.slot,
        });
        at = child.end().min(o.end());
    }
    Ok(found.into_iter().collect())
}
fn dependencies(
    db: &Connection,
    s: &ledger::Snapshot,
    i: &Info,
    p: Position,
) -> Result<Vec<Transfer>> {
    children(s, i, p)?
        .into_iter()
        .filter_map(|p| transfer(db, s, i, p).transpose())
        .collect()
}
impl Journal<'_> {
    /// Bounded lexicographic walk of admitted records; invalid cursors/limits
    /// fail, while an actual final position yields an empty next page.
    pub fn positions(
        &mut self,
        plan: Id,
        after: Option<Position>,
        limit: u16,
    ) -> Result<Vec<Position>> {
        if !(1..=128).contains(&limit) {
            return Err(Failure::Bounds.into());
        }
        let s = inspect(&self.state.connection)?;
        let i = ready(&s, plan)?;
        if let Some(p) = after {
            s.object(i.id()?, p.level, p.slot)?;
        }
        Ok(s.objects
            .range((plan, -1, 0)..=(plan, 2, u64::MAX))
            .filter_map(|((_, l, slot), _)| {
                let p = Position {
                    level: *l,
                    slot: *slot,
                };
                if after.is_none_or(|a| p > a) {
                    Some(p)
                } else {
                    None
                }
            })
            .take(limit as usize)
            .collect())
    }
    /// At most128 actual new uploads per page. Holes and inherited subtrees
    /// remain in the graph/provenance but never become upload work items.
    pub fn transfers(
        &mut self,
        plan: Id,
        after: Option<Position>,
        limit: u16,
    ) -> Result<Vec<Transfer>> {
        if !(1..=128).contains(&limit) {
            return Err(Failure::Bounds.into());
        }
        let s = inspect(&self.state.connection)?;
        let i = ready(&s, plan)?;
        let start = if let Some(p) = after {
            s.object(plan, p.level, p.slot)?;
            (plan, p.level, p.slot)
        } else {
            (plan, -1, 0)
        };
        let mut out = Vec::new();
        for ((_, level, slot), o) in s.objects.range(start..=(plan, 2, u64::MAX)) {
            let p = Position {
                level: *level,
                slot: *slot,
            };
            if after == Some(p) || !matches!(o.object.kind, Kind::Data { .. } | Kind::Page { .. }) {
                continue;
            }
            if let Some(t) = transfer(&self.state.connection, &s, &i, p)? {
                out.push(t);
            }
            if out.len() == limit as usize {
                break;
            }
        }
        Ok(out)
    }
    pub fn transfer(&mut self, plan: Id, p: Position) -> Result<Option<Transfer>> {
        let s = inspect(&self.state.connection)?;
        let i = ready(&s, plan)?;
        transfer(&self.state.connection, &s, &i, p)
    }
    /// Direct admitted children, including a zero run that starts in an earlier
    /// leaf. No descendants are invented below a retained inherited subtree.
    pub fn children(&mut self, plan: Id, p: Position) -> Result<Vec<Position>> {
        let s = inspect(&self.state.connection)?;
        let i = ready(&s, plan)?;
        children(&s, &i, p)
    }
    pub fn require_dependencies(&mut self, plan: Id, p: Position) -> Result<()> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let s = inspect(&tx)?;
        let i = ready(&s, plan)?;
        let children = dependencies(&tx, &s, &i, p)?;
        let checked = remote::checked_flushes(&tx)?;
        for t in children {
            check_receipt(&tx, &checked, &t)?;
        }
        drop(checked);
        tx.commit()?;
        Ok(())
    }
    /// Generic physical staging only. Caller must independently check exact-view
    /// semantics and provider admission before invoking a fresh materialization.
    pub fn materialize(
        &mut self,
        plan: Id,
        p: Position,
        options: staging::Options,
    ) -> Result<Option<Transfer>> {
        let info = self.info(plan)?;
        owned::fresh(
            &self.state.connection,
            self.state.incarnation.as_deref(),
            &self.state.native_sparse,
            &info,
        )?;
        self.require_dependencies(plan, p)?;
        let s = inspect(&self.state.connection)?;
        let i = ready(&s, plan)?;
        let Some(t) = transfer(&self.state.connection, &s, &i, p)? else {
            return Ok(None);
        };
        let o = s.object(plan, p.level, p.slot)?;
        let blob = match o.kind {
            Kind::Data { blob } | Kind::Page { blob } => blob,
            _ => return Err(Failure::Corrupt.into()),
        };
        let bytes = s.blobs.get(&blob).ok_or(Failure::Corrupt)?;
        let mut j = staging::Journal::new(self.state, options)?;
        let g = j.begin_operation(
            &t.operation,
            &t.target,
            staging::Base::Missing,
            None,
            t.size,
        )?;
        if j.info(g)?.phase == staging::Phase::Building {
            j.append_at(g, 0, bytes)?;
            j.seal(g)?;
        }
        let after = inspect(&self.state.connection)?;
        if s != after {
            return Err(Failure::Corrupt.into());
        }
        let actual = transfer(&self.state.connection, &after, &i, p)?.ok_or(Failure::Corrupt)?;
        if actual.generation != Some(g) {
            return Err(Failure::Corrupt.into());
        }
        let body = staging::read_data(&self.state.connection, g, 0, t.size as usize, false)?;
        if body != *bytes {
            return Err(Failure::Corrupt.into());
        }
        Ok(Some(actual))
    }
    pub fn tree_receipt(&mut self, plan: Id) -> Result<TreeReceipt> {
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let result = inspect_tree(&tx, plan)?;
        tx.commit()?;
        Ok(result)
    }
}
/// One immutable validation section in an exclusive owner command. Neither the
/// source snapshot nor physical proof may cross a write, commit or await.
/// Every tree still checks all admitted objects and each exact receipt.
pub(super) struct CheckedTrees<'a> {
    db: &'a Connection,
    source: &'a ledger::Snapshot,
    physical: remote::CheckedFlushes,
}
pub(super) fn checked_trees<'a>(
    db: &'a Connection,
    source: &'a ledger::Snapshot,
) -> Result<CheckedTrees<'a>> {
    no_triggers(db)?;
    Ok(CheckedTrees {
        db,
        source,
        physical: remote::checked_flushes(db)?,
    })
}
impl CheckedTrees<'_> {
    pub(super) fn tree(&self, plan: Id) -> Result<TreeReceipt> {
        inspect_tree_checked(self.db, self.source, &self.physical, plan)
    }
}
pub(super) fn inspect_tree(db: &Connection, plan: Id) -> Result<TreeReceipt> {
    let s = inspect(db)?;
    checked_trees(db, &s)?.tree(plan)
}
fn inspect_tree_checked(
    db: &Connection,
    s: &ledger::Snapshot,
    checked: &remote::CheckedFlushes,
    plan: Id,
) -> Result<TreeReceipt> {
    let i = ready(s, plan)?;
    let root = i.root()?.ok_or(Failure::Invalid)?;
    let mut root_receipt = None;
    let mut new_objects = 0;
    let mut inherited_objects = 0;
    let mut hole_records = 0;
    for ((_, level, slot), o) in s.objects.range((plan, -1, 0)..=(plan, 2, u64::MAX)) {
        if let Some(t) = transfer(
            db,
            s,
            &i,
            Position {
                level: *level,
                slot: *slot,
            },
        )? {
            let r = check_receipt(db, checked, &t)?;
            new_objects += 1;
            if t.reference() == root.page() {
                root_receipt = Some(r);
            }
        } else if s.reference(&i, &o.object)?.is_none() {
            hole_records += 1;
        } else {
            inherited_objects += 1;
        }
    }
    // Every raw record and every new-object receipt was inspected in this
    // one owner command. No cached graph proof crosses a mutation/await.
    let result = TreeReceipt {
        plan: i,
        root,
        root_receipt: root_receipt.ok_or(Failure::Corrupt)?,
        new_objects,
        inherited_objects,
        hole_records,
    };
    Ok(result)
}

impl Client {
    pub fn same_storage(&self, storage: &Storage) -> bool {
        storage.same_owner(&self.owner)
    }
    pub async fn positions(
        &self,
        plan: Id,
        after: Option<Position>,
        limit: u16,
    ) -> Result<Vec<Position>> {
        self.call(move |j| j.positions(plan, after, limit)).await
    }
    pub async fn transfers(
        &self,
        plan: Id,
        after: Option<Position>,
        limit: u16,
    ) -> Result<Vec<Transfer>> {
        self.call(move |j| j.transfers(plan, after, limit)).await
    }
    pub async fn transfer(&self, plan: Id, p: Position) -> Result<Option<Transfer>> {
        self.call(move |j| j.transfer(plan, p)).await
    }
    pub async fn children(&self, plan: Id, p: Position) -> Result<Vec<Position>> {
        self.call(move |j| j.children(plan, p)).await
    }
    pub async fn require_dependencies(&self, plan: Id, p: Position) -> Result<()> {
        self.call(move |j| j.require_dependencies(plan, p)).await
    }
    pub async fn materialize(
        &self,
        plan: Id,
        p: Position,
        storage: &Storage,
    ) -> Result<Option<Transfer>> {
        if !self.same_storage(storage) {
            return Err(Failure::Conflict.into());
        }
        let options = storage.staging_options();
        self.call(move |j| j.materialize(plan, p, options)).await
    }
    pub async fn tree_receipt(&self, plan: Id) -> Result<TreeReceipt> {
        self.call(move |j| j.tree_receipt(plan)).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture::*;
    use super::*;
    fn j(s: &mut State) -> Journal<'_> {
        Journal::new(s, Options::default()).unwrap()
    }
    fn ack(s: &mut State, t: &Transfer) {
        let mut r = remote::Journal::new(s, remote::Options::default()).unwrap();
        let i = r.prepare(t.generation().unwrap()).unwrap();
        let _permit = r.claim(i.operation, &i.target).unwrap();
        r.verified(remote::Verification {
            operation: i.operation,
            target: i.target,
            payload_sha256: i.payload_sha256,
            size: i.size,
            revision: remote::Revision::new("\"owned-model\"", None).unwrap(),
        })
        .unwrap();
        let receipt = r.acknowledge(i.operation).unwrap();
        assert!(t.receipt_matches(&receipt));
    }
    #[test]
    fn inherited_root_child_and_zero_run_require_only_new_page_receipts_in_exact_order() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut s = State::open_managed(&path).unwrap();
        let b = base(128 * C, BTreeMap::new());
        let v = view(&mut s, &b, 129 * C);
        let i = j(&mut s).begin(request(&v, &b, 9)).unwrap();
        let plan = i.id().unwrap();
        let i = j(&mut s)
            .append(
                plan,
                i.cursor(),
                Candidate::InheritedPage(proof(&b, 0, Some(0))),
            )
            .unwrap();
        let i = j(&mut s)
            .append(plan, i.cursor(), Candidate::Zero { length: C })
            .unwrap();
        let i = finish(&mut s, i);
        let root = Position { level: 1, slot: 0 };
        let leaf = Position { level: 0, slot: 1 };
        let positions = j(&mut s).positions(plan, None, 128).unwrap();
        assert_eq!(positions.len(), 4);
        let page = j(&mut s).transfers(plan, None, 1).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].position(), leaf);
        let page = j(&mut s).transfers(plan, Some(leaf), 1).unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].position(), root);
        assert!(
            j(&mut s)
                .transfers(plan, Some(root), 128)
                .unwrap()
                .is_empty()
        );
        assert!(j(&mut s).transfers(plan, None, 0).is_err());
        assert!(
            j(&mut s)
                .transfers(plan, Some(Position { level: 2, slot: 0 }), 128)
                .is_err()
        );
        assert_eq!(
            j(&mut s).children(plan, leaf).unwrap(),
            vec![Position {
                level: -1,
                slot: 128
            }]
        );
        assert_eq!(
            j(&mut s).children(plan, root).unwrap(),
            vec![Position { level: 0, slot: 0 }, leaf]
        );
        assert!(
            j(&mut s)
                .materialize(plan, root, staging::Options::default())
                .is_err()
        );
        assert!(
            j(&mut s)
                .transfer(plan, Position { level: 0, slot: 0 })
                .unwrap()
                .is_none()
        );
        assert!(
            j(&mut s)
                .transfer(plan, Position { level: -1, slot: 0 })
                .is_err(),
            "descendant was invented below inherited root"
        );
        let t = j(&mut s)
            .materialize(plan, leaf, staging::Options::default())
            .unwrap()
            .unwrap();
        assert_ne!(t.payload_sha256(), t.reference().sha256());
        ack(&mut s, &t);
        let r = j(&mut s)
            .materialize(plan, root, staging::Options::default())
            .unwrap()
            .unwrap();
        ack(&mut s, &r);
        let receipt = j(&mut s).tree_receipt(plan).unwrap();
        assert_eq!(receipt.root(), &i.root().unwrap().unwrap());
        assert_eq!(
            (
                receipt.new_objects(),
                receipt.inherited_objects(),
                receipt.hole_records()
            ),
            (2, 1, 1)
        );
        assert_eq!(
            j(&mut s)
                .materialize(plan, root, staging::Options::default())
                .unwrap()
                .unwrap(),
            r
        );
        drop(s);
        let mut s = State::open_managed(&path).unwrap();
        assert_eq!(j(&mut s).transfer(plan, root).unwrap().unwrap(), r);
        assert_eq!(j(&mut s).tree_receipt(plan).unwrap().new_objects(), 2);
        s.connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        s.connection
            .execute(
                "DELETE FROM remote_flushes WHERE generation_id=?1",
                [t.generation().unwrap().value()],
            )
            .unwrap();
        s.connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        assert!(j(&mut s).tree_receipt(plan).is_err());
        assert!(
            j(&mut s)
                .materialize(plan, root, staging::Options::default())
                .is_err()
        );
    }
    #[test]
    fn scalar_zero_and_empty_manifests_never_create_fake_data_transfers() {
        for size in [0, 17] {
            let tmp = tempfile::tempdir().unwrap();
            let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
            let b = base(size, BTreeMap::new());
            let v = view(&mut s, &b, size);
            let mut i = j(&mut s).begin(request(&v, &b, 8)).unwrap();
            let plan = i.id().unwrap();
            if size > 0 {
                i = j(&mut s)
                    .append(plan, i.cursor(), Candidate::Bytes(vec![0; size as usize]))
                    .unwrap();
            }
            finish(&mut s, i);
            if size > 0 {
                assert!(
                    j(&mut s)
                        .materialize(
                            plan,
                            Position { level: -1, slot: 0 },
                            staging::Options::default()
                        )
                        .unwrap()
                        .is_none()
                );
            }
            let t = j(&mut s)
                .materialize(
                    plan,
                    Position { level: 0, slot: 0 },
                    staging::Options::default(),
                )
                .unwrap()
                .unwrap();
            ack(&mut s, &t);
            let receipt = j(&mut s).tree_receipt(plan).unwrap();
            assert_eq!(receipt.new_objects(), 1);
            assert_eq!(receipt.hole_records(), u32::from(size > 0));
        }
    }
    #[test]
    fn every_physical_boundary_reopens_same_operation_and_refuses_collisions_and_triggers() {
        for boundary in 0..=3 {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("state");
            let mut s = State::open_managed(&path).unwrap();
            let b = base(18, BTreeMap::new());
            let v = view(&mut s, &b, 18);
            let i = j(&mut s).begin(request(&v, &b, 8)).unwrap();
            let plan = i.id().unwrap();
            let i = j(&mut s)
                .append(
                    plan,
                    i.cursor(),
                    Candidate::Bytes(b"unicode \xf0\x9f\xa6\x80 bytes".to_vec()),
                )
                .unwrap();
            finish(&mut s, i);
            let p = Position { level: -1, slot: 0 };
            let t = j(&mut s).transfer(plan, p).unwrap().unwrap();
            let mut generation = None;
            if boundary > 0 {
                let mut stage = staging::Journal::new(&mut s, staging::Options::default()).unwrap();
                let g = stage
                    .begin_operation(
                        &t.operation,
                        t.target(),
                        staging::Base::Missing,
                        None,
                        t.size(),
                    )
                    .unwrap();
                generation = Some(g);
                if boundary > 1 {
                    stage
                        .append_at(g, 0, b"unicode \xf0\x9f\xa6\x80 bytes")
                        .unwrap();
                }
                if boundary > 2 {
                    stage.seal(g).unwrap();
                }
            }
            drop(s);
            let mut s = State::open_managed(&path).unwrap();
            let actual = j(&mut s)
                .materialize(plan, p, staging::Options::default())
                .unwrap()
                .unwrap();
            assert_eq!(actual.operation, t.operation);
            if let Some(g) = generation {
                assert_eq!(actual.generation(), Some(g));
            }
            assert_eq!(
                j(&mut s)
                    .materialize(plan, p, staging::Options::default())
                    .unwrap()
                    .unwrap(),
                actual
            );
            s.connection.execute_batch("CREATE TRIGGER foreign_stage BEFORE INSERT ON staged_operations BEGIN SELECT RAISE(IGNORE); END").unwrap();
            assert!(j(&mut s).transfer(plan, p).is_err());
            s.connection
                .execute_batch("DROP TRIGGER foreign_stage")
                .unwrap();
            s.connection
                .execute(
                    "UPDATE staged_operations SET generation_id=999999 WHERE operation_key=?1",
                    [actual.operation.as_slice()],
                )
                .unwrap_err();
        }
    }
    #[test]
    fn staging_quota_and_actual_sqlite_full_preserve_exact_plan_and_resume_same_physical_id() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = State::open_managed(&tmp.path().join("state")).unwrap();
        let b = base(C, BTreeMap::new());
        let v = view(&mut s, &b, C);
        let bytes = vec![7; C as usize];
        let v = super::super::super::Journal::new(&mut s, super::super::super::Options::default())
            .unwrap()
            .write([5; 32], v.id(), 0, &bytes)
            .unwrap();
        let i = j(&mut s).begin(request(&v, &b, 8)).unwrap();
        let plan = i.id().unwrap();
        let i = j(&mut s)
            .append(plan, i.cursor(), Candidate::Bytes(bytes.clone()))
            .unwrap();
        finish(&mut s, i);
        let p = Position { level: -1, slot: 0 };
        let tiny = staging::Options {
            max_bytes: C - 1,
            ..staging::Options::default()
        };
        let err = j(&mut s).materialize(plan, p, tiny).unwrap_err();
        assert_eq!(
            err.downcast_ref::<staging::Failure>(),
            Some(&staging::Failure::QuotaExceeded)
        );
        let mapped = j(&mut s).transfer(plan, p).unwrap().unwrap();
        assert!(mapped.generation().is_some());
        s.connection
            .execute_batch("CREATE TABLE pressure(data BLOB)")
            .unwrap();
        let pages: i64 = s
            .connection
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        s.connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        loop {
            match s
                .connection
                .execute("INSERT INTO pressure VALUES(zeroblob(4096))", [])
            {
                Ok(_) => {}
                Err(e) => {
                    assert_eq!(e.sqlite_error_code(), Some(rusqlite::ErrorCode::DiskFull));
                    break;
                }
            }
        }
        let err = j(&mut s)
            .materialize(plan, p, staging::Options::default())
            .unwrap_err();
        assert_eq!(
            err.downcast_ref::<rusqlite::Error>()
                .and_then(|e| e.sqlite_error_code()),
            Some(rusqlite::ErrorCode::DiskFull)
        );
        assert_eq!(j(&mut s).transfer(plan, p).unwrap().unwrap(), mapped);
        s.connection
            .pragma_update(None, "max_page_count", 1_000_000)
            .unwrap();
        let staged = j(&mut s)
            .materialize(plan, p, staging::Options::default())
            .unwrap()
            .unwrap();
        assert_eq!(staged.generation(), mapped.generation());
        assert_eq!(
            staging::Journal::new(&mut s, staging::Options::default())
                .unwrap()
                .read(staged.generation().unwrap(), 0, C as usize)
                .unwrap(),
            bytes
        );
        assert_eq!(j(&mut s).info(plan).unwrap().view(), &v);
        assert_eq!(
            j(&mut s).materialize(plan, p, tiny).unwrap().unwrap(),
            staged,
            "narrowed quota defeated sealed replay"
        );
    }
}
