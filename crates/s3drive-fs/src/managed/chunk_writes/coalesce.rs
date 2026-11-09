//! Bounded in-memory coalescing of consecutive writes to one open node.
//!
//! Every journal mutation validates the whole retained local ledger, so a save
//! made of hundreds of small writes pays a cost that grows with each write and
//! soon exhausts the retained-view limit. This buffer merges writes that touch
//! or overlap and hands them to the journal as one mutation.
//!
//! Bytes held here are **not durable**: they exist only in the mount process
//! until the next commit point. See `docs/src/write-coalescing.md`.
use super::*;
use std::sync::atomic::AtomicU64;

/// Largest run held in memory; a write of at least this size is never buffered.
pub(super) const CAPACITY: usize = 1024 * 1024;
/// A buffered run is journaled after this long without a further write.
pub(super) const IDLE: Duration = Duration::from_secs(1);
/// Test-only switch: the journal refuses every buffered run as if it were full.
#[cfg(test)]
pub(super) static REFUSE_RUNS: AtomicBool = AtomicBool::new(false);
/// Source of run identities, so an idle timer never adopts a later run.
static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

/// Accepted but not yet journaled bytes of one node. The run is owned by one
/// open handle, or by none once that handle closed while the journal refused it.
pub(super) struct Pending {
    /// Native open whose capability journals the run; empty when it is kept
    /// after its owner closed, and any open handle of the node may journal it.
    open: String,
    /// Logical file size including the run.
    size: u64,
    /// Identity of this run and the number of merges it has absorbed.
    mark: (u64, u64),
    offset: u64,
    bytes: Vec<u8>,
}

/// What happened to a write offered to the buffer.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Offer {
    /// The write began a new run; the caller starts its idle timer.
    Started,
    /// The write was merged into the existing run.
    Extended,
    /// The handle no longer buffers; the caller journals the write itself.
    Direct,
}

impl Pending {
    /// Starts an empty run for `open` on a file whose current size is `size`.
    fn new(open: &str, size: u64) -> Self {
        Self {
            open: open.to_owned(),
            size,
            mark: (NEXT_RUN.fetch_add(1, Ordering::Relaxed), 0),
            offset: 0,
            bytes: Vec::new(),
        }
    }
    /// First byte position after the run.
    fn end(&self) -> u64 {
        self.offset + self.bytes.len() as u64
    }
    /// Whether a write touches or overlaps the run and their union still fits.
    fn accepts(&self, offset: u64, count: usize) -> bool {
        if self.bytes.is_empty() {
            return count <= CAPACITY;
        }
        let end = offset.saturating_add(count as u64);
        let union = end.max(self.end()) - offset.min(self.offset);
        offset <= self.end() && end >= self.offset && union <= CAPACITY as u64
    }
    /// Merges an accepted write; its bytes replace earlier ones they overlap.
    /// Because the write touches the run, every zero added here is overwritten.
    fn merge(&mut self, offset: u64, input: &[u8]) {
        if self.bytes.is_empty() {
            self.offset = offset;
        }
        let start = offset.min(self.offset);
        let end = (offset + input.len() as u64).max(self.end());
        let before = (self.offset - start) as usize;
        self.bytes.splice(0..0, std::iter::repeat_n(0, before));
        self.bytes.resize((end - start) as usize, 0);
        self.offset = start;
        let at = (offset - start) as usize;
        self.bytes[at..at + input.len()].copy_from_slice(input);
        self.size = self.size.max(end);
        self.mark.1 += 1;
    }
}

impl File {
    /// Locks the buffer slot, mapping a poisoned lock to an internal error.
    fn slot(&self) -> Result<std::sync::MutexGuard<'_, Option<Pending>>> {
        self.pending.lock().map_err(|_| Error::Internal)
    }
    /// Logical size while writes are buffered; `None` when the journal is current.
    pub(super) fn buffered_size(&self) -> Result<Option<u64>> {
        Ok(self.slot()?.as_ref().map(|p| p.size))
    }
    /// Run identity and merge count of the buffered run, if one exists.
    pub(super) fn buffered_mark(&self) -> Result<Option<(u64, u64)>> {
        Ok(self.slot()?.as_ref().map(|p| p.mark))
    }
    /// Whether the buffered run must be journaled before this write can merge:
    /// it belongs to another handle, or the write neither touches it nor fits.
    pub(super) fn obstructs(&self, open: &str, offset: u64, count: usize) -> Result<bool> {
        Ok(self
            .slot()?
            .as_ref()
            .is_some_and(|p| p.open != open || !p.accepts(offset, count)))
    }
    /// Buffers a write for `open` unless that handle stopped buffering. The
    /// caller holds `serial`, has journaled any obstructing run, and passes the
    /// file's current logical `size`.
    pub(super) fn offer(
        &self,
        open: &str,
        direct: &AtomicBool,
        size: u64,
        offset: u64,
        input: &[u8],
    ) -> Result<Offer> {
        let mut slot = self.slot()?;
        // Checked under the slot lock so a concurrent Close sees either this
        // write's bytes or a handle that no longer buffers, never neither.
        if direct.load(Ordering::Acquire) {
            return Ok(Offer::Direct);
        }
        let fits = |p: &Pending| p.open == open && p.accepts(offset, input.len());
        if !slot.as_ref().is_none_or(fits) {
            return Err(Error::Internal);
        }
        let started = slot.is_none();
        let run = slot.get_or_insert_with(|| Pending::new(open, size));
        run.merge(offset, input);
        self.metadata.lock().map_err(|_| Error::Internal)?.size = run.size;
        Ok(if started {
            Offer::Started
        } else {
            Offer::Extended
        })
    }
    /// Stops a handle from buffering further writes. Returns true when it
    /// still owns buffered bytes that must be journaled before it closes.
    pub(super) fn retire(&self, open: &str, direct: &AtomicBool) -> bool {
        let Ok(slot) = self.pending.lock() else {
            return false;
        };
        direct.store(true, Ordering::Release);
        slot.as_ref().is_some_and(|p| p.open == open)
    }
    /// Owner, offset and bytes of the buffered run, copied for journaling. The
    /// run stays buffered until `journaled` confirms the write.
    pub(super) fn run(&self) -> Result<Option<(String, u64, Vec<u8>)>> {
        Ok(self
            .slot()?
            .as_ref()
            .map(|p| (p.open.clone(), p.offset, p.bytes.clone())))
    }
    /// Forgets the run after its journal write and records the journal's size.
    pub(super) fn journaled(&self, size: u64) -> Result<()> {
        self.slot()?.take();
        self.metadata.lock().map_err(|_| Error::Internal)?.size = size;
        Ok(())
    }
    /// Records the journal's size, unless buffered bytes extend the file past it.
    pub(super) fn note_journal_size(&self, size: u64) -> Result<()> {
        let logical = self.buffered_size()?.unwrap_or(size);
        self.metadata.lock().map_err(|_| Error::Internal)?.size = logical;
        Ok(())
    }
    /// Detaches the run from `open`, which is closing while the journal still
    /// refuses the bytes. The run stays buffered and counts toward the size,
    /// owned by no handle. Returns the number of bytes kept.
    pub(super) fn orphan(&self, open: &str) -> Result<Option<usize>> {
        let mut slot = self.slot()?;
        let Some(run) = slot.as_mut().filter(|p| p.open == open) else {
            return Ok(None);
        };
        run.open.clear();
        Ok(Some(run.bytes.len()))
    }
    /// Close entry for `open`: it stops buffering and takes over a run kept
    /// after an earlier handle closed. Returns true when it now owns buffered
    /// bytes that must be journaled before its Close is observed.
    pub(super) fn claim(&self, open: &str, direct: &AtomicBool) -> bool {
        let Ok(mut slot) = self.pending.lock() else {
            return false;
        };
        direct.store(true, Ordering::Release);
        let Some(run) = slot.as_mut() else {
            return false;
        };
        if run.open.is_empty() {
            run.open = open.to_owned();
        }
        run.open == open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a run from a sequence of accepted writes.
    fn run(size: u64, writes: &[(u64, &[u8])]) -> Pending {
        let mut pending = Pending::new("open", size);
        for (offset, bytes) in writes {
            assert!(pending.accepts(*offset, bytes.len()), "write at {offset}");
            pending.merge(*offset, bytes);
        }
        pending
    }

    #[test]
    fn sequential_writes_become_one_run_and_extend_the_size() {
        let pending = run(4, &[(4, b"abcd"), (8, b"efgh"), (12, b"ij")]);
        assert_eq!(
            (pending.offset, pending.bytes.as_slice()),
            (4, &b"abcdefghij"[..])
        );
        assert_eq!((pending.size, pending.mark.1), (14, 3));
    }

    #[test]
    fn overlapping_and_preceding_writes_replace_and_prepend() {
        let pending = run(
            100,
            &[(10, b"0123456789"), (12, b"AB"), (6, b"wxyz"), (18, b"!!!")],
        );
        assert_eq!(pending.offset, 6);
        assert_eq!(pending.bytes, b"wxyz01AB4567!!!");
        assert_eq!(pending.size, 100, "writes inside the file keep its size");
    }

    #[test]
    fn a_gap_or_an_oversized_union_is_not_accepted() {
        let pending = run(0, &[(10, b"0123456789")]);
        assert!(pending.accepts(20, 1), "touching the end");
        assert!(pending.accepts(9, 1), "touching the start");
        assert!(!pending.accepts(21, 1), "gap after the run");
        assert!(!pending.accepts(8, 1), "gap before the run");
        assert!(!pending.accepts(20, CAPACITY - 9), "union above capacity");
        assert!(pending.accepts(20, CAPACITY - 10), "union at capacity");
    }

    #[test]
    fn merged_runs_equal_the_same_writes_applied_directly() {
        // Deterministic pseudo-random walk of small touching writes.
        let (mut model, mut pending) = (vec![0u8; 512], Pending::new("open", 512));
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let (mut low, mut high) = (200u64, 200u64);
        for round in 0..2_000u32 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // Any offset in [low - length, high] touches or overlaps the run.
            let length = 1 + state % 24;
            let lowest = low.saturating_sub(length);
            let offset = lowest + (state >> 8) % (high - lowest + 1);
            if offset + length > 512 {
                continue;
            }
            let bytes = vec![(round % 251) as u8 + 1; length as usize];
            assert!(pending.accepts(offset, bytes.len()));
            pending.merge(offset, &bytes);
            model[offset as usize..offset as usize + bytes.len()].copy_from_slice(&bytes);
            (low, high) = (low.min(offset), high.max(offset + bytes.len() as u64));
        }
        assert_eq!((pending.offset, pending.end()), (low, high));
        assert_eq!(pending.bytes, model[low as usize..high as usize]);
    }

    #[test]
    fn every_run_gets_a_distinct_identity() {
        assert_ne!(Pending::new("a", 0).mark.0, Pending::new("a", 0).mark.0);
    }
}
