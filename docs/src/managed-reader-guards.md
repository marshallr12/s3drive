# Managed readers retain their exact local view

Declared 2026-10-19. Isolated component phase from29db0b8/schema37. The parent
registry/guard components passed their scoped checks but still need parent/full
platform acceptance. No native chunk writes or garbage collection is enabled.

Acquire a clone-sharing local ReadGuard before descriptor validation, history
requests or S3 reads in reopen_edited_inner. Store that guard as the EditedFile's
view, with immutable Deref-to-View metadata access to avoid another copy of a
potentially large view record. EditedFile clones and Preparation clones share the
same guard. Reopened preparations acquire their own guard via the same opening
path. Local composition and chunk preparation must read through ReadGuard::read;
metadata comparisons and original provider/history authorization remain unchanged.

An opening failure, timeout or cancellation drops its guard. Known/unknown upload
recovery continues to use its existing separate metadata-only path. Do not add
SQL transactions around network I/O, new provider calls, deadline resets or a
constructor accepting arbitrary bytes. When the independent authorization batch
is integrated, its private prepare_chunk local-read path must use this same guard.

Coalesce cleanup wakes on the worker: a burst of dropped readers must not fill all
eight ordinary command slots with redundant wake commands. Record every pending
pin before signaling; one queued wake covers the map. Clear the wake flag when
that command is consumed, and retry cleanup after it so drops during the first
pass cannot be lost. Full queue/failing cleanup still preserve retention, with
no task spawning or unbounded background retry. Guard state and same-owner kernel
ownership semantics remain unchanged.

Ownership: state guard metadata Deref, worker wake coalescing and focused queue
case; managed composition/preparation; existing chunk-reader dispatch, a new
SDK-backed lifetime fixture, and this contract. No migration/dependency change.
Acceptance: all existing state guard tests plus a deterministic burst showing only
one wake; SDK quota proves file/preparation clones share one pin; dropping the last
clone frees capacity; failed/canceled opens free their pin; old exact reads and
source/history/provider denials remain enforced. Existing composition, limits and
preparation regressions must pass. Full workspace and own exact platform evidence
follow parent integration; portable tests cannot prove mounted lifecycle success.

## Focused checks (2026-10-19)

Seven state guard cases passed48.89s, including the deterministic coalesced-wake
check. The dedicated real authority/SDK lifetime case passed30.01s: file and
preparation clones share one pin, reopened preparations acquire a pin, wrong-volume
open cleanup returns capacity, quota denial makes no history/provider request,
and cancellation after an observed historical-root request returns capacity.
Initial SDK compilation exposed missing fixture imports/method name. SDK2 reached
all earlier assertions but its cancellation target double-prefixed an already
absolute fixture root path; exact failed source/log retained, target corrected,
and the same handshake assertion passed in SDK3. No production assertion weakened.
Workspace all-target Clippy passed35.45s with warnings denied. Broader state/reader
regression is next; parent/full/platform acceptance remains required.

## Complete component regression (2026-10-19)

Checkpointb28ca7b/schema37 passed481 state cases (one diagnostic ignored) in298.56s
and23 SDK reader cases (one native-only ignored) in46.92s. All337 frozen inputs
remained unchanged. Existing corruption/authentication/limits/reopen/preparation
cases passed alongside the new lifetime check. This is not the full workspace
gate or this combined source's native/platform acceptance.

The [indexed component archive](evidence/managed-reader-guards-component-20261019.tar.gz)
contains355 verified members/898322bytes: exact337-input source, both broad logs,
focused/Clippy logs, runner/result, maps and retained failed fixture sources.
SHA-256 `4a1324c00285c6d887987e3cf2de0abc9e09ef5ad888b88c7a47564ee86ee8c6`.
[Dated evidence](evidence/managed-reader-guards-20261019.json) records scope.
The one-use archiver is complete and must not be rerun. Parent acceptance and
integration with the accepted batch private-read path remain required before a
fresh full workspace/platform cohort. Mounted chunk writes and GC remain disabled.

## Declared integration with verified batching (2026-10-19)

Prepare codex/managed-reader-batched from8b29461 while the separate schema36
candidate44f0ee1 runs its own CI. Its full local gate passed, but it is not yet
accepted for promotion. The earlier schema37 guard component passed its state/
reader checks; neither result establishes this new combined337-input runtime.
This integration remains isolated until both parent acceptance and its own full
local/platform/source/artifact acceptance are complete.

Apply the accepted batching changes with a three-way merge, preserve the guard
in EditedFile/preparation reads, and change the new private DependencySession
chunk-read path to ReadGuard::read. No composition byte read may bypass the local
retention guard. Metadata-only unknown recovery remains before new upload
validation. Do not change source formats, schema37, dependency APIs, deadlines,
per-child semantics, provider authorization, worker cleanup or native activation.
Remove the inherited unsuccessful SDK admission semaphore so publication tests
run at ordinary four-thread concurrency, as in the combined scoped candidate.

Ownership: the five batch runtime paths plus resolution of its private chunk-read
path; removal of fixture admission; this contract and source/gate records. Existing
source/guard code remains the component-tested implementation. Verify the exact
337-input merge, run the new lifetime SDK check and full workspace gate, then
require this combined source's own native/platform/artifact cohort. Keep native
chunk writes and GC disabled. A passing parent cohort cannot accept these changes.

## Combined batching full local gate (2026-10-19)

Isolated checkpointefb2ab4/schema37 passed the complete local gate at16:50:32Z
with all337 frozen inputs unchanged. Publication24/168.58s, state481 (one
diagnostic ignored)/249.64s, reader23 (one native-only ignored)/44.94s,37core,
62S3, formatting/all-target Clippy/build,87Markdown links and80book chapters
passed. Ordinary four test threads were used; no SDK admission semaphore or
deadline increase was used. Both ordinary and private batch source reads retain
the shared guard; known physical recovery keeps its separate metadata-only path.

The [indexed local archive](evidence/managed-batched-local-20261019.tar.gz) contains
348 verified members/5148220bytes: exact337 committed/working inputs, full gate
logs/map/runner/result, and two independently hash/index-verified parent archives.
SHA-256 `3927d8dda7400c685e17d7cb44c35f76acf5aee5c1c299c4fd75dccd224745d1`.
[Dated evidence](evidence/managed-batched-20261019.json) records scope; full log SHA
`a4ee7d335147c38bdeb4ecae0f79e4707d0cb9816ba10fd7010e92cd1906b20d`.
The one-use local archiver is DONE and must not be rerun. Parent36 and this combined
source's own native/platform/source/artifact gates remain required before promotion.
No mounted chunk writes, source reclamation or physical reboot proof follows.


## Combined source acceptance (2026-10-19)

Own source903ade67e21cd71bb7ab02716c098993d9f6e82f passed
[run39289349689](https://github.com/marshallr12/s3drive/actions/runs/39289349689),
attempt1: book118185617231, Linux118185616746 and Windows118185617331 all
succeeded. Parent36 is independently accepted. All337 frozen runtime inputs
match this own tested source and the working candidate. Complete raw logs,
API ZIP digests, independent typed21+14 records, project oracles and all35 raw
record fields and JSON types passed. Artifacts12204512424/12205563584 are this
source's own replacement/disposition reports; no parent artifact was reused.

The [indexed hosted archive](evidence/managed-batched-hosted-20261019.tar.gz)
contains104 verified members/399826bytes, SHA-256
`a5f64762435bcb81f33f7c25a26b6d9f773587bb127aa310d0efc23b4cf08e2b`.
Local and hosted one-use checkers/comparator/archivers are DONE; never rerun.
Observer1101033 CLOSED0/all_success at17:24:27Z. The prepared checker had one
inherited reader-count assertion corrected from22 to23 before its first execution;
its original is retained locally and the archived checker requires23.

Own Linux481/Windows476 state cases (one diagnostic ignored each),24publication,
23reader (one native-only ignored),37core and62S3 passed. All six Linux and ten
Windows slow cases completed successfully. Fifteen interleaved Windows reference
tests had explicit successful completion. Separate actual native runs passed
FUSE9/73.44s, WinFsp7/97.07s, manifest reads/remount1.80s/2.39s, and paired Linux
without-systemd24.17s/Windows78.74s. Actual images were Ubuntu24.04
20261012.334.1 and WindowsServer2025 windows-2025-vs2026 20261007.264.1.

Three disposable Windows profiles stayed locked; dummy credentials were removed
and profile deletion deferred to hosted VM destruction. Unused WinFsp delay-import
linker warnings and tooling deprecations were retained and reviewed. These results
accept the guarded reader/batching libraries and existing driver regressions;
they do not enable mounted chunk writes or reclamation and do not prove real
Wasabi writes, physical MX/Windows10/11 reboot or production failover.
