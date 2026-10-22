# Native sparse epoch ownership and Close ordering

Implemented state candidate, not yet accepted, 2026-10-22. This is not accepted state behavior or native
activation. Base prerequisite source: cb568aa0417cf84d829a58236f9ac45c707b4ef8.
The existing schema47 sparse admission and fresh-send guards remain the baseline.

## Problem

Recording Close and then calling ordinary owned preparation cannot work: the
original-open guard correctly rejects a closed open. Delaying the actual Close
observation until network flush finishes is also incorrect. Moreover, a shared
per-node dirty view is not an original native open. A second handle cannot adopt
the first publisher's immutable predecessor lineage merely because it closes last.
The same issue applies to linked plans: leaving them without native provenance
would allow generic unowned publication to bypass the new original-open contract.

## Bounded state protocol

1. Bind each native file object to one original, session-bound open. FUSE and
   WinFsp data callbacks use that same object, not a second hidden open. A shared
   node cache may retain bytes, but does not invent file-object identity.
2. Atomically record the original local publisher of a source epoch with its
   first bound mutation. Other live handles can mutate the epoch while that
   publisher remains live. A new publisher requires a clean acknowledged/rebased
   epoch; another handle cannot take over an unresolved old epoch.
3. Bind every native preparation, linked or owned-unlinked, to that immutable
   original-open/epoch witness. Ordinary existing admission cannot silently adopt
   a native epoch. Owned preparations additionally retain all schema47 removal
   and current-event proofs and exact predecessor checks.
4. Observe Close in a bounded original-open token before any filesystem/network
   lock or wait. The token prevents fresh work if the queue is full or recording
   fails. Retain the exact callback identifier even when neither atomic admission
   nor the fallback Close fact can commit. A different callback cannot replace
   that identity on retry. It is not a persistent permission and cannot survive
   a runtime change.
5. On the exclusive state worker, atomically admit any needed exact final dirty
   snapshot, append the actual Close, and freeze the exact original plans. This
   is a distinct capability, not an exception added to ordinary sparse_current.
   The final snapshot uses a previously registered authenticated base context;
   Close itself performs no provider read or write.
6. Only those frozen plans may finish fresh work after Close, still under the
   same original runtime/session and all current claim, lineage, quota and
   source/root barriers. There is no new plan/view admission after durable Close.
   Exact known-result replay remains separate from fresh work.
7. A nonpublisher Close does not capture another open's dirty lineage. Any
   observed, uncompleted Close blocks subsequent source mutation. An active
   original publisher can still prepare/flush its own epoch for a nonpublisher
   Close. Every Close, not just last Close, requires the node's dirty source and
   all plan/root barriers to settle before releasing that fence. The controller
   must therefore flush and complete non-last closes too, without adoption.
   Rejected/no-effect settlement is not a clean source receipt.
8. Native completion atomically records a clean-head witness and a digest of the
   sorted exact frozen plans immediately before Completed. It cannot use generic
   completion to bypass the witness. A POSIX Close needs no invented Windows
   Cleanup; actual recorded Windows Cleanup retains its separate callback and
   completion requirements.
9. Namespace transitions first flush the node's dirty source, and bound mutations
   reject pending namespace transitions. Close during an uncertain removal must
   not invent an owned plan from an unknown receipt. Unknown effects, failed
   observation and dead-session unclaimed work retain explicit conflicts/bytes.

The candidate schema48 wire additions are bounded epoch-ownership/frozen-plan open
records and optional native preparation provenance. Generic event append cannot
mint these capabilities. Pre48 marker guards must reject the new fields/events,
old47 binaries must refuse48 without changing bytes, and normal47 migration must
preserve every legacy row. Creator format3 remains unchanged.

## Recovery and boundedness

The runtime holds at most the existing open quota of original close tokens and a
bounded base-context cache (256 tokens, 256 contexts and 16MiB context bytes).
Exact sealed retirement releases obsolete tokens when capacity is needed, not
historical open evidence. A retired token is not restored as a live capability;
retained records remain available through the ordinary historical journal. A missing token is not recreated as a live old open.
Cold recovery may reconcile only exact saved namespace/root outcomes before normal
source validation; it cannot manufacture callback observations or fresh native
permission. Unprepared dirty native epochs also block session abandonment.
An exact existing session-recovery proof may retire an obsolete clean mutation
fence, but never records a fabricated Close/Completed event or adopts old dirty
ownership. A later clean epoch gets its own original publisher. Restricted
metadata validation must not call full sparse ledgers that
read source/staging bodies, and must avoid recursive open/plan validation.

All command queues, deadlines, metadata, plans, source bytes and event counts keep
explicit bounds. Admission failure rolls back the complete transaction; recording
actual Close separately, when possible, must not grant a frozen-plan permission.
Quota, real SQLiteFull, cancellation, timeout and killed-owner failures preserve
retained data and do not imply successful completion.

Preparation identity is deterministic for the original provenance and source
view. A terminal rejected/no-effect result is not automatically turned into a
new plan for that unchanged view; bytes remain retained until a subsequent
legitimate mutation or explicit recovery. Pending namespace work and uncertain
close admission remain conservative conflicts.

Restricted namespace recovery remains exact-schema only. Opening an old47
profile requires the normal validated migration before using the48 restricted
capability; this candidate does not silently migrate through a metadata-only
recovery handle.

## Candidate checks and required acceptance

Seventeen dedicated state tests have been added, including16 killed transaction
boundaries, real SQLiteFull at epoch/Close event writes, exact callback retry,
POSIX and Windows-cleanup models, source receipt gating, cold metadata/body
separation, clean session retirement, unprepared dirty refusal, pre48 marker
refusal and generic admission/mutation bypass controls. Compilation alone does
not accept these tests; durable Linux/Windows execution and full source/log/artifact
review are still required. The workflow retains actual46/47 compatibility and
adds actual47/48 binaries rather than relabeling the current executable as47.

Local workspace/all-target Clippy with warnings denied, workspace build,39 core
tests, formatting, schema-fixture and documentation-link checks pass. The full
local gate and a focused native-state case correctly reject tmpfs/overlayfs durable
staging; those are not passing durable tests. Local mdBook is unavailable. Standard
hosted runners must provide durable tests and book validation without guard changes.

Still required:

- Original-open and same-epoch ownership, shared dirty views, independent handles,
  concurrent mutation/Close ordering, publisher/nonpublisher/last Close.
- Atomic admission/Close/freeze transaction boundaries, exact retry, rejected
  generic event forgery, wrong claim/lineage/session/runtime and missing context.
- Known replay versus fresh sends; closed/dead-session denial; no false clean
  receipt from terminal rejection or no-effect settlement; all bounds/faults.
- Cold metadata-only proof validation with corrupt source/staging bodies, entire
  role inspection and raw pre48 marker laundering controls.
- Actual47/48 old/new binaries plus existing compatibility and full OS regressions.
- Subsequent authenticated SDK and actual FUSE/WinFsp >8MiB Unicode unlink,
  save/replace, retained dirty reads/writes/truncate, last Close and crash/remount.

This protocol alone does not enable native flags. A chunk-aware disposition
controller and adapter routing still require implementation and their dedicated
native OS gates. Preserve the existing profile's unsupported operations until
those gates pass; do not use old native regression success as new acceptance.

## First hosted candidate correction (not acceptance)

Run [39867493619](https://github.com/marshallr12/s3drive/actions/runs/39867493619)
failed the new dedicated state gate on both platforms. The original candidate
incorrectly equated a plain SHA-256 preparation BLOB key with the manifest's
domain-separated page digest. The corrected epoch event retains the exact original
root descriptor and plain page key atomically with the first edit; later mutation,
plan admission and metadata recovery must match those identities. Metadata recovery
still never reads private page/dirty bodies or pretends to authenticate them.
Retired publisher records referenced by bounded retained plans are checked without
scanning unbounded retired history. No original runtime capability is restored.

Fixtures now create fresh private state subdirectories, explicitly retire only
finished clean-session recovery proofs, and require dirty recovery to refuse new
open admission. The disk-full fixture uses a connection-local, test-only SQLite
allocation probe at the event transaction boundary. It causes real SQLITE_FULL
and rollback, without installing a trigger or relaxing the trigger/schema guard.
The extra root-context refusal test and historical retirement check require a new
exact-source full hosted run. The failed run is retained; no capability is enabled.
