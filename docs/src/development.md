# Development and verification

## Build

The workspace pins Rust 1.99.1 in `rust-toolchain.toml`; Cargo dependencies are
locked. Native builds need a C compiler for bundled SQLite. Use MSVC with the
Visual Studio C++ workload on Windows. Windows builds now require WinFsp 2.1 Developer files;
installing the runtime is separate from starting a mount. The separate Linux mount test needs FUSE. Dev/test builds omit debug symbols to reduce the SDK's build footprint.

```bash
cargo build --workspace --locked
cargo run --locked -- status
cargo run --locked -- config-check examples/s3drive.toml
cargo run --locked -- resolve examples/s3drive.toml documents/reports/example.txt
cargo run --locked -- init-state --directory .s3drive
```

The initializer creates `metadata.sqlite3` in the given local directory. It is
safe to reopen the scaffold's current schema; future versions and databases
owned by other applications are rejected. It does not contact S3 or cache file
contents. Linux `mount` now serves the shared engine through FUSE; Windows
has a WinFsp implementation with separate native read-only acceptance evidence.

The separate `list`, `stat` and `read` commands perform S3 reads with a real
configuration. See [provider contract](provider-contract.md) for their usage
and exact scope; the example configuration still contains placeholder buckets.

## Gate

```bash
bash scripts/gate.sh
```

This runs formatting, Clippy with warnings denied, workspace tests, a workspace
build, local documentation-link checks, mdBook build and mdBook tests. It
preserves failures with `set -euo pipefail`. Install mdBook 0.5.4 with
`cargo install mdbook --version 0.5.4 --locked` if needed. The link checker checks
local target files; it does not validate external websites or heading anchors.

On Windows, run the Rust commands directly or through Git Bash. CI defines
native Linux and Windows Rust jobs and a separate Linux book job. CI installs a pinned WinFsp runtime/SDK and runs an explicit native Windows
mount test; it does not access providers or use cloud credentials. A separate
explicit Linux native test uses `/dev/fuse` and a local fixture.

Build-cache restore may fall back to the same OS/compiler with an older lockfile;
Cargo still checks the current locked dependency graph and every gate runs.

Windows testing runs remotely in the private repository's GitHub Actions
`windows-latest` VM, not by emulation or cross-compilation on the MX Linux
workstation. The 2026-10-16 run logs identify Windows Server 2025
(`10.0.26100`, image `windows-2025-vs2026`). This is native Windows driver/API
evidence, not acceptance on a user's Windows 10/11 desktop or a rebooted PC.
Earlier local Linux gates ran real FUSE and no-systemd startup. The current
executor has `NoNewPrivs=1`, preventing setuid `fusermount3`; the current phase
therefore requires native Linux CI rather than weakening local ownership or
execution checks. See [native write acceptance](native-writes.md).

Behavioral tests cover namespace isolation/traversal, exact key bytes and byte
limits, endpoint/config rejection, alias collisions, SQLite persistence/schema
refusal, Linux filename projection/collisions, inode identity, and required CLI
mount arguments. Additional real TCP tests
exercise the S3 adapter, including signatures, pagination, malformed responses,
range/validator checks, cancellation and stream deadlines. A subprocess CLI
test verifies explicit configuration precedence using isolated dummy profiles.
The native acceptance command and its coverage are documented in
[the Linux mount contract](linux-mount.md); it is intentionally separate from
portable tests and must be invoked explicitly.

Linux startup has another explicit native gate:

```bash
python3 scripts/test-linux-session.py
```

It requires Python 3.9+, `tmux`, `gio` (Debian/MX package `libglib2.0-bin`),
FUSE and the built CLI. It uses isolated registration/profile files and a real
mount with a synthetic root; it neither changes the user's startup nor contacts
a provider. It does not require systemd. See [startup evidence](startup-and-sessions.md)
for the distinction between terminal loss and actual reboot/logout acceptance.

The Windows native mount gate also invokes `scripts/test-windows-session.ps1`
while its local HTTP fixture is running. Use an elevated disposable Windows
runner. The harness creates its own ordinary account, grants/removes that
account's batch-logon right, registers startup as that account and checks access
from independent logons. It enables Task Scheduler's operational event log on
the disposable runner for failure diagnostics. Passwords exist only in memory
and a redirected input pipe; the fixture S3 profile uses dummy credentials.
The harness removes its account, task, mount and private test directory afterward.
On GitHub's explicitly disposable VM, Windows may retain a locked registry
profile after all fixture processes exit. Only that profile sharing violation
may defer to VM destruction, with a visible warning and removal of the dummy
S3 credentials. This requires `S3DRIVE_DISPOSABLE_WINDOWS_VM=1` in GitHub Actions
and all functional checks to have passed; task, mount, process, account or other
cleanup failures still fail the gate. Outside that disposable CI configuration,
profile deletion remains mandatory. The app's `Remove` deliberately retains the
real user's profile and credentials.
[GitHub VM lifecycle](https://docs.github.com/en/actions/how-tos/manage-runners/github-hosted-runners/use-github-hosted-runners)
It compares `TOKEN_STATISTICS.AuthenticationId` to distinguish authentication
sessions. `whoami /logonid` alone is insufficient for clients launched with
`CreateProcessWithLogonW`, which uses the caller's desktop logon SID.
[Token statistics](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_statistics),
[secondary logon API](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithlogonw)

## Read the book locally

```bash
mdbook build docs
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

The output is under `docs/book/` and ignored by Git. Serve on loopback and open
the local URL. No public GitHub Pages deployment is configured. Edit Markdown
under `docs/src/` and keep `SUMMARY.md` synchronized.
Original architecture images are committed under `docs/src/images/`.
`python3 scripts/draw-architecture.py` regenerates them using Pillow and DejaVu
fonts; those optional authoring dependencies are unnecessary to build the book.

## Dependency policy

`fuser` and `nix` live under a Linux target dependency table. Add any Windows
binding under a Windows target table. Unsafe FFI is restricted to reviewed
Windows native modules: WinFsp's adapter boundary and the state's directory/ACL
boundary. Other workspace code forbids or denies unsafe Rust. The AWS SDK implements the read-only
transport, while its mutation APIs are not exposed by S3Drive.

Inspect local resolved source before using new dependency APIs. Keep provider
compatibility evidence separate from library version compatibility. Review
licenses before adding/distributing native runtime dependencies. There is no
declared older MSRV: the pinned compiler is the supported development toolchain.

## Evidence boundaries

A green portable test suite means the scaffold works at its implemented scope.
It does not mean the drive mounts, Windows callbacks run, the cache survives
power loss, uploads are crash safe, or arbitrary S3 providers are compatible.
Record the exact source commit, command, host/platform and result when each
future acceptance gate is executed. Refresh the root handoff after a verified
phase so the next session starts from evidence rather than inference.

## Mounted cache acceptance

Both native mount gates call `tests/support/cache.rs`. It controls only their
isolated HTTP fixture and private temporary SQLite state. The corpus verifies
GET counts, denied/offline opens despite cached bytes, credential-generation
isolation, Unicode snapshots, restart/kill recovery, corruption/missing data,
quota/pin behavior and rollback after an injected insertion failure. A separate
state test uses SQLite's page limit to cause real SQLITE_FULL; native SQL-error
coverage does not claim physical disk exhaustion. Startup fixtures now enable
cache ownership and verify preserved state can reopen after daemon removal.

## Local staged-generation verification

`journal/tests.rs` covers schema-4 migration, range/sparse/empty generations,
immutability, quotas/deduplication, conflicts, corrupt state and clean-cache
isolation. Linux also uses an existing `/dev/shm` tmpfs fixture to check
that staging cannot acknowledge volatile storage. Nine child-process kill points surround append and publication
transaction changes/commits; injected post-commit response loss exercises
idempotent append/seal replay. Real SQLITE_FULL is induced using SQLite's page
limit. The memory-base fixture rejects a changed revision during hydration.
These state tests run natively on both CI OSes. They do not establish writable
FUSE/WinFsp behavior, remote publication or physical power-loss survival.

`worker/tests.rs` tests the async cache/journal owner: shared connection/lock,
queue pressure, pre-admission bounds, immutable local snapshots and competing
builders. It deterministically pauses the owner for queued cancellation and
lost append/seal replies, verifies real SQLITE_FULL and a separate SQLite
writer stall, and checks worker-panic recovery, draining and close/reopen.
Tests exercise the public cache/staging APIs; private owner jobs provide only
the local scheduling and fault-injection controls. All pauses have deadlines.
Use `cargo test -p s3drive-state --locked` for the focused storage gate, followed
by the repository gate and both native mount/startup jobs before publication.

`s3drive-fs/src/edits/tests.rs` compares 120 deterministic write/truncate steps
with a separate byte-vector model, retains old snapshots, and reopens final
state with a lower quota. It also covers 3 TiB sparse growth, transient/short/
changed/stalled base reads, cancellation/deadlines, duplicate concurrent plans,
competing plans, quota refusal and exact operation identity. The private pinned
reader seam supports these failure schedules; the public constructor takes a
real S3 `ReadHandle`.

`staged_edits.rs` runs the real SDK and coordinator in a subprocess with a
temporary dummy credential profile. The parent owns an ephemeral HTTP fixture
and changes a separate object only after the child has opened its old revision.
It requires exact Unicode hydration, correct patched bytes, HTTP 412 on the
stale base and no HTTP mutation. Child startup/execution have deadlines and
kill-on-drop cleanup. The state tests additionally cover schema-4 migration,
operation-mapping rollback, lost begin replies and manifest-bound intent keys;
all nine process-kill points now exercise operation-bound generations too.

`s3drive-s3/src/probe/tests.rs` drives the real SDK over bounded TCP fixtures.
It enforces headers and exact bytes, simulates ignored guards, lost/malformed
receipts, 503s, cancellation, a committed-then-deleted object, corrupt data and
foreign metadata. It verifies no hidden PUT retry, full checksum observation,
Unicode/literal keys, races, null-version normalization, empty/occupied scope
checks and fixture-only cleanup. The model's atomic guard is not provider proof.
`probe_cli.rs` tests no-network planning, exact digest/source/target validation,
private create-new artifacts and refusal to overwrite reports before network
work. The optional live commands require the [reviewed probe plan](conditional-uploads.md).

The [remote journal](remote-journal.md) tests local SQLite state, using simulated
verification evidence rather than cloud writes. They kill subprocesses at seven
prepare/claim/verification boundaries, drop a committed claim's async reply,
require one send permission, preserve newer dirty heads, and check real disk-full,
quota, corruption and migration failures. Windows runs in GitHub Actions on a
Windows Server 2025 VM with WinFsp installed; Linux runs here and on the Linux
runner. The passing Windows job for published `a77e170` is
[117225090437](https://github.com/marshallr12/s3drive/actions/runs/38975881747/job/117225090437).
Windows 10/11 desktop use, actual reboot, full sign-out and real SSH authentication
remain separate target-host acceptance checks.

`uploads/tests.rs` reuses the SDK/TCP probe fixture with real local SQLite
directories. It checks actual PUT counts after dropped/malformed responses,
body/metadata/receipt failures, cancellation and closed local owners. A helper
process is killed after server commit; reopening and observing must verify the
same attempt without another PUT. Independent local journals race both create
and replacement guards. The executor's public factory requires an explicit
[source-bound policy and complete evidence](write-policy.md). Existing mounts
and their configuration remain read-only; tests do not install an application policy. See the [executor contract](upload-executor.md).

The [generation-specific flush](flush-receipts.md) tests add schema-7 migration,
ancestor settlement and newer-head preservation. Process-kill fixtures stop
after root insertion, settlement insertion and commit. Real SQLite page limits
force full-disk rollback; a paused owner loses its post-commit reply and recovers
the same receipt. Snapshot bytes remain readable. Corrupted/missing/unrelated
markers cannot suppress pending work or grant a send claim. SDK/TCP fixtures
flush binary, empty and Unicode bodies, retain uncertain/rejected bytes and
recover a killed uploader without another PUT. No live Wasabi mutation occurs.
