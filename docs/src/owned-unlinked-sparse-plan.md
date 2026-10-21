# Sparse owned-unlinked publication readiness

Research date: 2026-10-21. Read-only source review against main8780ce4 and
SDK candidate8022fae3. Proposal only; no native flag, wire or schema change made.

Status update, 2026-10-21: phase 1 is now accepted as the
[schema46 event-bound authority](manifest-event-fence.md), with actual45/46
compatibility and its own exact-source full/native GitHub acceptance. The source
facts below describe the earlier planning baseline, not a denial of the new
opt-in authority guard. Phase 2 sparse journal admission is next; phases 2-4 remain
proposals, and no new sparse SDK or native lifecycle path is enabled.

The [node-wide barrier prerequisite](sparse-node-barriers.md) is being implemented
as a separate narrow phase-2 slice. It includes preparations before root records,
reciprocal namespace admission, and abandonment/retirement checks. Typed owned
admission and explicit rejected/no-effect settlement are still subsequent work.

The chunk choice requires extending the sparse mixed-root path, not substituting
the old bounded whole-generation writer. Current source facts:

- `sparse_views/preparation/publication/model.rs`, `Input::reservation`, rejects
  `guard.detached` and any `guard.removal`. Journal::begin builds removal=None.
  Simply changing the SDK caller cannot implement this feature.
- `namespace/removals.rs`, RemovalFence, contains original intent and owner only.
  The internal Tag separately retains current mode and event. publication_gate
  currently compares original fence, not an independently requested current event.
  A proposal must not describe that as an existing event-fencing guarantee.
- Authority ReserveManifest already accepts a removal fence and a test in
  namespace/publication/deferred/tests.rs exercises later owned publication.
  This does not prove sparse-journal or native integration.
- sparse publication::namespace_pending blocks namespace mutation while a root
  record lacks local source acknowledgment. adoption_fence checks all source
  plans for the node, including those without a root record. A closing barrier
  must include both admitted preparations and physical roots, rather than only
  saved authority Apply rows. Rejection/no-effect settlement remains explicit.
- MixedPublisher currently has separate publication and recovery semaphores;
  preserve its existing behavior while specifying any new concurrency contract.

Proposed independently green phases:

1. Authority event-bound reserve/publish contract. Specify original fence plus
   exact current mode/event and expected payload/revision. Preserve historical
   idempotent replies; reject cancelled, transitioned, replaced and successor
   ownership for fresh effects. Decide explicit new variant versus versioned
   optional guard only after checking old-reader and serde bytes. Test stale events paired with
   current node revisions, two clients and lost replies before
   exposing a local API. No native flag.
2. Sparse journal typed admission with capability and retained original open.
   Atomic persisted original/current provenance; format/schema guards before old
   marker rewrite; exact compatibility and rollback. Bind predecessor/successor
   chains and source settlement to that ownership epoch. Preserve old bytes and
   current dirty/newer views. Crash, actual disk-full, quota and corruption gates.
3. SDK sparse publisher and metadata-only recovery. Admission before fresh
   mutation, same owned worker and bounded concurrency/deadline, exact request
   replay before source/policy probes, consumed PUT observation without resend,
   no stale-session resurrection. Real SDK loopback races, worker termination,
   denied credentials/policy and body corruption; unchanged-source full/native CI.
4. Native lifecycle and cold recovery. FUSE/WinFsp actual >8MiB sparse Unicode
   unlink-while-open, save/replace, dirty-handle reads, flush, final close and
   remount. Final removal blocked across every unresolved plan/root/source ack.
   No unsupported flag is enabled before these pass. Physical MX/Windows sign-in,
   SSH/tmux and real provider/independent-host acceptance remain separate.

This is a source map and proposal, not an accepted implementation contract.
The precise wire/DDL shape and cold-start handle disposition still require the
phase-one design. No outside provider/driver claim was inferred from unit tests.
