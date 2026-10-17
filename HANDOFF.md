# S3Drive handoff

Updated: 2026-10-16. Current phase: **P5c1a native existing-file writes**; native acceptance passed.
Goal active. Linux/Windows read-only drives and startup foundations work;
library writes/flush are progressing toward safe writable native filesystems.
Do not call the broader editable-drive goal complete before its documented
safe-write contract and real OS acceptance pass.

## Requirements and current boundary

- Rust implementation in `~/src/s3drive`, mdBook findings/plans/images, private
  GitHub repository `marshallr12/s3drive`; these artifacts already exist.
- One or more S3-compatible buckets/prefixes exposed through source aliases.
  Wasabi is the first live provider; other endpoints need separate conformance.
- Linux FUSE and Windows WinFsp adapters have stable read-only acceptance.
  The current branch adds opt-in existing-file editing with native writable
  acceptance. Full desktop editing remains unfinished.
  New online opens authorize through S3; pinned old handles preserve revisions.
  No ordinary CLI upload command, file creation, delete/rename,
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
`4a0f04cb071cefa6a5b894308863e03ef74226f0` (remote-flush-receipts).
It passed terminal all-green [run 38986527921](https://github.com/marshallr12/s3drive/actions/runs/38986527921):
Linux 117259186073, Windows 117259186221 and book 117259186256.
Remote main SHA and PRIVATE visibility were verified after publication.

That exact baseline passed 49 S3 tests on each OS, 78 Linux / 76 Windows state
and 12 FS tests, offline CLI guards, 341 FUSE and 749 WinFsp requests, caching,
no-systemd startup and Windows cross-logon acceptance, with schema 7. Logs:
`/tmp/s3drive-flush-{linux,windows,book}-ci.log`.

Windows testing runs remotely on GitHub's **Windows Server 2025 VM**, not this
Linux workstation. Native folder/drive/security/UTF-16 tests and distinct
same-account logon AuthenticationId checks passed. Actual Windows 10/11 desktop,
full sign-out, workstation reboot and real incoming SSH authentication/PAM
remain target-host acceptance. Linux local PID 1 is `init`, not systemd.

## Current implementation and sources

Seven crates: core, s3, state, fs, fuse, winfsp and cli. Rust 1.99.1, edition
2024, AWS S3 SDK 1.152.0, rusqlite 0.40.2 with bundled SQLite. No version
upgrade in the current phase. OS dependencies remain target-specific.

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

## Current branch: native-writable-files

P5b2d flush receipts and schema 7 are published. Current P5c1a connects existing
file writes/truncation/append and remote fsync to both OS adapters. See
[the native-write contract](docs/src/native-writes.md) for configuration and
limits. One SQLite owner serves edits, cache and uploads. Four mutation slots
and a per-file lock bound execution. Immutable snapshots survive failures;
new opens authorize online. Reconciliation only observes claimed attempts;
no silent replay, unconditional overwrite or automatic clean-base rebase.
File creation, delete/rename, multipart, timestamp/ACL editing and payload GC
remain unsupported. Default staging limits remain 1 GiB / 4,096 generations.
The existing real user configuration and credential files remain unchanged.

Local gate passed, including SDK/SQLite shared-engine write/restart/conflict
integration: `/tmp/s3drive-native-write-gate.log`. Local native tests could not
mount: this executor has NoNewPrivs=1, preventing setuid fusermount3. A private
user/mount namespace mounts RO FUSE but its unmapped ancestor ownership is
refused by the existing private-state checks. Do not weaken ownership checks,
clear NoNewPrivs or use an external privileged launcher to bypass this.
Logs: `/tmp/s3drive-native-write-linux.log` and
`/tmp/s3drive-native-write-linux-namespace.log`. Neither is native RW acceptance.
Implementation `cf5e456` passed the expanded native suites in
[run 38990445390](https://github.com/marshallr12/s3drive/actions/runs/38990445390):
Linux job 117271446300, Windows job 117271446336, book 117271446202.
Both OSes exercised lost replies, uncertain restart without another PUT and
412 conflict preservation, alongside read-only/cache/session regressions.
Linux terminal success and 341 RO requests are recorded in
`/tmp/s3drive-native-write-linux-ci-expanded.log`. At this handoff refresh,
Windows native steps passed and its job was finishing cache upload; verify the
terminal result, then record the exact-tip final run after this refresh.
Initial `5fc39c4` also passed all three jobs in run 38990194704, including
749 read-only WinFsp requests and native writable restart. Logs:
`/tmp/s3drive-native-write-{linux,windows}-ci-initial.log`.

The final shared error mapping turns journal-rejected flushes into Conflict
(Linux ESTALE / Windows STATUS_FILE_INVALID). Portable engine assertions cover
that distinction. `/tmp/s3drive-native-write-gate-final.log` is a successful
full local gate; documentation and diagram refresh follow it. Both generated
architecture images were visually inspected. Normal desktop/login-session,
physical power-cut and actual workstation reboot remain separate acceptance.

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

Next implement guarded native file creation in existing directories (P5c1b),
then persistent directories and namespace workflows (P5c1c), reference/pin-aware
staging reclamation and explicit conflict recovery. Do not expose delete/rename
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
account, token or service has been created. Direct objects and managed immutable
generations have different compatibility tradeoffs; D018 remains proposed.

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
