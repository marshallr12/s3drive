# Atomic deferred canonical creation

Declared2026-10-21 from accepted main19ac76f/schema41/runtime368. The
authority/state prerequisite is now accepted; the dated candidate history below
is retained. This is an authority/state
prerequisite for chunk-backed delete-on-close, not native lifecycle activation.

The accepted empty-manifest mutation commits canonical64-byte root metadata,
file node, upload history and exact operation result together. Existing deferred
creation instead commits a metadata-empty node plus an original removal fence.
Combining those two calls would leave an observable/crash boundary without the
required original ownership. Source findings: namespace/publication/empty.rs,
namespace/removals.rs::{created,origin}, and removals/events.rs::proof.

The explicit wire mutation is `create-deferred-empty-manifest`, with the same
parent/revision/name/store/upload proof fields as `create-empty-manifest` and an
additional32-hex-digit nonzero `owner`. The feature is
`atomic-deferred-empty-manifest-v1`. Its receipt artifact is `published-removal`,
containing `payload` and `fence`. The fence intent is the original operation ID;
its owner is the exact requested owner. No path or later node can mint that fence.
The authority still records a caller's physical-upload claim; it does not gain
S3 credentials or independently verify an object body.

Reuse canonical root derivation and exact historical receipt checks. One immediate
SQLite transaction commits the node, canonical payload/upload history, deferred
removal row, initial removal event, volume sequence and exact operation result.
The initial reservation retains its ordinary canonical payload identity; the
removal fence governs later owned work. Validate the resulting history and
original ownership before committing. A failure must leave the entire old state
or the complete new state; no partial success receipt is returned.

Use schema42 as a compatibility fence, without new tables or old BLOB/hash
rewrites. Existing schema41 records remain byte-identical. New operations under
an earlier schema marker are rejected before migration; old binaries refuse42.
The64-table snapshot bound remains unchanged. Old mutation/artifact JSON vectors
must remain unchanged. Existing clients do not fabricate creation provenance or
silently accept the new artifact as an ordinary namespace admission.

Done criteria for this phase:

- Authority feature negotiation, exact wire validation, canonical empty root,
  original fence/event and retained upload-history reads work through real APIs.
- Exact replay and operation lookup retain the original receipt after cancellation,
  mode changes, rename/removal, later content, restart and concurrent contenders.
  Changed operation bytes, owner, store, parent revision and malformed proof fail
  explicitly. Fresh unrelated sequence advancement must not replace entry guards.
- Killed-owner checkpoints and SQLITE_FULL tests cover node, payload/history,
  removal/event, outcome, sequence and commit boundaries. Missing/ignored/rewritten
  rows and forged ownership/history refuse success and roll back.
- Migration preserves existing request/outcome bytes and rejects downgraded42
  operations. Preserve operation closing credits, quota/collision and64-table
  snapshot behavior. Full local gate and own hosted Linux/Windows/book regression
  acceptance are required before promotion.

Expected footprint: namespace mutation/artifact validation, shared canonical
creation helper and deferred module/tests, removal origin/event proofs, manifest
history reads, schema marker/migration checks, authority feature/wire tests,
native schema assertions and mdBook/handoff. No dependency, S3 executor, SDK
creation journal, sparse publisher or OS callback change is intended. Future
phases must import exact verified creation provenance into the same-owner open
journal and bind owned-unlinked reservations to the original fence/event before
enabling native save-replace/delete-on-close. GC, cold recovery, real-provider
and physical startup acceptance remain separate. Both hosting choices and
optional Turso remain open; no service or credentials are provisioned.


## Candidate status —2026-10-21

The authority/state candidate implements the explicit mutation and combined
artifact, using a private projection into the existing canonical physical-proof
helper. The projection is never stored or sent; the original owned request is
retained byte-for-byte. Replay checks the immutable initial removal event and
canonical upload history, without comparing an old receipt with a newer live
node. Later owned root publication and a replacement ownership epoch retain
the original receipt and cannot rebind its fence. No new tables or dependencies.

Focused2 passed13 state tests, one authenticated HTTP contender/restart case,
and workspace/all-target Clippy. Tests cover eight killed-owner points, explicit
SQLITE_FULL rollback/exact retry, operation closing credits, name/store/parent/
proof validation, missing/forged history/events, old-marker laundering (including
an artifact attached to an old operation), byte-preserving41→42 migration and
private offline snapshot inspection without restore rights. The HTTP case uses
normalization-colliding Unicode names in two stores and the production wire
client. It records caller upload claims, not actual S3 writes.

Earlier development evidence is retained: check1 found a helper visibility error;
tests2 found a gated unsigned SQLite conversion, resolved by reading signed SQL
integers and checking the conversion; tests3 exposed a missing-history NotFound
classification, corrected to an integrity error; focused1 stopped at a malformed
test literal before running Cargo. Tests1 passed3 initial cases and tests4 passed10
before the final later-publication/snapshot/epoch cases. Only the final unchanged
source can receive complete acceptance. Full local and own hosted/native gates
remain pending. Native lifecycle flags, client creation journals, provenance
import and sparse owned-unlinked publication are deliberately unchanged.


Candidatefeead770 complete local gate1 stopped after773.99s/exit101, all709
inputs unchanged.551 state tests passed,1 failed,1 ignored: an older ownership
fixture correctly refused a missing role but then expected its unchanged current
marker to be41 instead of42. Its explicit pre41 downgrade40 expectation remains
40; the current-marker branch is corrected to SCHEMA_VERSION. The hosted Linux
cohort also failed that fixture. No acceptance is borrowed from this cohort;
corrected source requires complete fresh gates. Gate1 logSHA
3869ebeb89e629d735dd541b4f57cf5066302916e1d26b1bcdf8c8f863792e5c.


## Authority/state acceptance —2026-10-21

Runtime `3e670915cdfe3dbafee9cd40c33d1a70cf979234`, schema42/all372, exactly16 runtime paths from accepted
main19ac76f. No new tables/dependencies or old BLOB/hash rewrites. Own full local
gate3 passed664.97s with all709 inputs unchanged; logSHA
`48a3dfd3767e37747ab70d9bbabbba70ed34f78055b382d07fcc34dd9ba04174`.
Own [CI39579144740](https://github.com/marshallr12/s3drive/actions/runs/39579144740)
attempt1 Linux/Windows/book all succeeded. Exact GitHub ZIP/all709 digests,
API artifact ZIP digests, strict typed21+14/project oracles, every raw-record
field/type and full raw-log/warning/slow/interleaved review passed.
All17 slow notices had later passing completions; all15 interleaved reference
completions were checked. Three disposable Windows VM profiles remained locked:
dummy credentials were removed, deletion deferred to VM disposal, following
pass lines reviewed. Tooling/linker deprecations remain in raw evidence. Hosted
images: Ubuntu24.04/20261018.339.1 and WindowsServer2025/windows-2025-vs2026/
20261018.275.1. These do not certify a physical desktop workstation.

The13 new state tests and authenticated HTTP case passed within the complete
suites. They cover eight killed-owner points, actual SQLITE_FULL, initial and
successor owner separation, later owned manifest publication, name/store/parent
races, missing/forged retained history and events, closing credits,41→42 migration
and private snapshot copy/inspection without restore rights. Existing real FUSE/
WinFsp, Unicode chunk folders and file I/O/remount,128KiB callback controls and
startup regressions passed on the same source. No new native lifecycle callback
was enabled and no S3 upload was performed by the new metadata-only tests.

A separate Linux binary compatibility control built the accepted schema41
sourcefea3b379, which refused a disposable schema42 database without changing its
bytes. Cargo rebuilt the new source afterward and its binary reopened42. The
control does not install or upgrade a real user database.

The first control's restore rebuilt the CLI but reused the old state library in
the shared target; a fresh fixture probe still printed41. That control failed
and its logs are retained. The successful control invalidated only the state
package at each worktree switch, verified actual42/41/42 behavior, and reproduced
the new binary hash before/after. A successful Cargo freshness decision alone
is not runtime-version evidence. This is a database schema marker; configuration
version8 and native lifecycle flags are unchanged.

Candidatefeead770/gate1 remains rejected: stale41 fixture expectation,551
state passed/1failed/1ignored locally; both hosted Rust jobs rejected it. Corrected
source3e670915 gate2 completed its tests but lost its supervisor (exit143) before
a final status could be recorded. The orphan child was verified live, then
reaped; its final exit was not inferred from logs. Only afterward did gate3
repeat with a session-isolated supervisor and retain the authoritative status.

- [Local indexed archive](evidence/deferred-canonical-local-20261021.tar.gz): 432 members, 1169783 bytes; SHA256 `621f693651c74986023ed71be8dc9f60c4330ec573244fb404eb2fcc3dd4e70f`. Every member verified.
- [Hosted indexed archive](evidence/deferred-canonical-hosted-20261021.tar.gz): 40 members, 224449 bytes; SHA256 `2bb73e5483c5bed7feb44e142afc058a82ade562394ba6fc1cbf7ef5543f156a`. Every member verified.

![Rust authority atomically stores canonical file metadata, original fence/event and exact result; the future SDK verifies the S3 object.](images/atomic-deferred-canonical.svg)

The physical root is still a caller claim at the metadata service. The next
phase must persist the original creator/physical proof and import its exact
receipt into the same-owner open journal. Sparse owned-unlinked publication must
bind the original fence and current removal event. Native delete-on-close,
save-replace/rename/delete, retention/GC and cold recovery remain unsupported in
the chunk profile. Real managed Wasabi, physical MX/Windows10/11 reboot/session,
independent-host deployment/restore and usable packaging remain separate gates.
Both hosting options and optional Turso remain open. No provider authorization,
credentials, service provision or user startup installation is implied.
