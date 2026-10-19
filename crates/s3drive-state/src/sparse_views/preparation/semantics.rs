//! Offline byte comparison against an immutable view and its frozen root.
//! No source authentication, durable semantic seal, upload or clean receipt.
use super::*;
use ledger::Snapshot;

/// Complete original chunk material when a candidate retains base spans.
/// Hole proofs have no bytes. Fully local/zero candidates require no witness.
pub struct OriginalChunk {
    pub witness: Vec<u8>,
    pub bytes: Option<Vec<u8>>,
}
impl fmt::Debug for OriginalChunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalChunk([untrusted private bytes])")
    }
}
impl OriginalChunk {
    fn bounds(&self) -> Result<()> {
        if self.witness.len() < 49
            || self.witness.len() > 50 + 2 * (2 + manifest::MAX_PAGE_BYTES)
            || self
                .bytes
                .as_ref()
                .is_some_and(|b| b.is_empty() || b.len() > manifest::CHUNK_BYTES as usize)
        {
            return Err(Failure::Bounds.into());
        }
        Ok(())
    }
}

/// Deliberately reusable inside the later publication ledger's transaction.
/// The returned unit is not a token that can outlive the validated snapshot.
fn check(snapshot: &Snapshot, plan: Id, slot: u64, original: Option<&OriginalChunk>) -> Result<()> {
    let info = snapshot.info(plan)?;
    if info.root()?.is_none() {
        return Err(Failure::Invalid.into());
    }
    let object = &snapshot
        .objects
        .get(&(plan, -1, slot))
        .ok_or(Failure::Invalid)?
        .object;
    let Kind::Data { blob } = object.kind else {
        return Err(Failure::Invalid.into());
    };
    let candidate = snapshot.blobs.get(&blob).ok_or(Failure::Corrupt)?;
    let ranges = info.view.range(object.offset(), candidate.len())?;
    let uses_base = ranges.iter().any(|r| matches!(r, Range::Base { .. }));
    let mut original_start = 0;
    let original_bytes = match (uses_base, original) {
        (false, None) => None,
        (false, Some(_)) | (true, None) => return Err(Failure::Invalid.into()),
        (true, Some(original)) => {
            original.bounds()?;
            // Keep only one root and at most two witness pages, rather than
            // cloning all retained preparation data for a single check.
            let mut blobs = BTreeMap::from([(
                info.input.base_page,
                snapshot
                    .blobs
                    .get(&info.input.base_page)
                    .ok_or(Failure::Corrupt)?
                    .clone(),
            )]);
            let witness = proof::Witness::admit(&original.witness, false, &mut blobs)?;
            let resolved = witness.resolve(&info.input, &info.view, &blobs)?;
            if resolved.level != -1 || resolved.offset != object.offset() {
                return Err(Failure::Conflict.into());
            }
            original_start = resolved.offset;
            match (resolved.reference, original.bytes.as_deref()) {
                (Some(reference), Some(bytes))
                    if bytes.len() as u64 == resolved.length
                        && &digest(bytes) == reference.sha256() =>
                {
                    Some(bytes)
                }
                (None, None) => None,
                _ => return Err(Failure::Corrupt.into()),
            }
        }
    };
    let mut at = 0usize;
    for range in ranges {
        let length = match &range {
            Range::Base { length, .. } | Range::Zero { length } | Range::Patch { length, .. } => {
                *length
            }
        };
        let end = at
            .checked_add(usize::try_from(length)?)
            .ok_or(Failure::Corrupt)?;
        let actual = candidate.get(at..end).ok_or(Failure::Corrupt)?;
        let matches = match range {
            Range::Zero { .. } => actual.iter().all(|b| *b == 0),
            Range::Base { offset, .. } => {
                if let Some(bytes) = original_bytes {
                    let start = usize::try_from(
                        offset.checked_sub(original_start).ok_or(Failure::Corrupt)?,
                    )?;
                    let end = start.checked_add(actual.len()).ok_or(Failure::Corrupt)?;
                    bytes.get(start..end).ok_or(Failure::Corrupt)? == actual
                } else {
                    // A validated hole witness supplies zeros, never fabricated
                    // original data or an inherited upload acknowledgment.
                    actual.iter().all(|b| *b == 0)
                }
            }
            Range::Patch { block, offset, .. } => {
                let bytes = snapshot.views.blocks.get(&block).ok_or(Failure::Corrupt)?;
                let start = usize::try_from(offset)?;
                let end = start.checked_add(actual.len()).ok_or(Failure::Corrupt)?;
                bytes.get(start..end).ok_or(Failure::Corrupt)? == actual
            }
        };
        if !matches {
            return Err(Failure::Conflict.into());
        }
        at = end;
    }
    if at != candidate.len() {
        return Err(Failure::Corrupt.into());
    }
    Ok(())
}
impl Journal<'_> {
    /// Check one completed plan's Data candidate relative to its frozen root.
    /// The caller must independently authenticate that root before publication.
    /// Returns no persisted evidence and never mutates or settles state.
    pub fn check_data(
        &mut self,
        plan: Id,
        slot: u64,
        original: Option<&OriginalChunk>,
    ) -> Result<()> {
        if let Some(original) = original {
            original.bounds()?;
        }
        if slot >= MAX_FILE_BYTES / manifest::CHUNK_BYTES {
            return Err(Failure::Bounds.into());
        }
        let tx = self
            .state
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        check(&ledger::inspect(&tx)?, plan, slot, original)?;
        tx.commit()?;
        Ok(())
    }
}
impl Client {
    pub async fn check_data(
        &self,
        plan: Id,
        slot: u64,
        original: Option<OriginalChunk>,
    ) -> Result<()> {
        if let Some(original) = &original {
            original.bounds()?;
        }
        if slot >= MAX_FILE_BYTES / manifest::CHUNK_BYTES {
            return Err(Failure::Bounds.into());
        }
        self.call(move |j| j.check_data(plan, slot, original.as_ref()))
            .await
    }
}
#[cfg(test)]
mod tests;
