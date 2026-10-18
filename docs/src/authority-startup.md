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
fixture child and waits for its scheduled supervisor's child retry without issuing
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
starts, actual child-failure retry under its scheduled supervisor and retained exact receipts.
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


## Remaining paired Windows implementation plan

Proposal: reuse the same disposable-account harness, with a separate writable
managed mount registration alongside its Authority registration. Prepare both
source policy/plan/report fixtures against the owned loopback S3 server, copy
those inputs without credentials, and rebase only local path fields inside the
ordinary account. That account must create its own private TLS/SQLite files,
bootstrap and pin the service, register stores through the production CLI,
publish the Unicode/binary/empty seed, and generate a separate managed journal.
Secondary-logon Rust clients must receive explicit paths to the fixture account's
dummy profile; the scheduled daemon must independently load that same account.

Use the existing credential pipe for each registration. Track both task names
and exact actions for failure cleanup; remove owner-private metadata as its
owner before deleting the disposable account. A metadata-only receipt check or
an empty mounted namespace cannot satisfy the paired-drive criterion.

A native FileStream must keep dirty bytes through an exact-owned-child
coordinator crash and real supervised child retry. Inject an owned fixture's lost PUT
reply, inspect the original operation/publication identities, restore observation
and require exact recovery without another PUT. A rival publisher must produce
an explicit retained conflict, with dirty bytes surviving mount removal and
journal reopening. Stop/remove the drive while proving authenticated coordinator
access still works, then remove only the authority registration and verify its
original database remains. Keep provider credentials, system certificate stores,
real login startup and hosting provisioning outside this disposable acceptance.


The first paired native run at `d67a17d` failed before startup registration:
[Linux job117547531601](https://github.com/marshallr12/s3drive/actions/runs/39084297874/job/117547531601)
reported that writable CLI bootstrap requires `portable-collision-v1`. The
fixture now selects that policy and uses `MoveEntry` with current node/parent
revision guards for the rival rename. A new portable preparation test exercises
that exact HTTPS bootstrap and guarded rename; it passed locally. This catches
setup regressions without claiming OS-mount acceptance. Source inspection also
corrected the actual-object assertion: the journal key is relative, so fixture
lookup must prepend the configured bucket and source prefix. Native rerun is
still required; the first failed run proves no paired mount or recovery success.


## Paired Windows candidate implementation

Implementation checkpoint, 2026-10-17: the disposable Windows harness now has a
separate `managed` mount task alongside its pinned Authority task. The ordinary
account rebases copied source policy/report/plan paths, creates its own dummy
profile and private TLS inputs, seeds both stores with the production publisher,
and generates a writable managed config through the CLI. Root fixture endpoints
must be numeric loopback HTTP; no hosted provider is contacted.

Independent secondary logons check Unicode/binary/empty/second-store data and
duplicate startup ownership. A native FileStream deliberately receives a lost
PUT response, freezes physical and logical request identities, keeps dirty bytes
through a verified coordinator-child kill, and waits for the same scheduled supervisor to retry its child.
Recovery must settle those same identities without another PUT. A rival payload
must then produce a failed flush and keep local edits. Removing the drive must
leave authenticated coordinator access working; reopening the private journal
must still find a dirty head distinct from its settled generation and read the
actual staged `dirty conflict` bytes through the production journal reader.
The explicit `logical_conflicts` table represents a later operator retention
acknowledgment, so this test must not fabricate that acknowledgment automatically.

A small file protocol controls only the Rust-owned S3 fixture's reply/observation
faults and counts. It contains command IDs and counters, not credentials. Profile
credentials are known disposable dummy values created inside the new account;
source credentials are never copied. Cleanup tracks both mountpoints and all
owned task actions before disposing of the account. The new Windows pair is an
explicit CI step before broad native regressions; every previous gate remains.

The portable `authority_restart::paired_windows_fixture_sdk_tls_recovery_and_conflict_preparation`
test passed locally with the real CLI, SDK, private-CA TLS, SQLite owner/journal
and killed coordinator. It catches fixture setup/replay/dirty-byte errors without
substituting for native WinFsp or Task Scheduler acceptance. PowerShell AST,
including the extracted embedded account worker, passed. Native outcome remains
pending; the full phase is not accepted.

## Further native findings

At `56a2abe`, [Linux job117548875680](https://github.com/marshallr12/s3drive/actions/runs/39084754318/job/117548875680)
reached an actual mounted path but failed because the Mount component did not
write the `runtime.json` expected by the paired controller. It now writes private
atomic PID hints for both components. These hints never authorize production
PID-based termination; supervisors still signal only their own Popen child, and
the fixture verifies a pidfd's actual argv/pin before injected termination. The
controller also waits for both records and detaches its own mount during failure
cleanup even when a record is missing. The standalone HTTP and HTTPS coordinator
supervision test passed locally after this change; paired FUSE rerun is pending.

Two older Windows jobs failed existing managed rename acceptance before reaching
the new coordinator step: [job117546161016](https://github.com/marshallr12/s3drive/actions/runs/39083840521/job/117546161016)
at a cross-store metadata move, and [job117547531749](https://github.com/marshallr12/s3drive/actions/runs/39084297874/job/117547531749)
at dirty-source replacement. Both returned native error1117; the second also
logged an authority transport failure. These are unresolved failures, not flaky
passes. Rename callbacks now log the existing typed filesystem error, and the
owned native fixture captures relevant namespace requests/outcomes and node
metadata on failure. No retries, fresh IDs or weaker assertions were added. The
new authority tests run earlier to obtain independent evidence while all native
regressions remain mandatory before publication.


## Measured Windows child-retry correction

On 2026-10-17, the first standalone native coordinator runtime at
[Windows job117548875636](https://github.com/marshallr12/s3drive/actions/runs/39084754318/job/117548875636)
passed install, private TLS access, independent AuthenticationIds and duplicate
starts. After the exact fixture child was killed, its supervisor logged exit
`-1`; Task Scheduler events201/102 recorded the action/task as completed with
return code4294967295. No replacement child appeared during the client's
100-second recovery deadline. The test failed. A later profile sharing-violation
cleanup error also appeared; the raw original recovery failure remains evidence.
This is an observation on that hosted Server2025 VM, not a claim about every
Scheduler configuration or Windows version.

Microsoft documents the restart count/interval settings, including a one-minute
minimum Scheduler interval. Those settings alone did not prove recovery for
this measured child-death case.
[RestartOnFailure schema](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-restartonfailure-settingstype-element),
[RestartInterval](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-restartinterval),
checked2026-10-17. The implementation now keeps one scheduled supervisor and
exclusive lock through up to four child attempts, ten seconds apart. It invokes
the same pinned command and private inputs on each attempt; it never initializes
missing metadata or creates a fresh operation identity. Stop interrupts backoff
and closes only the owned child's stdin. Normal exit and explicit stop do not
retry. Scheduler restart settings remain secondary, unverified launcher policy.

The native recovery assertion now requires a fresh child under the **same**
supervisor, plus original volume and exact receipt recovery, without calling
Start. The portable policy test extracts the actual loop and uses real child
exit codes to check success, failure recovery, bounded exhaustion, exclusive
ownership and Stop during backoff. It does not establish native Task Scheduler,
ACL, WinFsp or reboot behavior. Native corrected acceptance is still required.
