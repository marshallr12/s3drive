# Accepted prospective creation SDK

Updated 2026-10-21. **Accepted library implementation; not installed for user mounts.** This implements
SDK step B on top of the [accepted admission state layer](creator-open-state.md).
State schema 45, creator format 3, persisted bytes, authority protocol and native
lifecycle flags are unchanged. No provider, hosting or user startup change is
authorized by this work.

## New entry point

`Creator::create_for_open(draft, chunks, CreationOpen)` derives the owner from
the retained chunk capability and checks that it belongs to the executor's
SQLite worker and volume. A read-only store-control check runs inside the
original Creator's shared semaphore and deadline, before fresh RPCs. It grants
no provider permission or remote liveness. Source/store/owner/session constraints
are checked again by typed state admission.

For a new/unclaimed attempt, the original atomic creator/open admission commits
before generation staging, S3 PUT or authority Apply. Missing/obsolete sessions,
invalid or foreign bindings and creator/namespace/open/event quota failures
therefore cannot start those mutations. Existing `create` and `create_deferred`
retain format-1/2 input serialization and staging keys. No new Creator or
semaphore is constructed per call. The original boxed-future boundary remains.

The return value is still `HistoricalCreation`. It reports the original receipt
and original fence; it does not import a namespace row, activate an open, grant
a handle or prove current remote ownership. Caller-side exact import and local
activation retain their separate state gates. Native code will still need fresh
remote fence/event checks before a live handle or owned-unlinked publication.

## Retry and physical observation

Changed draft/claim input is a binding conflict. Known rejection stays a retained
conflict. Saved metadata requests/receipts replay before fresh feature, parent
or policy probes, even after the node changed or the original session became
obsolete. Replay preserves the original operation and grants no new handle.

A retained physical claim is different from fresh upload permission. Its
validated original creator/open record remains reserved after restart. The SDK
passes that same generation to the executor without asking for fresh session
admission. The executor can observe InFlight/Uncertain, reuse Verified, or refuse
Rejected/Cancelled outcomes; it cannot send a second PUT for a consumed claim.
Fresh policy/feature checks remain required where appropriate for a new attempt
or before preparing a new metadata request. Fresh/unclaimed uploads still call
typed admission and cannot reuse an obsolete session. No operation, parent,
source generation or claim is silently substituted to make recovery succeed.

## Metadata-only recovery

`Recovery::for_chunks(chunks, authority, store, limits, deadline).await` validates
the retained store control using the existing worker and returns a metadata-only
recovery object. It takes no S3 client, credentials, write policy or body bytes.
Construction is deadline-bounded. Close through the original `Storage`; this
recovery handle cannot independently close a shared worker.

Enumeration first validates the entire creator role and its witnesses, then
selects the matching volume/store and client owner. Ordinary unowned legacy
records remain recoverable. Legitimate other stores and foreign legacy owners
are not adopted. Direct resume of an out-of-scope operation is a binding error
before RPC; corrupt excluded records still fail the whole role check. Legacy
recovery constructors retain their existing scope behavior.

An unknown saved namespace request may be resent only as the exact original
idempotent Apply, then its exact outcome is retained. A saved receipt returns
without any RPC. Metadata-only recovery never calls fresh feature/parent/S3
probes. Unknown physical outcomes still require authorized provider observation;
an unprepared creator remains explicitly NotPrepared. No no-effect inference,
garbage collection or payload deletion is introduced.

## Verification and acceptance

Development workspace check and workspace/all-target Clippy (`-D warnings`)
passed (Clippy 36.18 seconds). Nine owned SDK/TCP cases passed in
21.18 seconds, then again in 31.57 seconds after quota-fixture validation was
strengthened. The owner/store-scope case subsequently passed in 3.92 seconds
and the added rejection/restart case in 2.23 seconds. These are development
cohorts. The final full gate and exact-source hosted/native regressions also
passed as recorded below. All development cohorts remain retained.

| Case | Evidence required and exercised |
| --- | --- |
| Flow | Real signed SDK PUT paused before server commit; independent SQLite reader sees format-3 creator, inactive original open and retained anchor, with no namespace row and unchanged provider objects; exact import/activation and historical replay |
| Controls | Foreign worker and undeclared store fail before RPC, absent session/invalid claim produce no PUT/Apply or new records |
| Quotas | Creator, namespace, open-row and event budgets saturated independently; seeded bulk history validated before call, quota cause checked, no PUT/Apply or partial second creator |
| Recovery | Actual authority commit with lost reply, restart, credentials/policy absent and staged bodies damaged; only exact Apply replay, then no-RPC receipt replay; obsolete open cannot activate |
| Physical | Actual lost PUT reply, new runtime, narrowed write policy and denied fresh feature; original claim observed to Verified before fresh feature refusal, then completed without resend |
| Unclaimed | Old runtime's admitted but unclaimed intent cannot start a PUT; exact state retained, metadata-only resume NotPrepared with no RPC |
| Deadline | Existing Creator/legacy/recovery calls share the original Busy slot; timeout retains saved request and releases the slot for exact retry |
| Process kill | Actual child termination after remote commit but before local receipt, then separate metadata-only child with credentials/policy absent and corrupt bodies |
| Stores/owners | Two stores plus ordinary/foreign-owner legacy records; scoped enumeration and resume, bad store refusal, and corruption in an excluded store not hidden |
| Rejection | Name collision retains the exact rejected request and immutable root; retry/resume send nothing; obsolete inactive open can close and retire without activation |

Bulk quota histories are **modeled**; quota refusal itself uses the actual SDK
entry point and public worker. S3 and authority requests use actual HTTP/AWS SDK
against owned loopback servers with dummy profiles. The PUT-order check is a
server gate before object commit, not a sleep. Process death and SQLite ownership
are real. This is not new live Wasabi, kernel lifecycle, physical workstation
or independent-host fencing acceptance. Existing native regressions passed
on both hosted OSes; new native lifecycle flags remain unsupported.

The [creation architecture](images/creator-open-admission.svg) shows the overall
flow. This accepted implementation wires the SDK portion; mounted lifecycle wiring follows
the sparse owned-unlinked publication and recovery/retention prerequisites.

## Frozen source and retrievable evidence

Runtime source `8022fae3d1b496c511321f0ee6294908bc749039` has 381 runtime
inputs, six runtime changes from accepted main `8780ce4`. Full local gate
exited zero in 988.85 seconds with all 739 inputs unchanged;
37 passing test cohorts include all ten new SDK cases. Every local slow
notice has a later exact pass. Local log SHA-256: `a03e6dd0792d1f299aa3b15abbc5e8e0d2408771bd686181353ff1b9fb863478`.

[Own CI 39633251767](https://github.com/marshallr12/s3drive/actions/runs/39633251767)
attempt 1 passed Linux, Windows and book jobs. The independently retrieved source
ZIP matches all 739 files. Both Rust jobs contain the ten new SDK case completions,
17 prospective state cases, 14 prior import state cases and three prior SDK cases.
Strict 21 replacement plus 14 disposition records, project PowerShell oracles,
all raw record fields/JSON types and complete raw log review passed.
23 hosted slow notices resolved; 15 interleaved completions were checked.
No schema or persisted shape changed; prior schema-45 compatibility evidence
remains in the [state contract](creator-open-state.md), not rerun or relabeled here.

- [Local evidence archive](evidence/creator-open-sdk-local-20261021.tar.gz): 406 indexed members, SHA-256 `8a4c1141b27de2cd6877ab8537dba8c9b3d3056c2ae88c8cd5e8a1460dc2d732`. Every member was checked after sealing.
- [Hosted evidence archive](evidence/creator-open-sdk-hosted-20261021.tar.gz): 35 indexed members, SHA-256 `c53073fae882e0353efc710f43b951e72984e56ab0a0c3f8f6591a248f5079ec`. Every member was checked after sealing.

Hosted runner images, warning contexts and artifact/source digests are retained in
the hosted archive. Disposable Windows profile cleanup may be deferred to VM
destruction only under the guarded cleanup contract; dummy credentials must be
removed and cleanup errors still fail the test. These are hosted regression
results, not physical MX/Windows10/11 reboot or live Wasabi acceptance.

The updated architecture SVG was rendered and visually reviewed;
[diagram review](evidence/creator-open-sdk-diagram-review-20261021.json) retains
the unavailable converter and corrected local rendering-adapter attempts.
The SVG and runtime were unchanged by those renderer corrections.
