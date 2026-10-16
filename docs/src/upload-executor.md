# Journal-driven single-PUT executor

**P5b2b, implemented 2026-10-16:** the durable journal is now connected to the
real AWS SDK's single-attempt PUT and full-byte observation path. Local TCP
fixtures exercise the complete sequence, including killing the executor process
after the server commits. **Production construction is disabled.** No upload
command, writable mount, remote flush or automatic dirty-byte reclamation is
enabled. Live Wasabi preconditions and an explicit application write policy
remain prerequisites for enabling an executor factory.

The exported `s3drive_s3::uploads::Executor` has private fields and no production
constructor. Its implementation is compiled and linted in normal builds; test
code alone constructs it with isolated local endpoints and dummy credentials.
Neither a `Capabilities` value nor a disposable probe report can construct one.
Existing `mode = "read-only"` configuration behavior is unchanged.

![The local journal grants one send claim; the gated SDK executor sends once and observes exact remote bytes.](images/remote-journal.png)

## Submit and reconcile have different authority

`submit(generation)` first checks the generation's target against the pinned
runtime credentials and configuration, including the full prefixed S3 key.
It does this before preparing an intent. The journal then selects the current
sealed head and its guard. The executor reads and hashes the bounded local body
before claiming a send permit. Claim rechecks the body, binding and current
head on the SQLite owner; only the committed non-cloneable permit can enter
the private send function.

The request carries the operation ID, full SHA-256 and exact recorded guard.
It uses known-length bytes, required-only SDK checksums and one SDK attempt.
Network I/O and body observation occur outside SQLite transactions and outside
the storage owner thread. The existing private transport is shared with the
[provider probe](conditional-uploads.md); fixture-only preflight and deletion
remain in the probe module.

An already claimed generation is observed when submitted again. It is never
sent a second time. A cancelled or rejected intent remains terminal; a verified
receipt is returned as historical evidence. Returning `Info` is not by itself
success: callers must inspect its phase, and only `Verified` denotes a checked
remote receipt.

`reconcile(generation)` performs no preparation and never calls PUT. Missing
intent is an explicit error. Prepared, cancelled, rejected and verified records
can be inspected without a provider request. In-flight or uncertain records
require complete matching remote evidence before verification. This separation
keeps recovery from accidentally becoming a retry queue.

## What establishes verification

After a valid PUT receipt, the executor still performs HEAD and a complete
conditional GET. Operation metadata, size, full body SHA-256 and revision must
match. The observed revision must also match the success receipt when that
receipt is available. A false receipt, wrong metadata, short/corrupt body or
revoked read permission cannot become a verified journal record.

After a lost or malformed response, matching operation metadata and complete
bytes can establish the remote result without another PUT. Missing and rival
objects instead leave the intent uncertain. A committed object followed by a
deletion is deliberately treated like a request that never committed: absence
cannot distinguish them and grants no new send permission. A 409/412 response
records a conditional rejection and preserves the local conflict bytes.

Errors include the local generation and, when known, operation ID. A failure
after network commit can also be a local acknowledgment failure; closing the
storage owner cannot undo the remote request. Reopening the journal and
observing the exact operation is the recovery path. Cancellation similarly
leaves the committed claim intact. No error, timeout or dropped future implies
rollback or authorizes an unconditional fallback.

## Bounds and evidence limits

An executor shares four execution slots across clones. Extra work fails before
journal admission rather than queuing file-sized bodies. Each request uses the
journal's 8 MiB single-PUT bound. Empty objects are supported. Larger local
generations are retained and readable; multipart is still required for their
remote commits. The session pins one resolved credential generation; it cannot
silently adopt a rotated key/token or another configuration's local history.

Local SDK/TCP tests cover exact Unicode/literal keys, empty bytes and a complete
8 MiB transfer, lost and
malformed responses, true/false receipts, 409/412, service failure, deletion or a
rival after commit, corrupt/short/wrong-metadata GETs, revoked reads, stalled
PUT/GET deadlines, slot saturation, credential mismatch, oversized/invalid keys,
lost local acknowledgment and process termination after server commit. They
assert the number of actual PUT requests, not merely the number of executor
method calls. A concurrent observer verifies an older generation while a newer
local head remains dirty; its descendant then uses the verified ETag guard.

Independent SQLite directories also race creates and replacements, requiring
exactly one verified winner and one retained 412 conflict under the fixture's
guard model. This is end-to-end executor evidence against that model. It is not
Wasabi acceptance, provider-wide linearizability, cross-machine locking or a
proof that ETags are unique generation counters. The existing ignored-guard
provider probe must still detect an incompatible endpoint.

## Required next work

Run the separately authorized live Wasabi probe before asserting conditional
write support there. Define a source-bound application write policy and enable
construction only after its acceptance requirements pass. This does not broaden
the pending six-key probe authorization to ordinary bucket contents.

Uncertain/rejected intent resolution, explicit credential rebinding, multipart
recovery, remote flush and reference/pin-aware acknowledgment cleanup remain
required. Writable FUSE/WinFsp callbacks and desktop application acceptance
follow those contracts. Existing mounts remain read-only.
