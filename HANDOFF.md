# S3Drive handoff

Updated 2026-10-21. Goal paused/incomplete for transfer to another workstation. Assistant Mira. No voice-context
handoff capability available. Read this and docs/src/implementation-plan.md first.

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

## Accepted SDK and state

Runtime8022fae3d1b496c511321f0ee6294908bc749039,381 runtime inputs,6 runtime
changes from main8780ce4. docs/src/creator-open-sdk.md is the contract/evidence.
Schema45/creatorformat3 unchanged. Underlying state20420d7 accepted separately
in docs/src/creator-open-state.md. Native lifecycle flags remain unsupported.
Publication proof: /tmp/s3drive-canonical-creator-open-sdk-publication.json;
frozen map /tmp/s3drive-creator-open-sdk-candidate1-frozen.json; candidate
/home/marshall/src/worktrees/s3drive-creator-open-sdk, branchcodex/creator-open-sdk.
Accepted main is 378c569b0b23b518db6f4e0404572261905dd702. Historical local paths above are not required for resumption; the committed evidence archives are portable.

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

## Verification

Own full local gate CLOSED0/988.85s/all739 unchanged,
37 passing cohorts/all10 new SDK cases; all slow notices resolved.
LogSHAa03e6dd0792d1f299aa3b15abbc5e8e0d2408771bd686181353ff1b9fb863478. Development check1, tests1/2(all9), targeted
stores/rejected and Clippy cohorts retained. No failed test hidden.
Own CI39633251767 attempt1 all3success; all739 source ZIP exact; strict
21+14/project/all35 raw field/type checks and complete logs reviewed. New10 SDK,
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

One local Cargo at a time; shared /home/marshall/src/s3drive/target, threads4.
Preserve unrelated/dirty work and every uncertain/failed cohort. No subagents.
Previous156-line handoff archived verbatim in
docs/archive/HANDOFF-before-creator-open-sdk-refresh-20261021.md,
SHA68c6c636ef662b5efcb33d85efb9803672ad14454ef1c1bc93a4708f4a4b1944. Older state evidence remains unchanged.

## Resume checkpoint: event-bound manifest authority

Continue on `codex/manifest-event-fence`, based on accepted main `378c569`.
The checkpoint is an unaccepted schema46 candidate. Main stays at schema45.
Read [the candidate contract](docs/src/manifest-event-fence.md) and
[the implementation plan](docs/src/implementation-plan.md) before edits.

On another workstation:

```sh
git clone https://github.com/marshallr12/s3drive.git
cd s3drive
git switch --track origin/codex/manifest-event-fence
```

If already cloned, fetch origin and switch to that branch. Local credentials,
Wasabi configuration, build outputs and /tmp artifacts are intentionally absent.
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

Next steps, before accepting or merging:
1. Run `bash scripts/gate.sh` with one Cargo process and four test threads.
2. Build actual schema45 CLI from main378c569 in a separate target directory;
   verify it initializes45. Build candidate46, prove old45 refuses46 with database
   bytes unchanged, and prove new46 can reopen. A retained Linux old45 executable
   existed only locally (SHA7c67ee45bde346306fd192ce600e508831304045078b286820f18550a162ad7b).
   Its metadata proof is archived; its binary is not transferred. Do not infer
   executable identity from Cargo freshness or assume /tmp exists elsewhere.
3. Obtain/review this candidate's own exact-source Linux/Windows/book CI and
   native regression evidence. Hosted Windows is not physical workstation proof.
4. Continue sparse local preparation, source settlement and node-wide unknown
   effect barriers only after this authority phase passes acceptance.

Main follow-up CI39641137658 was still in progress at transfer preparation;
inspect that same run rather than restarting it. Accepted SDK evidence uses its
own completed CI39633251767. Historical branches/worktrees are being cleaned
under the user's explicit request; do not rely on their local paths. Resume
from this branch and committed documentation. No background feature work should
continue on this workstation after the transfer checkpoint is pushed.

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
