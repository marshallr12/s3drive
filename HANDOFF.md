# S3Drive handoff

Updated 2026-10-27. Event-bound manifest authority is accepted at schema46;
the overall project remains incomplete. Read this, AGENTS.md and
docs/src/implementation-plan.md before continuing.

## Current accepted baseline (2026-10-27)

Main `74e1540bd765fd8f2a61000d073e15780f455f31` independently passed
[run40270589324](https://github.com/marshallr12/s3drive/actions/runs/40270589324):
all nine complete job logs, 810 exact source files and six verified artifacts;
Linux 1143 / Windows 1135 workspace tests and all original/extended gates.
Windows shared/debug/release 300-save times were 684.33/1093.25/348.67 seconds;
the unchanged 1200-second debug limit retained 106.75 seconds of margin.
The lifecycle/recovery cleanup remains fresh-only opt-in `component-retention-v1`;
existing defaults/profiles and protection of dirty, uncertain and pinned work are
unchanged. Hosted Git CLI application I/O and relocated package/startup acceptance
are complete, not Office GUI, physical reboot or live Wasabi acceptance.

The owner selected MIT licensing on 2026-10-27. The new
[Windows MSI](docs/src/windows-installer.md) and its additional CI gate are under
validation, not yet accepted or publicly released. Third-party notices remain
under their own terms. Earlier candidate/pending-main statements below are dated
historical checkpoints, superseded by this independent main result, not requests
to rerun completed phases. Real hardware/provider acceptance remains incomplete.

## Historical hosted item4 candidate checkpoint (2026-10-26)

Main `f2410ec8b25609b34926b87b5474d7ad3671c647` independently passed
[run40241620855](https://github.com/marshallr12/s3drive/actions/runs/40241620855):
all nine complete job logs, 808 exact source files, six artifact digests/reports,
Linux1141/Windows1133 workspace tests, six original300 workloads and all800
component cycles/400 integrity checks. Windows debug300 was1019.67s under1200s.
The candidate-only checkpoint below is historical; the separate main proof passed.

The user requested item4 testing through GitHub Actions on2026-10-26. Hosted
CLI-application and relocated-package/startup gates now have **candidate acceptance**.
They add actual Git checkout/replacement/rename/delete and reader sharing through
FUSE/WinFsp, then interrupted mount-process recovery; two deterministic packages
of one real binary; offline relocated durable initialization; and the shipped
startup scripts with the relocated binary. No product behavior/default changes.
No GUI application, physical reboot, cross-machine reproducible compiler output,
live Wasabi, public binary distribution or licensing acceptance is implied.
Provider headroom/access and actual hardware/GUI/reboot remain blocked separately.

Exact candidate `34e42a41b28512a045e5e813cc8eaf071b363edd`, tree
`4555e7493fa7b6e6735fce8e259d9ef45e8222d0`, passed
[run40258999976](https://github.com/marshallr12/s3drive/actions/runs/40258999976):
all nine complete logs, 810 exact source files, six artifact digests/reports,
Linux1143/Windows1135 workspace tests and all original and extended gates.
Original shared/debug/release300 times were360.00/429.37/99.90s on Linux and
689.96/1096.64/341.19s on Windows; debug retains103.36s under unchanged1200s.
Both OSes passed400 component cycles/200 cold integrity checks, with all nine
permanent table maxima1. Long1000-save/large64KiB workloads were474.55/284.59s
on Linux and1245.61/610.74s on Windows. Product runtime/defaults are unchanged.
This documentation-only update preserves accepted runtime/tests/CI. The merge
commit still requires its own independent complete main verification.

Windows relocated startup exercised the existing disposable-runner-only profile
cleanup policy: after functional/security checks and unmount, an OS-held profile
sharing violation may defer profile deletion to VM destruction. Fixture-owned
process absence and dummy-credential removal remain mandatory; other cleanup
errors fail. This policy is not authorized for user hardware. See the
[full hosted scope and rejected fixture history](docs/src/beta-completion.md).

## Candidate code accepted; real-use beta remains incomplete (2026-10-25)

Exact source `59a10fe36b98d85d200bd24f7a51c79147e7a200`, tree
`7c8bab19508d6b0ff1ebf55e59fdec2c1892ec7d`, passed independent review of
[run40229890964](https://github.com/marshallr12/s3drive/actions/runs/40229890964): all nine complete job logs, 808 exact source files,
six artifact digests, all four schema46–50 compatibility reports and 35 typed
Windows reference observations. Linux1141/Windows1133 parent-harness
workspace tests passed. The two warm-cache schema30 regressions are corrected;
the original migration tests and all earlier acceptance gates remain unchanged.

Original shared/debug/release300 workloads passed in 610.45/758.85/160.83s on Linux and
707.23/1102.82/354.93s on Windows, with every remount segment and final cold read verified.
Windows debug300 retains 97.18s below its unchanged1200s limit;
this is limited headroom, not a hardware-independent performance guarantee.
Additional release1000-save/100-remount and four64KiB-file300-save workloads
passed on both OS. Each OS also passed four100-cycle component runs:800 combined
cycles,400 mounts and400 closed SQLite integrity/FK checks. Component permanent
row maxima were1, DB/WAL maxima778240/4161232 bytes and physical payload320 bytes.
Diagnostic instructions were21,363,286,243,16.46% below the accepted-main baseline.

Lifecycle and recovery cleanup are candidate-accepted for the **fresh opt-in
`component-retention-v1`** profile. Unrelated open/dirty/read-protected work,
uncertain outcomes and unfinished recovery remain protected; safe closed groups
are reclaimed without fabricating Close. Defaults and the existing linked/owned
profiles are unchanged. No remote object GC or alpha-store migration is added.

Previous accepted baseline main is `3fd2f05e6d833d45cc1bfb70eabe6aa754524402`
([run40157712514](https://github.com/marshallr12/s3drive/actions/runs/40157712514)).
Candidate evidence must not be substituted for the separate exact merge/main
verification. A documentation-only acceptance update does not change runtime,
tests or CI. The merge commit requires its own complete source/log/artifact review.

Real Wasabi headroom/provider acceptance, physical MiniAir driver/native-application
UI/startup installation/reboot and public release/licensing remain incomplete.
Private developer packaging and relocated CLI startup passed hosted Linux/Windows;
that is not installation or physical/provider acceptance. Private validation
archives, credentials and developer bundles remain unpublished.

## Historical beta implementation checkpoints (2026-10-25)

Accepted main is now `3fd2f05e6d833d45cc1bfb70eabe6aa754524402`, independently
verified in [run40157712514](https://github.com/marshallr12/s3drive/actions/runs/40157712514).
All seven jobs and 802 exact source blobs were reviewed; Linux1103/Windows1098
workspace passes, six complete300-save workloads and six artifact digests passed.
Windows debug mounted300 took1131.24s under the unchanged1200s limit. The older
pending-main statements below are historical, superseded by this result.

User requested all four remaining milestones: lifecycle cleanup, recovery
cleanup, performance/longer soak and real-use beta/packaging. Work is on
`codex/lifecycle-retention-completion` from the accepted main. Existing native
owned/unlinked lifecycle is already implemented; do not rebuild it or equate it
with retention. Current linked-retention creation/namespace refusals remain.

The first safety correction preserves the observer session of a persisted
recovery even if it owns no open. Both session admission and retirement pruning
must retain that session until its recovery group can be safely reclaimed.
New pending/finished recovery third-owner tests compile; local execution is
blocked by the unchanged tmpfs durability guard. Hosted acceptance is pending.
No old-format migration, provider writes or user-machine changes are included.
See [the current completion plan](docs/src/beta-completion.md). Private evidence
remains in the sibling validation directory, never in public commits.

Current unaccepted linked/recovery/performance branch head is
`cf2ab03fba6a32dc8fae58f699e1c795efca7dfb`, running expanded suite
[40206107073](https://github.com/marshallr12/s3drive/actions/runs/40206107073).
Changes include narrow no-effect recovery group cleanup, SQL statement caching
with unchanged validation, additional1000-save and four64KiB-file soaks, and
private deterministic test packaging. General creation/namespace and recovered
publisher reclamation are still unfinished. MiniAir readiness found no WinFsp;
installation/native application/reboot checks and actual Wasabi headroom remain
blocked. See the completion plan for precise scope. No new main acceptance or
public release is claimed.

Experimental `codex/quiescent-lifecycle-proof` now implements fresh closed
connected-component cleanup and exposes development-only `component-retention-v1`.
It preserves unrelated dirty/open/read-protected work, expires namespace/creator/
source identities and retains permanent owner/session/generation counters.
Exact160113a passed50 focused state and four shared lifecycle/recovery cases;
22480e91 passed51 state and five shared cases, but both actual native platforms
failed a final reopened read. The correction306ed181 carries an independent
reader lease through source dispatch until original-open activation, with a
paused-admission/previous-Close regression. Its actual/full gates are pending.
See the [design and evidence boundaries](docs/src/quiescent-lifecycle-retention.md).
Do not merge from focused or shared passes alone. Main remains unchanged.

On cf2ab03, the full Linux job passed1110 workspace tests and its three300-save
runs; both added1000-save/four64KiB-file300 runs passed with all cold segments.
Windows original debug300 passed its workflow step in1180s/1200, only20s margin;
full Windows verification is pending. This is not robust wall-time headroom or
physical-hardware acceptance. Private test packaging works on hosted systems,
not an installer or public release. Provider/headroom, MiniAir driver/native UI,
startup installation and reboot evidence remain externally blocked.

Latest component diagnostic source `ba665c1a6ffcd4daf9a54906e9139dccfbe1a878`
passed53 focused state cases, the dispatch-admission regression, five shared
cases and both actual native component cases (Linux34.44s, Windows90.98s) in
[run40213501378](https://github.com/marshallr12/s3drive/actions/runs/40213501378).
The100-cycle fixture had a second-owner create-new bug found during review;
this is not long-soak acceptance. The next fixture correction reopens and
cold-verifies its surviving file, adds a two-owner native regression, and bounds
all permanent owner/session/source rows and physical payloads explicitly.
Production code is unchanged since306ed181. Full original gates, extended soaks,
artifact/source verification and independent main acceptance remain required.

The longer component soak found an unresolved acceptance blocker on c98496f:
Linux completed28 cycles, then cycle31 Close reported `database disk image is
malformed` and retained recovery. The whole workload correctly failed in
[run40214811902](https://github.com/marshallr12/s3drive/actions/runs/40214811902).
This is not attributed to physical corruption or a dependency without evidence.
The original fixture did not retain DB/WAL. New failure-only read-only SQLite
integrity diagnostics retain the synthetic directory privately on its runner;
no raw archive is uploaded. The unchanged100-cycle step runs earlier to diagnose
promptly. Source32cbd07 passed54 focused proofs (including physical alias refusal)
and three actual cases on each OS, but these do not override the longer failure.
All component acceptance and merge remain blocked pending diagnosis/fix/full gates.

Investigation found a deterministic Linux lock-lifetime defect in preexisting
`OwnedDirectory::finish_initialization`: closing ordinary verification handles
for DB/WAL/SHM after SQLite opens releases its POSIX locks. An external process
could acquire the reopened live owner's SHM deadman lock; the baseline probe
fails and Linux-only retained-handle correction passes. Verification handles
now outlive SQLite but close before releasing directory ownership, including
error paths. Windows handling is unchanged. External read-only observer tests
and directory regressions pass locally (locking evidence, not durable staging).
The soak retains live observers, adds per-segment SQLite integrity checks and
three extra100-cycle repeats under a separate45-minute budget; no original
workload or deadline changes. Full exact-source hosted verification is pending.

The lock-fix candidate `0dab6bde61771ce35f6826933d798402f597f228` in
[run40217900789](https://github.com/marshallr12/s3drive/actions/runs/40217900789)
passed all four100-cycle component runs on each OS:800 cycles/400 mounts total,
400 closed SQLite integrity/FK checks, exact cold bytes and zero closed forests.
Both extended1000-save and four64KiB-file300 workloads passed. Permanent-row
maxima were1, component DB/WAL maxima778240/4177712 bytes, physical payload320.
These complete extended logs were independently inspected, not just green steps.
But Windows original debug300 reached290 then hit its unchanged1200s deadline.
The candidate is NOT accepted; main remains3fd2f05. Linux full was still running.

The next performance slice reuses bounded prepared SQL bytecode for hot ledger,
anchor, journal and receipt reads (128 statements per State connection), never
query rows, validation outcomes or authority. Every binding and read remains fresh;
exact SQL strings/limits are unchanged. Prepare-only schema probes remain uncached.
This targets16.91% of the prior diagnostic instructions spent preparing SQL.
New warm-cache schema/trigger/row rollback checks and the original live-proof
corruption checks must pass. Performance benefit and full acceptance are pending;
no workload/deadline/profile relaxation or repeated-run lottery is authorized.

Prepared-bytecode source `faa6b0bff1410c4c0fd40d1f4d3598cde364771a`
[run40224366616](https://github.com/marshallr12/s3drive/actions/runs/40224366616)
passed58 focused state tests, both actual native suites, all800 component cycles
and both OS larger/longer workloads. Measured instructions fell7.58% versus
0dab6bd (16.58% versus accepted-main diagnostic). Windows original debug300
passed its unchanged20-minute step; complete full Windows evidence was pending.
However, Linux full workspace caught two existing schema30 tests refusing a
missing retention table after deliberately demoting a warmed connection. This is
a false refusal, not data corruption. The candidate is not accepted.

The narrow correction leaves retention's schema-presence prepare uncached:
its existing absent-old-table fallback must observe errors during prepare, not
only during execution. All other converted prepare-error paths were audited;
this was the only one. Existing prepare-only schema probes remain uncached.
A new direct warm-cache regression covers versions29/49/50, partial versus fully
missing tables and rollback; current-format and partial tables still fail closed.
Original historical migration tests remain unchanged. No new alpha migration
scope, deadline change or main acceptance is implied; corrected full CI required.

## Corrected fresh linked-native candidate accepted (2026-10-25)

Exact candidate `135a35bcbc8df575f7a9193f40da5e3adffead3c`, tree
`b6dcb5cf7452e67552237c71d8792f2750cc2add`, passed independent review of
[run40148159139](https://github.com/marshallr12/s3drive/actions/runs/40148159139):
all seven jobs, Linux1103/Windows1098 parent-harness workspace passes,
802 exact source blobs, six artifact digests and 35 typed Windows references.
Both OS passed shared300, actual debug/release mounted300, every remount segment
and final cold read, plus full workspace, original lifecycle, compatibility,
reader-capacity, crash and full-disk rollback gates. Corrected-main verification
remains pending at this documentation revision.

Previous merged main `3ed77d43b94dca7d0c2ab05f140df9f68a3b4777` failed its
Windows mounted300 deadline after 250 saves despite earlier candidate acceptance.
That failure is not erased by this corrective candidate. Its Windows debug step
passed in 19m08s under the unchanged 20-minute limit, only about 52 seconds of
margin, not robust headroom. Main must pass its own full suite before acceptance.

See [accepted scope and development history](docs/src/fresh-native-retention.md).
This is opt-in cleanup for existing linked manifest files, not whole mounted
cleanup. Creation, namespace rename/removal/replacement, owned/deferred work and
retained recovery cleanup remain unsupported/protected. Defaults are unchanged;
no real store or provider object was deleted. Alpha databases are disposable:
do not add old-format migration or backward-compatibility work. Unsupported
formats still fail closed. Main `5a46fc0` remains the last accepted main baseline
until the corrected merge's independent full verification completes.

## Accepted ordinary-store cleanup and migration candidate (2026-10-23)

Exact candidate `f77f75684436c0afbed0fba15797d9819ffa61b0` passed independent
source/log/artifact review of [run40030136888](https://github.com/marshallr12/s3drive/actions/runs/40030136888).
This accepts schema50 [transactional local retention](docs/src/transactional-local-retention.md),
[populated ordinary-store migration](docs/src/populated-retry-migration.md),
[physical snapshot lifetime protection](docs/src/physical-snapshot-retention.md)
and explicit ordinary SDK scoped admission. Main verification is pending at this
handoff revision; acceptance here identifies the exact candidate, not a later merge.

Linux1053/Windows1049 workspace tests passed in37 cohorts, with20 retry-safety,
seven physical-reader and15 retention-filter cases per OS, the ordinary SDK
loopback migration/retirement fixture, and unchanged native debug/release gates.
All794 source files, six artifact ZIPs and genuine46/47,47/48,48/49 and49/50
executable compatibility were reviewed. Both OS300-cycle fixtures retained one
source anchor and one physical root, with DB peak1,474,560 bytes and WAL
peak4,420,792 bytes; late peaks were identical. Actual full-disk rollback, killed
owners, forged anchors, unrelated inherited witnesses and cloned readers passed.

This is explicit ordinary-library support, not native/populated-native migration
or automatic mounted cleanup. Native/owned owners remain refused, as do dirty,
conflicting, uncertain and reader-protected histories. Defaults, quotas, corruption
proofs and native deadlines are unchanged. No real user store or provider object
was cleaned or migrated. Historical failed candidates and their corrections are
recorded in the retention document; private validation archives remain unpublished.

## Accepted retry/checkpoint state prerequisite; no history reclamation

Main `870284330d3ce61983fe0080bc1acbe877ec9259` is accepted for the bounded
schema49 retry/checkpoint state prerequisite. Exact candidate
[run39998336518](https://github.com/marshallr12/s3drive/actions/runs/39998336518)
and unchanged main
[run40004695124](https://github.com/marshallr12/s3drive/actions/runs/40004695124)
passed full source/log/artifact review: Linux1026/Windows1022 workspace tests in
37 cohorts,12 retry/checkpoint cases per OS,26 native state cases,nine shared and
three actual native cases per OS in debug/release,784 exact source files,five ZIP
digests,35 typed reference records and genuine46/47,47/48,48/49 executable
compatibility. Main whole native debug/release timings were62.14/9.53 seconds on
Linux and106.24/20.11 on Windows, below the unchanged120-second bound.

Main `86d1e0ea7e27889e418863e3ab1b58e6e629f56c` is accepted after complete
[run39977846665](https://github.com/marshallr12/s3drive/actions/runs/39977846665)
review: Linux1014/Windows1010 workspace tests in37 cohorts,26 native state cases,
nine shared lifecycle cases and three actual native cases per OS in debug and
release, actual46/47 and47/48 compatibility, four artifact ZIPs,35 typed native
reference records and780 exact source files. Whole native debug/release timing
was61.15/9.58 seconds on Linux and100.96/18.82 on Windows, with the original
120-second sequence bound unchanged. This accepts the bounded repeated-open
replay/source-index improvement, not retained-history capacity or disk boundedness.

Historical failure remains relevant: normal-build source1ae0d16 passed
CI39898701182 and was merged unchanged ataffbf519461557209ba9be8c874be8d4636a2558,
but main run39931022610 failed the Windows complete debug sequence. Rename took
43.79 seconds and detached sync reached120.004 seconds before final Close/reopen.
Accepted corrections remain transaction-local; no cross-command proof caching,
relaxed deadlines or suppressed native workload failures are permitted.

The new [retry/checkpoint prerequisite](docs/src/retry-checkpoint-safety.md) is
an accepted schema49 state-library prerequisite. Its permanent explicit opt-in
closes preparation/root/physical admission over a source epoch and derives a
revalidated observational clean-checkpoint/terminal-evidence capability. It must
refuse activation on populated downstream/native owners without changing them.
It does NOT upgrade those owners, enable native/SDK callers, permit missing
ancestors, reclaim rows/bytes, or complete the full retry/retention protocol.
Unknown outcomes, dirty heads and reader pins remain protective. A released
reader token is never evidence of history reclamation.

Acceptance covers only this opt-in prerequisite and the regression evidence above.
The new48/49 check uses separately built old/current executables, verifies old48
refusal without byte changes, and preserves the complete old schema/data dump
apart from the exactly verified empty additive mode table. Local durable tests
remain blocked by the unchanged filesystem guard; hosted evidence supplies that
gate. Failed local/cohort evidence stays private and retained.

Native activation still requires managed version8 sparse-lifecycle=owned-unlinked-v1.
Omission preserves the default. Retention/GC, sparse delete-on-close/deferred
creation, real-provider writes/deployment and beta installation are not in scope.
No provider or user data is deleted; no credentials/default settings change.

## Accepted native sparse state baseline (historical)

Exact state candidate a1217b85 passed full acceptance in
[39872409080](https://github.com/marshallr12/s3drive/actions/runs/39872409080):
Linux983/Windows979 tests across37 cohorts,17 dedicated state lifecycle cases on
both OSes, actual47/48 and46/47 compatibility,35 exact typed native reference
records and all771 source files reviewed. It was merged with its unchanged tree
at8f8f51abc0c1c1cf253c3a50288970586045fda1. Main verification run39876643605 passed full exact-source review (same counts,
all771 files and four artifacts). Read `docs/src/native-sparse-close-ordering.md` for the bounded contract.
This is state ordering acceptance only, not new native filesystem acceptance.
The SDK/controller bridge6ec538d passed full review of39873777798: Linux988/
Windows984 tests,37 cohorts, five dedicated bridge cases on each OS, four artifacts
and774 exact source files. It remains on the feature branch; adapter feature
acceptance and production activation are still pending.
Recovery prerequisite main8777d30 was separately verified by39867508230:
Linux966/Windows962 tests,37 cohorts,35 native records and764 exact source files.

## Current checkpoint: accepted cold-recovery prerequisite, native work ongoing

The bounded restricted namespace/open recovery prerequisite is accepted at
cb568aa0417cf84d829a58236f9ac45c707b4ef8 in
[run39849211849](https://github.com/marshallr12/s3drive/actions/runs/39849211849).
Read `docs/src/chunk-namespace-cold-recovery.md`. Linux966/Windows962 workspace
tests passed in37 cohorts each, including8 new state/3 new SDK cases, prior16
state/6 SDK cases, actual46/47 compatibility and35 exact typed native records.
All764 source files matched. Complete logs/artifact digests were reviewed.
This capability replays only saved namespace requests/outcomes and already-linked
open acknowledgments without source-body startup dependence. It does not create
sessions/callback observations, admit fresh work or complete/retire opens.

Schema47 and native flags are unchanged. Actual new native integration remains
open: atomic original-publisher/source-epoch/Close ordering, chunk-compatible live
controller, startup sequencing, dirty existing-handle reads, replacement and
last-close settlement, followed by dedicated FUSE/WinFsp lifecycle/remount gates.
Observe Close before filesystem/network waits; settle all barriers before close
completion or releasing ownership. Do not relax closed/dead-session safeguards,
adopt another original open's lineage, or treat existing native regressions as
new lifecycle acceptance. Verify merged main's own full CI and retain branches.

## Current checkpoint: accepted owned sparse SDK

Typed SDK admission, coherent current-event publication, exact historical replay
and explicit rejected/no-effect settlement are accepted at runtime/test/CI source
1645861186421c3c5c10a9b4c3d3244389665e7f, based on main de0e4894.
Read `docs/src/owned-unlinked-sparse-sdk.md` for the contract and evidence.
[Run39822691603](https://github.com/marshallr12/s3drive/actions/runs/39822691603)
passes Linux955/Windows951 tests in37 cohorts each, all6 SDK and16 state cases,
separate native regression gates, all35 typed artifact records and actual46/47
compatibility. All759 source files match. Schema47 and native flags are unchanged.
The first fixture failure is recorded, not discarded. Local overlayfs/tmpfs full
tests remain unsupported; never relax durability guards. Keep archives private.

Next: actual owned-unlinked sparse native unlink-while-open, save/replace,
dirty-handle reads, final-close settlement and cold/remount recovery, with separate
Linux FUSE and Windows WinFsp acceptance before enablement. Existing native
regressions are not proof of these new lifecycle flows. Physical workstation,
real providers, retention/GC and remote deployment remain separate. Verify main's
own full CI after the approved integration. Do not delete user-owned branches.

## Current checkpoint: accepted typed owned sparse state

Schema47 state work is described in `docs/src/owned-unlinked-sparse-state.md`.
Source499aa6378360f2bdc76bf020b58e73cb20c4437d is accepted in
[run39796709806](https://github.com/marshallr12/s3drive/actions/runs/39796709806):
Linux949/Windows945 passed,37 workspace cohorts each, all16 new state tests,
separate native gates,35 exact typed artifact records and actual46/47 compatibility.
All757 source ZIP files match. Original-open/current-event provenance, source
lineage and explicit rejected/no-effect settlement are complete at the state layer.
The SDK slice is now accepted above; native lifecycle enablement remains pending.

## Current checkpoint: accepted sparse node-wide effect barriers

The narrow barrier prerequisite is accepted at runtime/test source
`2b08169f5e85d92bb7cc3a5834913ae490c0f11b`, based on main
`022c832aed4174203dfe29451979dc9d5391ac00`. Its own
[run39673658957](https://github.com/marshallr12/s3drive/actions/runs/39673658957)
has all three jobs terminal green. Full logs: Linux933/Windows929 passed,
37 cohorts each; all eleven new barriers, marker regression and process-kill/
SQLite-full checks pass. Separate native gates and all 35 exact typed artifact
records pass. All 753 source ZIP files match. Read
`docs/src/sparse-node-barriers.md` for evidence, scope and limitations.

The first candidate's Linux run39670781416 caught an existing readiness-file
creation/write race; its Windows job passed. The corrected hook publishes via
atomic rename and is now genuinely test-only, with a deterministic negative
control and regression. Keep both cohorts; do not treat a retry alone as proof.

Local fmt/Clippy/build/docs passed, but durable suites cannot run on this cloud
workstation's overlayfs/tmpfs. Use standard GitHub runners without relaxing
guards. Keep the local validation archive unpublished. Before integration check
fresh main/branch heads; verify main CI afterward. Preserve the local-only
`dot/schema46-validation` evidence branch and unmerged pending-scan experiment.

Next: typed sparse owned-unlinked admission and persisted original-open/current-
event provenance, predecessor/successor/source settlement and explicit rejected/
no-effect handling. SDK and native lifecycle acceptance remain subsequent phases.
This barrier slice changes no schema/wire format/native flag or provider behavior.

## Earlier checkpoint: accepted event-bound manifest authority

Runtime implementation `0a236de`, transfer source `a6e91434`, was validated
without changing any runtime input. Its own
[Linux/Windows/book run 39650193633](https://github.com/marshallr12/s3drive/actions/runs/39650193633)
is terminal success at `0eacebaac719b97df56ecf64f6e75b9ed101ba59`.
All 751 source ZIP files exactly match the transfer candidate; all 384
runtime/configuration/test inputs remain unchanged. Full logs and both Windows
artifacts were reviewed, including all 35 raw records, field types and values.
See [the accepted contract](docs/src/manifest-event-fence.md) for evidence detail.

- Full workspace suites: Linux 921 passed/0 failed/15 ignored; Windows 917
  passed/0 failed/31 ignored, each 37 cohorts. All 15 new state tests and the
  authenticated HTTP race/lost-reply/restart case pass on both. Native FUSE,
  WinFsp, Unicode/chunk I/O, remount and startup/recovery gates pass separately.
- Actual separately built schema45 and46 CLIs passed eight compatibility
  invocations. Old45 refuses fresh/upgraded46 with database/sidecar bytes
  unchanged;46 reopens both. A genuine45-created database with a Unicode legacy
  row upgrades with identical complete schema/table data dumps.
- The cloud workstation's `bash scripts/gate.sh` failed on unsupported
  overlayfs/tmpfs storage. Those failures are retained, not relabeled passing.
  Under the user's instruction to use suitable GitHub hosts, every command in
  that gate passed there, along with the separate native acceptance. Filesystem
  safeguards were not relaxed and no real provider credentials were used.
- This accepts the authority phase only. Sparse journal/SDK publication,
  node-wide unresolved-effect barriers and new native lifecycle enablement are
  still next work. Physical MX/Windows sign-in/reboot and real provider or
  independent-host deployment remain separate, unverified gates.

The user requested integration in this order: the hosted test branch into
`codex/manifest-event-fence`, then that branch into main after acceptance/review.
Check fresh heads at each step and verify main's full CI after merging. Only
then retire redundant clean worktrees and merged local/remote branches. Preserve
the unmerged `codex/pending-scan-proofs` experiment and unique local evidence.
The local validation archive is intentionally unpublished; do not add it to
the public repository. No user-computer cleanup is authorized by this workflow.

After integration, resume on a new branch from freshly fetched main rather than
relying on a retired validation branch or an old workstation path. Read
`docs/src/owned-unlinked-sparse-plan.md` before the next independently green
sparse-journal phase. Do not treat this authority acceptance as native activation.

## Decisions and permissions

- marshallr12/s3drive is intentionally PUBLIC. Visibility checks GET-only; no
  mutation without a new specific user request. Original private goal superseded.
- Managed immutable chunks/manifests, normal mounted names, Rust coordinator and
  private local SQLite outside mount. Optional Turso/both hosting choices open.
- MX without systemd and Windows; sign-in plus same-account SSH/tmux continuity.
  Physical MX/Windows10/11 reboot/sign-in remains unverified.
- No new provider writes, credentials, root/startup install or hosting authorized.
  Old Wasabi3-object and6-key/14-PUT scopes exhausted/cleaned2026-10-16T23:36:18Z.
  Never replay them or print config/keys. No real writable policy/user mount.
- bash scripts/gate.sh; native/provider/recovery gates separate. Preserve dirty
  bytes and exact uncertain identities; no unconditional overwrite fallback.

## Accepted SDK and state baseline (historical schema45)

Runtime8022fae3d1b496c511321f0ee6294908bc749039,381 runtime inputs,6 runtime
changes from main8780ce4. docs/src/creator-open-sdk.md is the contract/evidence.
Schema45/creatorformat3 unchanged. Underlying state20420d7 accepted separately
in docs/src/creator-open-state.md. Native lifecycle flags remain unsupported.
Publication proof: /tmp/s3drive-canonical-creator-open-sdk-publication.json;
frozen map /tmp/s3drive-creator-open-sdk-candidate1-frozen.json; candidate
/home/marshall/src/worktrees/s3drive-creator-open-sdk, branchcodex/creator-open-sdk.
The accepted schema45 baseline is 378c569b0b23b518db6f4e0404572261905dd702.
Historical local paths above are not required for resumption; the committed
evidence archives are portable. Current authority acceptance is recorded above.

Creator::create_for_open derives original owner from same-worker/volume chunk
capability. Read-only require_store is inside original shared semaphore/deadline.
New admission commits creator/inactive session-bound open/anchor/closing budget
before staging, PUT or Apply. Old create/create_deferred retain bytes/stagekeys.
Return is HistoricalCreation, not namespace import, activation or live handle.

Claimed uploads after restart retain original generation and consumed claim;
executor observes/reuses or refuses, never resends. Saved request/receipt replay
precedes fresh probes; unclaimed obsolete sessions cannot send. New metadata
request still needs current authority support. Original operation/source/owner
and claim are never replaced. Recovery::for_chunks has no S3 client/policy/body;
validates whole creator role then scopes store/owner, allowing ordinary unowned
legacy rows. Corrupt excluded records still fail. Close via original Storage.

State45 atomically reserves3 prospective namespace slots plus actual open/event
budget. Exact import converts existing reservation once; zero-growth conversion
allowed under checked physical cap. Unknown creators block abandonment/retire
proofs before namespace import. Rejection may explicitly close obsolete inactive
open without activation. Local facts never prove current remote ownership.
Marker-only45/raw43/44 forward guards and legacy recovery remain as accepted.

## Historical SDK verification

Own full local gate CLOSED0/988.85s/all739 unchanged,
37 passing cohorts/all10 new SDK cases; all slow notices resolved.
LogSHAa03e6dd0792d1f299aa3b15abbc5e8e0d2408771bd686181353ff1b9fb863478. Development check1, tests1/2(all9), targeted
stores/rejected and Clippy cohorts retained. No failed test hidden.
Own CI39633251767 attempt1 all3success; all739 source ZIP exact; strict
21+14/project/all 35 raw field/type checks and complete logs reviewed. New10 SDK,
prior17 prospective/14 import state/3 SDK completions in both Rust jobs.
Actual loopback HTTP/AWS SDK, precommit server gate, restart/worker termination,
credential/policy absence and damaged staging; modeled bulk quota fixtures
validated before real public SDK refusal. No new SDK SQL_FULL claim; stateA
already covers actual SQL_FULL. No new schema compatibility test relabeled.

Existing FUSE/WinFsp Unicode/chunk I/O/flush/directories/remount/startup and
128KiB callback regressions pass. Hosted WindowsServer2025 is separate from
physical workstation or live Wasabi acceptance. All warnings retained/reviewed;
profile lock deferral only under guarded disposable VM cleanup, credentials
removed, errors throw before PASS. Raw artifacts/source/hashes in hosted archive.

Local archive406 members SHA8a4c1141b27de2cd6877ab8537dba8c9b3d3056c2ae88c8cd5e8a1460dc2d732; all checked,
docs/src/evidence/creator-open-sdk-local-20261021.tar.gz. Writer DONE, NEVER rerun.

Hosted archive35 members SHAc53073fae882e0353efc710f43b951e72984e56ab0a0c3f8f6591a248f5079ec; all checked,
docs/src/evidence/creator-open-sdk-hosted-20261021.tar.gz. Writer DONE, NEVER rerun.

## Next work

1. docs/src/owned-unlinked-sparse-plan.md maps the next independently green
   authority/state/SDK/native phases. Sparse publication must bind original fence/current event,
   retain dirty/newer views and block final removal across unresolved effects.
   Enable native delete/save-replace/rename only after library/state/SDK/FUSE
   and WinFsp recovery gates. No user decision needed to plan this next phase.
2. Cold namespace/open recovery, full retry-forest closure, retention/GC and
   snapshot restore remain; prototype64-file/512-view/64MiB dirty/history limits,
   no reclamation. Old profiles3-7 retain8MiB whole-generation bound.
3. v8 supervisors; physical MX/Windows10/11 sign-in/reboot/SSH/tmux, independent
   host partitions/fencing/deployment/restore, policy renewal, packaging/licensing
   and real application workflows remain. Fresh bounded Wasabi plan before
   requesting new mutations. Ignored local profilewasabi-s3drive/us-east-2.

One local Cargo at a time, four test threads; use this checkout's target directory.
Preserve unrelated/dirty work and every uncertain/failed cohort. No subagents.
Previous156-line handoff archived verbatim in
docs/archive/HANDOFF-before-creator-open-sdk-refresh-20261021.md,
SHA68c6c636ef662b5efcb33d85efb9803672ad14454ef1c1bc93a4708f4a4b1944. Older state evidence remains unchanged.

## Historical transfer checkpoint: event-bound manifest authority

The transfer checkpoint was unaccepted on `codex/manifest-event-fence`, based
on accepted schema45 main `378c569`. Its acceptance requirements have now been
closed as recorded above; these historical notes do not override that status.
Local credentials, Wasabi configuration, build outputs and /tmp artifacts were
intentionally absent from the transfer.
Use a separate local state directory; never open a user's schema45 database with
this candidate as an acceptance experiment. Repository visibility remains public.

Implemented: optional event-bound manifest guard, atomic RemovalEpoch read,
active/committed publication modes, immutable historical replay checks, bounded
raw pre46 metadata guard and schema46 marker. No sparse SDK publication call
site, native lifecycle flag or provider writes enabled.

Checkpoint verification: 15 focused state tests pass, including eight killed
workers and actual SQLiteFull; authenticated loopback HTTP race/lost-reply/restart
case passes; workspace/all-targets Clippy with warnings denied passes. These are
focused checks, not a full acceptance gate. Portable development evidence,
including failed earlier compilation/test assumptions and subsequent corrections,
is indexed in `docs/src/evidence/manifest-event-checkpoint-20261021.tar.gz`.

At transfer, full-gate equivalence, actual45/46 executable compatibility and
the candidate's own exact-source hosted/native evidence remained pending.
They are now closed above. Hosted Windows is still not physical-workstation
proof. Do not infer executable identity from Cargo freshness or assume old
temporary files exist on another workstation.

Main follow-up CI39641137658 was inspected, not restarted: all three jobs are
terminal success at378c569. It is historical SDK evidence, not schema46
acceptance. Accepted SDK evidence also uses its own completed CI39633251767.

## Worktree cleanup at transfer

User explicitly requested retirement of obsolete worktrees and local/remote
branches. Removed44 clean obsolete worktrees,110 local and107 remote branches.
Main and active event-fence worktree remain. A dirty detached experiment remains
at `/tmp/s3drive-staged-range-study-20261018-tjtdatmk/source`; its related local
`codex/pending-scan-proofs` and its remote branch are preserved. These are historical experiment
references, not the active development branch. Do not delete dirty experiment
bytes or interpret its portable tests as acceptance of this phase.

Before deletion, all Git refs/history were preserved in verified local recovery
bundle `/home/marshall/src/s3drive-retired-20261021.bundle` (76286231 bytes;
SHA2563834eb741cbd17342d2a6b201eb27550532dd30721426d7cea27043c02c30e4c).
The bundle is outside the repository and is not automatically transferred to
another workstation. Accepted runtime and evidence remain in main history;
the active checkpoint is pushed separately. The committed cleanup archive
`docs/src/evidence/workstation-cleanup-20261021.tar.gz` records exact retired
heads, accepted ancestor/runtime matches, superseded backups and guarded results.
No repository visibility setting was changed.

Cleanup addendum: the remote-only duplicate `codex/pending-scan-repeat-64aa073`
was also removed with its exact expected64aa073 lease. Its identical head remains
under `codex/pending-scan-proofs` and in the recovery bundle. Final remote branch
count is three: main, manifest-event-fence and pending-scan-proofs. The original
cleanup archive records the preceding106 deletions; this addendum accounts for
the107th. Checkpoint implementation commit is0a236de; this documentation update
records the final cleanup state without changing runtime inputs.
