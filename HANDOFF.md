# S3Drive handoff

Updated: 2026-10-17. Current phase: **P6m3b3b1 guarded authority entry transactions** on `codex/managed-entry-transactions`. Metadata implementation and the full local gate pass; terminal Linux/Windows/book CI, existing native regressions and publication remain required. Native rename/removal are still unsupported.
Goal active. Linux/Windows read-only drives and startup foundations work;
library writes/flush are progressing toward safe writable native filesystems.
Do not call the broader editable-drive goal complete before its documented
safe-write contract and real OS acceptance pass.

## Requirements and current boundary

- Rust implementation in `~/src/s3drive`, mdBook findings/plans/images, private
  GitHub repository `marshallr12/s3drive`; these artifacts already exist.
- User selected **managed layout with stronger coordination**, 2026-10-16.
  Target: shared authoritative namespace plus immutable S3 generations; ordinary
  S3 tools see internal keys. No implicit conversion of existing configurations.
- Existing direct layout exposes buckets/prefixes through source aliases.
  Wasabi is the first live provider; other endpoints need separate conformance.
- Linux FUSE and Windows WinFsp adapters have stable read-only acceptance.
  Existing opt-in editing and guarded file creation passed native acceptance.
  Version-3 managed read-only integration is accepted. Version-4 managed existing-file callbacks are natively accepted. Version-5 creation/directories passed their native gates; rename/removal/save-replace remains unfinished.
  New online opens authorize through S3; pinned old handles preserve revisions.
  No ordinary CLI upload command, delete/rename,
  multipart, payload reclamation, new offline opens or desktop packaging yet.
- Startup at sign-in after reboot; same-user incoming SSH sessions and surviving
  tmux/Codex/Claude processes must share the mount. The user's “SSL sessions”
  means SSH. MX Linux without systemd is required. Launcher logic was tested locally;
  native session regression runs in CI. Actual MX mount/reboot acceptance remains
  unverified under this executor's NoNewPrivs restriction.
- Prefer SQLite: state/cache/staged payloads live outside the mounted tree,
  on private local disk under one process lock and bounded owner thread.
- Existing `s3drive.local.toml` remains read-only. No real application write
  policy, startup registration, cloud coordination service or credential changed.
- Name: Mira. No voice-context handoff capability has been available; don't
  claim one occurred.

## Authoritative publication baseline

Published private main at phase start:
`ec0f4902c2572d37300dd4f4c7b262bce2faa40b` (managed creation/directories).
Terminal all-green [run 39014033363](https://github.com/marshallr12/s3drive/actions/runs/39014033363):
Linux `117342308172`, Windows `117342308295`, book `117342308235`.
Schema 12, 116 Linux / 112 Windows state tests, 13 FS and six native cases per
OS, with no-systemd startup and Windows non-admin cross-logon acceptance.
Logs: `/tmp/s3drive-namespace-final-{linux,windows,book}-ci.log`.
Local/remote main/private visibility and a clean worktree were verified.

Current branch adds `NodeGuard`, portable-volume `MoveEntry`/`RemoveEntry`,
`namespace-entry-cas-v1` and a namespace artifact with the retained displaced
node. Exact source/target/both-parent guards, collision-key admission, type,
cycle, emptiness and child-count bounds commit with exact dedup outcomes.
Tombstones preserve payloads and placements; no new schema or S3 operation.
Ignored SQL result insertions now roll back the entire effect. Requests are
boxed within the existing bounded authority queue after the larger guard
variant exceeded the enum size lint.

Seven new state tests and an actual TCP race/restart/replay test cover this
contract, including real SQLITE_FULL rollback/retry and ignored writes.
Focused evidence: `/tmp/s3drive-entry-guard-final-tests.log`,
`/tmp/s3drive-entry-state-authority-tests.log` and
`/tmp/s3drive-entry-clippy-final.log`.
Full local gate: `/tmp/s3drive-entry-gate.log`, exit 0; 123 Linux state tests, five authority TCP tests, 50 S3 and 13 FS tests. Documentation links/build/tests also pass.
See `docs/src/managed-namespace-plan.md` for the contract and remaining bridge.
This metadata phase does not enable native rename/removal or acknowledge
unpublished dirty source bytes. Native creation and prior editing behavior stay
at their accepted boundaries.

Use `bash scripts/gate.sh` locally, then require terminal exact-tip Linux,
Windows and book jobs before main publication. Native cases remain serialized
(`--test-threads=1`); no assertions or deadlines are weakened. No new live Wasabi
mutation, real startup registration or credential/config change is authorized.

Windows testing runs remotely on GitHub's **Windows Server 2025 VM**, not this
Linux workstation. Native folder/drive/security/UTF-16 tests and distinct
same-account logon AuthenticationId checks passed. Actual Windows 10/11 desktop,
full sign-out, workstation reboot and real incoming SSH authentication/PAM
remain target-host acceptance. Linux local PID 1 is `init`, not systemd.

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
tests. The new diagram was generated and visually inspected.
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

P6m3b3a is published. Finish P6m3b3b1 verification/publication, then extend the private namespace intent/acknowledgment bridge, detached open-unlinked publication and real FUSE/WinFsp rename/removal, followed by application save workflows as laid out in the namespace plan. Stable storage/file identities,
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
