# S3Drive handoff

Updated: 2026-10-17. Current phase: **deferred replacement reference and native
Windows lifecycle integration** on `codex/windows-replacement-reference`.
Goal ACTIVE. Read `docs/src/windows-replacement-reference.md` and the plan.

Published baseline verified live: private clean local/remote main
`27697fd7c3974a5f3ed11e3000a7d5325a2340c8`, terminal all-green
[run39059948714](https://github.com/marshallr12/s3drive/actions/runs/39059948714).
Linux117475664757, Windows117475664609, book117475664551. 240/236 state,
nine HTTP, sixteen SDK, seven native cases per OS, fourteen typed NTFS traces,
four pinned upstream and nine malformed-fixture checks passed. Linux no-systemd
and Windows non-admin independent-logon startup passed. Source-bound artifact
12111735021 matched all typed records/OS at2026-10-17T19:01:08Z.
Proof `/tmp/s3drive-mount-recovery-publication.json`; logs
`/tmp/s3drive-session-{linux,windows,book}.log`. Schema20/config6 baseline.
Privacy must be verified live; previously restored from public, confirmed private.

Controller session recovery is now accepted: session bindings use a fresh runtime
identity only after exclusive State directory acquisition. Same-owner supersede
and replacement of a lost client identity are refused. Old admissions/callbacks
replay exactly; separate abandonment proof keeps missing cleanup/close/activation
missing. Freeze old open hashes and same-fence cancellation before RPC, then
retain full evidence and retire completed/recovered records from live budgets.
Five SDK processes retain four immutable PUTs with binary/empty/Unicode fixtures,
unknown cancellation, failed proof, recorded removal and uncertain admission.
Exact old link replay remains bookkeeping despite later unknown node effects.
Planned replies still do not prove kernel receipt; do not enable callbacks yet.

Current reference harness: seven replacement cases on NTFS and a disposable
in-memory host using the same pinned Rust wrapper/WinFsp legacy mode. The test
host permits replacement if Rename reaches user mode and removes by identity;
observations distinguish a kernel refusal from callback forwarding and preserve
old-handle late I/O. It is not S3Drive or native deletion acceptance. Initial
workflow explicitly measures before long workspace tests; no guessed fixture.
Review exact-source artifacts, freeze typed platform/results and add malformed
fixture rejection checks; remove -Measure from required CI before publication.
Keep old14/four-upstream/seven-native/startup gates intact. Require final local
gate and terminal exact-tip hosted checks before guarded private publication.

Next implement the full native lifecycle bridge from measured behavior, including
reply uncertainty, retained I/O, multiple file objects, stable-ID cleanup and
rename/replacement. Do not permanently substitute refused operations or
SetDelete-only support. Keep config6/native SetDelete/DoC unsupported until all
new native acceptance passes. Local workstation NoNewPrivs=1 remains a hard
restriction; never bypass it. No new Wasabi writes, credential/startup changes,
Turso provisioning or target-workstation reboot/PAM/SSH success is authorized or
verified. Research lives in mdBook with dates/citations, not inferred platform
claims. Preserve dirty bytes and explicit conflicts; no fresh-ID fallback.

## Previous expanded reference phase

Published baseline verified live: private clean local/remote main
`0c8f6c71a48b1e4368e4408c74be8e804b035c06`, terminal all-green
[run 39039582208](https://github.com/marshallr12/s3drive/actions/runs/39039582208).
Linux `117416151491`, Windows `117416151413`, book `117416151454`;
173/169 state, seven TCP, 11 SDK cases, seven native cases per OS and unchanged
startup/eight-reference acceptance. Proof `/tmp/s3drive-cancel-publication.json`.

Current reference: source `53f09b1a832cbdebc54d9e0c3d9e318e8f5b8004`,
[run 39041403340](https://github.com/marshallr12/s3drive/actions/runs/39041403340),
source-bound artifact 12104790407, Windows Server 2025 Datacenter,
2026-10-17 14:41:31 UTC. All fourteen probes and four pinned legacy upstream
create/delete_pending/rename/rename_open disk/network cases passed before upload.
The original eight records matched exactly. Artifact downloaded to
`/tmp/s3drive-extra-reference-measured/windows-disposition-reference.json`.
Final frozen fixture now contains all fourteen records, actual OS and original
provenance. No guessed values or eight-plus-six allowance remain.

Key observed results: deferred file/directory rename succeeds; renamed file
cleanup follows its destination while a nonempty renamed directory/child stays.
New create+DoC permits initial sharing, later denies opens when pending.
Multiple DoC opens can clear then reassert pending between cleanups. Duplicate
aliases delay cleanup until the final alias closes. NTFS extended on-close clear
works; it is not supported by the selected WinFsp mode, so not a workaround.
These are NTFS observations, not deferred WinFsp parity or S3Drive deletion.

Local final gate passed, exit 0 (`/tmp/s3drive-extra-reference-gate2.log`),
173 state, seven HTTP, 11 SDK cases, lint/build/book. All three PowerShell files
passed AST syntax checks; positive measured artifact and nine malformed-data
rejections passed. The measurement run also completed terminal all-green.
Final exact-tip hosted native/reference/startup CI is required
before guarded publication. Keep schema16/config6, native callbacks and real
provider/workstation state unchanged. Do not bypass NoNewPrivs.

Next implement atomic deferred create and same-fence owner rename/move, then
persistent per-open/node-wide native lifecycle. Before deferred replacement,
probe both source/target dispositions and guarantee old target cleanup cannot
delete the replacement. Do not substitute permanently refused rename/DoC or
SetDelete-only behavior. Goal remains ACTIVE; native Windows deletion is still
unsupported until the full lifecycle acceptance gate passes.

## Previous cancellation recovery phase

Current baseline verified live: private clean local/remote main
`2d9d4799d09ab1c62c1fbcccb2b320da9333af10`, terminal all-green
[run 39037304416](https://github.com/marshallr12/s3drive/actions/runs/39037304416).
Linux `117409620202`, Windows `117409620337`, book `117409620302`; 167/163 state,
seven TCP, nine managed SDK cases, seven native cases per OS and unchanged
startup/reference acceptance. Proof `/tmp/s3drive-deferred-publication.json`.

This phase admits explicitly fenced cancellation past only an unresolved
same-epoch mode transition. Keep the original unknown outcome and expose a
typed cancellation proof from exact acknowledged history; ignore only proven
closed transition barriers. Do not fabricate an authority result when quota
prevented retaining one. Preserve payload/local observations and dirty data,
prepared-publication barriers and successor epochs. No new schema/config,
provider permission, native callback or workstation startup changes.

Local verification: final repository gate exit 0, 173 state tests, seven
HTTP authority tests, 11 managed-publication cases, 50 S3 and 13 FS tests;
formatting, clippy, builds, local documentation links and mdBook checks passed.
Logs `/tmp/s3drive-cancel-gate3.log`, `/tmp/s3drive-cancel-tests2.log`,
`/tmp/s3drive-cancel-sdk1.log`. The final guard test is included in gate3.
Actual SDK cases kill clients after the unknown transition and cancellation,
then recover exact IDs, publish a successor and preserve binary/empty/Unicode
bytes without duplicate PUTs. Real SQLite-full cancellation rollback and
exhausted authority/client quota with an unretained original result passed.
Typed proofs retain unknown audit rows. Publisher returns CancelledTransition
without RPC for such a closed unknown request. Late exact results preserve
newer observed metadata and ownership. Schema 16/config 6 stay unchanged.

Publication requires terminal all-green CI for exactly the candidate SHA,
including existing seven native cases per OS, eight NTFS reference traces,
pinned upstream WinFsp disk/network create_test and startup gates. Confirm
local/remote main, privacy and clean tree, then guarded fast-forward without
checking out an older tree beneath the mdBook watcher. The exact-commit GitHub
workflow result is authoritative; do not infer acceptance from this source note.

Next implement atomic deferred create and owner-fenced rename/move before the
Windows per-descriptor/node-wide lifecycle. First measure deferred rename and
multiple DoC descriptor reference cases; NTFS observations do not establish
WinFsp parity. Do not enable a SetDelete-only or permanently name-fenced shortcut.
No live Wasabi writes, new provider policy, Turso deployment, workstation startup
or credential changes are authorized by this phase. Never bypass workstation
NoNewPrivs. Goal remains ACTIVE; native Windows deletion remains unsupported.

## Previous deferred-authority phase and verification

Published baseline at phase start: **e091a3da82163891f50491be5580e02a602f79d9**,
terminal all-green [run 39033496306](https://github.com/marshallr12/s3drive/actions/runs/39033496306).
Linux `117398599481`, Windows `117398599626`, book `117398599670`.
Seven native S3Drive cases per OS, eight NTFS reference probes, pinned upstream
WinFsp disk/network create_test and startup gates passed. Reference fixtures
compare every observed scalar. The older reference/publication notes below
are retained history, not the current branch identity.

Schema 16 adds PrepareRemoval (deferred) and same-fence TransitionRemoval.
Deferred directories allow reads/child operations; pending promotion requires
emptiness. Reserved source/target namespace effects stay fenced. Owner-fenced
verified publication works in either mode. Append-only exact-result events
validate current mode and origin; migration seeds old pending reservations
without rewriting old Node/request/result bytes. Deferred/pending quotas
reserve two/one closing slots in both authority and local journal.

Client ownership remains derived from exact acknowledged history. Transitions
retain original frozen source/store binding and update mode/latest receipt;
unknown changes block work and prepared publications block mode changes.
Atomic observations preserve staged generations. No config version change
(version 6), new provider policy, S3 DELETE or real Wasabi mutation.

Full local gate passed (exit 0): 167 state tests, seven authority TCP tests,
nine managed-publication cases, 50 S3 and 13 FS tests, formatting/lint/build/book.
Actual SQLite-full transition, mode/anchor corruption, migration/refusal, local
atomic acknowledgments and quota, eight killed authority boundaries and SDK
killed-owner recovery passed. Implementation commit `43dbdea85ff2b76802d0d8a0072fdfe3e4441257`
passed terminal all-green [run 39036011774](https://github.com/marshallr12/s3drive/actions/runs/39036011774).
Linux `117405802944`, Windows `117405802717`, book `117405802942`.
Remote state counts: 167 Linux / 163 Windows; seven real native S3Drive cases
per OS; Linux no-systemd and Windows non-admin independent-logon startup passed.
Eight NTFS probes matched every recorded scalar; pinned upstream WinFsp legacy
disk/network create_test passed separately. Strict schema-16 startup passed.
Logs `/tmp/s3drive-deferred-first-{linux,windows,book}.log`. The final
documentation commit still requires exact-tip CI before guarded publication.
Logs `/tmp/s3drive-deferred-{tests3,tcp1,sdk1,gate2}.log`.

Next: require terminal final-documentation Linux/Windows/book CI and all
native/reference/startup results, then guarded
fast-forward publication preserving the mdBook watcher tree. Before native
Windows deletion, first implement explicit cancel-as-recovery for unknown
same-fence mode transitions, including exhausted history capacity and late
receipts without rewinding observations. Then add atomic deferred create, owner-fenced rename/move,
per-descriptor/node-wide durable state and recovery; measure multiple DoC and
deferred rename cases. Do not substitute permanent name fencing or SetDelete-only
support for the full handle lifecycle. Workstation NNP restriction and real
MX/desktop reboot/PAM/SSH acceptance remain unresolved; never bypass NNP.

## Previous pending-owner publication baseline

Private main at phase start: **fd526929b9c6555bf85594842e6fb20b7db763fb**,
terminal all-green [run 39030411026](https://github.com/marshallr12/s3drive/actions/runs/39030411026).
Linux `117389554286`, Windows `117389554264`, book `117389554175`.
156 Linux / 152 Windows state tests, six authority TCP, eight managed-publication
cases including child helper, seven real native cases per OS; Linux no-systemd
and Windows Server 2025 non-admin independent-logon startup passed. Schema 15
startup assertion passed strictly. Local/remote main, clean tree and privacy
were verified. Logs: `/tmp/s3drive-pending-owner-{linux,windows,book}.log`;
proof: `/tmp/s3drive-pending-owner-publication.json`.

Schema 14 persists stable client ownership and atomic namespace observations.
Schema 15 adds owner-fenced pending publication and explicit retained confirmed
conflicts without changing old hashes. Only matching local arm/frozen bindings
and logical edit ownership admit pending physical work. SDK paths retain exact
metadata Conflict or physical 409/412; explicit retain_edit_conflict releases
closing admission without settling/discarding bytes. Unknown outcomes still
block closing. Lost reserve/upload/metadata/local results and killed owners,
empty/binary/Unicode data, retained rival/orphan bytes and new guarded successor
are verified in actual SDK/TCP fixtures. No S3 DELETE or real provider mutation.
Normal-stack boxed futures, four owned operation slots and drain-before-close
remain mandatory. Version 6 supplies rename/replacement both OSes and Linux
unlink/rmdir/fsyncdir; native Windows deletion is still refused.

## Accepted Windows reference phase

Primary WinFsp 2.1 create_test explicitly allows a nonempty directory DoC open
and cleanup, preserves its child, then removes the empty directory on a later
DoC cleanup. Current ArmRemoval's emptiness/open-denial contract cannot be the
whole deferred handle lifecycle. The cited sources and proposed state split are
in the new book chapter. Reference acceptance precedes authority implementation
because cross-context cancellation and child creation must not be invented.

Eight explicit ignored Rust NTFS probes live under the audited native FFI test
boundary in s3drive-winfsp/src/native/disposition_reference.rs. JSON observations
cover repeated cancellation, final nonarming cleanup, retained I/O, deferred
opens, nonempty/empty directory cleanup and ordinary nonempty refusal. Three
probes initially collected cross-context/legacy-DoC-cancel/child-create traces.
First native run 39032499844 at 6e5fef7 passed; measured NTFS results are now
recorded in the book and JSON fixture, and the script compares every field.
Cross-context ordinary cancellation clears both handles; legacy DoC cancellation
is reasserted at cleanup; deferred directory child creation succeeds. Do not
assume NTFS/WinFsp cross-context equivalence or skip deferred rename/new-file DoC
and multiple-DoC-descriptor probes. Test-only
serde_json/tempfile dependencies stay Windows-specific; lock only adds the two
existing workspace dependencies to that package, no version upgrades.

scripts/test-windows-disposition-reference.ps1 runs those probes, then already
pinned upstream create_test with legacy-unlink-rename on its own WinFsp disk/net
memfs backends. A dedicated Windows CI step requires both; selected test missing,
nonzero exit or timeout fails. These reference cases are separate from the seven
S3Drive native cases per OS. No production callback, schema or configuration
version changes. No real credentials, startup entry or Wasabi objects changed.

New SVG lifecycle illustration is browser-rendered and its text rectangles fit
within the image. Browser screenshot capture timed out; do not claim pixel
screenshot review. CairoSVG/rsvg-convert are unavailable locally; no system
packages were installed. Full local gate passed, exit 0:
`/tmp/s3drive-disposition-reference-gate2.log` (156 state, eight managed-publication
cases, six authority TCP, 50 S3, 13 FS, build/lint/book). The new Windows-only
probes are not executed here. Terminal exact-tip Linux/Windows/book CI, reference
JSON review and original native/startup regressions are required before publication. The first run passed all existing seven native
cases per OS and startup checks, plus the eight NTFS and upstream WinFsp reference
cases. Final trace-fixture comparison requires a new exact-tip run.

Next: use reference evidence to define deferred name reservation, guarded
promotion with immutable transition proof (not a weakened corruption check),
nonempty cleanup behavior, then persistent per-descriptor/node-wide lifecycle,
operator diagnostics and recovery. Do not replace full Windows support with
SetDelete-only or early-open denial. Missing cleanup and uncertain cancellation
must retain protected work; never silently re-arm/delete with fresh guards.

The mdBook server can recreate removed chapters during old-main checkout. For
publication verify clean tested tree, guarded fast-forward main's ref, then
switch to the identical tree. Preserve unexpected untracked contents.

## Requirements and authorization

Rust workspace/mdBook/private `marshallr12/s3drive`; managed layout with stronger
coordination selected 2026-10-16. Prefer SQLite outside the mount. Linux includes
MX without systemd, sign-in startup and same-user SSH/tmux persistence.
Windows tests run on remote Windows Server 2025 CI, not this Linux workstation.
Actual desktop reboot/sign-out/SSH/PAM and remote service acceptance remain
unverified. NoNewPrivs=1 prevents new local FUSE mounts; never bypass it.
No new live Wasabi mutation is authorized. Earlier fixtures were fully cleaned.
No real write policy, credential, user startup entry or remote/Turso service was
changed. Name: Mira; no voice-context setter has been available.

## Previous accepted authority phase and evidence

Schema 13 adds `namespace_removals`; old serialized nodes and ordinary content
bindings/guards keep their hashes. `ArmRemoval` guards node/parent and reserves
its live name. `CancelRemoval` matches intent+owner without reverting bytes.
`CommitRemoval` matches current node/parent/fence and retains a tombstone.
Every namespace route blocks pending sources/targets/parents. Normal Node RPC
refuses pending nodes; explicit inspection returns the live node/fence. Pending
rows are checked against the immutable arm operation and current identity.

Ordinary or pre-arm publications are blocked while pending. Explicit
`ReservePendingContent` plus matching fence and complete prior payload permits
owner publication. Wrong/expired fences conflict; cancel/commit invalidate
unfinished fenced publication. Delete-pending is temporary and NOT memoized or
acknowledged as a terminal namespace outcome. Exact ordinary content retry after
cancellation remains possible. Logical operation capacity reserves a terminal-result slot for each pending
node; ordinary requests cannot consume closing headroom. Physical disk full
remains an unknown result. No S3 deletion or ownership timeout exists.

Focused state tests pass (12 including child helper): actual full disk,
ignored mutations/receipts/sequence, migration/refusal, corruption, replay,
stale guards, namespace bypass, content fences and forced owner death after
arm/cancel/re-arm/commit. TCP race + dropped body/restart recovery passes.
The actual SDK ordinary-publication test passes on the normal stack: zero PUTs
while pending, cancel + forced client death, exact retry/settlement with one PUT.
Logs: `/tmp/s3drive-removal-state-final.log`,
`/tmp/s3drive-removal-tcp-final.log`, `/tmp/s3drive-removal-sdk2.log`.
Full local gate passed, exit 0: `/tmp/s3drive-removal-gate5.log` (141 Linux
state tests, six authority TCP, five managed publication cases, 50 S3, 13 FS,
plus build/lint/book). First candidate ed2bcee / run 39025899757: Linux all seven native cases
and no-systemd startup passed; the new pending-directory case also passed on
Windows. The Windows job failed its existing non-admin startup regression:
`scripts/test-windows-session.ps1` still required schema 12. Its strict expected
version is now 13; no assertion was removed. Corrected full native CI passed: run 39026528993 above.
Older-schema fixture builders now remove schema 13 before simulating older
versions. Never use CREATE TABLE IF NOT EXISTS to hide damaged migration shape.

## Next acceptance and lifecycle work

Run `bash scripts/gate.sh`, then require terminal exact-tip Linux/Windows/book CI
and all seven native cases per OS before publication. Do not infer completion
while a runner is saving cache. Next: durable per-open and node-wide
lifecycle/recovery before native callbacks.
Windows STATUS_DELETE_PENDING is inspected in resolved windows 0.61.3; WinFsp
directory opens now authorize the node itself, and both adapters map pending
state explicitly. The native mutation case also tests a real directory handle before arm, refusal
while pending and successful open after cancellation, with zero PUTs. No deletion
callback is enabled. The new PNG diagram was generated and visually inspected;
the unlinked-file diagram now labels its accepted version-6 native scope.

Remaining P6m3b3b4b work must persist per-open/node-wide lifecycle state before
RPCs and integrate the owner publication/conflict path into that lifecycle.
Study the
versioned WinFsp lifecycle: deleting cleanup need not use the arming context;
cleanup cannot return errors; delete-on-close bypasses SetDelete. Never delete
inside SetDelete. Resolve uncertain cancellation versus later cleanup flags
without silently re-arming fresh guards after external changes. Native callbacks
wait for P6m3b3b4c real lifecycle/recovery acceptance. Broader remaining work:
full application saves, remote/Turso deployment, multi-host invalidation, managed
Wasabi acceptance with new approval, large files/reclamation and actual startup.

## Current implementation and sources

Nine crates: core, s3, state, fs, fuse, winfsp, cli, authority and managed. Rust 1.99.1, edition
2024, AWS S3 SDK 1.152.0, rusqlite 0.40.2 with bundled SQLite. P6m1 adds pinned axum 0.8.9,
serde_json 1.0.151 and reqwest 0.13.5 (now also the loopback authority client), with resolved sources inspected. OS dependencies remain target-specific.

- `s3drive-core/src/lib.rs`: strict source config and alias/key resolution;
  version 1 is read-only; version 2 requires explicit source policies and local
  state for read-write. Multiple sources use `[[sources]]`.
- `s3drive-s3/src/reader.rs`: bounded LIST/HEAD/revision-pinned GET, explicit
  profile/endpoint/region, sanitized failures, credential-bound immutable handles.
- `s3drive-state`: managed ownership, migration, bounded clean-cache BLOBs,
  non-evictable staged chunks/manifests/local heads and immutable edit mappings.
  Cache/staging/remote commands share one connection, eight-slot queue and
  owner thread; unknown accepted results require inspection.
- `s3drive-fs/src/edits.rs`: immutable patch/truncate/sparse plans and pinned
  base hydration, durable edit-intent keys, replay and read-your-writes snapshots.
- `s3drive-state/src/remote.rs`: schema-6 attempts, one non-cloneable send permit,
  durable uncertainty/rejections and verified-parent ETag guards. No replay of a
  claimed PUT, implicit rebase, unconditional fallback or forgotten remote identity.
- `s3drive-s3/src/transport.rs`, `uploads.rs`: pinned one-attempt PUT and exact
  HEAD/full-GET observation. Public factory requires explicit source/prefix/byte/
  operation/metadata/age policy plus complete plan-bound local evidence and the
  matching credential generation. Four slots across clones, 8 MiB single-PUT
  bound. Expired/narrowed policy still observes claimed attempts.
- `probe.rs`, `probe/transport.rs`: separately reviewed six-key fixture tool.
  Plain DELETE exists only for verified owned fixtures in unversioned scopes.
  `upload-policy-check` validates local documents without credentials/network.
- `scripts/linux-session.py`: detached supervisor, XDG startup and optional SSH
  login-profile hook; no systemd required. `scripts/windows-session.ps1`:
  least-privilege password-backed scheduled task with shared NTFS folder.
  Do not change machine policy or install a real user startup entry implicitly.

## P6m3b1: durable logical edits and publication bridge

See [the contract](docs/src/managed-edits.md). Schema 10 adds private client-local
file bindings, logical edit records and exclusive reservation/publication ID
ownership. They use the same bounded owner and staged chunks as existing work.
Global volume/node identity stays separate from both the frozen local journal
target and each physical S3 blob key. Authority/client DB roles remain separate.

`Publisher::bind_edit_file`, `stage_local`, `stage_edit` and `flush_edit` now
connect this journal to the real SDK/authority publication protocol. Stage local
whole-file generations without network; persist exact publication input before
physical work; require verified physical and metadata receipts before atomically
committing logical edit receipt + settled-file pointer. An older acknowledgment
must leave a newer local head dirty. No silent external revision/credential
adoption; reopening at this client's already-verified base reuses original local
identity. A generic staged pending record is not a logical flush acknowledgment.

Storage tests cover nine killed-owner boundaries, disk full after metadata
success, ignored SQL updates, stale parents/concurrent builders, quota, corrupted
retained rows, exclusive operation IDs and schema-9 migration/role preservation.
The actual SDK/TCP test coalesces a predecessor without upload, then kills a
client after S3+metadata commit while a newer
local head exists; it must recover local settlement without another PUT, publish
the newer bytes, and retain a later losing blob/local conflict after an external
rename. Its three immutable PUTs are confined to owned loopback fixtures.

Local evidence: `/tmp/s3drive-logical-{state-final,recovery-tests-final,sdk-tests,clippy-final}.log`.
Local full gate passed at `/tmp/s3drive-logical-gate-complete.log`: 106 Linux
state tests, 50 S3, 12 FS, four authority TCP and the SDK publication/recovery
tests. Full local gate passed: `/tmp/s3drive-detached-gate-complete.log`, exit 0; 129 Linux state tests, 50 S3, 13 FS, five authority TCP and four managed publication cases (including its child helper). Book links/build/tests pass. The new diagram was generated and visually inspected.
Native FUSE/WinFsp/startup regressions are separate; publication requires terminal
Linux/Windows/book CI for the exact tip. A controlled Linux process-spawn diagnostic reproduced inherited-directory-lock
retention after State drop. The owner now explicitly unlocks after SQLite closes;
strict reopen assertions remain immediate, and live-owner exclusion is unchanged.
The regression performs 100 migration rounds during process spawning. Evidence:
`/tmp/s3drive-logical-migration-spawn-regression.log`. The topic workflow is the
current CI evidence; do not infer completion while cache saving remains active.

Version-3 managed native mounts remain read-only. Native managed local edit
views, patch/truncate/fsync and recovery integration are **P6m3b2**. Include
verified clean external refresh, dirty conflict retention and per-file publication
serialization; address unrelated-volume-guard conflicts before native writers.
This requirement is now fulfilled by the published P6m3b2 checkpoint. P6m3b3 adds portable names and create/rename/replace/unlink; P6m4
supervises/deploys authority/Turso and proves multi-host/sign-in behavior.
No real Wasabi mutation, credential/config change or startup entry is authorized
or performed here. Preserve unsupported callbacks until their native criteria pass.

## P6m3a retained native reader boundary

Explicit version-3 read-only mounts use authority-defined names and 1..16 global
store-to-local-source mappings. Four verified whole-file snapshots bound retained
payload buffers to 32 MiB (8 MiB/file). No persistent managed reader cache or
client-local DB is required. Windows rejects ambiguous exact-UTF-8 projections.
`authority-token` creates one private token without printing/overwriting it;
`authority-serve` runs a private SQLite development service on numeric loopback.
There is no remote deployment/import CLI or real startup registration.
See [managed native reads](docs/src/managed-native.md) for acceptance and examples.

## P6m2 implementation branch: codex/managed-publication

`s3drive-managed::Publisher` now joins the real SDK executor, actual authority
HTTP and a separate client-local logical journal. See
[the protocol](docs/src/managed-publication.md). Schema 9 preserves old state
and adds immutable store descriptors, reservations, node payload references and
hashed local publication records. Existing mounts/configuration stay direct.
This earlier library boundary is now followed by P6m3a explicit managed read-only
config. No cloud service or live Wasabi mutation is enabled.

Authority reservation binds file/revision/store/generated key/size/SHA-256.
A new blob is an independent create-only physical root; no parent ETag guard
is inherited from a different file generation. Persist local input before
staging/network, seal and link exact bytes, reserve at the authority, upload once
and verify, persist the exact publication request, metadata CAS, then local
logical acknowledgment. A physical upload receipt never settles logical intent.
Unknown metadata requests replay only their original operation/arguments/guard;
conflicts do not rebase. Pending bytes/old/losing/unlinked blobs are all retained.

The routing digest excludes machine-local profile/alias names but is NOT proof
of a provider-global principal/tenant. Local source/credential bindings remain
private and pinned. The authority trusts authenticated clients' SDK verification;
it does not have S3 credentials or verify dishonest client attestations.
`Executor::recovery_target` validates runtime binding without granting new write
policy; final new-claim admission remains guarded. Narrowed/expired policy can
still inspect claimed attempts. `Publisher` gets its local owner from Executor.

New published reads authorize at the authority and S3, pin the recorded revision
and verify all bytes. Four retained whole-file snapshots hold at most 32 MiB
and survive rename/unlink; four mutations are separately admitted. The initial
empty metadata prototype needs no S3 data. 8 MiB files, exact-UTF8 names and
loopback-only metadata HTTP remain explicit prototype limits. Native logical
heads/edit overlay, portable name comparison, multi-store routing, replace-save,
large files, distributed deployment/Turso and pin/reference GC remain work.

Local acceptance passed: actual SDK/HTTP/SQLite integration with two client DBs
and three processes, five blobs/five PUTs across two forced kills; competing
writers, stale file revision with fresh volume guard, lost S3/metadata replies,
Unicode, empty data, snapshot bounds, unchanged-ETag corruption and rename/unlink.
Five managed state tests add sealed/unlinked recovery, exact request/receipt and
checksum refusal, pending enumeration beyond settled rows, schema-8 migration,
real SQLITE_FULL during logical acknowledgment after blob verification, and
ignored-update refusal before reporting a durable logical receipt.
Current counts: 91 Linux state, 50 S3, 12 FS, four authority TCP tests, plus the
managed SDK parent/child tests. Windows/native acceptance requires exact-tip CI.
Logs: `/tmp/s3drive-managed-publish-{integration-final,integrity-test,state-all,route-test,clippy-final}.log`
and `/tmp/s3drive-managed-publication-gate.log`; the final full local gate passed
at `/tmp/s3drive-managed-publication-gate-tip.log`. New and updated PNG diagrams
were generated and visually inspected. Wasabi generation-churn/minimum-duration
research is cited in the book; the account's actual pricing plan is unverified.

The direct-directory experiment is archived on `codex/persistent-directories`
at `281bfd1` and is NOT merged. Its initial run 38995625850 failed both native
jobs. A double-slash test helper was fixed, without rerunning or claiming native
acceptance. It is superseded by the user's accepted managed-layout decision.
Do not resume/publish it as the target writable architecture.

Current executor has NoNewPrivs=1. Do not clear it, weaken ownership checks or
launch a helper outside that constraint. Native Linux evidence comes from CI;
Windows uses the remote GitHub Server 2025 VM. No startup registration installed.

## Publication protocol

Before publishing this or any subsequent change, require terminal exact-tip
Linux/Windows/book CI and inspect native results. Then fast-forward/push main
and verify remote SHA/private visibility. Do not infer terminal success while
a job is still saving its cache. Preserve unrelated user files.
`bash scripts/gate.sh` remains the local gate; separate native commands are
`cargo test -p s3drive-cli --test linux_mount --locked -- --ignored --nocapture --test-threads=1`
and `python3 scripts/test-linux-session.py`. Windows runs on the GitHub VM.
No new live cloud mutation is authorized.

## Completed live Wasabi approval: consumed, do not rerun

Source alias `wasabi`, bucket `s3drivetest`, region `us-east-2`, endpoint
`https://s3.us-east-2.wasabisys.com`, profile `wasabi-s3drive`, LIST form dialect.
Access keys stay in the local AWS credential mechanism; never print/config-dump
or commit them. Local config/plan/report are ignored by Git.

User explicitly allowed six exact disposable fixtures under
`s3drive-conformance/d147830ba5257d762e14c3962cde60f9/`, at most fourteen PUTs,
4 KiB/object and under 1 MiB transfer. Plan digest:
`98914e5210466c6c1e074ea36184ea580d20c93749363e18650e5ddf14906de6`.

This ran ONCE at 2026-10-16T23:36:18Z using `8eeda5b`, passed all ten checks,
deleted all six owned keys and found the prefix empty. All fourteen PUT attempts
are consumed. No further uploads or multipart test is authorized by that answer.
New cloud mutations need a new concrete reviewed scope and human authorization.
Complete mode-0600 report: `s3drive.probe-run.local.toml`; plan:
`s3drive.probe-plan.local.toml`. Sanitized hash-bound evidence:
`docs/src/evidence/wasabi-conditional-put-2026-10-16.json`.
The private credential-generation digest is only in the full local report;
the committed summary is not a programmatic capability token.

The full report passed the offline policy checker at `78e649a`, using a removed
private temporary policy, without credential lookup/network; original file
hashes stayed unchanged. Earlier three read fixtures and all other probe fixtures
were already removed. Do not infer remaining cleanup from old plans/logs.

## Next implementation and durable decisions

P6m3b3b1 is published. Finish P6m3b3b2 verification/publication, then integrate real FUSE/WinFsp rename/removal and retained open-unlinked file views, followed by application save workflows as laid out in the namespace plan. Namespace changes must use the same publish/edit serialization as local writes and capture verified source/target states. Windows needs explicit reversible delete intent, cancellation, crash and cleanup recovery: `set_delete` must not delete immediately, while cleanup cannot return an error. Re-read the resolved API and design the durable lifecycle before enabling it. Stable storage/file identities,
create-only unique blob keys and metadata CAS are now separate from the direct
writer's mutable-key ETag lineage; preserve that distinction during integration. Keep uncertain bytes and new physical blob
references distinct; an S3 upload alone does not acknowledge filesystem flush.
Reference/pin-aware reclamation and explicit conflict recovery remain necessary. Do not expose delete/rename
based on fixture cleanup: that is not provider-verified application deletion. Preserve explicit conflict/uncertain states and metadata limits.
Do not keep adding isolated wrappers while deferring integration indefinitely.
Multipart, explicit uncertain-intent recovery/forking and credential rebinding
need independently testable contracts; follow `docs/src/implementation-plan.md`.

`docs/src/multipart-plan.md` cites refreshed AWS/Wasabi limits and the proposed
immutable part/upload-ID/completion protocol. Multipart is not single-PUT
acceptance; a future live MPU run needs a new plan/approval. Unknown upload IDs
cannot prove no publication. Completion must parse embedded errors, guard once
and observe uncertainty; never automatically rebase/replay after conflict.

`docs/src/turso-coordination.md`: optional Turso Cloud behind an authoritative
coordination API, local SQLite retained. Do not use last-push-wins offline sync
for ownership; S3 overwrites don't consult a SQL lease. No Turso dependency,
account, token or service has been created. Managed layout is user-selected
(D031); the Turso backend itself remains proposed (D018). Refreshed official
SQL-over-HTTP docs describe interactive batons and short transaction expiry;
never span an upload with a SQL transaction or assume HTTP 200 proves commit.

The book's `decisions.md` holds durable decisions including WinFsp GPL-3.0
prototype/distribution deferral, exact-name collision failures and ETags as
opaque validators rather than generation fences. Preserve dirty bytes and
return conflicts. Runtime cached/old snapshot reads do not authorize new offline
opens. Physical power-cut and target-workstation reboot tests remain unverified.

## Handoff history

The complete prior handoff is preserved unchanged in
[the repository archive](docs/archive/HANDOFF-before-flush-receipts.md).
The archive retains the original 724-line handoff.
Archive bytes match the original published handoff; no durable decision or old
evidence was discarded.
