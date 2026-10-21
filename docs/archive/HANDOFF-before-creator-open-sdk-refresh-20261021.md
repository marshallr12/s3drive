# S3Drive handoff

Updated 2026-10-21. Goal ACTIVE/incomplete. Assistant Mira. No voice-context
handoff capability available. Read this and docs/src/implementation-plan.md first.

## Decisions and permissions

- marshallr12/s3drive is intentionally PUBLIC. No visibility mutation without a
  new direct request; GET-only checks. Original private goal is superseded.
- Managed immutable chunks/manifests and normal mounted names; Rust authority,
  private local SQLite outside mount. Optional Turso/both hosting choices open;
  no remote service provisioned.
- MX without systemd and Windows; sign-in plus same-account SSH/tmux continuity
  required. Physical MX/Windows10/11 reboot/sign-in is not verified.
- No new provider writes, credentials, root/startup install, hosting or Turso
  changes authorized. Old Wasabi3-object and6-key/14-PUT scopes exhausted/cleaned
  2026-10-16T23:36:18Z. Never replay them; never print config/keys.
- Run bash scripts/gate.sh. Native/provider/recovery gates are separate. Preserve
  dirty bytes and exact uncertain identities; no unconditional overwrite fallback.

## Accepted state layer

Runtime20420d73bf07ccb0929c3b188e32c6d23a37dfec, schema45/all380,25 runtime
changes from main02133e0. docs/src/creator-open-state.md is the contract/evidence.
No SDK or native lifecycle call site enabled. Verify actual main/remote instead
of guessing the canonical squash ID. Publication proof:
/tmp/s3drive-canonical-creator-open-state-publication.json.
Candidate worktree /home/marshall/src/worktrees/s3drive-creator-open-admission;
runtime frozen map /tmp/s3drive-creator-open-state-candidate1-frozen.json.

ChunkClient::creations(limits)/Journal::for_chunks admits owner+CreationOpen
format3. One immediate transaction stores original creator/source, inactive
controlled session-bound open, retained anchor and shared namespace closing
budget before commit. Generic creator/open admission cannot manufacture it.
Original current runtime session/capability/source/store/owner must match.

Unimported/nonrejected creators hold3 namespace slots. Exact successful import
converts3 to row1+deferred2 once; terminal authority rejection frees prospective
namespace budget but not open history. Other effects cannot steal the op ID.
A proven zero-growth conversion can proceed while unrelated admitted closing
credit is pending; no fresh capacity or physical row limit is increased.

Raw open-anchor validation avoids recursive creator/open/namespace loaders.
Full metadata projections and selected physical/anchor witnesses remain checked;
full-role APIs still check all witnesses. Fresh activation requires original
session and locally deferred original fence. Unknown creators block abandonment,
recovery-proof loading and retirement, including before any namespace row exists.
Known rejection can close an obsolete session without synthetic activation.
Local facts do not prove live remote ownership or grant a kernel handle.

Schema45 adds no DDL/table. Raw pre45 and original-version pre44 guards precede
transaction-local marker changes. Valid43/44 roles retain every byte/hash; bad
upgrades roll back. Earlier unsupported roles stay refusals. Legacy format1/2
stage keys are unchanged; saturated historical imports remain separate recovery
work, never relabeled or discarded.

## Verification and artifacts

Full local gate CLOSED0/983.01s/all733 unchanged,37 cohorts/all17 new cases
passed; all slow notices resolved. LogSHA
ba753a63a14675cace65c3ca1fcdea8aa75d35ea95c57af40caa4f3b06c1e1cc.
Own CI39616964879 attempt1 Linux/Windows/book all success. Source ZIP/all733,
strict21+14/project/all35 raw field/type and complete log review passed. Both
hosted Rust jobs pass17 prospective,14 prior import and3 prior SDK completions
(worker entries included). Four actual killed-worker boundaries, SQL_FULL,
independent open/event/namespace quota, duplicate/concurrent IDs, corruption,
no-effect forgery, successor fence and migration covered. Physical receipts and
bulk quota history modeled; old SDK fixtures use actual loopback HTTP/AWS SDK.

Existing real FUSE/WinFsp v8 Unicode I/O/directories/flush/resize/remount,128KiB
callback and startup regressions passed. Ubuntu24.04/20261012.334.1 and
WindowsServer2025/windows-2025-vs2026/20261018.275.1. No live Wasabi or physical
MX/Windows10/11 acceptance.17 slow notices and15 bounded interleaved completions
reviewed. Three disposable-profile warnings: dummy credentials removed; locked
profile deletion deferred to VM destruction only under checked cleanup guards.
Tool/node and unused delay-import warnings retained; no test faults.

Actual old44 Linux CLI refused45 without changing database bytes; new45 reopened.
OldSHA4fce77cea8b52753c59bf55a5cde7f24374b7ef70ef58e1be4e3bdf289e6a6bc.
Final45 SHAd08bd8bcdf785aa6c697ddcbccd42bd4a61d06c7fce6a7334502b14cbbd9ccc0.
Failed equal-digest assumption between test-stage and final-build executables is
retained, with independent final-binary checks. No source/guard changed for it.
Use actual schema checks; never infer runtime identity from Cargo freshness.

Indexed archives in docs/src/evidence: local405 members SHA123b8b7e2d343726290004e77bbcaf5a5cc31d811e749cdc916ede44ec1a056a;
supplement5 SHA7dbf2771616a26d332c5f174dc6d48a4420fd66761cfdf71456d640aac0ab617;
hosted37 SHA0af4ed455c7a66da01c34c773d10e641687c9d2c5b0ea70debeafd521c4a6f2a.
Every member checked; archive writers DONE, NEVER rerun. Initial missing legacy
fixture field was corrected; failed/successful development logs retained. No
subagents spawned. No user database, config, bucket or startup install touched.

## Next work

IN-FLIGHT SDK CANDIDATE: codex/creator-open-sdk at
/home/marshall/src/worktrees/s3drive-creator-open-sdk. Contract:
docs/src/creator-open-sdk.md. No schema/native flag change. Adds
Creator::create_for_open and async Recovery::for_chunks; read-only
ChunkClient::require_store. Same original semaphore/deadline, original claimed
upload observation without fresh session permission, no-credential saved-
metadata recovery, store/owner scope with complete role validation before filter.
Check1 passed; SDK tests1 nine/21.18s, tests2 nine/31.57s, stores test3/3.92s,
rejection test4/2.23s and Clippy1/36.18s passed. Full gate/own hosted acceptance
not yet done. All /tmp/s3drive-creator-open-sdk-* evidence must be retained.
Main8780ce4 remains the accepted state-only baseline. Prior main follow-up CI
39625428358 reports all3 jobs success (status-only follow-up); verify live.

1. SDK step B: typed same-worker creation admission, original shared semaphore/
   deadline/boxed future, exact retained-request recovery and claimed-upload
   observation without resend. Fresh unclaimed admission cannot revive obsolete
   sessions. Add metadata-only mixed-role chunk recovery with no S3 client for
   saved requests/receipts. Source findings and test gates are in the contract;
   /tmp/s3drive-creator-open-sdk-readiness.md is supplementary, recheck lines.
2. Sparse owned-unlinked root publication must bind original fence/current event,
   preserve dirty/newer views and block final removal across unresolved effects.
   Enable native delete/save-replace/rename only after state/SDK/FUSE+WinFsp gates.
3. Cold namespace/open recovery, full retry-forest closure, retention/GC and
   snapshot restore remain.64-file/512-view/64MiB dirty/history quotas prototype;
   no reclamation enabled. Old profiles3-7 retain8MiB whole-generation bound.
4. v8 supervisors, physical MX/Windows10/11 reboot/sign-in/SSH/tmux, independent
   host partitions/fencing/deployment/restore, policy renewal, packaging/licensing
   and application workflows remain. Prepare a fresh bounded Wasabi plan before
   asking for new live mutations. Ignored local config: s3drivetest/us-east-2,
   profilewasabi-s3drive. No real writable policy or user mount installed.

One local Cargo at a time; shared /home/marshall/src/s3drive/target, threads4.
Preserve dirty/unrelated work and every failed/uncertain verification cohort.
Previous134-line handoff archived verbatim at
 docs/archive/HANDOFF-before-creator-open-state-refresh-20261021.md,
SHA0d88dc391f9255f7bdcb6a6713506c71198ecf6eff393e7ba6aa9c0ba95064bb.

## SDK local checkpoint (2026-10-21; hosted pending)

Frozen8022fae3/all381 runtime,6 runtime changes from8780ce4. Full local gate
CLOSED0/988.85s/all739 unchanged,37 passing cohorts/all10 SDK; slow notices
resolved. Local archive406 members SHA8a4c1141b27de2cd6877ab8537dba8c9b3d3056c2ae88c8cd5e8a1460dc2d732
is in docs/src/evidence and verified. Writer DONE, NEVER rerun. Prior in-flight
block is superseded by this checkpoint for local verification only. Main stays
8780ce4 until own CI39633251767 and raw/strict/artifact review pass.
Observer3081034/collector3081035; /tmp/s3drive-creator-open-sdk-hosted1-*; check
live status and existing result before starting/restarting anything. Windows
still in workspace tests, Linux/book passed. All tmp evidence retained.
Acceptance scripts are prepared but not run: *-manual-review.py,
*-mark-reviewed.py (only after actual raw inspection), *-hosted-archive1.py
(one-use), *-update-docs.py, *-doc-gate.py, *-promote.py, *-remote-check.py.
No further schema/binary compatibility test needed; schema45 unchanged.
Next sparse authority/state/SDK/native source map is
docs/src/owned-unlinked-sparse-plan.md; publication guard currently
checks original fence, not an independently requested current event. Proposal
only. No subagents, user state/config/provider/visibility/startup changes.

Live verification2026-10-21T16:53Z: GitHub job119336755828 still in progress,
cargo workspace tests since16:26Z. Read-only live page independently shows all
10 new SDK cases passed plus earlier native/callback/replacement/disposition
steps passed. Whole job/source/raw artifact acceptance still pending; do not
mark accepted from partial UI. Observer/collector remain running; no CI restart.
Public visibility unchanged. The next-phase proposal is now in mdBook.
