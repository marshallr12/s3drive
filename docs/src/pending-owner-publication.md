# Publication while deletion is pending

**Implementation contract, 2026-10-17.** This extends the accepted schema-14
[client ownership checkpoint](windows-delete-plan.md#client-ownership-checkpoint-p6m3b3b4b1).
It does not enable native Windows deletion or change any live provider policy.

## Owned publication

An already bound local file may stage dirty bytes while its name is reserved.
New `ReservePendingContent` inputs require this client's acknowledged arm,
matching owner/intent fence, frozen source/store/fingerprint, exact current
logical base revision and complete prior payload reference. Unknown namespace
outcomes must reconcile first. Ordinary, foreign, stale and detached guards
cannot substitute for that fence. Existing immutable operation inputs remain
loadable for recovery after a fence expires; new work cannot adopt them.
Physical staging must link to that already frozen logical edit; a raw pending
publication cannot bypass the local namespace closing barrier.

Publication still requires one-shot create-only S3 upload, complete byte/hash
observation, the exact authority metadata receipt and atomic local logical
settlement. Successful owner writes advance the pending file's payload without
releasing its name. Cancellation preserves the newest verified payload; commit
retains it on the tombstone. No unconditional upload, automatic guard refresh,
S3 deletion or garbage collection is introduced.

## Retained conflicts and closing admission

A prepared publication cannot be ignored merely because its caller received an
error. Schema 15 records two proven terminal refusals: a metadata `Conflict`
bound to the exact reserve/publish request, or an existing immutable remote
attempt in rejected state with HTTP 409/412 and matching operation, target and
bytes. Timeouts, missing observations, in-flight work, temporary delete-pending,
operation mismatch, service/quota errors and policy expiry remain unresolved.

The managed journal keeps a separate integrity-checked rejection record. No
retry may turn it into a success or send a replacement PUT. A caller must
explicitly retain that conflict in the logical journal before namespace closing
admission treats its publication as resolved. The marker binds the original
logical generation/publication and exact rejection; neither bytes, operation
ownership, dirty head nor original binding is deleted or marked settled.
Unknown work cannot acquire such a marker. A later new generation may publish
with its own explicit current guard; old attempts remain immutable and retained.

Markers and rejection rows are checked on every use. Ignored SQL writes, full
disk, corrupted or missing evidence must fail closed. A lost local reply is
recovered using the same identifiers. Namespace receipts continue to atomically
update file observations; a retained conflict never authorizes silent rebasing.

## Acceptance

Require state tests for owner admission, unchanged old hashes, exact retained
failure evidence, ignored writes/full disk, migration/refusal and newer dirty
heads. Actual SDK/TCP tests must cover successful pending-owner writes, lost
remote/local replies and killed clients, exact recovery without duplicate PUTs,
cancel/commit retaining published bytes, physical and metadata conflicts, and
unknown outcomes refusing closure. Run the repository gate and terminal
exact-tip Linux/Windows/book CI, preserving existing native/startup cases.
These tests use owned loopback fixtures with dummy credentials. Native WinFsp
disposition/cleanup, actual reboot and managed Wasabi acceptance remain separate.

## Current verification evidence

Six focused state cases pass, covering both admission paths, frozen operation
ownership, exact rejection evidence, explicit retention, readback after ignored
inserts/deleted rows, corruption, actual SQLite full at both retention boundaries,
restart and schema-14 migration without changing old hashes. Conflicting schema
shapes preserve the original database and version. State publication proof
fixtures are not physical S3 evidence.

Two actual SDK/HTTP scenarios pass on the normal stack. The first kills client
and authority after a lost reservation reply, an uncertain committed PUT with
observation refused, a lost committed metadata reply, and final local/namespace
acknowledgments. Binary Unicode-name and empty-file generations upload once
each, retain exact payloads through cancel/commit and recover without duplicate
PUTs. Unknown outcomes refuse conflict retention and closing before an RPC.

The second injects a real conditional S3 rejection, ignores its first local
rejection insert, then kills both owners. Restart persists the same rejection
without another PUT. A separate publication gets a real metadata Conflict after
the fixture injects a future Info sequence; its verified orphan remains stored.
Explicitly retained conflicts survive another crash, permit cancellation or
commit without settling dirty bytes, and allow a new explicitly guarded
generation. Rival, orphan and replacement objects all remain; the scenario
performs no S3 DELETE beyond fixture setup's synthetic probe cleanup.

Focused logs: `/tmp/s3drive-pending-publication-focused3.log` and
`/tmp/s3drive-pending-owner-sdk3.log`. The full local gate passed, exit 0:
156 state tests, eight managed-publication cases including child helper, six
authority TCP, 50 S3 and 13 FS, plus build/lint/book. Terminal exact-tip native CI
remains required; Windows session acceptance must strictly use schema 15.
