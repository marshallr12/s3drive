# Fresh linked-native retention (work in progress)

Status: implementation candidate, not accepted. No hosted or actual mounted
acceptance is claimed by this document. The previously accepted default and
`owned-unlinked-v1` profile remain unchanged.

Alpha databases are disposable. This work does not convert old mounted stores
or promise backward compatibility. An explicitly selected
`linked-retention-v1` profile requires a fresh owner and establishes mount
binding format 2, source-scoped identities and preparation retry mode atomically
before its first operation. Existing format-one stores are refused, not deleted.

## Bounded scope

The candidate targets repeated edits, flushes, actual Close and remount of
existing linked manifest files. Creation, rename, removal/unlink, replacement,
owned/deferred publication and retained session recovery are not supported by
this profile. Their original journals remain protective; this is not completion
of the whole owned/unlinked native lifecycle or general namespace retention.
No provider object is deleted, and no automatic upgrade or default activation
is introduced.

Open and session identities include an owner-bound monotonic admission ordinal.
Only atomic admission consumes the next ordinal. Exact retained records can be
replayed; after retirement an old identity expires rather than being fabricated
as a current success or admitted anew. A concurrent candidate for an already
consumed ordinal must fail. There is one session per live runtime and bounded
retained open/session capacity, including retired opens held by readers.

Preparation, generation and root-publication IDs derive from the exact original
open and source epoch. Collection requires a current clean head, terminal roots,
completed actual Close on all target opens, and no logical or physical reader.
Clones retain the same reader protection. Dirty, uncertain or conflicting work
is never discarded. Long-held readers can impose backpressure rather than
forcing a cut or raising quotas.

## Independent terminal evidence

The compact publisher proof retains the exact Close/freeze/completion chain
suffix and its original retired-open seal, not an unbounded open-event journal.
A separate bounded per-target witness in the permanent mount binding binds its
digest, selected plan/generation, target, clean view and admission frontier.
Native anchors and witnesses must form an exact bijection; missing witnesses
are not repaired from anchors. Every original selected and unselected candidate
is validated before the transaction, and the surviving graph is validated after
source, physical and open retirement. All changes commit together.

A no-effect closed linked open with no source epoch or preparation can expire
without retaining a historical result, because it has no surviving dependency
and its ordinal cannot be reused. A failed or unknown Close is not eligible.

## Pending acceptance

The local workstation uses overlayfs/tmpfs; its durable-state test guard remains
unchanged and refuses those filesystems. Local compilation is not durable or
native acceptance. Required hosted evidence includes Linux and Windows repeated
cycles beyond history limits, DB/WAL/staging bounds, stale identity rejection,
coherently rehashed forged proofs, reader/clone lifetime, concurrency, process
kill and actual SQLITE_FULL rollback, plus real FUSE/WinFsp debug and release
save/Close/remount coverage. Existing whole-sequence deadlines remain unchanged.

The candidate regression fixtures include a private shared-runtime capacity test:
retain an independent cloned edited-file reader, close 256 native opens, observe
backpressure, release the last reader and reopen through the ordinary filesystem
API without an explicit collector call. It also checks exact bytes after a cold
restart. This test has its own bounded subprocess deadline and does not alter
any existing native deadline. State tests separately pin an older physical root
that must actually disappear after release and validate rollback including both
permanent opt-in mode rows. These describe required checks, not passed results.

Development runs have exposed performance blockers, not acceptance: early
shared-cycle runs exceeded their existing step deadlines, and the retained-open
pressure check detected six duplicate full-history scans during one pinned
refusal. The current candidate reuses a privately constructed validation scope
only inside one immutable read transaction. Frontier, mode, every retained open
and session, native semantic edges, source/preparation rows and compact terminal
witnesses remain validated. No proof survives a write or another invocation.
Late-corruption regressions include a coherently resealed unselected native edge
and changed frontier, session, identity and both permanent mode rows. The
256-open pressure check also retains an earlier compact anchor. These fixes
still require exact-source hosted execution under unchanged limits.

The next runtime-pressure attempt also shares validated owner/session facts and
four prepared SQL statements within each read-only retained-open scan. Every
open and event is still reread and replayed, and unused sessions are still
checked. Standalone mutation readers do not reuse that context. Deterministic
counters require all 256 histories and every event to be visited while preparing
only four scan queries and validating the shared owner/session once. Additional
late-corruption cases cover open owners, event hashes, resealed unknown or
volume-mismatched sessions, and an unused damaged session. Runtime progress
reports open and Close cost separately; its deadline remains unchanged.

A separate diagnostic job now measures individual state primitives, not total
filesystem operations. In run40073081377, at 16 retained opens, ordinary
session/record/admission reads performed four full scope scans, native Close
performed twelve, and completion performed ten. The controller invokes several
of these primitives per filesystem open or Close, so these timings cannot be
substituted for the mounted gate. The next candidate consolidates each
format-two mount requirement into one explicitly owned read transaction, or
one read-only interval of the caller's existing transaction. It returns only
the validated binding, never the shared scope. Tests require fresh validation
after a write even within the same outer transaction, preserve the caller's
transaction ownership, and check that the read-only path changes no rows.

The explicit-snapshot change reduced diagnostic work but did not pass runtime
pressure: Linux reached 162 retained opens before the unchanged 900-second
limit. The next change shares only the adjacent eager isolation check and
source inspection inside one transaction. It keeps eager rejection for empty
foreign stores, every inspection after a mutation, and all reader pin checks.
Deterministic diagnostics require heads to use two scopes, guard acquisition
four, and guard release three; a pin corrupted after the initial read must
still fail fresh validation and roll back exactly. Runtime acceptance remains
pending.

Run40074965175 passed the Windows shared-runtime pressure gate, but Linux still
hit its unchanged limit, so that candidate is not accepted. The following
measured refactor makes plan listing use one explicit read snapshot and lets
completion consume the exact preparation snapshot already checked by its
namespace barrier. Root/source/dependency validation remains mandatory, the
snapshot is dropped before completion writes, and final semantic checks remain
after both completion events. Diagnostics require plans to use one scope and
completion five, and test that an unsettled preparation still blocks completion
without changing any persisted row. Other workflows and actual native gates
remain pending.

The next Linux run still stopped at 178 retained opens. The following Close
refactor consolidates two read-only intervals: token/runtime/binding validation
with the initial preparation snapshot, then a fresh full semantic and body
snapshot after the entire freeze loop. No validation scope crosses preparation
or event writes, and the intermediate post-Close preparation check remains
fresh. Diagnostics require Close to use four scopes rather than nine. Tests
cover empty and dirty-plan Close and exact retries in both opt-in modes, plus
resealed unselected semantic corruption, frontier changes and an unused damaged
session after the initial read. The latter exercises the actual final read
helper with exact rollback, not an injected end-to-end mid-Close fault. All
runtime and native acceptance limits remain unchanged and pending.

The Close reduction passed its diagnostics but both runtime attempts stopped at
194 retained opens. The next optimization targets controller call multiplicity:
one explicit read-only native open preflight checks the full mount/native graph,
current runtime session, all live open records and exact pending recovery. Only
an empty target with no global requested-ID collision and no pending recovery
may skip eight otherwise repeated no-effect session/record/recovery reads.
Nonempty, obsolete, existing-ID and pending-recovery paths retain their original
logic. Only a selection boolean leaves the transaction; remote inspection and
all prepare/activate/bind checks still run freshly. Tests cover fallback cases,
corrupt unrelated retired history, and a mutation after preflight that must
still fail admission without changing rows. The measured preflight itself must
use one full scope; this is not yet proof that the runtime deadline is met.

The empty-node preflight passed its focused checks, but both runtime gates still
stopped at 210 retained opens. Measured open time improved while Close remained
the larger cost. The next change combines current-head, native-plan and root
acknowledgment selection into one read transaction per flush. It retains full
preparation bodies and all root/source/lineage/physical receipt checks, including
unselected roots. A preparation write discards the old selection and requires a
new one. Returned data never authorizes effects: the original SDK reopen,
semantic check, upload, publish and acknowledgment paths remain unchanged, as do
native locks and Close ordering. Diagnostics require one scope per selection;
both modes, changed head epochs, wrong bindings, and a coherently resealed
unselected acknowledged physical proof have focused tests. This reduces repeated
whole-history scans, but runtime acceptance is still pending.

The flush-selection refactor passed all eleven diagnostics but both runtime
gates stopped at 226 retained opens. The next package targets the remaining
repeated whole-history reads together: bind uses one immutable scope instead of
three, completion uses one scope before its event pair and a new scope only
after both events (two instead of five), and a sole ordinary POSIX Close may
replace seven prefix reads with one. The Close selector requires the exact
observed Close, current session, sole live target record and no other callback,
creation, admission, namespace or recovery work. Non-POSIX and contended paths
keep the original policy. Every check after the first flush, both flushes,
remote inspection, completion and retirement remain unchanged. No scope crosses
a write or await. Bind now allocates its runtime token only after successful
read validation, so failed reads leave no dormant token entry. Diagnostics cover
exact completion retries and barriers in both modes, prefix exclusions and
fresh later records, plus late corruption against the actual final completion
validator. Fresh-record testing is command-level, not an injected controller
flush interleaving. Full runtime and native acceptance remain pending.

An additional SDK fixture now exercises a real controller interleaving rather
than only sequential state commands. Its test Content implementation opens a
second valid handle during the first flush after a successful sole-Close
selection. It requires exactly one flush, completed/retired original Close,
and a still-active second handle, proving the later records lookup is used.
It adds no production hook and retains the existing five-PUT fixture bound.
This is shared-controller SDK coverage, not native-driver acceptance, and is
pending hosted execution.

Run40083665977 passed Linux's full reader-capacity gate, but Windows still hit
900 seconds after 242 retained opens, so it is not accepted. The next mode-two
optimization combines only no-effect outer recovery and GC target discovery:
full mount/native/source/root/physical proofs plus exact creator preflight,
isolation exclusions and membership must pass; namespace/creator history must
be absent and every root acknowledged. Otherwise the original recovery loops
and fresh heads read remain. A successful read returns head node IDs in the
same order as heads(), not permission to delete; each collector invocation still
validates current state and pins independently. Dirty or prepared source work
without a root may legitimately return a list and still requires later flush.
This targets seven scans to one. Tests cover newly unacknowledged root work,
multiple-target order, malformed empty creator role, stale discovery with pins
or coherent physical corruption, and mode-one fallback. The real controller
interleaving fixture above is bundled. All native deadlines and capacity limits
remain unchanged; cross-platform acceptance is pending.

The quiet-recovery candidate passed both OS reader-capacity gates. Exact source
411fa49 also passed all 24 native retry cases and six SDK cases on Linux. Its
shared fixture completed all 300 saves and the final cold read in 996.58 seconds,
within the unchanged 20-minute limit, but failed an incorrect final PUT count.
Cycles 0 and 251 contain only zeros: sparse preparation represents each as a
hole, so each needs a manifest page but no data object. The exact expected total
is one seed page, 300 save pages and 298 data objects, or 599 PUTs. The corrected
fixture derives that schedule and checks it after every ten-save stage, seed
and final read, retaining the final unique immutable-key assertion. This is a
test correction, not accepted mounted evidence; full exact-source and actual
FUSE/WinFsp gates remain pending.

Correcting the PUT schedule did not establish cross-platform runtime acceptance:
run40089367598 timed out at 240 saves on Windows and 190 on Linux, with the
completed-stage counts correct and storage still bounded. A dirty-save diagnostic
in run40093411234 measured 82 scope scans in physical staging, 38 in collection
and 36 in modeled publication commit after a compact anchor. These are state
primitive costs, not complete SDK or native syscall timings.

The next package batches the materializer's initial immutable checks and obtains
a separate fresh full snapshot after staging. Staging admission before and after
source registration remains independent, as do provider-boundary checks. It also
reuses exact preparation snapshots within publication base/begin/acknowledgment,
native preparation and checkpoint validation. Collection checks all retired
native edges before its first witness write and constructs a new full scope only
after every deletion. No scope crosses a write, commit or await; retained
snapshots used for equality are expected data, not validation authority. Focused
tests cover dependency receipts, current-format errors, late semantic/body and
generation damage, exact checkpoint equality and newly acquired readers. The
expected physical scope reduction is 82 to 46, subject to hosted confirmation;
all 300-save, capacity, kill/full-disk and actual mounted gates remain required.

Hosted diagnostics confirmed physical staging at 46 scans and collection at 10,
but run40095067571 still exceeded the shared 20-minute limit on Linux after 210
saves. Actual shared-FS instrumentation in run40099932644 completed 20 saves in
89.13 seconds. Each ten-save owner spent about 26 seconds in flush and nine in
Close, with nearly matching process CPU time. This is a short Linux diagnostic,
not 300-save or native-driver acceptance; it identifies local validation work,
not authority/network waiting, as the dominant measured cost.

The next change shares only the existing isolation check within a synchronous
source/preparation inspection, including its anchor and retry-mode readers.
This private context contains a connection and one isolation flag, not a native,
source, terminal or physical proof. Every mode singleton/hash, every anchor and
independent witness, every source body and retained event is still checked.
Each later inspection creates a fresh context even in the same transaction;
no context crosses a mutation or await. Full native ReadScope remains distinct.
Tests construct two independently collected native target anchors and retained
events, then damage either coherently resealed anchor, modes, frontier, witness,
source bytes and sessions between calls in one transaction. Both native modes
also cover valid dirty intermediate states and late damage with exact rollback.
Predicted anchored source/preparation isolation scans fall to one and modeled
physical staging from 46 to 22; these predictions and actual FS timing still
require hosted execution. All original acceptance gates remain unchanged.

The isolation-only package passed its 24 focused diagnostics and both capacity
release/reopen gates. Linux also passed shared 300-save/restart validation, but
Windows shared saves and Linux actual mounted saves each reached only 230 saves
before the unchanged 20-minute deadline. It is not accepted. The short actual
FS diagnostic improved from 89.13 to 80.66 seconds across different runners;
that observation was not sufficient runtime margin.

Run40106383062 profiled the actual FS child with official Ubuntu Callgrind after
an ordinary save/Close had established a compact anchor. The identical ordinary
debug binary passed both its unprofiled baseline and profiled two-save sequence,
including exact previous/final bytes, six PUTs and a final cold read. These are
instruction-event counts, not CPU-time percentages: retention loading accounted
for 55.36% inclusively, including anchor validation at 27.38%. There were 158
scoped anchor loads for 74 immutable read-scope constructions. Profiling overhead
is not acceptance timing; raw profiling archives are not published.

The proposed correction retains a fully checked anchor set only inside its
existing private, transaction-borrowed immutable ReadScope. Construction still
checks every anchor, original terminal, independent witness, physical receipt,
mode and native semantic graph. Subsequent source/preparation reads in that
same unchanged interval reuse the anchor set; independent scopes and unscoped
inspectors remain fresh. Supplemental active-transaction, 64-bit DML-count,
main/temp schema-cookie, user-version and foreign-key guards reject ordinary
misuse. They are not a hostile arbitrary-SQL fence: no write, await, raw
transaction control or deliberately rewritten schema cookie is permitted in
these audited private call paths. Tests include DML followed by savepoint
rollback, trigger/temp DDL, pragma changes, ended transactions, fresh-scope
validation and two-target corruption.

Within each ordinary anchor load, verified canonical JSON bytes are reused for
the same row hash and the exact existing role-hash tuple encoding. The ordinary
serde role hash remains an independent write-path oracle; differential tests
cover 0, 1, 2 and 256 entries, escaping and Unicode, ordering and duplicates.
Noncanonical whitespace or key order still fails. No stored hash definition or
format changes. Hosted fault/integrity, actual FS/profile measurements and every
original cross-platform acceptance gate remain required before acceptance.

The immutable-anchor package at `62c8ed3` passed Linux shared, actual mounted
debug and release 300-save gates, but its full workspace suite exposed a test
harness stack overflow in the mixed-lineage child. It is not accepted. The
workspace-unified local debug binary had a synchronous dispatcher frame of
`0x1ce468` bytes before its child and selected constructor frames. Constructing
all concrete arm futures directly as boxing arguments caused that large frame;
the lineage scenario does not install native retention state.

The test-only correction moves each selected future's immediate construction
into a non-inlined closure factory. All dispatch cases, borrowed lifetimes,
assertions and stack limits remain unchanged. The same local workspace build
now measures `0xbb8` bytes for dispatch and `0x48` for the selected lineage
factory; the lineage constructor remains `0x10568`. A pure regression verifies
construction happens exactly once immediately and polling only afterward.
These local frame measurements use the workstation's reduced-debug-info build,
not hosted acceptance. CI adds an early workspace-unified exact lineage test;
the full workspace and all native gates still must pass on the final source.

The same anchor candidate's Windows actual mounted gate reached 220 saves before
the unchanged 20-minute deadline. The test-dispatch correction does not resolve
that separate production liveness blocker. A separate two-owner, 20-save native
measurement now records actual syscall wall times and the owned mount daemon's
CPU endpoints. It retains prior bytes, exact PUT/progress checks, cold mounted
read and table/storage bounds. Linux reports CLK_TCK units; Windows reads only
the live owned PID's TotalProcessorTime in 100-nanosecond units. Missing samples
are explicit errors, never zero CPU. Sampling is separately bounded and its
wall overhead reported; daemon background work during sampling may contribute
to the CPU delta. Handle-drop return is not a server Close completion timestamp.
The original 120-second syscall sequence and all 300-save gates are unchanged;
this small diagnostic cannot substitute for their acceptance.
