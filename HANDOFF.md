# S3Drive handoff

Updated: 2026-10-16. Current phase: **P6m1 managed namespace authority**; implemented, local tests passed.
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
  Managed mounts and full desktop editing remain unfinished.
  New online opens authorize through S3; pinned old handles preserve revisions.
  No ordinary CLI upload command, directory creation, delete/rename,
  multipart, payload reclamation, new offline opens or desktop packaging yet.
- Startup at sign-in after reboot; same-user incoming SSH sessions and surviving
  tmux/Codex/Claude processes must share the mount. The user's “SSL sessions”
  means SSH. MX Linux without systemd is required and tested locally.
- Prefer SQLite: state/cache/staged payloads live outside the mounted tree,
  on private local disk under one process lock and bounded owner thread.
- Existing `s3drive.local.toml` remains read-only. No real application write
  policy, startup registration, cloud coordination service or credential changed.
- Name: Mira. No voice-context handoff capability has been available; don't
  claim one occurred.

## Authoritative publication baseline

Published private main at phase start:
`640a37ecf99d2d1e1c1874aeba3013a1e99d117d` (guarded file creation).
Terminal all-green [run 38993215272](https://github.com/marshallr12/s3drive/actions/runs/38993215272):
Linux 117279903466, Windows 117279903627 and book 117279903624.
Remote main SHA and PRIVATE visibility were verified after publication.

Baseline: 49 S3 tests per OS, 79 Linux / 77 Windows state and 12 FS tests;
three native cases per OS (RO/cache/session, existing writes, creation), with
341 RO FUSE / 749 RO WinFsp requests. No-systemd and Windows cross-logon gates,
Unicode creation, 1,024-byte SDK key boundary, lost replies, restart without
replay and rival preservation passed. Logs: `/tmp/s3drive-create-{linux,windows,book}-ci.log`.

Windows testing runs remotely on GitHub's **Windows Server 2025 VM**, not this
Linux workstation. Native folder/drive/security/UTF-16 tests and distinct
same-account logon AuthenticationId checks passed. Actual Windows 10/11 desktop,
full sign-out, workstation reboot and real incoming SSH authentication/PAM
remain target-host acceptance. Linux local PID 1 is `init`, not systemd.

## Current implementation and sources

Eight crates: core, s3, state, fs, fuse, winfsp, cli and authority. Rust 1.99.1, edition
2024, AWS S3 SDK 1.152.0, rusqlite 0.40.2 with bundled SQLite. P6m1 adds pinned axum 0.8.9,
serde_json 1.0.151 and test-only reqwest 0.13.5, with resolved sources inspected. OS dependencies remain target-specific.

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

## P6m1 implementation branch: codex/managed-authority

P6m1 implements persistent namespace transactions in `s3drive-state::namespace`
and a bounded, authenticated loopback HTTP service in `s3drive-authority`.
See [the managed contract](docs/src/managed-layout.md). Schema 8 preserves client
state and adds isolated namespace volume/nodes/operation tables. Authority IDs
are independent of local AWS profiles. Initialization refuses populated client
state; source registration/staging refuse an initialized authority database.

Implemented: directory/empty-file metadata create, subtree move without overwrite,
empty removal/tombstones, sequence CAS and durable exact-request operation-result
replay. One owner thread/8-command queue; loopback only, 4 KiB JSON, explicit
256-bit token, 8 HTTP operation slots. No deployment CLI or token persisted.
`exact-utf8-v1` is a prototype byte-exact policy, NOT Windows case equivalence.
No managed payload upload/reference, native mount or real remote deployment yet.

Four independent real-TCP tests passed locally, including concurrent create,
opposing directory moves, subtree identity, Unicode, lost HTTP reply/restart,
authentication and strict decoding. Seven namespace state tests (one child
helper) passed, including process death, real SQLite full, injected rollback,
migration, role separation and retained bounds. Workspace lint/tests passed:
86 Linux state, 49 S3 and 12 FS tests, plus the four new TCP tests. Existing
migration fixtures now remove schema-8 namespace tables when emulating schemas
2–6; their original preservation/conflicting-schema assertions remain intact.
The first gate exposed those outdated fixtures; the corrected gate log is
`/tmp/s3drive-managed-gate-final.log`. Also see `/tmp/s3drive-managed-tests.log`
and `/tmp/s3drive-managed-state-tests.log`. Exact-tip CI is required below;
retrieve it with `gh run list --branch codex/managed-authority --limit 3`.
Initial implementation `ba7be22` passed Linux/book and Windows workspace tests
(84 state plus four authority TCP tests), and both writable native cases, in
run 38997028876. The Windows session fixture still asserted `schema=7` and
failed after the schema-8 upgrade. Its assertion is corrected to schema 8;
full native acceptance must pass on the corrected tip before publication.
Initial logs: `/tmp/s3drive-managed-linux-ci.log` and
`/tmp/s3drive-managed-windows-ci-initial.log`. The follow-up local gate log is
`/tmp/s3drive-managed-gate-schema8.log`.
The new managed architecture PNG was generated
and visually inspected. No real user configuration or cloud state changed.

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
`cargo test -p s3drive-cli --test linux_mount --locked -- --ignored --nocapture`
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

After P6m1 acceptance/publication, implement P6m2 immutable payload publication
and P6m3 shared-engine/native integration. Stable storage/file identities,
create-only unique blob keys and metadata CAS must be separate from the direct
writer's mutable-key ETag lineage. Keep uncertain bytes and new physical blob
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
