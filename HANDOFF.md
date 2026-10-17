# S3Drive handoff

Updated: 2026-10-17. Current phase: **P6m3b3b3 native namespace integration**
on `codex/managed-native-namespace`. Candidate implements version-6
rename/replacement on both OSes and Linux unlink/rmdir with retained handles.
Portable mutation/recovery/cancellation tests pass on the normal stack.
The full local gate passed; terminal exact-tip Linux/Windows/book CI remains required.
Windows deferred deletion is explicitly unsupported. Goal ACTIVE.

## Requirements and authorization

Rust workspace/mdBook/private `marshallr12/s3drive`; managed layout with stronger
coordination selected 2026-10-16. Prefer SQLite outside the mount. Linux includes
MX without systemd; sign-in startup must survive same-user SSH/tmux disconnect.
Windows testing is remote Windows Server 2025 CI, never this Linux workstation.
Actual desktop reboot/sign-out/SSH/PAM and remote managed service acceptance
remain unverified. NoNewPrivs=1 prevents a new local FUSE mount: never bypass it.

No new live Wasabi mutation is authorized. Earlier approved fixtures were fully
cleaned. `s3drive.local.toml` stays read-only. Do not expose credentials/configs
or change actual startup registration, provider policy or service deployment.
Name: Mira; no voice-context setter is available, so no handoff was claimed.

## Verified publication baseline

Private main at phase start: **5d8fe4f63d9aae5aaa6e0856be538bb93333ae2b**.
Terminal all-green [run 39018485778](https://github.com/marshallr12/s3drive/actions/runs/39018485778):
Linux `117355366505`, Windows `117355366503`, book `117355366299`.
129 Linux / 125 Windows state tests, 50 S3, 13 FS, five authority TCP and four
managed publication cases; six native cases per OS plus Linux no-systemd and
Windows non-admin cross-logon regressions. Logs:
`/tmp/s3drive-detached-final-{linux,windows,book}-ci.log`.
Local/remote main, private visibility and clean state verified at publication.

## Current candidate

See `docs/src/managed-native-mutations.md` and `examples/managed-mutations.toml`.
No schema/dependency change. Version 3/4/5 keep their previous capabilities.
Version 6 requires portable names plus entry/detached CAS features. Source dirty
bytes publish before a move; prepared target publications reconcile first;
unprepared displaced/unlinked dirty bytes stay retained. Namespace receipt
acknowledgment updates affected observations atomically. Linux old handles can
fsync to a tombstone without restoring its name or changing a reused path.

Managed operations use four owned permits, publication/edit lock order, bounded
owned namespace/publication tasks and explicit stop/drain before SQLite closes.
Publisher flush futures are type-erased boxes to bound nested SDK poll frames.
Do not accept RUST_MIN_STACK increases or weakened assertions as a fix.
Portable test passes at `/tmp/s3drive-native-namespace-faults2.log`: three exact
PUTs, killed clients after committed rename/removal, and cancelled caller with
shutdown waiting on a held reply. New native case per OS tests actual save/
replace, Unicode directory moves, second bucket, cycles, remount and lost rename
reply recovery; Linux also removal/retained handles. Native results pending.
Full local gate passed, exit 0: `/tmp/s3drive-native-namespace-gate.log`.

Resolved fuser 0.18 rename flags/namespace callbacks, WinFsp 0.13.1 rename/delete/
cleanup contracts, windows 0.61.3 access constants and Tokio owned permits/tasks
were inspected. Windows SetDelete is reversible and cleanup cannot report an
error: keep deletion unsupported until its durable protocol and native recovery
acceptance pass. Never delete from SetDelete to fake DeleteFile success.

Run `bash scripts/gate.sh`, then require terminal exact-tip Linux/Windows/book
jobs including seven native cases per OS and existing startup regressions.
Do not publish main early while a job is still saving cache. Preserve native
serialization and all assertions/deadlines. Remaining goal work includes Windows
delete, full application save acceptance, remote/Turso authority deployment,
multi-host invalidation, live managed-provider acceptance and actual workstation
startup tests. No immutable blob/local dirty generation reclamation exists.

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
