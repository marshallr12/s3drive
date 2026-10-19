# Authenticated clean-version observation

Declared 2026-10-19, isolated `codex/external-clean-reader`, based on local intent
source67a75e3/schema38. Parent full/hosted acceptance is pending; keep this phase
isolated until both parent and this source pass their own required checks.

Add a same-owner guarded observation preflight. It checks the scoped token and
returns a known immutable adoption before fresh cleanliness/work state, quotas or
read-pin allocation. Otherwise, validate exact clean current head and all unresolved
work, then allocate the shared ReadGuard before delivering the worker reply.
Cancellation/unread replies must drop the real guard through existing bounded
cleanup. Reuse the accepted worker; no task, queue, SQL owner or schema extension.
Preflight is not a durable preparation, remote capability or future commit permit.
The final local intent transaction must repeat its ordinary admission checks.

Add Reader::observe_clean(local, token, parent), returning a private observation
result with immutable View access and Recovered/Unchanged/Adopted disposition.
Known local recovery performs no authority/provider requests and uses no fresh
reader/pin admission. Check its volume against the reader before returning it.
A fresh call retains the old local guard, uses one existing active-operation and
snapshot permit and one overall reader deadline for all local/network work. Open
the live node and independently verify its manifest root with the normal SDK
HEAD/range/digest checks. Require the same file/store/source fingerprint/root
scope, reauthorize the pinned version and provider root revision, and recheck the
live authority payload immediately before the local CAS. Metadata-only rename may
remain valid when content and placement are unchanged. Unsupported empty/legacy
origins, deletion, foreign routing/principal, denied/stale/corrupt roots, dirty or
concurrently changed local heads must fail explicitly. Never substitute a
historical payload for a live observation, reset deadlines, fetch file data at
open, or hold SQL transactions across network I/O.

Known result recovery is historical local evidence, not fresh global state or
new permission to read/upload. Fresh success identifies authenticated content at
observation time, not a promise that another writer cannot change it after the
last check. No public candidate setter or apply-later authenticated capability.
Read opens after adoption use ordinary guarded/history authorization. A timeout
or canceled reply may leave the exact local operation committed; retry the same
old-parent token. No-change admits no operation, matching the local contract.

Ownership: state observation preflight/worker guard delivery, managed observation
module/export, SDK-backed chunk-reader cases and documentation. No dependency or
schema change beyond parent38, native activation, network mutation, retention/GC
or live-provider request.

Acceptance: SDK-backed authority/S3 fixtures verify larger/smaller/empty and Unicode
files, retained old reads, same-binding no history admission, exact known recovery
without RPC even when provider access is denied, dirty/pending/foreign scope/quota
refusal, root integrity and authorization faults, changed live payload during root
fetch, concurrent local edits, cancellation/timeout and cleanup, rename with stable
payload, bounded active/snapshot admission and one overall deadline. State tests
verify canceled/unread guarded preflight and known recovery at full pin quota.
Run focused tests, all-target Clippy, freeze the coherent candidate and run the
complete local gate; require own exact Linux/Windows/source/native/artifact checks
before promotion. These still do not establish mounted chunk writes, GC, physical
reboot or real Wasabi mutation acceptance.


## First focused attempt (2026-10-19)

Sourcea8cc73e/all342 unchanged passed both guarded worker cases in32.01s and
all three real authority/SDK scenarios in30.05s. The latter cover larger/smaller/
empty manifests, Unicode names, retained old bytes, known recovery without RPC,
unchanged polling, corrupt/denied provider/history results, pending/dirty/foreign
scope, metadata-empty origin refusal, live replacement, rename, local edit,
full pin/snapshot/active quotas, cancellation and overall timeout.

Workspace all-target Clippy rejected the344-byte preflight enum because its known
View variant is much larger than the8-byte guard variant. The exact failed source,
map/logs/runner/result remain under /tmp/s3drive-external-clean-reader-sdk1-*.
Source9913dd8 boxes only that known metadata View and updates the private consumer
and assertion; no behavior or test expectation was weakened. SDK2 repeats both
focused legs and Clippy on its separately frozen342 inputs, runner1176459.
The complete own local/platform gate remains required; main stays schema37.
