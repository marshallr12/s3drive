# Coordinator startup and pinned restarts

Design/research date: **2026-10-17**, America/New_York. Current fixture follow-up to
`7962058` on `codex/authority-startup`. **Phase acceptance is incomplete.**
The accepted main baseline is `712882e`,
[CI39079281204](https://github.com/marshallr12/s3drive/actions/runs/39079281204).
No startup registration in the workstation user's real account or hosted
metadata deployment has been installed. Tests use disposable registrations.

The designated coordinator host runs the shared Rust service
`s3drive authority-serve` and owns authoritative SQLite metadata. Other machines
run their local drives and use that same service; they must not clone its
SQLite database or start independent authorities for the same volume. Clients
send file bytes directly to S3. A co-located workstation can start both
components at sign-in; a separately hosted coordinator can use its host's
process manager. A managed mount cannot reconstruct the namespace from S3
payloads if its authoritative database is missing.

![Separate per-account supervisors start the pinned Rust coordinator and local drive. The coordinator owns its existing SQLite namespace; the drive keeps its own journal and sends file bytes directly to S3.](images/authority-startup.svg)

## Current evidence

| Requirement or check | Current result | Evidence and limit |
| --- | --- | --- |
| Existing-only restart, WAL, migration and exact receipts | Passed local Rust/CLI tests | Eight state cases; real HTTP/TLS CLI cases; wrong/missing/client identities refuse before startup |
| Linux coordinator desktop/login supervision | Passed local and hosted HTTP/HTTPS checks | Real gio/tmux, duplicate starts, killed child, unread reply recovery, missing database refusal; no systemd |
| Paired Linux native drive and coordinator | Passed on hosted Linux at `e48cb09` and current `ce784e4` | [Current job117555117517](https://github.com/marshallr12/s3drive/actions/runs/39086854659/job/117555117517): all eight native cases, exact uncertain identities/no repeated PUT, dirty rival conflict and independent removal |
| Windows Authority install/TLS/separate logons/duplicates | Observed working at `56a2abe` | [Job117548875636](https://github.com/marshallr12/s3drive/actions/runs/39084754318/job/117548875636); the same test then failed killed-child recovery, so the full gate did not pass |
| Corrected Windows child retry | Implemented; native recovery still unverified | `ce784e4` passed all four policy cases but its runtime test failed a metadata read before the child kill |
| Paired Windows managed drive and coordinator | Implemented; native result pending | Separate scheduled tasks, actual FileStream uncertainty/conflict workflow; portable CLI/SDK/TLS preparation passed |
| Full local Rust/build/book gate | Passed for the diagnostic follow-up | 265 state, ten HTTP, six TLS,17 SDK, five CLI restart/worker and seven managed cases; format/Clippy/build and book links/build/tests passed |
| Final exact-source platform acceptance | Failed diagnostic parent; fixture follow-up under verification | [Windows117562340718](https://github.com/marshallr12/s3drive/actions/runs/39089283961/job/117562340718) failed the TLS fault matrix before startup/pairing. Original AuthorityProbe failure remains unexplained. Require all gates and strict21/14 artifacts before promotion |

Both Linux job logs record the full pair; the current `ce784e4` run passed at
01:39:46 UTC on2026-10-18 (2026-10-17 America/New_York). They record real FUSE reads of Unicode, binary,
empty and second-store files; tmux launcher loss; original-volume recovery;
unchanged physical/logical uncertain IDs without another PUT; persisted binary
bytes; retained dirty conflict after a rival publication and coordinator restart;
journal reopening; and removal of the drive while the coordinator remains live.
Its final removal preserves fixture metadata, token, config and profile mode.
That result is hosted fixture evidence, not physical reboot/PAM/SSH, managed
Wasabi write acceptance, real multi-host partitions, or Windows10/11 proof.

## Transport failure investigation

The `ce784e4` Windows run failed an authenticated Info read after operation
lookup/exact replay, before the intentional coordinator kill. The task and
supervisor remained running. Its unit `WireError` discarded the cause; a
5.41-second worker duration alone does not prove a particular timeout, TLS,
HTTP or SQLite defect. Earlier native rename failures also remain unexplained.

The diagnostic follow-up retains only enums/numbers: configuration or invalid
input, connect/request/response-body phase, timeout flag, stable socket kind,
HTTP framing predicates, HTTP status, oversized response or invalid reply.
Display/Debug and Error sources cannot disclose endpoint, token, certificate,
request/response body or dependency error text. The client keeps its original
two-second connect/ten-second request deadlines, enabled TLS verification,
redirect refusal and disabled automatic retries. Mutation failures remain
unknown and require inspection of their original durable operation.

The Rust TLS listener logs at most sixteen connection failures using fixed categories and one
suppression notice per listener lifetime. It distinguishes capacity refusal,
handshake rejection/timeout, HTTP timeout/incompleteness/transport failure and
connection lifetime expiry. It never formats peer addresses or dependency
errors. A real CLI test sends24 malformed handshakes, checks that output stops
at the budget without disclosing private input, and verifies authenticated
requests still work afterward. Suppression is diagnostic only; it does not
change acceptance, deadlines or recovery.

Real TLS fault cases cover status503, an oversized advertised body, invalid
JSON, truncated body, no response headers and a stalled response body; tests
also check redaction and absence of a second request. Wrong trust is classified
at connect and bad bearer authorization as status401. These are local wire
checks, not a corrected Windows runtime acceptance claim. The Windows worker
now reports safe completed stages and the last typed readiness failure.

The diagnostic run at `7962058`,
[CI39089283961](https://github.com/marshallr12/s3drive/actions/runs/39089283961),
passed book and the complete Linux native/startup suite. Windows passed its
early native lifecycle, then failed the new TLS fault matrix: the truncated-body
case got a request timeout instead of a body error. Another fixture task reported
header EOF, without a case label. The Windows startup/pairing checks were skipped.
This result does not establish the cause of the original coordinator probe error.

The test follow-up prepares all synchronous certificate/client/acceptor factories
on blocking workers before starting any request deadlines. All six network cases
still run concurrently with their original expectations and deadlines. Per-case
progress, header byte counts, setup/request timing and joined fixture completion
now distinguish a fault that was established from a request that stalled earlier.
It does not increase production timeouts, add retries or weaken TLS checks.
Whether blocking setup explains the Windows failure remains unverified.

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

The offline `authority-check` command checks private startup inputs and
existing identity without starting a listener, migrating schemas or recovering
attempts. It requires exclusive access; stop the foreground bootstrap process
before checking/installing supervision. This does not verify S3/provider access.

## Supervision contract

The per-account launchers have an explicit Authority component, separate
registration namespace and expected-volume settings. Mount registrations keep
their v1 format; Authority uses v2. Manifests contain paths, IDs and listen
settings, never token, TLS key or S3 credential contents. Every supervised
coordinator launch uses the existing-only Rust path and cannot fall back to
bootstrap.

Linux uses a detached process/session, private lock, XDG desktop entry and
optional explicit shell-profile hook, including MX without systemd. Windows
uses an account-owned password-backed Task Scheduler/batch-logon process. Its
password and batch-logon prerequisites are described in
[startup and sessions](startup-and-sessions.md). Real-user installation remains
an operator action; the acceptance harness provisions only a new disposable
ordinary account.

Each supervisor owns at most one child at a time and keeps its lock through up
to four attempts, ten seconds apart. Stop interrupts backoff; normal child exit
and explicit stop terminate supervision. The Windows extension remains under
native acceptance. Stop signals/closes stdin only on the supervisor's own child
object. Runtime PID files are operational hints, not authorization to signal a
PID from a previous run. The native fixture separately verifies a Linux pidfd or
Windows image/parent/pin before injected termination.

Use a stable explicit listen port. Numeric loopback HTTP remains supported;
nonloopback listeners require TLS. Original private token/key/cert rules and
client hostname/CA verification remain unchanged. Coordinator and mount
registrations are independent: drive removal must leave a shared coordinator
available to other clients. Duplicate starts reuse ownership or refuse the
existing database owner. No retry may create a replacement volume, fresh
operation ID, or unconditional provider overwrite.

Status reports supervisor ownership separately from authenticated metadata
health and provider access. A lock, PID, mounted path or readiness line alone
proves neither. The intended terminal-loss behavior is availability to later
same-account desktop/SSH/tmux clients; actual workstation sign-in/reboot/PAM/SSH
acceptance remains separate from process-restart tests.

The foreground Rust server remains usable with a separately provisioned host
process manager. This phase does not choose a hosting provider, enable Turso,
clone authority databases, add automatic failover, or claim leader election.
The user keeps SQLite-host and optional Turso deployment choices open.

## Candidate command interface

Paired Linux supervision passed hosted FUSE acceptance at `e48cb09`.
Corrected Windows recovery and paired native acceptance are still pending. Use the volume ID and naming policy printed by the
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

## Validation workflows

Run the full local gate from the repository:

```text
bash scripts/gate.sh
```

The eight focused restart-state tests cover missing paths/files, wrong identity
without migration, local-client/uninitialized state, corrupt/future/foreign
roots/schemas, actual schema8 migration, database replacement after preflight,
and killed-owner state whose identity/receipt is provably only in WAL. The real
CLI cases also check an occupied listen port during offline inspection,
refusal before readiness/listening, and exact unread-reply recovery across an
HTTP-to-HTTPS pinned restart.

Separate native commands are required; portable cases cannot replace them:

```text
cargo test -p s3drive-cli --test authority_restart supervised_linux_coordinator_http_and_tls_with_private_ca --locked -- --ignored --nocapture
cargo test -p s3drive-cli --test linux_mount actual_managed_supervised_coordinator_and_drive_recovery --locked -- --ignored --nocapture --exact
cargo test -p s3drive-cli --test authority_restart supervised_windows_coordinator_tls_independent_logons_and_recovery --locked -- --ignored --nocapture --exact
cargo test -p s3drive-cli --test authority_restart supervised_windows_managed_drive_and_coordinator_recovery --locked -- --ignored --nocapture --exact
```

Linux uses disposable XDG/profile paths, gio and a tmux login without inherited
desktop XDG variables. The native paired controller starts before any FUSE file
descriptors open; later control uses pipes. Forking in the native parent with
those descriptors open can itself trigger flushes, so it would distort this
acceptance. Read-only SQLite inspection compares exact active attempt identity;
production ownership/migration rules are unchanged. The current workstation has
`NoNewPrivs: 1`; it must not be bypassed to run FUSE. Hosted native Linux supplies
that acceptance.

Windows tests reuse the randomly created ordinary account, credential pipe,
explicit batch-logon right and controlled cleanup. The account creates its own
private TLS/SQLite files and dummy AWS profile, rebases only local paths in the
copied source policy/plan/report, seeds both stores through the production
publisher, and generates its writable config with the real CLI. Source
credentials are never copied. Fixture endpoints must be numeric loopback HTTP;
no hosted S3 provider is contacted and system certificate trust is not changed.

The Windows pair tests separate AuthenticationIds and duplicate startup. A live
FileStream keeps dirty bytes through a lost PUT response and an exact-owned
coordinator-child kill. The same scheduled supervisor must produce a new child,
recover the original volume/receipts and settle the exact physical/logical
request identities without another PUT. A rival payload must yield a failed
flush while retaining local bytes. Drive removal must leave authenticated
coordinator access working, and journal reopening must find head!=settled and
read the actual staged conflict bytes through the production journal reader.
The `logical_conflicts` table is a later explicit retention acknowledgment; the
fixture must not fabricate that acknowledgment automatically.

A small command/counter file protocol controls only the owned Rust S3 fixture's
reply/observation faults. Cleanup tracks both mountpoints and every owned task
before account disposal, and removes private fixture metadata as its owner.
Windows testing is hosted Server2025/WinFsp2.1.25156.ddca7bd, not this workstation
or accepted Windows10/11 behavior. A disposable-VM-only, already documented
profile sharing violation may defer profile deletion to VM destruction only
after all acceptance assertions pass and dummy credentials are removed.

Portable preparation uses real CLI/SDK/private-CA TLS/SQLite processes, killed
coordinator recovery, exact request snapshots and durable conflicting bytes:

```text
cargo test -p s3drive-cli --test authority_restart paired_windows_fixture_sdk_tls_recovery_and_conflict_preparation --locked
pwsh -NoProfile -NonInteractive -File scripts/test-windows-supervisor-policy.ps1
```

The latter extracts the actual supervisor loop and uses real child exit codes
for normal termination, recovery, four-attempt exhaustion, exclusive ownership
and Stop during backoff. Neither command proves Task Scheduler, ACL, WinFsp or
reboot behavior. CI runs the Windows policy/Authority/pair checks before broader
native regressions for independent evidence; all previous gates remain required.

## Measured failures and corrections

| Source and job | Observed failure | Correction or current disposition |
| --- | --- | --- |
| `d67a17d`, [Linux117547531601](https://github.com/marshallr12/s3drive/actions/runs/39084297874/job/117547531601) | Writable CLI bootstrap rejected exact naming before startup | Portable naming and guarded `MoveEntry` producer rename; portable preparation passed |
| `56a2abe`, [Linux117548875680](https://github.com/marshallr12/s3drive/actions/runs/39084754318/job/117548875680) | Actual mount existed but the Mount component did not write runtime.json | Private atomic hints now cover both components; controller waits for records and always attempts failure cleanup; later paired FUSE acceptance passed at `e48cb09` |
| `8ce3ddc`, [Windows117546161016](https://github.com/marshallr12/s3drive/actions/runs/39083840521/job/117546161016) | Existing cross-store metadata rename returned native1117 | Unresolved; typed callback and safe namespace request/outcome/node diagnostics added; no retries or weakened assertions |
| `d67a17d`, [Windows117547531749](https://github.com/marshallr12/s3drive/actions/runs/39084297874/job/117547531749) | Existing dirty-source replacement returned native1117, with authority transport failure logged | Unresolved; same diagnostics; the later `56a2abe` broad native suite passed but does not explain away these failures |
| `56a2abe`, [Windows117548875636](https://github.com/marshallr12/s3drive/actions/runs/39084754318/job/117548875636) | After successful install/TLS/logons/duplicates, killed-child recovery failed its100-second deadline | Supervisor now owns bounded child retries; corrected native result pending |

The Windows child-death log showed supervisor exit `-1`, followed by Scheduler
events201/102 recording completion with return code4294967295. No replacement
child appeared during the deadline. A subsequent profile cleanup sharing
violation was also reported; the original recovery failure remains visible.
This is one hosted Server2025 observation, not a universal Scheduler rule.

Microsoft documents restart count/interval settings and a one-minute minimum
Scheduler interval. The settings alone did not establish recovery for the
measured child-death case.
[RestartOnFailure schema](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-restartonfailure-settingstype-element),
[RestartInterval](https://learn.microsoft.com/en-us/windows/win32/taskschd/tasksettings-restartinterval),
checked2026-10-17. The `ce784e4` correction instead keeps one supervisor/lock
through four attempts and interruptible ten-second delays, always using the same
pinned command/private inputs. Scheduler restart properties remain secondary,
unverified launcher policy. Native acceptance must demonstrate recovery under
that same supervisor without issuing Start.
