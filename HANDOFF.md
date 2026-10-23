# S3Drive handoff

Updated 2026-10-23. Event-bound manifest authority is accepted at schema46;
the overall project remains incomplete. Read this, AGENTS.md and
docs/src/implementation-plan.md before continuing.

## Accepted performance baseline; retry-checkpoint prerequisite under development

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
an unaccepted schema49 candidate. Its permanent explicit state-library opt-in
closes preparation/root/physical admission over a source epoch and derives a
revalidated observational clean-checkpoint/terminal-evidence capability. It must
refuse activation on populated downstream/native owners without changing them.
It does NOT upgrade those owners, enable native/SDK callers, permit missing
ancestors, reclaim rows/bytes, or complete the full retry/retention protocol.
Unknown outcomes, dirty heads and reader pins remain protective. A released
reader token is never evidence of history reclamation.

Candidate acceptance requires its own complete Linux/Windows standard Actions,
new48/49 actual executable compatibility alongside46/47 and47/48, exact source,
full logs and all native artifact checks. Failed local/cohort evidence stays
private and retained. Do not merge before that review; verify main afterward.

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
