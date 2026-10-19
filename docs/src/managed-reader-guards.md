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
