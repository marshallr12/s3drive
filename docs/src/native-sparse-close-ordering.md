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

The corrected run [39869226128](https://github.com/marshallr12/s3drive/actions/runs/39869226128)
passed16 of17 dedicated tests on each OS. Its remaining assertion incorrectly
expected an error for exact known-plan replay by another live handle on the same
node. Replay must preserve the original publisher and frozen plan, not adopt them;
the fixture now asserts that identity and still rejects fresh/stale admission.
Preparation additionally rejects a caller token for another node/source/control,
including before returning a known plan. A new assertion exercises that boundary.
These changes still require full hosted acceptance.

## SDK/controller bridge candidate (not native acceptance)

The SDK now has a nonserializable original-open bridge. Its Close constructor
marks the synchronous fence before the returned future can be polled or dropped.
Edits use authenticated pinned root/page context and exact worker, node, source,
store and fingerprint binding. Known preparations are replayed without changing
their original publisher; fresh preparation remains state-guarded. Frozen work
still independently passes existing reader, immutable upload, root publication
and exact source-acknowledgment gates.

The shared disposition controller can use the chunk owner's namespace/Open
capability instead of constructing a whole-file publisher for a chunk store.
Its content hook must flush every Close, including non-last Close, and complete
through the native source witness. Namespace transitions share the adapter's
serialization gate and flush retained native content first. Actual Cleanup remains
separate; obsolete-session Close is not fabricated or marked complete during cold
recovery. Existing whole-file behavior retains its original backend and barriers.

Five dedicated local-HTTP SDK/controller fixtures compile: linked, pending and
committed removal source publication; linked controller Close; committed-removal
controller Close/release. They exercise dropped-unpolled Close futures, multi-plan
lineage, refusal before exact source acknowledgments, dirty read through pinned
handles, and source completion without inventing POSIX Cleanup. Hosted execution,
full legacy regressions and the new Linux/Windows adapter lifecycle gates remain
outstanding. This bridge does not activate native filesystem capability flags.

Recovery prerequisite main8777d3078cdf87360893899fe448b0dc53c7b742 was independently
verified in [39867508230](https://github.com/marshallr12/s3drive/actions/runs/39867508230):
Linux966/Windows962 tests across37 cohorts; eight cold state and three cold SDK
cases, prior owned sparse gates, actual46/47 compatibility, and35 strictly compared
native reference artifact records. All764 source files and full logs/artifact
hashes were reviewed. Those existing native references are not acceptance of this
new SDK/controller or adapter behavior.

Local bridge verification: workspace Clippy with warnings denied, formatting,
schema fixture expectations and109 Markdown link checks pass. All five SDK cases
compile, but execution stops during durable authority startup on this cloud
workstation (overlayfs/tmpfs). This is not a test pass; the guard stays intact and
standard hosted Linux/Windows runners must execute the dedicated cases.

## Adapter candidate (2026-10-23, not accepted or enabled)

The explicit Cargo feature `native-sparse-candidate` is an acceptance build only.
Default version8 capability flags remain disabled. Candidate FUSE RELEASE observes
actual Close, independently of FLUSH; WinFsp reuses the same original file object
for data, Cleanup and Close. The mount keeps at most256 original capabilities,
with four admitted I/O jobs and120-second outer deadlines. Close observes its
shared fence synchronously and queues a retained, bounded job on the owning
runtime before returning its join future. Other I/O retains cancellation behavior.

All native source mutation, namespace policy and Close content barriers share a
serialization gate. A dirty read refreshes only the same verified pinned binding;
after source acknowledgment, the existing reader verifies the new binding. Every
Close flushes all retained native plans in lineage order using their original
publisher, even when another handle remains. Exact acknowledged plans are skipped;
a missing original runtime capability is a conflict, not adoption. POSIX unlink
and held-target replacement first flush content and record an owned removal before
the actual namespace effect. Actual Windows Cleanup remains distinct from Close.
Delete-on-close/deferred creation is still unsupported for the sparse adapter.

Cold startup classifies existing storage under exclusive read-only ownership.
Classification cannot migrate, acknowledge data or authorize an effect. Existing
current-schema chunk state must pass restricted namespace/open metadata validation
and exact saved-request replay before full State, policy, feature or provider
startup. Recognized older stores proceed only through the ordinary full validator
and migration before fresh startup checks; errors never fall back. A current cache
without a chunk role must contain no native provenance and still passes full State.
Bounded retained-open enumeration allows exact original controller-link ACK IDs,
not inferred callbacks, completion or session recreation. Unknown dirty native
epochs remain preserved conflicts. Saved root results use existing exact SDK
recovery and source ACK; clean obsolete sessions use explicit abandonment policy.

Prepared dedicated gates, still requiring execution and full review:

- Four shared-engine cases: held unlink, held-target replacement, exact cold-known
  root replay, and cold-unknown refusal with unchanged bytes/callback evidence.
- Three actual OS cases on each platform: unlink/replace/dirty handles/remount,
  killed daemon with a known saved root result, and killed daemon with an
  unprepared epoch that must refuse mounting without changing source bytes.
- Two additional metadata startup state tests cover read-only classification,
  source-body independence, bounded open enumeration, exclusive ownership,
  nonmigration and current-role corruption without fallback.

These are not passes merely because the fixtures compile. Complete legacy native
regressions, exact-source logs/artifacts and both actual OS cohorts are mandatory.
No real provider mutation, new credential or user-machine installation is involved.

## State checkpoint acceptance (2026-10-23)

Exact a1217b85c7470bbd4f97c0e8dc79cbe8c8e9747a passed
[39872409080](https://github.com/marshallr12/s3drive/actions/runs/39872409080).
Full logs confirm Linux983/Windows979 tests across37 cohorts, the17 dedicated
native state cases, prior16 owned state/six SDK and eight cold state/three SDK
cases, and all existing native/startup regressions. Four artifact ZIP hashes
were checked, both actual47/48 and historical46/47 binary compatibility records
were reviewed, and35 native records matched typed fixtures and full Windows logs.
All771 source files matched the accepted Git tree. Source ZIP SHA-256:
`53cea62632456f653f0b6b08ea1e4ccdba8c9b332a1b9c66ecd373e41ba71454`.
The unchanged accepted tree was merged at8f8f51abc0c1c1cf253c3a50288970586045fda1;
[main39876643605](https://github.com/marshallr12/s3drive/actions/runs/39876643605)
is still under verification. This does not accept SDK/controller or adapter work.

Candidate WinFsp POSIX unlink/rename advertisement is separate from delete-on-close
admission. Only the explicit test feature enables the sparse POSIX path; sparse
FILE_DELETE_ON_CLOSE remains unsupported. Default capabilities remain unchanged.

## SDK/controller bridge acceptance (2026-10-23)

Exact6ec538d76a26fd12a36f8def6847ad4502e00c78 passed
[39873777798](https://github.com/marshallr12/s3drive/actions/runs/39873777798).
Linux988/Windows984 tests passed across37 cohorts; all five new bridge cases ran
in dedicated gates and the full suite on both OSes. Full logs, four artifact ZIP
hashes, actual47/48 and historical46/47 compatibility, and35 strictly compared
native reference records were reviewed. All774 source files matched. Source ZIP
SHA-256: `87db4eeabb5bb0aa7af08f5b1fad3de9ac10395aeff719077f5f1147f5197481`.
The bridge remains on the feature branch, not main. No adapter acceptance or
native capability enablement follows merely from this bridge checkpoint.

## First adapter gate correction (2026-10-23)

Candidate8d224b5/run39880794317 is not accepted. Linux shared-engine tests passed
held unlink and both cold-known/cold-unknown recovery cases, but held-target
replacement failed because the adapter supplied a duplicate same-control
`destination_control`. The state validator correctly rejects that redundant
control. The adapter now supplies a second control only when the destination's
complete store/source/fingerprint control differs from the source; distinct
controls remain required and validated. The existing replacement fixture is
unchanged. Actual OS tests were not reached in that failed Linux run.

## Explicit opt-in profile (2026-10-23, candidate)

The owner chose explicit opt-in rather than changing existing version8 behavior.
The version8 managed setting `sparse-lifecycle = 'owned-unlinked-v1'` selects the
new contract only in the acceptance build until final-source gates pass. Omission
preserves the existing version8 dirty-head recovery, captured-view concurrency,
creation and directory behavior. Unknown option values or other config versions
are rejected; builds without the candidate feature reject the opt-in explicitly.
Sparse delete-on-close/deferred creation remains unsupported.

Opted-in writes serialize behind publication/namespace barriers. Close still
observes its original fence synchronously. Cold unprepared native epochs remain
conflicts; removing the opt-in cannot adopt retained original opens. Conversely,
opt-in cannot adopt an old legacy dirty epoch: settle it under its original profile
before switching. Known roots are reconciled before clean transition, with no fresh
IDs, policy bypass or source repair. Two additional shared cases verify these
profile transitions and ordered writes/Close; six shared plus three actual OS
fixtures now apply. Unchanged legacy version8 shared and real mount gates also run
with candidate code compiled, not only with the feature absent. Full final-source
and main verification remain required before production availability.

Opt-in run39883093594 rejected two immediate profile retry checks: startup
correctly refused dirty epoch adoption, but returned before its ordinary storage
worker released the exclusive directory lock. Failed native startup now explicitly
awaits storage-owner closure before returning its original refusal. No filesystem
callback exists yet; this is not native Close/completion or source acknowledgment.
The transition checks retain their exact error/byte expectations.


Run39883860660 verifies the startup-owner closure correction: all six shared
profile/lifecycle cases and seven compiled-feature legacy cases pass on both OSes.
Its actual gates did not mount: the fixture tried indexing an absent TOML opt-in
key. The fixture now inserts that key into the managed table explicitly. This is
not native acceptance. Earlier run39881340386 exposed Windows known-root remount
stack overflow, Windows post-unlink write conflict, and a Linux combined fixture
timeout; those failures remain open until dedicated reruns establish otherwise.
Cold startup recovery awaits are boxed, and the known-root shared remount now runs
on a 128-KiB thread. A seventh shared case isolates Windows-style transient
SetDelete/Cleanup/Close while other handles survive. It does not substitute for
actual WinFsp callbacks. Native operation and fixture deadlines remain unchanged.

Run39885763631 passes six of seven shared cases on both OSes, including the
transient disposition-owner Close sequence. The unchanged 128-KiB cold-start case
still overflows; boxing nested awaits alone did not bound the caller stack.
Candidate version8 managed construction now owns a cloned configuration and polls
startup on a Tokio worker. The small caller stack only awaits its join handle;
dropping the startup await aborts that owned task instead of detaching a mount.
Recovery requests, ownership fences and fixture stack/deadline bounds are unchanged.
Actual OS acceptance remains pending; this correction is not a passing result.


Run39886633963 passes all seven shared cases and both actual killed-owner
known/unknown cases on Linux and Windows. Windows known-root remount no longer
overflows. Actual Windows post-unlink write still conflicts. The Linux combined
fixture reaches final Close at119 seconds and then hits its120-second bound, with
measured progress throughout. Unlink and replacement now use independent bounded
phases on the same live mount; each retains the120-second bound and all assertions.

The candidate adapter joins an already observed cross-handle Close job before
admitting a surviving handle's new write/resize or Open. Synchronous original
Close fencing remains first; joining never fabricates completion, releases a
retained failure, or permits the closed handle to write. Failed/cancelled jobs
remain conflicts. Three portable signal tests and an eighth shared fixture cover
completion/failure and a surviving write issued before the Close future is joined.
These corrections still require exact-source actual OS and full regression gates.
