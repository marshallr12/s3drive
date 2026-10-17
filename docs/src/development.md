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


## Managed namespace authority prototype

P6m1 adds `s3drive-authority` (axum loopback service, dedicated SQLite owner) and
`s3drive-state::namespace` (schema 8 transactions). Run
`cargo test -p s3drive-authority --locked` for independent real-TCP clients and
`cargo test -p s3drive-state namespace --locked` for migration, failure/recovery
and retained bounds. They are part of the normal workspace gate on both OSes.
See [managed layout](managed-layout.md) for the exact protocol and boundaries.

These tests create private temporary authority directories and dummy tokens;
they do not read Wasabi credentials, mutate buckets or install a service. The
existing native suites remain necessary to detect regressions from the schema
change, and their original cases exercise **direct** mounts. P6m3a adds a separate
managed native case; neither suite certifies a remotely deployed/Turso backend.


## Managed publication integration

`cargo test -p s3drive-cli --test managed_publication --locked` runs real SDK and
authority HTTP traffic against an owned fixture, then kills/restarts clients at
unknown upload and metadata-publication boundaries. It requires no mount privilege
or live credentials and is part of Linux/Windows workspace CI. Schema 9 also adds
five local managed-journal recovery/migration/acknowledgment tests. These do not
replace managed native or live provider acceptance; see [the contract](managed-publication.md).


## Managed native read integration

`cargo test -p s3drive-cli --test managed_native --locked` uses actual authority
CLI child processes, private SQLite/token directories, two S3 buckets and two
dummy profile names. It verifies store-route mismatch refusal, read-only
behavior, Unicode bytes, old/new snapshots, stable identity during rename and
old-handle reads after unlink. It is included in the portable gate.

Both ignored native suites now include
`actual_managed_namespace_and_immutable_reads`, using the same fixture and real
FUSE/WinFsp mounts. They additionally check S3 authorization denial, directory
name projection, metadata-service loss, authority restart and remount. Run the
existing native commands above; the Windows gate runs on GitHub's Windows VM.
This workstation's NoNewPrivs restriction still prevents native FUSE acceptance
here. No live Wasabi mutation, user service installation or Turso account setup
is part of these tests. See [managed native contract](managed-native.md).


## Managed local edit recovery

The [logical-edit storage/publisher contract](managed-edits.md) is schema 10.
State tests force nine owner-process deaths plus real SQLITE_FULL at logical
settlement, corruption and schema-9 migration failures. The managed publication
CLI test also kills a real SDK/authority client after metadata commit but before
local settlement. Its restart must settle without another PUT, retain a newer
head and preserve a later conflicting blob. No new Wasabi approval is consumed.
Native suites still exercise managed read-only and direct-write regressions;
writable managed callbacks have their own subsequent acceptance case below.

### Linux ownership release during process creation

Investigated 2026-10-17. Concurrent child-process creation exposed a migration
fixture seeing `state directory is already in use` after its State was dropped.
The controlled reproduction showed inherited-descriptor lock retention: Linux
flock ownership is associated with the open file description, so closing only
the parent's descriptor need not release it.
[Linux flock manual](https://man7.org/linux/man-pages/man2/flock.2.html)

The owner now calls `File::unlock` explicitly on drop, after the SQLite
connection closes, and releases acquired locks on initialization errors. A still-open State continues to reject competing owners;
an inherited descriptor does not become a new State owner. The pinned Rust
1.99.1 Unix/Windows implementations were inspected before using this API.
[Rust File unlock contract](https://doc.rust-lang.org/std/fs/struct.File.html#method.unlock)

Strict migration reopen checks remain immediate, without retries. A Linux
regression runs one hundred migration rounds while another thread creates
processes, alongside the existing live-owner and process-death exclusion tests.


## Managed existing-file editing acceptance (2026-10-17)

[Version-4 managed editing](managed-write-plan.md) adds schema-11 verified base
observations and `content-cas-v1`. The portable managed engine test uses actual
SDK/authority processes and exactly nine immutable PUTs total: two seed payloads,
a coalesced recovered head, empty-file publication, another store, rename-safe
publication, a second dirty/read open and two external updates. Retrying the retained conflict adds no PUT.
State tests separately verify independent file guards, old request hashes,
metadata-only renames, same-file rejection, migration and retained dirty bases.
The pinned getrandom 0.4.3 `fill` implementation was inspected locally before using
it for durable operation IDs; no new dependency version was introduced.

The Linux/Windows ignored suites each add
`actual_managed_existing_file_writes_and_recovery`. They must pass real driver
edits, read-only close without publication, Unicode/empty/two-store operations,
metadata rename, a killed dirty mount and restart, an unknown committed PUT whose
observation is blocked, another restart without PUT replay, and retained competing
changes. Native editing results must come from terminal CI for the exact source;
the [branch CI history](https://github.com/marshallr12/s3drive/actions?query=branch%3Acodex%2Fmanaged-native-writes)
identifies candidate runs. Windows runs on a remote Server 2025 VM. This local
MX executor cannot acquire a FUSE mount under NoNewPrivs; do not work around it.

The authoritative phase-start checkpoint is
`e216fb869ef58e77969b7ef92dd69e2932f0b21d`,
[run 39006862182](https://github.com/marshallr12/s3drive/actions/runs/39006862182),
which passed Linux/Windows/book with the four prior native cases per OS.
That checkpoint certifies the local-edit library, not the new native writer.
No new live Wasabi upload, startup registration or Turso deployment is authorized
or performed by the fixture gates.

Linux native producer commands use a process started before native handles exist.
Fork/exec can close inherited CLOEXEC writable handles and trigger a valid FUSE
flush, so spawning a competitor only after dirtying a native file does not isolate
the intended competing-publication ordering. The harness keeps the flush/conflict
assertions and sends rename/update commands to the prestarted producer instead.
It also retains sanitized managed error diagnostics after mount readiness.
Research checked 2026-10-17: the [libfuse flush contract](https://github.com/libfuse/libfuse/blob/master/include/fuse.h)
describes per-descriptor close callbacks, and the [Linux exec change](https://kernel.googlesource.com/pub/scm/linux/kernel/git/netdev/net/%2B/e780259b54e618ceb4763fbc21314acf3565e813%5E!/)
explicitly identifies close-on-exec waiting on filesystem flush handlers, including FUSE.

Run the ignored native suites with `--test-threads=1` as CI does. Independent
cases share one Rust test process, so a subprocess in a different case can also
close inherited FUSE handles. Windows cases additionally share driver and Shell
resources; five parallel cases exposed a PowerShell/Explorer deadline failure.
Serializing cases isolates their fixtures without increasing deadlines, retrying
failures, or removing their concurrent-handle/client checks. Native job limits
remain ten minutes. Portable tests remain parallel.


### Verified native checkpoint

Both native acceptance steps completed successfully at
`9a0185680f397070d31172f880c0fcc37eb71b82`,
[run 39009806732](https://github.com/marshallr12/s3drive/actions/runs/39009806732).
The [metadata snapshot](evidence/managed-native-writes-2026-10-17.json) records
the exact source, job IDs, step outcomes and exclusions. Linux's full job and
no-systemd startup regression also passed. Windows completed its native step;
its overall job was saving its build cache when this snapshot was captured.
A successful native step is not a claim that an in-progress overall run is green.

The final candidate serializes native cases on each OS and must pass the full
Linux/Windows/book jobs at its own exact tip before main publication. Its
[branch CI history](https://github.com/marshallr12/s3drive/actions?query=branch%3Acodex%2Fmanaged-native-writes)
provides that terminal publication evidence. All five native cases remain enabled;
no deadline, assertion or recovery check was removed.


## Managed creation and directory acceptance (2026-10-17)

The [namespace phase](managed-namespace-plan.md) adds a sixth ignored native case,
`actual_managed_file_and_directory_creation`, to both OS suites. Preserve serial
case execution and all existing read/write/startup regressions. The Windows
workstation boundary remains a remote Server 2025 VM; actual MX mount/reboot/SSH
and Windows desktop acceptance are not inferred from these fixtures.

Pure state tests cover portable collision admission, source placement, stale
parent guards, unrelated-sequence independence, corrupt sibling keys, schema-11
migration/rollback, exact namespace inputs, ignored ACK updates, real SQLITE_FULL
and retained unknown outcomes. The actual CLI/SDK test separately loses an
already-committed create reply and kills/restarts its caller; recovery must not
create another node. It also checks a peer with another default store and an
authority-process restart. No live Wasabi mutations are performed.

Resolved APIs inspected: unicode-normalization 0.1.25 Unicode-17 tables and `nfkc`
iterator, Rust 1.99.1 Unicode-17 character data, WinFsp 0.13.1 create/cleanup/delete
trait contracts, fuser 0.18.0 mkdir/create callbacks, axum 0.8.9 routing/state,
and Tokio 1.53.3 notification/blocking-task contracts. The newest downloaded
normalization crate, 0.1.26, has Unicode-18 tables; it is intentionally not used
for the versioned Unicode-17 namespace policy. Windows runtime and unit checks
audit every BMP upcase alias before enabling namespace creation. Table drift
requires explicit policy review, not an automatic namespace-key rewrite.


### Creation native checkpoint

Both native steps passed at `d89e6f372c23498e423c2017bbfa03426fdd61cb`,
[run 39013347425](https://github.com/marshallr12/s3drive/actions/runs/39013347425).
The [metadata snapshot](evidence/managed-namespace-2026-10-17.json) records six
cases on each driver, including the new creation/recovery case. Linux's complete
job and no-systemd startup check passed. Windows also passed its portable tests,
including the BMP native upcase audit; its job was saving its cache when the
snapshot was captured. This records native-step acceptance, not a premature
claim that the overall workflow had finished.

The final documentation candidate still requires terminal Linux/Windows/book CI
at its own exact tip before publication. Consult the
[branch CI history](https://github.com/marshallr12/s3drive/actions?query=branch%3Acodex%2Fmanaged-namespace)
for that result. No native case, assertion, recovery check or deadline was removed.
