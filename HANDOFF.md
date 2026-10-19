# S3Drive handoff

Updated 2026-10-19. Goal ACTIVE. Assistant: Mira. No voice-context handoff capability
is available. Read this handoff and [implementation plan](docs/src/implementation-plan.md)
before implementation. Keep research, plans and acceptance evidence in docs/src.

## Current user decisions and boundaries

- Repository marshallr12/s3drive is intentionally PUBLIC. AGENTS.md supersedes the
  original private goal and historical notes. Never change visibility without a
  new direct request; checks are GET-only.
- Managed immutable chunks/manifests, coordinated normal filenames, a shared Rust
  authority and separate private client SQLite. Both authority hosting options
  remain open; Turso is researched, unimplemented/unprovisioned here.
- Startup must support MX Linux without systemd and same-account SSH/tmux access
  after terminal disconnect, plus Windows. Physical sign-in/reboot is a separate gate.
- Run bash scripts/gate.sh; keep native/provider/recovery gates separate. Never
  report uploads, mounts or flushes without evidence. Preserve dirty/conflicting
  data and unsupported paths; no unconditional overwrite fallback.
- No new Wasabi writes, provider probes, credential/root/startup/hosting changes
  are authorized by the current local fixture work. Do not dump configuration.

## Accepted runtime and canonical publication

Accepted runtime950479ca3aacd19059fe3e8474f548502476f36a/schema40/all357 supersedes
bb17161/all354 for the SDK. Complete local gate4 and own CI39325779322 all3 jobs
passed. Full raw/source/API ZIP, typed21+14, project oracles and all35 raw records'
fields/types passed. Exact canonical publication is recorded in
/tmp/s3drive-canonical-empty-creator-publication.json once the current publication
completes; verify its commit/remote identity rather than copying an older main SHA.
Only8 runtime paths differ from accepted354. [Acceptance and diagrams](docs/src/empty-manifest-publisher.md).

Existing native FUSE9/WinFsp7, manifest-read/remount and no-systemd/Windows startup
regressions passed for this exact source. They are the old mounted path. Mounted
writes still use the older8MiB whole-generation writer; native chunk writes,
retention and physical MX/Windows10/11 reboot/sign-in remain unfinished. Authority
snapshots support create/inspect, not writable restore/failover.

## Accepted creation/recovery SDK

Worktree /home/marshall/src/worktrees/s3drive-empty-manifest-publisher,
branch codex/empty-manifest-publisher. Tested source
950479ca3aacd19059fe3e8474f548502476f36a, schema40/runtime357. Adds
s3drive_managed::creation::Creator/Recovery, existing-only state recovery,
metadata inspection/shared receipt transitions and owned SDK fixtures. Uses one
Executor-bound SQL owner, one conditional immutable PUT claim, verified64-byte root,
exact saved namespace Apply and historical receipts. No schema/native callbacks.

The proof audit found a real counterexample after source7068013 passed its gates:
rewriting both request and copied physical snapshot, then recomputing local hashes,
was admitted against unchanged original upload metadata. The fix compares the
complete saved physical snapshot with the independently checksummed verified
remote-intent record during inspection and preparation. Bounded metadata only;
no staged/sparse body reads or repair. Red log proof-red1 failed as expected;
focused proof-state1 passed18/1.53s and proof-sdk1 passed6/11.12s.

Complete local gate4 runner1593117/child1593122 CLOSED0 at21:25:33Z. All670
tracked/nonignored inputs unchanged; runtime/workflow/script catalog357.
Publication30/194.43s, reader26+1nativeignored/85.89s, state524+1diagnosticignored/
257.49s, core39/S362/fmt/Clippy/build/93links/book passed. Logs/results under
/tmp/s3drive-empty-creator-gate4-*. Runtime map candidate4-frozen.json and current
checkpoint /tmp/s3drive-empty-creator-checkpoint.json.

375-member indexed local archive docs/src/evidence/empty-creator-proof-local-20261019.tar.gz,
SHA0a004618809fbda20e9703f1c8838013889c9a3eaad1bc4102aaea49d5e6b2d2.
One-use archiver73208 DONE; never rerun. Source706's earlier384-member local
archive and the25-member failed stack-cohort archive remain independently indexed.
Superseded706 hosted/local/red context:15-member archive
empty-creator-superseded-20261019.tar.gz,
SHAc09b504e7399699fa3f2f151c46ad385d1e8cadaa0616231a67a2572e3090082.
That all-green cohort is unaccepted for this phase because of the later red test.

## Completed hosted verification and next action

Corrected CI39325779322 attempt1 all3 success: Linux118311122005,
Windows118311122136, book118311122026. Observer1599455 CLOSED0/all_success at
21:44:32Z. Creator4 raw review, exact source/API ZIP checker78328 and raw-record
comparison completed. One-use hosted archiver5184 DONE; never rerun. Own artifacts
replacement12223045178/disposition12222694397 passed strict typed21/14, project
oracles and every raw record field/type. Full evidence under
/tmp/s3drive-empty-creator4-hosted-* and docs/src/evidence/empty-creator-proof-hosted-20261019.tar.gz.
46 members, SHA7855dc88476cc58858f766fa9bbe7e5dd663681b34c9521505dbfa5cb1131677.
All16 slow warnings and15 interleaved reference endings completed. Only3 known
disposable profile locks remained; dummy credentials removed, deletion deferred
to VM destruction. Tooling/linker warnings retained. State524/519 plus1ignored;
publication30/reader26+1nativeignored/core39/S362. Ubuntu24.04 image20261012.334.1;
WindowsServer2025 windows-2025-vs2026 image20261007.264.1. Not physical desktop proof.

Creator3 acceptance helpers are obsolete/never executed. Source706's all-green
cohort remains unaccepted for this phase due the subsequent red proof regression.
Cohorts3e767ac and97aab69 failed real child stack overflow and remain archived.
No stack limits were raised and no cases skipped. Current357 includes the erased
future dispatcher and independent-proof fix.

Canonical promotion must compare all357 bytes against candidate4-frozen.json and
preserve unrelated edits; visibility checks are read-only. Documentation after the
frozen source is separately link/book checked. Do not rerun one-use archivers on
later docs or treat a later CI result as the original cohort's raw evidence.

Next worktree /home/marshall/src/worktrees/s3drive-chunk-client-composition,
branch codex/chunk-client-composition, declared contract commita09507c. It starts
from950 SDK code/docsb146 and currently has only the composition contract; no
new role/migration or mount callback is implemented there. Refresh from this
accepted SDK publication before implementation. Its contract records old logical
namespace barriers and the deferred metadata-empty creation boundary; do not just
remove constructor exclusions or silently reuse empty-file origins.

## Next implementation toward the actual drives

Use the accepted SDK as the ordinary native creation path. Next establish typed
same-owner composition of sparse views/creation with namespace and Windows open/
session ledgers. Current constructors deliberately reject coexistence; do not
remove guards wholesale or adopt populated old identities. Bind volume, stores,
source controls and permanent client identity, test reciprocal isolation, migration,
late triggers, corruption and restart. Read-only footprint findings are in
/tmp/s3drive-native-composition-scout.json and
[the native bridge plan](docs/src/native-chunk-integration.md).

Then enable shared FUSE/WinFsp operations: create, write beyond8MiB, sparse grow/
truncate, read-your-writes, exact flush, reopen, save/replace, rename/delete,
open-unlinked close and remount. Current sparse publication hardcodes linked guard;
unlinked publication needs exact owned-removal proof. Keep old bytes and unknown
outcomes. Close every retry namespace before retirement/GC; scaffold16/128 creation
and4096 authority histories are not a finished long-running retention policy.

Still required: legacy/empty origin migration where needed, real managed Wasabi
large-file acceptance, independent-host partitions/deployment/fencing/restore,
physical MX/Windows10/11 sign-in/reboot/SSH behavior, usable policy renewal and
packaging/distribution/licensing. No Turso account or hosting was provisioned.

## Provider scope and secrets

Ignored local config: /home/marshall/src/s3drive/s3drive.local.toml.
Wasabi aliaswasabi, s3drivetest/us-east-2, profilewasabi-s3drive. No keys in chat/Git.
Earlier3-object Unicode scope and6-key conformance scope were completed/cleaned.
The latter consumed all14 PUT attempts on2026-10-16 at23:36:18Z. These approvals
are exhausted. Optional later plan pointer /tmp/s3drive-next-wasabi-plan-location.txt
was never approved/executed. Any new live mutations need a concrete bounded plan
and explicit authorization; never extend old expiry or replay old probes.

## Durable history

The complete pre-refresh handoff is archived verbatim at
[creation SDK history](docs/archive/HANDOFF-before-creation-sdk-refresh-20261019.md).
SHA-256: da4e56413a3896c5c4047e6c079771efadf3522cb39b9e5ee23981fe25b8a402. Before431 lines; this refreshed handoff keeps current
state and next actions together. Older accepted source/job/artifact IDs, one-use
tools, rejected explanations and previous archive links remain in that archive.
All historical private-repository requirements are superseded by AGENTS.md.
