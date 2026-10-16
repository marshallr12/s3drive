# S3Drive handoff

Updated: 2026-10-16. P3 read-only mounts have passed native acceptance on Linux
and Windows. Scoped live Wasabi reads also passed. P3.5a adds Linux startup
without systemd; native session tests passed locally. P3.5b adds Windows
startup with native cross-logon acceptance. P4a adds credential-bound handles
and SQLite schema 2 migration/registry. Goal active; actual workstation
reboot/SSH login checks, cache, recovery, safe writes and packaging remain.

## Goal and scope

Enable S3-compatible storage to appear as local drives/filesystems on Linux and
Windows using Rust. Research, seven-crate workspace, mdBook and private GitHub
repository `marshallr12/s3drive` are complete. The first usable read-only
milestone is verified. Do not call the broader editable-drive goal complete
until the documented safe-write contract and its OS acceptance gates pass.

## Current implementation

- Shared bounded namespace engine; one directory per configured source alias.
- Linux FUSE and Windows WinFsp foreground mounts use the same S3 reader.
  Owner-only/read-only permissions, revision-pinned file handles, bounded
  directory snapshots, explicit collisions and cancellation paths are enforced.
- S3 exposes LIST/HEAD/range GET only. It validates ranges, lengths and revisions,
  bounds deadlines and uses explicit endpoint/region/profile precedence.
- Windows uses native UTF-16 case comparison and owner SID security descriptors.
  Invalid/case-colliding names fail instead of being silently renamed/hidden.
  The GPL-3.0 WinFsp wrapper is used for private prototyping; future application
  distribution terms and packaging remain undecided.
- `config-check`, `resolve`, `list`, `stat`, `read`, `init-state`, `status` and
  both native `mount` paths work. Windows requires WinFsp 2.1 Developer files
  for a source build and the installed runtime/driver for mounting.
- Read handles now pin the credentials resolved for their successful HEAD;
  later GETs cannot silently switch credentials. New opens resolve again and
  expose an opaque credential/configuration digest only after successful HEAD.
  The digest is not a global account ID or an authorization token. Config,
  access key, secret or token changes cause a new binding. Debug is redacted.
- SQLite schema 2 adds a source-binding registry and preserves schema 1 rows
  separately as `legacy_cached_objects_v1`; migration failures roll back.
  Registration is idempotent and rejects conflicting fingerprints. Mounts do
  not yet use the registry; the first cache must authorize each new open online.
- No uploads, mutations, disk cache, offline support or desktop packaging yet.

## Current user requirements and startup work

The drive should return at sign-in after reboot and remain available to later
same-account SSH sessions, including tmux/Codex/Claude work after disconnect.
Interpret the user's "remote SSL sessions" as SSH. MX Linux without systemd
is explicitly required. Do not prescribe systemd as a prerequisite or an
interactive-only Windows task as proof of cross-session access.

`scripts/linux-session.py` installs a private detached supervisor, XDG desktop
entry and optional explicit login-profile hook around the Rust mount. It uses
standard-library Python 3.9+, a new OS session, redirected private logs and a
process lock. It bounds retries, exposes start/status/stop/remove, preserves
user profile content/mode and refuses removal of edited managed entries.
No real user startup registration was installed and no reboot/logout performed.

`python3 scripts/test-linux-session.py` passed locally on Linux
6.12.29-amd64, PID 1 `init`, no `/run/systemd/system`, Python 3.11.2,
tmux 3.3a and gio 2.74.6. Log: `/tmp/s3drive-session-native.log`.
It tests real FUSE + desktop entry parsing, racing launches, restart from a
tmux login hook without inherited XDG variables, terminal destruction and
fresh-process reads, edited-entry refusal and cleanup. Only a synthetic source
root is read; no cloud account is contacted. No mount remains afterward.
CI now requires this gate in its Linux job. Source checkpoint
`664f51cdbf0e44cf2410da4cd3bef606e03fe5fc` passed every job in
[run 38933462454](https://github.com/marshallr12/s3drive/actions/runs/38933462454),
including Linux startup, Linux/Windows native mounts and book checks.
Actual reboot, full desktop logout
policy and incoming SSH authentication/PAM need separate acceptance.

Windows foreground mounts accept a new absolute NTFS directory path, with
existing parents and no junctions/symlinks. Tests cover Unicode reads, occupied
targets, graceful teardown and forced process termination. The persistent
`scripts/windows-session.ps1` registers an owner-account, least-privilege,
password-backed scheduled task. Its supervisor uses stdin EOF for graceful
unmount. No Windows password is stored in application files/argv. The account
requires batch-logon rights; the launcher never changes security policy.

Native startup acceptance passed at `029765ec79c99a029037356ba61278fc7de6ab7b`
in [run 38945436975](https://github.com/marshallr12/s3drive/actions/runs/38945436975).
Tests run remotely on GitHub's Windows Server 2025 VM (`10.0.26100`), not on
this Linux workstation. The test creates an ordinary account and verifies two
distinct TOKEN_STATISTICS.AuthenticationId values reading the same daemon's
exact S3/Unicode bytes after its installer exits. Duplicate starts preserve
its PID; another account is denied access. Remove deletes the task/mount and
preserves config/credentials. The prior drive/folder Unicode corpus also runs.
Actual Windows 10/11 desktop, reboot, full sign-out and SSH authentication/trigger
behavior remain target-host acceptance work. No real user Windows credential or
startup registration changed. Passwordless accounts or restrictive batch-logon
policy require a service alternative, which is not implemented.

The elevated fixture provisions/removes only its new account's batch right.
Short-lived clients use CreateProfile's returned path without loading HKCU.
Windows can retain a locked registry profile after all fixture processes exit.
CI explicitly permits only that sharing violation to defer to GitHub VM
destruction, after successful functional checks, dummy credential removal and
account deletion; all other cleanup failures remain fatal. Normal local tests
still require profile deletion. This is fixture disposal, not application
uninstall behavior. See `docs/src/development.md` and `startup-and-sessions.md`.
`whoami /logonid` can be inherited by CreateProcessWithLogonW clients, so use
token AuthenticationId for session uniqueness. Earlier failing runs and logs
`/tmp/s3drive-windows-session-job10.log` through `job12.log` retain the diagnosis;
do not report them as complete acceptance.

## Turso research and architecture images

`docs/src/turso-coordination.md` records current/released versus announced
features as of 2026-10-16, with primary sources. Engine v0.8.3 was the latest
release; Cloud concurrent writes were announced as early preview. New Sync's
last-push-wins reconciliation is unsuitable for exclusive ownership. Recommend
evaluating Turso Cloud behind an authoritative coordination API while keeping
local SQLite; no Turso dependency, database, token or subscription was added.
Conditional remote transactions, common guard records, idempotency, fencing and
SQL/S3 reconciliation still need a scoped integration proof. An ordinary S3
overwrite does not consult a Turso lease; direct mode still requires verified
S3 preconditions. Managed immutable generations would change object layout.

Three original PNG images under `docs/src/images/` explain the system,
session lifecycle and proposed coordination. The optional Pillow generator is
`scripts/draw-architecture.py`; generated images were visually inspected.

## User configuration and live Wasabi evidence

User-selected Wasabi bucket `s3drivetest`, region `us-east-2` (N. Virginia),
endpoint `https://s3.us-east-2.wasabisys.com`, alias `wasabi`. Ignored local
`s3drive.local.toml` passes validation. The user corrected credential field
names to `aws_access_key_id` / `aws_secret_access_key` in local profile
`wasabi-s3drive`. Key values were neither displayed nor modified.

The user explicitly approved creating/deleting three fixtures under
`s3drive-conformance/20261016-lxrfkg8p/`. SDK pagination, HEAD, exact full/range/
EOF reads and Linux FUSE sequential/concurrent reads passed. Fixtures covered
524288 binary bytes, an empty file, and `nested/a%2Fb + café.txt`.
All three were removed, the isolated prefix was confirmed empty, and the
mount exited cleanly. Bucket versioning is disabled. Stale validators,
versioned reads, Windows against Wasabi and all write/recovery semantics remain
unverified. Do not infer broader mutation authorization from this limited test.

Live testing found Wasabi listing spaces encoded as `+`. Explicit
`list_url_encoding = "form"` was added to the ignored local config and Wasabi
example. Default percent-only decoding remains unchanged. Dialect participates
in configuration fingerprint v2; wire tests cover exact decoding/containment.
See `docs/src/wasabi.md` and `docs/src/evidence/wasabi-2026-10-16.json` for hashes,
source scope, test manifest and exclusions. Private ephemeral logs/fixture
files are under `/tmp/s3drive-wasabi-fixtures-lxrfkg8p/`.

## Native acceptance

The shared HTTP fixture uses ephemeral ports and isolated dummy credentials.
It checks signing-header shape, not an independent SigV4 implementation.
Both native gates exercise two buckets, configured prefixes, 310-entry
pagination, sequential/random/concurrent reads, EOF, stale handles/fresh reopen,
rejected writes, and teardown during an in-flight GET. Windows additionally
checks case/device-name rejection, sharing, owner ACLs, PowerShell, Explorer's
Shell.Application namespace and the upstream read-only `getvolinfo_test`.

At the user's request, both gates include a shared Unicode corpus in
`crates/s3drive-cli/tests/support/unicode.rs`: 11 filenames and a Unicode
folder, covering Japanese, Chinese, Cyrillic, Arabic, Devanagari, supplementary
emoji, a zero-width joiner sequence and literal percent/space/plus characters.
NFC/NFD accent spellings remain separate keys with distinct bytes. The gates
verify exact listings, directory traversal and correct contents for every key.

Local Linux gate passed with 145 GET/HEAD requests/two mounts on Linux
6.12.29-amd64, fusermount3 3.14.0 and Rust 1.99.1. Logs:
`/tmp/s3drive-unicode-linux.log`, `/tmp/s3drive-unicode-gate.log`.
No local mount remains. Commands:

```bash
bash scripts/gate.sh
cargo test -p s3drive-cli --test linux_mount --locked -- --ignored --nocapture
```

Windows native acceptance passed at commit
`25235a72e83d75389f0f95d53ff998b4bab6c80c`, CI run
[38928960490](https://github.com/marshallr12/s3drive/actions/runs/38928960490).
The Windows command is:

```powershell
cargo test -p s3drive-cli --test windows_mount --locked -- --ignored --nocapture
```

CI installs digest-pinned WinFsp 2.1.25156/runtime/SDK and upstream test binary.
The native test requires `S3DRIVE_WINFSP_TEST_EXE`; the installed WinFsp `bin`
directory must be on PATH for the test executable's direct DLL import. CI
checks its startup before mounting. PowerShell isolates its module path from
PowerShell 7. Both normal stdin-EOF shutdown and cancellation remove the drive.
Actual console Ctrl+C/Break events and a visually inspected Explorer window
are not covered; Shell COM access is. Read `docs/src/windows-mount.md` for limits.

Earlier failed CI runs: 38924822475 (ambiguous WinFsp guard type), 38925797029
(PowerShell module path), 38927733040 (upstream test DLL loading). Each defect
was fixed and the complete native gate rerun; none is acceptance evidence.

## Publication and next work

Linux startup/research baseline `3eeb40bf94f990e2917c49b4107f74a4679ef61a`
passed final-source CI run 38933907710. Windows startup source checkpoint
`029765ec79c99a029037356ba61278fc7de6ab7b` has native acceptance in run
38945436975; subsequent updates document the results and refresh the diagrams.
The publication procedure is a terminal successful CI run for the final branch
tip, followed by fast-forwarding/pushing private `origin/main`. Check live Git
and Actions state when resuming; preserve ignored user configuration. The local
gate log is `/tmp/s3drive-windows-session-final-gate.log`.

P4a uses branch `cache-source-binding`: credential-bound reads and SQLite migration.
Local verification passed: focused tests (18 S3 / 8 state), full repository
gate, real Linux FUSE (145 S3 requests and the Unicode corpus), and startup on
this non-systemd workstation. Logs: `/tmp/s3drive-cache-binding-tests.log`,
`/tmp/s3drive-cache-binding-gate.log`, `/tmp/s3drive-cache-binding-linux.log`,
`/tmp/s3drive-cache-binding-session.log`. CI runs the new binding/migration tests
on both OSes and the existing real Windows mount/startup gate; publication
requires all jobs successful at the exact branch tip. Retrieve the result with
`gh run list --branch cache-source-binding` and match its head SHA to Git.
Resolved AWS SDK 1.152.0's Config::credentials_provider getter is deprecated
and always returns None. S3Reader retains the actual provider separately;
open resolves it within the overall deadline and replaces the pinned client's
identity resolver. Do not recover the provider through that getter.

Target-workstation reboot/sign-in, full sign-out policy and SSH authentication
acceptance remain pending. Next implementation is P4b: private cache-directory
permissions/ACLs, exclusive process ownership, revision-bound clean data cache,
bounded SQLite worker, eviction/pinning and restart/disk-full/offline-miss tests.
Use the handle binding plus exact key/revision/size; no binding alone authorizes
cached access. Keep cache/state outside mounted trees. Writes require separate staged
generations and a durable journal (P5a), provider conditional commits/recovery
(P5b), then desktop mutations (P5c). Do not expose unconditional overwrite as a
shortcut. Broader live provider probes require scoped disposable fixtures.

Read the mdBook implementation plan, provider/OS contracts and decisions.
No subagents were dispatched. The overall goal is not complete.
