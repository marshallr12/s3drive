# Fresh linked-native retention

Status: exact candidate `3a7b95d4aa74deb15ea12f0820882893c5919f85`, tree
`ac8d23d199151beeb90b5dfd43f7494d16f65003`, passed independent source, full-log
and artifact review of [run40130958279](https://github.com/marshallr12/s3drive/actions/runs/40130958279),
attempt 1, on 2026-10-24. All seven jobs passed. The main follow-up below failed;
corrective verification remains pending. Candidate acceptance is not a claim
about a later main commit. The default and `owned-unlinked-v1` profile remain
unchanged.

Main follow-up: merge `3ed77d43b94dca7d0c2ab05f140df9f68a3b4777` contains
that accepted candidate plus evidence-only documentation. Its independent
[main run40140286274](https://github.com/marshallr12/s3drive/actions/runs/40140286274)
failed the unchanged Windows mounted300 deadline after 250 saves on 2026-10-24
UTC. No integrity assertion failed before the timeout, but main is not accepted.
The earlier candidate result remains historical evidence, not a substitute for
this failed main gate. Corrective performance work is pending; no unchanged
rerun, deadline increase or smaller workload establishes acceptance.

Both Linux FUSE and Windows WinFsp completed all 300 saves, actual Close calls,
30 owner/remount segments and the final cold read in debug and release. Shared
300-save restart coverage also passed on both OS. Debug mounted300 took 1072.49s
on Linux and 1064.62s on Windows under the unchanged 1200s step bound; release
took 257.06s and 349.51s respectively. These are measured runs, not a guarantee
of fixed timing on other hardware. The original per-sequence native deadlines,
reader-capacity bound and integrity gates were not raised or reduced.

Workspace parent-harness totals were Linux 1099 passed and Windows 1094 passed,
with zero failures (21/37 ignored respectively). Nested child summaries are not
double-counted. Each OS passed 38 fresh-native retry cases; the focused Linux diagnostic
job passed 31 tests. Across all six 300-save fixtures, database peak was 692224 bytes
and staged payload peak 145 bytes. Shared WAL peaked at 4350752 bytes; mounted
debug/release WAL peaked at 4379592 on Linux and 4136512 on Windows, returning
to zero at the final cold read. Measured storage stayed within fixed bounds over 300 saves.

Independent review matched all 801 source blobs, all six artifact digests, and
21 replacement plus 14 disposition reference records. The full workspace,
original native lifecycle, current/historical executable compatibility, crash,
SQLITE_FULL rollback, stale/forged proof and reader/clone tests passed. Only the
explicit existing-linked-file scope below is accepted, not general mounted
namespace cleanup or real-provider deployment.

Alpha databases are disposable. This work does not convert old mounted stores
or promise backward compatibility. An explicitly selected
`linked-retention-v1` profile requires a fresh owner and establishes mount
binding format 2, source-scoped identities and preparation retry mode atomically
before its first operation. Existing format-one stores are refused, not deleted.

## Bounded scope

The opt-in profile supports repeated edits, flushes, actual Close and remount of
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

## Accepted candidate checks

The local workstation uses overlayfs/tmpfs; its durable-state test guard remains
unchanged and refuses those filesystems. Local compilation is not durable or
native acceptance. The accepted hosted evidence includes Linux and Windows repeated
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
permanent opt-in mode rows. These checks passed on the exact accepted candidate.

The following notes preserve historical development results and rejected
candidates; the exact acceptance statement above supersedes their pending
status. Development runs exposed performance blockers: early
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

The actual native 20-save measurement passed on both systems, taking 55.70
seconds on Linux and 137.14 seconds on Windows. Windows' second owner spent
65.36 seconds in syscalls and 61.16 seconds of daemon CPU: sync_all was 25.94
seconds and reopen/read/drop 19.56 seconds. This is not adequate 300-save margin.
The next package batches only repeated reads within individual owner commands:
source guard acquisition keeps its validated View and pin prestate together
(two full inspections rather than four), pinned reads and head/view discovery
use one fresh snapshot, and release retains separate pre/post inspections.
Public Journal construction remains unchanged; no reusable Journal caches proof.
Fresh linked prepare_publish similarly combines its pre-write source, root and
native ownership checks, drops the scope before saving, and retains independent
post-write validation. Proposed anchor-load counts are checked in focused tests,
including cloned readers, changed/missing pins, late source/anchor damage,
rotating-session pin fault rollback and exact publication retries. Hosted
measurements and all original gates remain pending for this package; no speedup
or cross-platform acceptance is claimed from static count reductions.

### SHA256 backend investigation (2026-10-24, diagnostic only)

The latest profiled two-save shared fixture spends 21.33% of Callgrind
instruction events inside SHA256 compression (inclusive), not 21.33% of native
Windows wall time. Resolved `sha2` 0.11.1 already selects SHA-NI plus SSE4.1 at
runtime where available. Profiler CPU emulation may change that selection;
ordinary-runner capability reporting is required before inferring the real path.
The old RustCrypto assembly feature is not a Windows MSVC solution:
[the upstream assembly project](https://github.com/RustCrypto/asm-hashes)
only supports Windows GNU and reports MSVC builds broken.

A separate ignored test compares the already locked `ring` 0.17.14 and
`aws-lc-rs` 1.18.2 against the unchanged production `sha2` implementation.
These are direct test-only dependencies; no version, production hash, encoding,
compiler optimization, deadline or acceptance gate changes. The ordinary debug
runner test verifies fixed vectors, one-shot and fragmented domain/body digests,
then reports bounded repeated warm-cache synthetic timings with rotated order.
Context construction is included. CPU feature flags describe capability, not a
measured backend trace. AWS-LC and ring support Windows MSVC through their
[documented build requirements](https://aws.github.io/aws-lc-rs/requirements/windows)
and [packaged assembly handling](https://github.com/briansmith/ring/blob/main/BUILDING.md).
No backend replacement or full-filesystem speedup is established by this test.

### Bounded proof-hash backend candidate (2026-10-24)

Ordinary Linux and Windows diagnostic runners both reported SHA-NI/SSE4.1
capability and exact digest agreement. On Windows, 128 whole-wrapper hashes of
256-byte bodies took median 1536/316/97 microseconds for sha2/ring/AWS-LC;
4096-byte bodies took 16264/659/403. These are warm-cache synthetic measurements,
not filesystem improvements. The unchanged native20 fixture still took 126.16s.

The next candidate uses the already locked ring 0.17.14 only for private bounded
proof accumulators in sparse views/preparation/publication, mount bindings and
namespace/open journals. Domains, canonical serialization, field order, SQL,
validation cadence and stored 32-byte SHA256 identities are unchanged. Contexts
contain inline state rather than AWS-LC's EVP allocation, so that alternative's
initialization/allocation panic path is not introduced. No hash is cached.

Payload one-shot digests, journal/remote metadata, core/provider hashing,
multipart and authority snapshot whole-file streaming remain on sha2. In
particular, ring's finalization rejects messages exceeding 2^64−1 bits, whereas
multipart admits larger logical sizes; this candidate does not change that
boundary. Migrated accumulators consume bounded metadata (sparse/preparation
metadata 32MiB, retained anchor 2MiB each/32MiB aggregate, bounded open events and
names), not whole logical file streams. Fixed vectors and an independent sha2
oracle cover 1MiB fragmented streams, reset, exact domain encodings and
current-format persisted anchor/role hashes. Standalone state tests on both OS
check the new direct dependency without relying on workspace feature unification.
Actual native20 and all original full 300/fault/compatibility gates remain pending
for this candidate. No production activation or performance acceptance is claimed.

### Guarded metadata reuse candidate (2026-10-24)

The proof-hash candidate passed the short actual Windows mounted20 fixture in
80.38s, versus 126.16s for the preceding production bytes. Its full Windows
mounted300 gate nevertheless reached only 280 saves before the unchanged
20-minute deadline. Linux mounted debug/release300 passed, but that does not
replace the missing Windows result. This remains an unaccepted candidate.

The next package removes duplicate representation work within the existing
private immutable read scope. Each loaded/captured anchor retains the length of
its exact canonical bytes for quota accounting, without serializing again or
changing persisted fields. A bound-native scope installs source headers/history,
pins, anchors and root metadata only after complete native graph validation.
Subsequent full source reads reuse that metadata but independently read every
block, enforce block quotas/types/digests, verify every Write reference length,
and reject missing or unreferenced blocks. No source-byte proof is inferred from
header validation. Absent-cache and ordinary unscoped paths retain fresh loading.

The scope remains confined to the same audited read-only transaction, with the
existing supplementary mutation/schema guards. It cannot cross a write, await
or transaction boundary; these guards are not a defense against arbitrary SQL
from hostile internal callers. Focused tests count one source-header and root
metadata decode while requiring two independent body reads, exercise late body
and metadata damage, and preserve exact rollback checks. Hosted performance and
all original acceptance gates are still required; no deadline is increased.

### Corrective pure-value and read-selection work (not yet accepted)

The main Windows timeout remains unresolved pending exact-source execution.
The next correction retains at most one 16 KiB canonical byte slice and three
fixed-size payload-reference hashes on the dedicated state worker. It parses
anchors again on every read, checks exact freshly queried bytes, and repeats all
row, role, semantic, owner, frontier, witness and physical checks. Only pure
canonical serialization and reference derivations are reused. The memo is
inactive outside the worker lifetime and clears on exit or unwind; it does not
retain a decoded object graph or any successful validation result.

Immutable preparation `Info` values derive their ID once from private owned
input. Upload/dependency and publication work selections combine adjacent
read-only observations in one transaction, ending it before provider awaits or
local mutation. Materialization retains the global trigger guard and fresh
child-receipt checks; subsequent effect admission and post-write validation are
unchanged. These changes require fresh adversarial tests, both-platform native
measurements and all original full gates. Instruction-profile estimates are not
filesystem latency or acceptance evidence.
