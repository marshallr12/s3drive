# S3Drive handoff

Updated: 2026-10-16. P3 read-only mounts have passed native acceptance on Linux
and Windows. Scoped live Wasabi reads also passed. P3.5a adds Linux startup
without systemd; native session tests passed locally. P3.5b adds Windows
startup with native cross-logon acceptance. P4a adds credential-bound handles
and SQLite schema 2 migration/registry. P4b1 adds private managed state directories
and process ownership. P4b2a implements a bounded clean-cache storage worker
with SQLite BLOBs. P4b2b connects it to optional mounted caching, with native
acceptance required for each published source. P5a1 adds the local staged-
generation/journal library. P5a2a adds one shared async owner for cache/journal
commands and immutable local snapshot handles. P5a2b adds local edit plans and
pinned-base hydration. P5b1 adds an explicit disposable-key transport/probe.
P5b2a adds local durable remote-attempt records and single-use send claims.
P5b2b connects a quarantined SDK executor with local wire/recovery evidence;
live Wasabi write evidence awaits the new scoped authorization. Goal active;
application remote commits,
writable mounts, workstation reboot/SSH checks and packaging remain.

## Goal and scope

Enable S3-compatible storage to appear as local drives/filesystems on Linux and
Windows using Rust. Research, seven-crate workspace, mdBook and private GitHub
repository `marshallr12/s3drive` are complete. The first usable read-only
milestone is verified. Do not call the broader editable-drive goal complete
until the documented safe-write contract and its OS acceptance gates pass.

## Current phase: P5b2b journal-driven SDK executor

Branch `journaled-s3-executor` starts from published private main
`803ee3785cd0dafbe51c3862edacdd01891a8a1e`. P5b2a passed terminal all-green
[run 38979093279](https://github.com/marshallr12/s3drive/actions/runs/38979093279)
and was verified on remote main. Its native evidence includes 70 Linux / 68
Windows state tests, 341 FUSE and 749 WinFsp read-only requests, cache/session
checks and book. Logs: `/tmp/s3drive-remote-intent-{linux,windows,book}-ci.log`.
Windows runs on GitHub's Windows Server 2025 VM, not this Linux workstation.
The user was explicitly told this; actual Windows 10/11 desktop, reboot/full
sign-out and real SSH authentication remain unverified on target workstations.

`crates/s3drive-s3/src/uploads.rs` connects the durable journal to real SDK
single-PUT and full-byte observation. Its exported `Executor` has private
fields and **no production constructor**; tests alone construct it with local
TCP endpoints and dummy credentials. Normal builds compile/lint its methods.
No capability enum or probe report constructs it. Existing configurations,
CLI commands and mounted adapters retain their read-only application boundary.
Provider acceptance and explicit application write policy precede a factory.

`submit` checks the pinned runtime binding/full prefixed key before preparing,
reads/hashes bounded local bytes before claiming, and consumes a non-cloneable
permit in one private send function. An already claimed intent only observes;
`reconcile` never prepares or sends. Full HEAD/conditional-GET operation, length,
metadata, revision and SHA-256 checks precede verified journal state, even after
a valid PUT response. A false receipt cannot verify. No lost response, absent or
rival observation, error or cancellation authorizes replay or rebase. 409/412
remains a retained conflict. Local acknowledgment failure after network commit
recovers by reopening and observing the same attempt, never issuing a new PUT.

The executor shares four slots across clones, fails extra admission before
journal preparation, and retains the 8 MiB single-PUT bound. Unknown/rejected
records and all local bytes remain retained. No multipart, remote flush, dirty
payload reclamation, credential rebinding, unresolved-attempt resolution or
writable callbacks are added. Error context carries generation/operation IDs.

Private reusable transport is now `s3drive-s3/src/transport.rs`; fixture-only
preflight/versioning/deletion stays in `probe/transport.rs`. The provider crate
adds the existing state dependency for the coordinator, without a dependency
cycle or transitive version changes. The HTTP fixture is reused by executor
tests and adds adversarial response modes, not live-provider behavior claims.

The final local gate passed 43 S3 tests, 70 state tests, filesystem/CLI checks,
clippy, builds and book checks. It includes independent-journal create/replace races,
Unicode/empty/exact-8-MiB bodies, dropped/malformed success, exact GET verification,
corrupt/short/wrong metadata, false receipts, revoked reads, stalled PUT/GET,
slot pressure, mismatched credentials, invalid/full/oversized keys, lost local
ack and a killed executor process after server commit. Both outcomes preserve
local conflict bytes; assertions count actual SDK PUTs. Existing local-head
lock timing failure from the prior phase has not recurred in these full gates.

Gate log: `/tmp/s3drive-upload-executor-gate.log` (exit 0). Real FUSE passed 341
read-only requests; the no-systemd cache-enabled startup/session gate passed
with schema 6. Logs: `/tmp/s3drive-upload-executor-linux.log` and
`/tmp/s3drive-upload-executor-session.log`, both exit 0. Final book diagnostics
are in `/tmp/s3drive-upload-executor-book.log`. Require exact-final-tip Linux/Windows/book CI before main
publication. Find the run with `gh run list --branch journaled-s3-executor`.
See `docs/src/upload-executor.md`; the updated `remote-journal.png` was visually
inspected. No local config, credentials, real startup registration or Wasabi
objects changed. The scoped cloud approval question below remains pending.

Next: live provider acceptance after an actual user approval; explicit source-
bound application write policy/factory; unresolved-intent/credential rebinding;
multipart, remote flush and reference/pin-aware acknowledgment cleanup, then
real writable FUSE/WinFsp application workflows. The overall goal remains active.

## Current implementation

- Shared bounded namespace engine; one directory per configured source alias.
- Linux FUSE and Windows WinFsp foreground mounts use the same S3 reader.
  Owner-only/read-only permissions, revision-pinned file handles, bounded
  directory snapshots, explicit collisions and cancellation paths are enforced.
- The mounted S3 reader exposes LIST/HEAD/range GET only. It validates ranges, lengths and revisions,
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
  Registration is idempotent and rejects conflicting fingerprints.
  Cache-enabled mounts register successful credential generations and still
  authorize each new open online.
- Public `State::open_managed` now owns a private directory and nonblocking
  kernel file lock for the connection lifetime. `init-state` uses it; its parent
  must already exist. Link/type/ownership checks precede SQLite access. Linux
  supports a local filesystem allowlist; Windows requires fixed NTFS and checks
  ACLs through a small audited native module. See the book for exact limitations.
- Schema 3 adds `clean_blobs` and a bounded async cache API backed by one OS
  thread. Default limits: 64 MiB logical payload, 1 MiB/file, 4,096 entries.
  Keys bind the authorized source generation, fingerprint, exact object key,
  ETag/version and size; independent SHA-256 checksums verify payloads. LRU
  eviction skips pinned entries; quota pressure is an explicit bypass result.
  SQLite insertion and eviction are atomic. See the storage chapter for the exact limits. The shared engine now uses this worker when `[cache]` is set.
  New file opens authorize through S3 before cache access. Fully cached open
  handles retain their authorized immutable bytes until release; denied or
  offline new opens fail. Large files and pinned-quota pressure use direct
  conditional range reads. Both adapters close the worker after callbacks stop.
- Schema 4 adds non-evictable chunked staged generations, manifests, retained
  chunk markers, local heads and pending intent. `Journal` borrows owned State
  on a blocking owner thread. Append/seal retries are idempotent with explicit
  cursors/receipts. Metadata and requested bytes are verified on read. Staging
  mutation refuses unmanaged state and known Linux tmpfs; inspection remains
  available. No writable filesystem path uses this API yet.
- `worker::Storage` now gives cache/staging handles the same connection, lock,
  bounded queue and owner thread. Rejected commands are distinct from accepted
  commands with lost/expired replies. Guarded append/seal operations reconcile
  those uncertain outcomes. Snapshot handles read a fixed sealed generation.
- Schema 5 maps complete edit-intent keys to generations. New mapped manifests
  use domain v2 including that key; older unmapped v1 manifests remain readable.
  `s3drive-fs::edits` plans patch/truncate/sparse growth, copies immutable local
  parent pieces, and hydrates remote unchanged ranges through a pinned handle.
  A successful plan returns a new local snapshot; mounted writes remain disabled.
- No application uploads, writable mounts, new offline opens or desktop packaging yet.
- A separate explicit `probe-writes` CLI can create/replace/delete only six
  reviewed fixture keys. Its private transport makes one mutation attempt and
  observes metadata plus full bytes after uncertain results. Application uploads
  and mounted mutations remain disabled.

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
Local verification passed: full repository gate (19 S3 / 8 state tests), real
Linux FUSE (145 S3 requests and the Unicode corpus), and startup on
this non-systemd workstation. Logs: `/tmp/s3drive-cache-binding-stack-fix.log`,
`/tmp/s3drive-cache-binding-gate.log`, `/tmp/s3drive-cache-binding-linux.log`,
`/tmp/s3drive-cache-binding-session.log`. CI runs the new binding/migration tests
on both OSes and the existing real Windows mount/startup gate; publication
requires all jobs successful at the exact branch tip. Retrieve the result with
`gh run list --branch cache-source-binding` and match its head SHA to Git.
The first P4a candidate `3f3af5a` passed portable tests but failed the real
Windows mount with exit `0xc00000fd` (stack overflow) in run 38948825021.
Its nested open future was 32,008 bytes on Linux. Heap-pinning the HEAD future
keeps open below a 4 KiB inline budget, now enforced by a regression test.
The repaired final source `0a773bf3b4fc5b309c6629cc1cab10ed05419f4c` passed all
jobs in run 38949836161 and was published to main. Its real Windows gate passed
626 read-only S3 requests, Unicode and cross-logon checks. Full Windows log:
`/tmp/s3drive-cache-binding-final-windows.log`. The size check alone does not
establish actual callback stack use.
Resolved AWS SDK 1.152.0's Config::credentials_provider getter is deprecated
and always returns None. S3Reader retains the actual provider separately;
open resolves it within the overall deadline and replaces the pinned client's
identity resolver. Do not recover the provider through that getter.
The SDK profile provider parses static credential/config files once. Restart
the mount after editing them; resolving a provider per open is not a file watch.
Refreshable provider chains keep their own refresh behavior. Cache bindings use
the actual returned credential tuple, never an inferred profile identity.

Target-workstation reboot/sign-in, full sign-out policy and SSH authentication
acceptance remain pending. P4b1 was published to main at
`13227328a88708a4aed18cb582d8cd73cd534fd1`; all jobs passed in
[run 38954161719](https://github.com/marshallr12/s3drive/actions/runs/38954161719).
Its Windows gate passed 626 requests, Unicode and independent logons. Log:
`/tmp/s3drive-directory-final-windows.log`. The native fixture removes its
protected DB as the owning ordinary account before deleting that account;
application uninstall still preserves data. All fixture cleanup errors except
the documented disposable-profile lock remain fatal. Local ownership tests
include process death, links/ACLs, and foreign DB byte/permission preservation.

P4b2a is implemented on branch `revision-cache`. Source lives in
`crates/s3drive-state/src/cache.rs`, its `cache/tests.rs`, and migration 003.
`Cache::open` initializes on its worker thread; `get`/`put` use an eight-command
queue and 30-second caller deadlines. Oversized inputs and pinned quota
pressure return None; full queues and SQL errors are explicit. `close()` releases
SQLite and the process lock before acknowledging. Pinned data remains immutable,
but callers must stop filesystem callbacks before closing the worker.

Small clean BLOBs replace the originally proposed separate small-file payloads:
this gives bytes/checksum/metadata and eviction one SQLite transaction boundary.
Large cached files and durable staged writes remain future external-file work.
The logical cache quota excludes DB/WAL overhead. Performance is unmeasured.
P4b2b now opts mounted reads into this worker. No real user config or state was changed.

Local tests cover key/credential/revision/Unicode separation, corruption and
missing rows, pins/LRU/zero-byte entry quotas, restart with reduced limits,
queue saturation, a live SQLite writer lock without blocking Tokio, actual
SQLITE_FULL rollback, and child-process kills after a committed payload and
during uncommitted eviction. Logs: `/tmp/s3drive-blob-tests.log`,
`/tmp/s3drive-blob-clippy.log`, `/tmp/s3drive-blob-gate.log`.
Run the full repository gate, then require terminal all-green Linux/Windows/book
CI at the branch's exact final SHA before fast-forwarding/pushing main.
Implementation commit `bf21ba6f19a93d749595f266c9d5b21d5b3cbe98` passed all jobs
in [run 38956348568](https://github.com/marshallr12/s3drive/actions/runs/38956348568):
26 Linux and 25 Windows state tests, native Linux FUSE/startup, 626 Windows
mount requests and cross-logon acceptance. Logs: `/tmp/s3drive-blob-linux-ci.log`
and `/tmp/s3drive-blob-windows-ci.log`. The final documentation update fixes a
rendered decision-table row; locate exact-tip CI with
`gh run list --branch revision-cache` before publication.

P4b2a's final source `393737f737f6647ba7efe1919b8b5899a319b018` passed
[run 38957620186](https://github.com/marshallr12/s3drive/actions/runs/38957620186)
and was published to main. Final Windows log:
`/tmp/s3drive-blob-final-windows.log`.

P4b2b is implemented on `mounted-cache`. `Config.cache` is optional, with an
absolute private local directory and byte/file/entry limits. The CLI rejects
cache/mount tree overlap before state initialization. It does not expand `~`
or environment variables in TOML. Standalone S3 read tools remain direct.
The shared engine uses sixteen fixed lock stripes and four slots, lazy whole
small-file fills and immutable snapshots per open file. New opens always HEAD
before key construction. Cache quota pressure switches that handle to range
reads without retaining an unaccounted snapshot. SQLite disk-full is classified
across the worker boundary and mapped to ENOSPC/STATUS_DISK_FULL; other SQL errors
remain I/O failures. Adapters explicitly close the cache after callbacks stop.
SDK range futures are heap-pinned to limit inline native callback stack use.

Both native gates now call `crates/s3drive-cli/tests/support/cache.rs`:
GET-denied hits, coalescing, Unicode/NFC/NFD, old snapshots and new revisions,
denied/deleted/offline new opens, restart/process death, corrupt/missing repair,
changed static secrets, pinned quota bypass, SQL insertion failure propagation,
and cancellation during a stalled cached fill. SQL failure is injected only in
the stopped disposable DB; real SQLITE_FULL has separate state evidence.
Linux ordinary external unmount refuses open reads; the stalled-fill test uses
the daemon's SIGTERM cancellation path and requires successful teardown.
Both startup fixtures now enable caching. Linux checks retained state ownership
across stop/start and after removal. Windows additionally uses an ordinary
account and separate authentication sessions, then reopens preserved cache state
as that account after removal. Its fixture cleanup removes private DBs as owner.

Local logs: `/tmp/s3drive-mounted-linux.log` (native cached/uncached gates),
`/tmp/s3drive-mounted-session.log` (non-systemd startup with cache),
`/tmp/s3drive-mounted-gate.log` and `/tmp/s3drive-mounted-book.log`.
Require all-green CI at the exact final branch SHA before fast-forwarding main;
locate it with `gh run list --branch mounted-cache`. Current Windows behavior
must be proved remotely, never inferred from the Linux pass.

P4b2b's final source `2764fee28eccc40cde71c6d801f6a2a6f269de39` passed all jobs
in [run 38960792977](https://github.com/marshallr12/s3drive/actions/runs/38960792977)
and was published to main. Native evidence: 341 FUSE requests, 749 WinFsp
requests, cached acceptance and ordinary-account cross-logon cache access.
Logs: `/tmp/s3drive-mounted-linux-ci.log`, `/tmp/s3drive-mounted-windows-ci.log`.

P5a1 is implemented on `staged-generations`: migration 004 and
`crates/s3drive-state/src/journal.rs` with `journal/tests.rs`. It uses SQLite
chunks instead of external staging files so publication has one transactional
boundary. Separate `retained_staged_chunks` markers protect published payloads
without updating their BLOB rows. The original external-file design remains an
optional future backend, not a dependency of the local-write foundation.

Targets preserve source binding/fingerprint, exact key and original base
condition. Builders append bounded immutable chunks or explicit sparse zeros.
`seal` verifies coverage/checksums and compares the local parent, then atomically
retains chunks, seals the manifest, inserts pending intent and advances the head.
Local receipts do not acknowledge S3 publication. Retrying an exact append or
seal cannot duplicate bytes or rewind a newer head. `abort` accepts unpublished
builders only; retained chunks and sealed generations cannot be reclaimed.

The blocking API borrows the already-owned State for future worker integration.
Defaults: 1 GiB unique payload, 4,096 generations and 1,000,000 extents per
file; chunks <=1 MiB and reads <=8 MiB. Logical lengths use i64 offsets; sparse
holes avoid corresponding allocation. Quotas exclude metadata/WAL overhead.
A smaller quota leaves recovery reads available. Generations are never evicted.
Linux tmpfs may hold clean cache, but cannot acknowledge staging mutations;
`/dev/shm` tests verify the distinction. Windows remains fixed-NTFS only.
Current-schema open no longer rewrites the version pragma unnecessarily.

Tests exercise restart/old snapshots, empty and 3 TiB sparse generations,
concurrent-builder conflicts, wrong parents/bases, lost append/seal responses,
quota/dedup/abort behavior, corruption/missing or rewound heads, clean-cache
isolation, schema-3 migration and real SQLITE_FULL append rollback. Nine child
kill points cover append and publication changes/commits. A memory-base fixture
changes revision during hydration and cannot publish the incomplete mixture.
This does not test hardware power cuts, writable OS callbacks or S3 mutations.

Logs: `/tmp/s3drive-staging-tests.log`, `/tmp/s3drive-staging-clippy.log`,
`/tmp/s3drive-staging-gate.log`. The new book chapter `docs/src/staged-writes.md`
and `images/staged-generations.png` explain the boundary; the PNG was visually
inspected. Require the full local gate and exact-final-tip Linux/Windows/book CI
before fast-forwarding main. Locate source evidence with
`gh run list --branch staged-generations`; do not infer native Windows results
from local Linux tests. No real user config, credentials or state were changed.

P5a1's exact final source `ce743adbee23c24656733d23545830f6c02db93f` passed
[run 38966280679](https://github.com/marshallr12/s3drive/actions/runs/38966280679)
and was published to private main. Native evidence: 42 Linux / 40 Windows state
tests, 341 FUSE requests, 749 WinFsp requests, cache and session gates.

P5a2a is implemented on `shared-state-worker` in
`crates/s3drive-state/src/worker.rs` and `worker/tests.rs`. `Storage::open`
validates cache/staging limits before creating state. Its cache and staging
handles share one dedicated thread, eight-command queue, connection and process
lock. `Cache::open` uses the same implementation. No schema or dependency change.
Input payload/range/page bounds are checked before admission; deadlines are
30 seconds including queue time. Worker failures preserve concrete SQL/journal
error types. Queue-full/closed errors prove no admission; missing/expired replies
are explicitly unknown. Queued canceled work is skipped; executing work can
commit despite cancellation. Never infer rollback from timeout.

Staging exposes guarded append/zero append, seal, metadata/head, bounded reads,
recovery pages and explicit builder abort. `begin` has no operation ID yet:
unknown begin outcomes require draft inspection, not blind allocation retries.
Local snapshots validate a sealed manifest and retain generation identity over
newer heads. They hold no whole-file buffer and become unusable after owner
close. Sealed generations remain retained forever; future GC must implement
real pin accounting. Close drains earlier commands, drops DB/lock and then
acknowledges. Dropping the last handle also drains accepted work.

New tests exercise real shared ownership/cache pressure, competing builders,
queue pressure and pre-admission limits, queued cancellation, lost running
append/seal replies, immutable snapshots, SQLITE_FULL, a SQLite writer stall,
worker-panic recovery, last-client drain and close/reopen. Test pauses have
deadlines and remain outside the public API. The book describes the contract
and adds `images/state-worker.png`; architecture/publication diagrams are updated.
No live user config, credentials, startup registration or cloud objects changed.
The full local gate passed with 51 Linux state tests. Native FUSE passed 341
read-only fixture requests, and the no-systemd cache-enabled startup/session
gate passed on this workstation. `state-worker.png` was visually inspected.
Local logs: `/tmp/s3drive-worker-tests.log`, `/tmp/s3drive-worker-clippy.log`,
`/tmp/s3drive-worker-gate.log`, `/tmp/s3drive-worker-linux.log` and
`/tmp/s3drive-worker-session.log`. Require successful full local gate and
exact-tip Linux/Windows/book CI before fast-forwarding main. Use
`gh run list --branch shared-state-worker` to locate authoritative run state.

P5a2a's exact final source `0d282e4aecfb4548aacac095710646a10e884bdb` passed
[run 38968191261](https://github.com/marshallr12/s3drive/actions/runs/38968191261)
and was published to private main. Native evidence: 51 Linux / 49 Windows state
tests, 341 FUSE requests, 749 WinFsp requests, cache/session gates and book.
Full CI log: `/tmp/s3drive-worker-ci.log`.

P5a2b is implemented on `file-edit-plans`. Sources:
`crates/s3drive-fs/src/edits.rs`, its `edits/tests.rs`,
`crates/s3drive-cli/tests/staged_edits.rs`, migration 005, and journal/worker
extensions. `Coordinator` allows four executing plans per volume. Editors use
fixed empty/local/pinned-S3 inputs. Nonempty patches are <=8 MiB; unchanged S3
pieces <=1 MiB have a 30-second deadline outside SQL. Sparse local zeros retain
their logical spans. Each completed edit seals one immutable local snapshot.
Read that snapshot for read-your-writes and select it as the next editor input.

`begin_operation` commits a complete intent key and generation atomically, so
lost begin replies/restart recover the same ID. Keys hash target, input/base,
requested size and operation/payload digest. They are semantic local deduplication,
not globally unique remote operation IDs or authorization tokens. Reconstructing
the same fixed-input plan identifies the same operation. Future adapters must
advance their input after a successful write. Old sealed replay never rewinds
a newer head. Metadata conflicts fail; sealed v2 manifests bind their operation
keys and detect missing/changed mappings. Migration 5 preserves old v1 bodies
and rolls back conflicting tables/version changes.

Plan failures expose a known builder ID and preserve partial progress. Replays
verify exact guarded pieces, potentially rereading hydrated remote ranges.
If the old revision is no longer readable, fail and retain the draft. A completed
plan replays locally without S3 reads. Plan/patch remainder is caller-owned until
publication; no whole edit-plan document is persisted. Identical concurrent
plans share one receipt; different plans compare the same local parent and
conflict. There is no total edit-duration guarantee or upload executor.
Parent-manifest metadata is currently scanned per copied piece; fragmented-file
performance is unmeasured. No sealed-generation GC is implemented.

Tests: 120 deterministic model steps, sparse growth/shrink/overwrite, restart
with reduced quota, transient/short/changed/stalled memory bases, cancellation,
deadlines, quotas, concurrent plans, mapping rollback/corruption, lost begin
replies, and all nine process-kill boundaries with operation-bound generations.
The real SDK HTTP fixture hydrates an exact Unicode key and rejects a changed
base with HTTP 412. It uses a child process and isolated dummy credential files;
all waits have deadlines and kill-on-drop cleanup. No HTTP mutation is allowed.
The Windows session fixture now expects init-state schema 5.

The local full gate passed: 12 filesystem and 56 state tests plus SDK hydration
and book checks. Real Linux FUSE passed 341 requests, and the cache-enabled
no-systemd session gate passed at schema 5. The new `file-edits.png` diagram was
visually inspected; related book chapters/diagrams describe the boundary.
Logs: `/tmp/s3drive-edits-gate.log`, `/tmp/s3drive-edits-tests.log`,
`/tmp/s3drive-edits-sdk.log`, `/tmp/s3drive-edits-linux.log`,
`/tmp/s3drive-edits-session.log` and `/tmp/s3drive-edits-book.log`.
Require exact-final-tip Linux/Windows/book CI before main publication; locate it
with `gh run list --branch file-edit-plans`. No live user config, credentials,
startup registration or cloud objects were changed.

P5a2b's exact final source `4414711ee313ab97c86ea0e71de6438b8ebce635` passed
[run 38971696786](https://github.com/marshallr12/s3drive/actions/runs/38971696786)
and was published to private main. Native evidence: 12 filesystem tests on each
OS, 56 Linux / 54 Windows state tests, SDK Unicode hydration, 341 FUSE requests,
749 WinFsp requests, startup/session gates and book. Full log:
`/tmp/s3drive-edits-ci.log`.

P5b1 is implemented on `conditional-write-probes`:
`s3drive-s3/src/probe.rs`, its private `probe/transport.rs` and `probe/tests.rs`,
plus the CLI's `probe-writes plan/run/cleanup` and `probe_cli.rs`. No DB migration.
New direct dependencies reuse already-locked getrandom 0.4.3, serde and toml.
`plan` performs no credential resolution/network and writes a fresh TOML plan
with a random 128-bit namespace, source binding, six exact keys and limits.
`run` requires exact source/plan regeneration and digest confirmation, reserves
a new report path before network work, pins one credential generation, and
refuses versioned or nonempty scopes before PUT. The private transport uses
one SDK attempt, 30-second request deadlines, required-only SDK checksums,
guarded known-length bodies, operation markers and SHA-256 metadata. Probe
objects are <=4 KiB, fourteen PUTs maximum per execution; transport bound is
8 MiB. CLI mutation futures are heap-pinned out of mount startup's envelope.

Lost/malformed success, service errors and cancellation never trigger an
automatic second PUT. Observation requires exact marker/length plus complete
conditional GET metadata, revision and body checksum. `null` versions remain
mutable. An absent/rival object cannot prove a lost attempt never committed;
unknown application attempts will need durable unresolved states. Reports are
test evidence, not application capability tokens or remote acknowledgment.

Cleanup is explicit fixture-only plain DELETE after verifying a known marker
and exact fixture body in the reserved unversioned scope. It refuses foreign,
corrupt or versioned data. It is not the application's delete protocol or an
unconditional fallback from a failed application guard. Interrupted runs can
use `cleanup` with the same confirmed plan and a fresh report path; no PUT.
The original Wasabi three-object read approval does not authorize these new
create/replace probes. No live write probe has run in this phase yet.

Prepared ignored mode-0600 plan: `s3drive.probe-plan.local.toml`.
Bucket `s3drivetest`, exact prefix
`s3drive-conformance/d147830ba5257d762e14c3962cde60f9/`.
Digest `98914e5210466c6c1e074ea36184ea580d20c93749363e18650e5ddf14906de6`.
An async question requests explicit authorization for six keys, conditional
replacements and cleanup, under 1 MiB ordinary transfer. Check the actual user
reply before executing; tool acceptance is not approval. Keep this question
pending while progressing independent local work. The plan has been queued
for display in Codex; do not claim it was visibly opened. Config/credentials,
real startup registration and existing cloud objects remain untouched.

Local gate and native Linux checks passed before final documentation updates:
29 S3 tests, existing 12 filesystem / 56 state tests, SDK hydration/CLI guards,
341 FUSE requests and the no-systemd cache-enabled session gate. Logs:
`/tmp/s3drive-probe-gate.log`, `/tmp/s3drive-probe-tests.log`,
`/tmp/s3drive-probe-linux.log`, `/tmp/s3drive-probe-session.log`.
The new book chapter `conditional-uploads.md` describes primary-source research,
the exact scope, command sequence, evidence and remaining state-machine work.
Run the final local gate and exact-tip Linux/Windows/book CI before publication;
locate the run with `gh run list --branch conditional-write-probes`.

Next required work is P5b2: conditional remote commits and uncertain-result
reconciliation, followed by reference/pin-aware remote-ack cleanup.
Preserve parent/supersession dependencies and define
credential rebinding before P5b conditionally publishes to S3. No remote ack or
sealed-generation GC is implemented yet. P5c exposes writes only after those
contracts and their provider/OS acceptance tests pass.
Broader live Wasabi cached-read acceptance and workload/performance tests remain
separate pending work; new provider uploads require concrete scoped authorization.
Use the handle binding plus exact key/revision/size; no binding alone authorizes
cached access. Keep cache/state outside mounted trees. Writes require separate staged
generations and a durable journal (P5a), provider conditional commits/recovery
(P5b), then desktop mutations (P5c). Do not expose unconditional overwrite as a
shortcut. Broader live provider probes require scoped disposable fixtures.

Read the mdBook implementation plan, provider/OS contracts and decisions.
No subagents were dispatched. The overall goal is not complete.
