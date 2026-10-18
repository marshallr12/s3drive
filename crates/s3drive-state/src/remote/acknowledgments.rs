//! Flush receipts and local supersession metadata; no payloads are reclaimed.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct FlushReceipt {
    info: Info,
    covered: u32,
    checksum: [u8; 32],
}
impl FlushReceipt {
    /// Historical verified remote result for precisely this generation.
    pub fn info(&self) -> &Info {
        &self.info
    }
    /// This generation and its complete local ancestor chain, including any
    /// ancestors already acknowledged by earlier flushes.
    pub fn covered_generations(&self) -> u32 {
        self.covered
    }
    pub fn checksum(&self) -> &[u8; 32] {
        &self.checksum
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settlement {
    pub generation: GenerationId,
    pub covering_generation: GenerationId,
    pub covering_operation: OperationId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AckPoint {
    Root,
    Settlements,
    Committed,
}

impl Journal<'_> {
    /// Local-only and idempotent. The exact operation must already be Verified.
    /// Ancestor edits are settled by this full-file publication. Newer heads,
    /// local bytes, retained markers and all remote identities stay unchanged.
    pub fn acknowledge(&mut self, operation: OperationId) -> Result<FlushReceipt> {
        self.acknowledge_inner(operation, |_| Ok(()))
    }
    fn acknowledge_inner(
        &mut self,
        operation: OperationId,
        mut hook: impl FnMut(AckPoint) -> Result<()>,
    ) -> Result<FlushReceipt> {
        self.state.verify_staging_storage()?;
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        scan(&tx, |_| Ok(()))?;
        let prior = validate(&tx)?;
        let info = load_operation(&tx, operation)?;
        if info.phase != Phase::Verified {
            return Err(Failure::InvalidState.into());
        }
        if let Some(root) = prior.roots.get(&operation.0) {
            return Ok(receipt(info, root));
        }
        let mut chain = Vec::new();
        let mut next = Some(info.generation);
        while let Some(id) = next {
            if chain.len() >= MAX_RECORDS as usize {
                return Err(Failure::QuotaExceeded.into());
            }
            let generation = journal::load(&tx, id)?;
            journal::validate_manifest(&tx, &generation, false)?;
            if generation.target != info.target {
                return Err(Failure::Corrupt.into());
            }
            if let Some(other) = for_generation(&tx, id)?
                && !matches!(other.phase, Phase::Verified | Phase::Cancelled)
            {
                return Err(Failure::TargetBlocked.into());
            }
            next = generation.parent;
            chain.push((id, generation.manifest_hash.ok_or(Failure::Corrupt)?));
        }
        let new_nodes = chain
            .iter()
            .filter(|(id, _)| !prior.nodes.contains_key(id))
            .count();
        if prior.nodes.len() + new_nodes > MAX_RECORDS as usize
            || prior.roots.len() >= MAX_RECORDS as usize
        {
            return Err(Failure::QuotaExceeded.into());
        }
        let covered = chain.len() as u32;
        let checksum = receipt_hash(&info, covered);
        tx.execute(
            "INSERT INTO remote_flushes VALUES (?1,?2,?3,?4)",
            params![
                operation.0.as_slice(),
                info.generation.value(),
                covered,
                checksum.as_slice()
            ],
        )?;
        hook(AckPoint::Root)?;
        for (id, manifest) in chain {
            if !prior.nodes.contains_key(&id) {
                let hash = settlement_hash(id, manifest, checksum);
                tx.execute(
                    "INSERT INTO write_settlements VALUES (?1,?2,?3,?4)",
                    params![
                        id.value(),
                        operation.0.as_slice(),
                        manifest.as_slice(),
                        hash.as_slice()
                    ],
                )?;
            }
        }
        // Validate the complete forest before any row may suppress pending work.
        validate(&tx)?;
        hook(AckPoint::Settlements)?;
        tx.commit()?;
        hook(AckPoint::Committed)?;
        Ok(FlushReceipt {
            info,
            covered,
            checksum,
        })
    }
    pub fn flush_receipt(&self, generation: GenerationId) -> Result<Option<FlushReceipt>> {
        self.checked_flushes()?
            .receipt(&self.state.connection, generation)
    }
    pub(crate) fn checked_flushes(&self) -> Result<CheckedFlushes> {
        // Validate every root, settlement, manifest and ancestry interval before
        // any result can suppress work. Retain only bounded compact roots.
        let all = validate(&self.state.connection)?;
        Ok(CheckedFlushes { roots: all.roots })
    }
    pub fn settlement(&self, generation: GenerationId) -> Result<Option<Settlement>> {
        let all = validate(&self.state.connection)?;
        let Some(node) = all.nodes.get(&generation) else {
            return Ok(None);
        };
        let root = all.roots.get(&node.covering.0).ok_or(Failure::Corrupt)?;
        Ok(Some(Settlement {
            generation,
            covering_generation: root.generation,
            covering_operation: root.operation,
        }))
    }
}
fn for_generation(connection: &Connection, generation: GenerationId) -> Result<Option<Info>> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM remote_intents WHERE generation_id=?1)",
        [generation.value()],
        |r| r.get(0),
    )?;
    exists.then(|| load(connection, generation)).transpose()
}
struct Root {
    operation: OperationId,
    generation: GenerationId,
    covered: u32,
    checksum: [u8; 32],
    target: [u8; 32],
}
struct Node {
    parent: Option<GenerationId>,
    target: [u8; 32],
    base: [u8; 32],
    covering: OperationId,
    manifest: [u8; 32],
    checksum: [u8; 32],
}
struct Validated {
    roots: BTreeMap<[u8; 32], Root>,
    nodes: BTreeMap<GenerationId, Node>,
}
/// Private proof for one synchronous read command on the exclusive State owner.
/// Never cache this across commands, mutations or awaits. It is deliberately
/// neither Clone nor public, and contains no payloads or complete Info records.
pub(crate) struct CheckedFlushes {
    roots: BTreeMap<[u8; 32], Root>,
}
impl CheckedFlushes {
    pub(crate) fn receipt(
        &self,
        connection: &Connection,
        generation: GenerationId,
    ) -> Result<Option<FlushReceipt>> {
        let Some(root) = self.roots.values().find(|r| r.generation == generation) else {
            return Ok(None);
        };
        let info = load_operation(connection, root.operation)?;
        // Bind the reloaded complete result to the compact proof, even though
        // the owner must not mutate state during this command.
        if info.generation != generation
            || info.phase != Phase::Verified
            || receipt_hash(&info, root.covered) != root.checksum
            || target_hash(&info.target) != root.target
        {
            return Err(Failure::Corrupt.into());
        }
        Ok(Some(receipt(info, root)))
    }
}
fn receipt(info: Info, root: &Root) -> FlushReceipt {
    FlushReceipt {
        info,
        covered: root.covered,
        checksum: root.checksum,
    }
}
fn receipt_hash(info: &Info, covered: u32) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"s3drive-remote-flush-v1\0");
    h.update(info.state_hash);
    h.update(covered.to_be_bytes());
    h.finalize().into()
}
fn settlement_hash(generation: GenerationId, manifest: [u8; 32], root: [u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"s3drive-write-settlement-v1\0");
    h.update(generation.value().to_be_bytes());
    h.update(manifest);
    h.update(root);
    h.finalize().into()
}
fn target_hash(target: &Target) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(target.source());
    field(&mut h, Some(target.fingerprint().as_bytes()));
    field(&mut h, Some(target.object_key().as_bytes()));
    h.finalize().into()
}
fn base_hash(base: &Base) -> [u8; 32] {
    let mut h = Sha256::new();
    match base {
        Base::Missing => {
            h.update([0]);
        }
        Base::Existing { etag, version } => {
            h.update([1]);
            field(&mut h, etag.as_deref().map(str::as_bytes));
            field(&mut h, version.as_deref().map(str::as_bytes));
        }
    }
    h.finalize().into()
}
// Fixed-size row projections bound allocations even if a damaged DB contains
// huge blobs. Only compact hashes/IDs are retained; no bodies or whole histories.
fn validate(connection: &Connection) -> Result<Validated> {
    let mut roots = BTreeMap::new();
    let mut q = connection.prepare("SELECT length(operation_id),length(receipt_hash),substr(operation_id,1,32),generation_id,covered_generations,substr(receipt_hash,1,32) FROM remote_flushes NOT INDEXED")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        if roots.len() >= MAX_RECORDS as usize
            || row.get::<_, i64>(0)? != 32
            || row.get::<_, i64>(1)? != 32
        {
            return Err(Failure::Corrupt.into());
        }
        let operation = OperationId(digest(row.get(2)?)?);
        let generation = GenerationId::from_database(row.get(3)?)?;
        let covered: u32 = row.get(4)?;
        let checksum = digest(row.get(5)?)?;
        let info = load_operation(connection, operation)?;
        if info.generation != generation
            || info.phase != Phase::Verified
            || !(1..=MAX_RECORDS).contains(&covered)
            || receipt_hash(&info, covered) != checksum
        {
            return Err(Failure::Corrupt.into());
        }
        let target = target_hash(&info.target);
        if roots
            .insert(
                operation.0,
                Root {
                    operation,
                    generation,
                    covered,
                    checksum,
                    target,
                },
            )
            .is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut nodes = BTreeMap::new();
    let mut q = connection.prepare("SELECT generation_id,length(covering_operation),length(manifest_hash),length(settlement_hash),substr(covering_operation,1,32),substr(manifest_hash,1,32),substr(settlement_hash,1,32) FROM write_settlements NOT INDEXED")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        if nodes.len() >= MAX_RECORDS as usize
            || (1..=3).any(|i| row.get::<_, i64>(i).ok() != Some(32))
        {
            return Err(Failure::Corrupt.into());
        }
        let id = GenerationId::from_database(row.get(0)?)?;
        let covering = OperationId(digest(row.get(4)?)?);
        let manifest = digest(row.get(5)?)?;
        let checksum = digest(row.get(6)?)?;
        let generation = journal::load(connection, id)?;
        journal::validate_manifest(connection, &generation, false)?;
        if generation.phase != journal::Phase::Sealed || generation.manifest_hash != Some(manifest)
        {
            return Err(Failure::Corrupt.into());
        }
        if nodes
            .insert(
                id,
                Node {
                    parent: generation.parent,
                    target: target_hash(&generation.target),
                    base: base_hash(&generation.base),
                    covering,
                    manifest,
                    checksum,
                },
            )
            .is_some()
        {
            return Err(Failure::Corrupt.into());
        }
    }
    let mut children: BTreeMap<Option<GenerationId>, Vec<GenerationId>> = BTreeMap::new();
    for (id, node) in &nodes {
        if let Some(parent) = node.parent {
            let older = nodes.get(&parent).ok_or(Failure::Corrupt)?;
            if parent >= *id || older.target != node.target || older.base != node.base {
                return Err(Failure::Corrupt.into());
            }
        }
        children.entry(node.parent).or_default().push(*id);
    }
    // Iterative DFS yields ancestry intervals and depth in O(n log n), avoiding
    // recursion/stack growth and quadratic per-receipt ancestor scans.
    let mut intervals = BTreeMap::new();
    let mut clock = 0u32;
    let mut stack = children
        .get(&None)
        .into_iter()
        .flatten()
        .map(|id| (*id, 1u32, false))
        .collect::<Vec<_>>();
    while let Some((id, depth, exiting)) = stack.pop() {
        if exiting {
            let (enter, _, _) = intervals.get(&id).copied().ok_or(Failure::Corrupt)?;
            intervals.insert(id, (enter, clock, depth));
            clock += 1;
        } else {
            intervals.insert(id, (clock, clock, depth));
            clock += 1;
            stack.push((id, depth, true));
            stack.extend(
                children
                    .get(&Some(id))
                    .into_iter()
                    .flatten()
                    .map(|id| (*id, depth + 1, false)),
            );
        }
    }
    if intervals.len() != nodes.len() {
        return Err(Failure::Corrupt.into());
    }
    for root in roots.values() {
        if intervals.get(&root.generation).map(|v| v.2) != Some(root.covered) {
            return Err(Failure::Corrupt.into());
        }
    }
    for (id, node) in &nodes {
        let root = roots.get(&node.covering.0).ok_or(Failure::Corrupt)?;
        let a = intervals[id];
        let b = intervals.get(&root.generation).ok_or(Failure::Corrupt)?;
        if node.target != root.target
            || a.0 > b.0
            || a.1 < b.1
            || settlement_hash(*id, node.manifest, root.checksum) != node.checksum
        {
            return Err(Failure::Corrupt.into());
        }
    }
    Ok(Validated { roots, nodes })
}
pub(crate) fn validate_all(connection: &Connection) -> Result<()> {
    validate(connection).map(|_| ())
}

#[cfg(test)]
mod tests;
