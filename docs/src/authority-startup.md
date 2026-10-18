# Coordinator startup and pinned restarts

Design date: **2026-10-17**. Under implementation on
`codex/authority-startup`; no actual user startup entry or hosted service has
been installed. Bootstrap/setup was accepted at `712882e`, terminal
[CI39079281204](https://github.com/marshallr12/s3drive/actions/runs/39079281204).

The designated coordinator host must keep the shared Rust metadata service
available. A workstation hosting both components can start them at sign-in.
Other workstations start only their local drives and use that same shared
coordinator; they must not clone its SQLite database or start independent
authorities for the same volume. A separately hosted foreground service can
use its host's process manager instead of desktop/login hooks.
Existing [Linux and Windows mount launchers](startup-and-sessions.md)
supervise only mounts. A managed mount cannot reconstruct the authoritative
namespace from S3 payloads when the coordinator database is missing.

![Separate per-account supervisors start the pinned Rust coordinator and local drive. The coordinator owns its existing SQLite namespace; the drive keeps its own journal and sends file bytes directly to S3.](images/authority-startup.svg)

## Required restart behavior

Initial `authority-serve` bootstrap may initialize an empty authority and prints
the resulting volume ID. Supervised restarts must instead use an explicit
expected volume and naming policy. They must never generate a replacement ID,
initialize a missing database or adopt a different/local-client database.

The existing-only path acquires the same private local directory ownership and
stable lock. The directory, database and lock must already exist. Links, hard
links, unrecognized files, unsupported local storage and competing owners keep
their existing refusal behavior. Before migrations or attempt recovery, a
read-only SQLite transaction checks the application/schema, volume, naming
policy and root. It reads ordinary WAL state without asserting `immutable=1`
or disabling locking. Immutable mode skips locking/change detection and is
unsuitable for this mutable, restartable authority. Read-only WAL access may
still create operational WAL/shared-memory files; inspection is not a
byte-for-byte filesystem snapshot or a backup operation.
[SQLite URI parameters](https://www.sqlite.org/uri.html),
[SQLite read-only WAL databases](https://www.sqlite.org/wal.html),
checked 2026-10-17.

Only a matching known namespace database can proceed through the existing
transactional migrations and exact recovery. The read-write connection omits
SQLite's CREATE flag and does not initialize a namespace. A mismatch or missing
database fails before listening, printing readiness, or starting a mount.
Keep the original database, staged data, operation IDs and credentials intact.

An offline `authority-check` command will check private startup inputs and
existing identity without starting a listener, migrating schemas or recovering
attempts. It requires exclusive access; stop the foreground bootstrap process
before checking/installing supervision. This does not verify S3/provider access.

## Supervision contract

Extend the existing per-account launchers with an explicit authority component,
separate registration namespace and expected-volume settings. Existing mount
registrations retain their behavior and format. Authority registration stores
only paths/IDs/listen settings, never token, TLS key or S3 credential contents.
It invokes Rust's existing-only path and cannot fall back to bootstrap.

Linux retains the detached process/session, private process lock, XDG desktop
entry and optional shell login hook, including MX without systemd. Windows
retains the account-owned Task Scheduler/batch-logon process and independent
same-account session access. The existing account-password and batch-right
prerequisites still apply. Installation into the user's real account remains a
separate operator action; tests use disposable registrations/accounts only.

Use a stable explicit listen port. TLS/nonloopback and owner-private token/key/
certificate rules remain unchanged. Coordinator and mount registrations are
separate: stopping/removing one drive must not stop a shared coordinator used
by other clients. Stop only a supervisor's own child; do not kill a PID merely
because it appears in an old file. Duplicate starts reuse the same supervisor
or fail on existing database ownership. Failed starts get bounded retries;
no fresh volume, operation ID or unconditional provider write is a retry policy.

Status must distinguish supervisor ownership from a live authenticated metadata
check. A process lock, PID, mounted path or readiness line alone is not proof
of current provider access. Mount startup can retry while the pinned coordinator
becomes available. Terminal disconnection must leave both processes available
to later same-account SSH/tmux/desktop clients.

The same foreground Rust server remains available for separately provisioned
hosting. This phase does not select a hosting provider, provision accounts,
enable Turso, add automatic failover, clone authority databases, or claim
cross-host leader election. The user keeps SQLite-host and Turso options open.

## Candidate command interface

Linux supervision is locally tested; Windows runtime and paired native drive
acceptance are pending. Use the volume ID and naming policy printed by the
original bootstrap. Stop that foreground process before the exclusive offline
check or supervisor installation. All parent directories must already exist.
The following co-located example uses placeholders `VOLUME`, `TOKEN` and
`DATABASE`, not S3 credentials:

```text
s3drive authority-check --directory DATABASE --expect-volume VOLUME --name-policy portable-collision-v1 --token-file TOKEN --listen 127.0.0.1:43128
s3drive authority-serve --directory DATABASE --expect-volume VOLUME --name-policy portable-collision-v1 --token-file TOKEN --listen 127.0.0.1:43128
```

The second command is the foreground existing-only mode. It is also suitable
for an explicitly provisioned host process manager. First initialization omits
`--expect-volume`; a supervisor must always supply it. HTTP remains numeric
loopback-only. Add the original private `--tls-cert`/`--tls-key` pair for HTTPS;
nonloopback listeners require TLS. Clients still use a hostname/IP that matches
the certificate, with their existing explicit trust settings.

For a disposable Linux login registration, with stable executable paths:

```text
python3 scripts/linux-session.py install shared --component authority --executable /absolute/s3drive --authority-directory DATABASE --authority-volume VOLUME --authority-name-policy portable-collision-v1 --authority-token-file TOKEN --authority-listen 127.0.0.1:43128 --shell-profile /absolute/test-profile
python3 scripts/linux-session.py start shared --component authority
python3 scripts/linux-session.py status shared --component authority
python3 scripts/linux-session.py stop shared --component authority
python3 scripts/linux-session.py remove shared --component authority
```

Add `--tls-cert` and `--tls-key` together when needed. The authority manifest,
login block and desktop entry have their own namespace; an existing mount
registration named `shared` is separate. The installed hook supplies the
component explicitly, including when a later SSH shell has no desktop XDG
environment. Status reports ownership and the expected ID; use the existing
`managed info` command for a live authenticated metadata request.

The corresponding Windows candidate interface is:

```powershell
.\scripts\windows-session.ps1 -Action Install -Component Authority -Name shared -Executable C:\S3Drive\s3drive.exe -AuthorityDirectory DATABASE -AuthorityVolume VOLUME -AuthorityNamePolicy portable-collision-v1 -AuthorityTokenFile TOKEN -AuthorityListen 127.0.0.1:43128
.\scripts\windows-session.ps1 -Action Start -Component Authority -Name shared
.\scripts\windows-session.ps1 -Action Status -Component Authority -Name shared
.\scripts\windows-session.ps1 -Action Stop -Component Authority -Name shared
.\scripts\windows-session.ps1 -Action Remove -Component Authority -Name shared
```

Use absolute local Windows paths for the placeholders, and `-TlsCert`/`-TlsKey`
as a pair for HTTPS. Account-password entry stays local and is stored by Task
Scheduler, with the same batch-logon prerequisites as the mount launcher.
Authority startup uses a separate task/state namespace. Stop/removal retains
the database, token, certificates, configuration and logs.

## Acceptance criteria

1. Missing directory/database/lock, local-client state, wrong volume/policy,
   corrupt root and future/foreign schemas fail without new namespace or
   migration/recovery effects. Existing dirty bytes and exact records survive.
2. Matching known old schemas migrate atomically without rewriting existing
   operation identities/results. Actual WAL-only committed state is observed.
3. Exclusive ownership survives duplicate starts and owner termination; a
   killed coordinator resumes exact retained operations and the original volume.
4. Real CLI checks fail before binding/listening for mismatches and malformed
   private inputs. Both HTTP and HTTPS use the pinned restart path.
5. Linux disposable desktop/login hooks survive terminal loss, reuse ownership,
   recover a killed child and remove only their registration; no systemd/root.
6. Windows native non-admin scheduled startup survives independent logons,
   duplicates, child restart and controlled removal with private ACLs.
7. Both native managed drives use the supervised coordinator after initial
   launcher loss and coordinator recovery, preserving Unicode bytes, uncertain
   IDs, dirty data and conflicts. Physical reboot/PAM/SSH and Windows10/11 remain
   separate target-machine acceptance, not inferred from process restarts.
8. Full local gate and terminal exact-source Linux/Windows/book, native,
   startup and strict21/14 reference gates pass before private publication.

The Rust restart/identity work is the prerequisite. Script/supervisor and paired
native acceptance must also pass before this phase is described as implemented
coordinator startup. Backup/failover, real multi-host partition tests, live
managed Wasabi acceptance and large-file support remain subsequent requirements.

## Local implementation checkpoint

The full local gate passed on 2026-10-17 (`/tmp/s3drive-authority-startup-gate3.log`):
265 state tests, ten HTTP, five TLS, 17 SDK, two new restart CLI cases plus the
existing setup/shared-engine suites, formatting, Clippy, builds and 56-document
link/mdBook checks. Eight focused restart-state cases cover missing metadata,
wrong/client identity, corrupt root, an actual schema 8 upgrade and a database
replacement after preflight. The replacement is checked again within the same
transaction before migration, preventing initialization of an empty replacement
or migration of another volume. Earlier gate attempts stopped at test-helper
type-complexity and nested-if lints; both were fixed before the complete gate.

The two real CLI HTTP/TLS tests also prove that an offline check succeeds while
its configured port is occupied, mismatches refuse before readiness/listening,
and a killed process with an unread metadata reply restarts under the same ID
over HTTPS without repeating the effect.

The separate Linux supervisor acceptance passed locally in both numeric
loopback HTTP and Rust HTTPS/private-CA modes, using real `gio` and
tmux, disposable XDG/profile paths and a real Rust coordinator. It proves
terminal loss, duplicate starts, killed-child/WAL recovery, exact operation
replay, Unicode namespace names, missing-database refusal and removal preserving
the original metadata/token/profile. The explicit command is:

```text
cargo test -p s3drive-cli --test authority_restart supervised_linux_coordinator_http_and_tls_with_private_ca --locked -- --ignored --nocapture
```

The ignored wrapper was added after the full gate; its focused run, formatting
and all-target Clippy passed. The final gate must run again after remaining
platform/native work. This acceptance does not mount FUSE, use S3, install
anything into the user's real login profile, or prove physical reboot behavior.

The Windows component and its new native acceptance harness pass PowerShell
AST parsing locally. The Rust subprocess worker also passed a portable live
TLS/SQLite crash-and-replay test, including private-input creation, Unicode
namespace contents and exact receipt recovery with no extra sequence increment.
This validates the client fixture, not Task Scheduler on Windows.

A separate hosted test now uses the same randomly created ordinary account,
password pipe, batch-logon right and profile cleanup as the existing Mount gate:

```text
cargo test -p s3drive-cli --test authority_restart supervised_windows_coordinator_tls_independent_logons_and_recovery --locked -- --ignored --nocapture --exact
```

The candidate checks private TLS inputs and namespace ACLs, independent
AuthenticationIds, duplicate starts, wrong-volume registration refusal and
Task Scheduler settings. It kills only an opened, image/parent/pin-verified
fixture child and waits for the scheduler's configured retry without issuing
Start. Authenticated Rust clients then recover the original volume and replay
the exact saved operation receipt before controlled removal. Passwords stay
in memory/pipes; neither system trust stores nor real user registrations change.
The actual hosted outcome is pending; existing Mount-only startup results do
not establish Authority behavior. This test covers acknowledged metadata receipt
recovery. Paired native drives, uncertain flush IDs and conflicts still require
the separate acceptance described below.

## Next native acceptance work

Extend `scripts/test-windows-session.ps1` and its caller in
`crates/s3drive-cli/tests/windows_mount.rs`, reusing the existing randomly named
non-admin account, credential pipe, explicit batch-logon right and controlled
cleanup. Cover a separate Authority registration with private ephemeral TLS
files, original volume/port, independent same-account metadata clients, duplicate
starts, actual Task Scheduler child-failure retry and retained exact receipts.
Do not install a certificate into the workstation trust store or disable TLS
verification for a self-signed fixture; use the Rust client's explicit private CA.

Then pair both real native drives with those supervisors. Reuse the owned SDK
fixture and `writes::setup` policy/plan/report, copied as fixture inputs without
copying credentials. Source alias/profile/physical scope must still match that
report, and the disposable account resolves its own dummy profile. A Rust test
worker can reuse the existing production `Publisher`/storage helpers in
`tests/support/managed.rs` to seed Unicode/binary/empty data over HTTPS and emit
no secret contents. Configure a fresh managed journal through the setup CLI.

The mounted writer must retain local dirty bytes while the coordinator is
killed, reject or retain an uncertain flush, and finish the original work after
the supervisor restarts the same database. Verify exact S3 bytes/PUT counts,
unchanged original IDs and explicit rival-write conflicts. Stopping a mount
must preserve the independent shared coordinator. On Linux use temporary
desktop/profile hooks for both components and destroy their launching tmux
terminal; keep the current workstation's NoNewPrivs restriction intact and use
hosted FUSE acceptance for the paired drive test. Native Windows testing remains
on the hosted Server2025 VM; neither syntax checks nor portable tests replace it.


## Paired Linux native candidate

A new `linux_mount::actual_managed_supervised_coordinator_and_drive_recovery`
case runs the real FUSE adapter, managed publisher and SDK fixture with **both**
Linux login supervisors. It bootstraps and seeds a private-CA HTTPS authority,
stops the original foreground process, and registers the original volume/port.
Disposable desktop and shell hooks launch the coordinator and writable drive;
the launching tmux server exits and duplicate starts must keep both owners.

The candidate verifies actual Unicode/binary/empty and second-bucket reads,
then deliberately loses an immutable PUT response and hides observation. With
the native descriptor still open, it kills the exact pidfd-verified coordinator,
requires the offline flush to fail without changing the retained operation ID,
intent hash or physical key, and verifies local dirty bytes. Supervisor recovery
must use the same volume, settle that exact attempt, preserve the logical
publication request/reservation and never repeat the PUT. The fixture compares
the actual immutable S3 object body with the expected binary bytes.
A drive crash/restart checks the persisted bytes; a rival publisher then wins
while local edits remain dirty. A second coordinator restart must preserve that
explicit conflict and all local bytes, including subsequent journal reopening.
Drive removal must leave the independent coordinator online; final removal
preserves metadata, config, token, credential fixture and original profile mode.

Process control runs in a Python fixture child started **before** any native
file descriptors open. Later commands use pipes, avoiding a Unix fork in the
native test that could itself trigger FUSE flushes through inherited descriptors.
Only read-only SQLite inspection observes the active fixture's exact attempt
identity. Production ownership, migration and recovery paths are unchanged.

Local all-target Clippy and Python syntax checks passed. Native outcome remains
pending on hosted Linux: this workstation still has `NoNewPrivs: 1`, which must
not be bypassed. Neither this candidate nor the standalone Windows coordinator
test replaces the required paired native Windows acceptance.
