# Experimental quiescent lifecycle retention

Design checkpoint: 2026-10-25. **Unaccepted component-retention implementation**, not a
completed beta milestone. A separate experimental mounted profile is available. The accepted
`linked-retention-v1`, `owned-unlinked-v1` and absent/default profiles remain
unchanged. No old-store conversion and no remote object deletion are proposed.

## Why a different cut

Existing operations already implement creation, rename, replacement, unlink,
retained handles and recovery. Their local records remain dependencies of open
activation, original dirty epochs, namespace ownership and exact retries. The
linked-file anchor deliberately cannot summarize those dependencies. In
particular, abandonment of an obsolete session is not an observed native Close.

This prototype introduces a separate fresh combined-owner contract, admitted by
`chunk_mount::admit_quiescent`, while keeping the existing version2 owner and
monotonic admission frontier. Its explicit additional mode field prevents
ordinary or linked admission from silently adopting this owner. The mode cannot
contain linked-retention anchors. It exposes state APIs and an experimental fresh mounted profile.

Instead of manufacturing a smaller proof for surviving data, a successful cut
removes an entire validated local dependency forest. No local source survives
the cut that could need the erased provenance. Subsequent access must acquire
current verified authority metadata and remote content again. The owner identity,
consumed identity frontier, scoped/retry mode and source bindings remain.

## Admission and expiry

Direct namespace, canonical creator and cold source-import requests use separate owner-bound domains
of the permanent monotonic frontier. Exact retained retries precede consumption;
a forgotten original request cannot be inserted again. Existing callback policy
uses deterministic children of an original open ID, callback and fixed role.
Such namespace admission requires the complete original retained event chain and
exact linked input; a retired open cannot admit a new child. Erasing the parent
therefore expires its callback children as well. No callback identity is replaced
merely to retry an uncertain operation. A source re-import consumes a fresh
ordinal even if its remote descriptor is unchanged, so an erased source parent
ID cannot be recreated by retrying its original token. The initial graph and
all child mutation IDs therefore differ after a completed cut.

Prospective creator/open atomic admission remains explicitly refused in this
prototype. It needs its own paired reservation proof, rather than consuming two
IDs in separate transactions. No native application coverage is implied until
its actual creation route has been tested.

## Current conservative eligibility

`publication::retention::quiescent::retire` uses one immediate transaction:

1. Revalidate the exact mount key/mode, full source/preparation/root graphs and
   namespace/creator/open proofs. Refuse application triggers.
2. Require no logical pin or physical reader, no linked anchor, and every source
   head clean. A close barrier, partial preparation, dirty epoch or unknown root
   does not become clean because a process exited.
3. Require each preparation's root receipt and acknowledgment, every namespace
   outcome known, no surviving owned removal, every original open terminal and
   retired, and every recovery group finished and wholly selected. Recovery
   observers must belong to a different runtime from original members.
4. Require every creator outcome and physical witness settled. The exact set of
   known creator/transfer/root generations must equal all staged generations;
   unrelated staging blocks the cut rather than being erased.
5. End the read proof, remove the selected source/lifecycle/physical records,
   then construct an entirely fresh proof. The permanent binding, identity/session rows, source bindings and mode/reader
   counters must be byte-for-byte unchanged; source, publication, namespace,
   creator, recovery/open and physical journals must be empty. Cyclic source foreign keys are deferred
   only within the transaction and explicitly checked before commit; checking
   is never disabled.

The original `retire_quiescent` API selects the whole store. The newer
`retire_component` API selects a closed namespace-connected component while
retaining unrelated live handles, readers and dirty work exactly. Broader bounded
native lifecycle acceptance remains required before the milestone is complete.

## Acceptance still required

The prototype has modeled tests for separate admission profiles, expired source
re-imports, physical clone/acquisition races, exact retained
retries, active-open and reader-clone deferral, dirty/incomplete preservation,
closed publisher eviction and completed recovery without fabricating Close.
Compilation alone is not durable execution. Hosted tests must establish these
claims on actual supported storage before acceptance.

Before native enablement/acceptance, add and inspect:

- Exact stale namespace, creator, open, source command and owner rejection after
  a cut; unchanged owner/frontier and cold live namespace/content reload.
- Every transactional cutpoint under killed owners, SQLite-full rollback,
  deletion refusal, corruption and independent concurrent logical/physical reads.
- Completed creation, rename, replacement and unlink dependency groups, including
  original held-open dirty handles, rejected effects and unknown authority replies.
- Safe component selection while unrelated files stay open; bounded rows/bytes
  when dependencies close and explicit quota/backpressure otherwise.
- Actual Linux FUSE and Windows WinFsp repeated lifecycle tests, remounts, exact
  cold bytes, original Close/retry/crash/read gates, unchanged deadlines and the
  existing six300-save plus added larger/longer workload acceptance.

The filesystem must invalidate only its own evicted cached file entries; it
cannot drop independent readers to force eligibility. Package/startup/provider
and physical-machine evidence remains a separate gate in the
[beta completion plan](beta-completion.md).


### First hosted state-protocol result (not acceptance)

On exact `e11c5d66b6c8da37285b61618b947248f6218fe7`,
[run40206741871](https://github.com/marshallr12/s3drive/actions/runs/40206741871)
passed45 Linux focused diagnostic cases, including six initial quiescent tests.
Complete focused logs were reviewed. The full suite and Windows remain pending;
the follow-on source-expiry/post-cut/concurrent-acquisition additions are newer
code and do not inherit that result. No mounted option is enabled.

## Connected-component implementation in progress

The state-only collector also has a node-seeded component variant. Namespace
operations and original-open linked operations connect their affected file nodes;
parent directories are not file-content dependencies. It refuses dirty heads,
readers, active opens, unfinished recovery or uncertain publication in the selected
component, while preserving the exact source/root/namespace/creator/open records
outside it. Selected physical generations must be disjoint from every surviving
publication and creator. Owned creation receipts cannot be erased before namespace
import and closure of the owned removal fence. Identity frontiers and owner/session
bindings remain permanent.

A two-file test repeatedly reclaims a clean closed file while an unrelated dirty
file, its open and a cloned logical reader survive, both with and without an
unfinished preparation. This compiles with workspace Clippy; durable hosted
execution and namespace/crash coverage remain pending. The mounted profile is
still not exposed. This is implementation progress, not milestone acceptance.

## Experimental mounted integration (not accepted)

`component-retention-v1` selects the fresh contract explicitly; existing defaults
and `linked-retention-v1` remain distinct. It uses expiring source, creation and
namespace admissions and retries component collection after actual Close, known
creation/namespace completion and before cold source admission. A dedicated
retention mutex excludes collection from the source-head-to-reader admission
window; independent readers and active original opens still protect their data.
It never manufactures Close, changes remote deletion policy or collects S3 objects.

Shared and actual native unlink/replacement/remount scenarios are added separately
from all existing gates, with exact zero-forest assertions after closed scenarios.
Their hosted execution, plus broader bounded concurrent lifecycle workloads,
remains pending. This profile is development-only, not a beta readiness claim.

The first mounted diagnostic exposed a missing prerequisite: retry-safe stores
correctly refused the creator's generic staging path (`ScopedRequired`). The
correction admits physical creation only when an exact retained, consumed creator
identity authorizes its operation key, target and body length; remote preparation
additionally requires its linked, validated generation. It does not reopen generic
staging. Tests cover pre-admission refusal, wrong key/size, and pre-link remote
refusal. Candidate discovery now includes metadata-only records; six component
process-kill boundaries retain an unrelated dirty file across rollback or commit.
The SQLite AUTOINCREMENT sequence is explicitly included in unchanged permanent
state checks, preventing generation identity reuse after empty-forest collection.

On exact `160113a61d404ca81a9e2dcf5a5dc749a1317e8b`,
[run40210526528](https://github.com/marshallr12/s3drive/actions/runs/40210526528)
passed50 focused state cases and all4 shared create/unlink/replacement/cold-recovery
cases, with closed-forest zero-count assertions. The complete diagnostic log was
reviewed. Actual native and full-suite acceptance are still pending. A subsequent
repeat test retains two unrelated dirty-file handles while cycling creation,
rename, unlinked writes and actual Close; no test-only collection is invoked.
Local aggregate gate reached successful format/Clippy but failed durable authority
initialization on this unsupported local filesystem; it is not a local pass.

Actual callbacks on both Linux and Windows at `22480e91` exposed a final-reopen
read failure after otherwise successful unlink/replacement operations. Shared
coverage (51 state and five lifecycle cases) passed, so it did not establish
native acceptance. Source admission now carries an independent reader lease
through dispatch until original-open activation; a prior Close may clear only its
own cached reader, never that arriving open's lease. A deterministic regression
pauses at that dispatch boundary and completes the prior Close before activation.
The original deadlines remain unchanged. This correction still requires fresh
shared, actual-native, crash and full-suite verification.

The reader-admission correction on exact `306ed1817bd40d6830741c6136cbddca4733693e`
passed the actual Linux unlink/replacement/held-handle/cold-remount case in16.70s
in [run40211998842](https://github.com/marshallr12/s3drive/actions/runs/40211998842),
with its complete focused log reviewed. Windows and full acceptance remain
pending at this checkpoint. New acceptance additions exercise repeated actual
native lifecycle with an unrelated dirty reader, expired namespace identity,
and genuine SQLite page-cap exhaustion (complete validated cut or exact rollback,
not a claim of physical-disk/WAL exhaustion simulation).

The additional native component soak runs100 create/rename/unlink/held-write/Close
cycles over25 independent owners and50 actual mounts, retaining an unrelated
dirty writer/reader during each four-cycle segment. Every segment checks live
row/DB/WAL bounds, both final cold bytes and zero closed local forest, and all
previously removed names. The parent verifies exact cumulative cycle counts and
551 unique immutable PUTs. It has its own45-minute release workload budget;
original120-second live sequence,180-second child and all existing gates remain
unchanged. This is separate from the existing linked-profile1000-save and
four-file64KiB300-save soaks. Complete per-cycle observations are emitted even
on success. Execution is pending on the corresponding source commit.

The first100-cycle fixture revision incorrectly requested exclusive creation of
its surviving protected file on every owner. The corrected fixture creates only
once and cold-verifies/reopens it thereafter. A separate eight-cycle/two-owner
native regression covers this boundary before longer acceptance. Live and closed
checks also bound permanent sessions, source bindings, owner/frontier records,
reader-session rows, SQLite sequences and physical chunk bytes, not merely the
erasable forest. Closed checks cover all physical/source/open child tables.

Physical alias review: `staged_operations.generation_id` has a UNIQUE schema
constraint and creator linking requires its own exact staging key, target,
sealed body and original generation. A focused negative test keeps an original
complete physical witness, independently admits another creator, and verifies
that borrowing the original generation returns typed Conflict. Adding a second
key for that generation fails the actual uniqueness constraint. Both refusals
preserve the entire catalog and original physical bytes; staging/linking a truly
independent generation then succeeds. This tests the earlier invariant rather
than constructing a falsely valid shared-generation witness. Component selection
also independently refuses intersections with surviving creator/plan generations.

## Linux SQLite lock-lifetime correction (acceptance pending)

Research2026-10-25: a longer live-reader soak failed at cycle31 with SQLite's
malformed-image error; a separate unchanged-production100-cycle run passed.
Neither result justifies ignoring the intermittent failure. A deterministic
cross-process probe then showed that a reopened live SQLite owner no longer
held its SHM deadman lock. `OwnedDirectory::finish_initialization` opened and
closed ordinary descriptors for SQLite's DB/WAL/SHM after SQLite established
its own locks. POSIX close releases the process's locks on that inode, even
when another descriptor still needs them. This is a documented application
hazard, not evidence that upgrading SQLite would fix it.
[SQLite locking guidance](https://www.sqlite.org/howtocorrupt.html#posix_advisory_locks_canceled_by_a_separate_thread_doing_close_).

The Linux correction retains post-open verification handles before validating
or tightening permissions. `State` drops SQLite first; the owned directory then
drops verification handles **before** releasing its exclusive directory lock,
including initialization-error paths. Windows descriptor behavior is unchanged.
No permission, storage, reader or deadline guard is weakened. The deterministic
probe fails on the earlier code and passes after retention; a separate external
read-only SQLite observer exercises repeated writer owners and integrity checks.
Local tmpfs results establish locking behavior only, not durable acknowledgment.
Actual FUSE/WinFsp, repeated long soaks, crash and full acceptance remain required.
