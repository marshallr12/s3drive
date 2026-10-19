# S3Drive handoff

Updated 2026-10-19. Goal ACTIVE. Assistant: Mira. No voice-context handoff
capability is available. Read this file and [implementation plan](docs/src/implementation-plan.md)
before implementation. Research, decisions and acceptance evidence belong in docs/src.

## User decisions and operating rules

The repository is intentionally **PUBLIC**. The explicit 2026-10-18 decision in
AGENTS.md supersedes the old private-repository goal text and historical guards.
Never change visibility without a new direct request. Visibility checks are GET-only.
The earlier changes to private were assistant mistakes, not an identified GitHub
policy; the exact investigation remains in the archive linked below.

The selected target uses managed immutable chunks and manifests, coordinated
filenames/metadata, a Rust shared authority and separate local SQLite state. Keep
both local and remote authority hosting options open; Turso is researched but
unprovisioned. See [architecture](docs/src/architecture.md).

Startup must support sign-in and same-account SSH/tmux sessions after disconnect,
including MX Linux without systemd and Windows. Do not substitute a desktop-only
session or claim physical reboot acceptance from CI.

Run `bash scripts/gate.sh` for the complete local gate. Platform mounts, provider,
recovery and physical session gates remain separate. Inspect resolved dependency
sources before using their APIs. No credentials/config source in logs or Git.
Preserve dirty data and explicit conflicts; never unconditional overwrite fallback.
Use target-specific platform dependencies. SQLite state stays outside the mount.

## Accepted canonical implementation

Main runtime includes accepted authenticated external-clean observation,
schema38/all342 exact inputs. Own tested source12a2f2a7d836aab518c7b0d60675234d6470ef97,
CI39298010083 attempt1: book118215485753/Linux118215486002/Windows118215486257
all succeeded. Complete local gate passed18:06:20Z unchanged. Complete raw/source/
API ZIP, independent typed21+14/project/all35 raw fields and JSON types passed.
Actual native FUSE9/WinFsp7, manifest ranges/remount and paired no-systemd/Windows
startup passed. Images Ubuntu24.04 20261012.334.1; WindowsServer2025
windows-2025-vs2026 20261007.264.1. These are not native chunk-write, Wasabi or
physical desktop/reboot proofs. Three locked disposable Windows profiles had dummy
credentials removed; final profile deletion deferred to hosted VM destruction.

[Observation acceptance](docs/src/external-clean-reader.md): indexed376-member
local archive SHAbe0848b254fb9eb9a300fd7d0bd21f4deef4e9fbd940c496b2e2ba913e774b08
and109-member hosted archive SHA95f70fe3f45d554cac275f7325b51644c54012a43e237544fd5c50a2752ac10c.
All one-use observation342 acceptance/comparator/archivers are DONE; NEVER rerun.
Canonical publication proof: /tmp/s3drive-canonical-external-clean-reader-publication.json.
Ordinary later documentation commits do not change accepted runtime. Parent340
local clean-adoption intent remains accepted; its indexed archives are retained.

The accepted path includes authenticated manifest reads, immutable local patches,
mixed plans/dependency uploads, exact root publication/recovery, local lineage and
clean epochs. Mounted writes still use the older bounded whole-generation path.
Offline authority snapshots support create/inspect, not writable restore/failover.
Earlier failed Windows transport cohorts are retained; later success does not prove
their cause. Never equate portable unit tests with an actual mount or flush.

## Active isolated work and exact next actions

Accepted local intent prerequisite: `codex/external-clean-local` at
/home/marshall/src/worktrees/s3drive-external-clean-local, source67a75e3047a867e2499ef08ded64fe00b72a4aa9.
Schema38 adds distinct format4 Observe intent, scoped old-epoch retry, clean-head
CAS, full unresolved-work validation and no-change admission without history growth.
No authenticated remote facade, mounted chunk write or GC. [Contract](docs/src/external-clean-local.md).
Focused12 passed1.23s (five actual kills/SQLITE_FULL/worker race/pins), separate
publication-history case0.86s; state all-target Clippy17.34s. Initial check before
test module creation failedE0583; module supplied before the successful focused run.
Full gate1 runner1114325/child1114329 CLOSED0 at17:46:14Z, all340 unchanged.
Publication24/163.16s,state494+1ignored/256.86s,reader23+1nativeignored/45.10s,
37core/62S3/fmt/workspaceClippy/build/90Markdown/86chapter log entries passed.
Indexed355-member local archive SHA6771882a97cca90e85ca7570d6b05e6fa8ce4bda7e6956628d609cc10bacc582
verified; one-use local archiver18521 DONE, never rerun. Exact340 frozen under
/tmp/s3drive-external-clean-local-candidate1-{frozen.json,source.tar.gz}.
Logs/status /tmp/s3drive-external-clean-local-gate1-{running,result}.json and .log.
Own CI39295182975 attempt1 all3 success: book118205713291/Linux118205712929/
Windows118205713142. Observer1115366 CLOSED0/all_success18:07:07Z. Own one-use
raw review/checker22739/comparator+hosted archiver39877 DONE, never rerun. Raw/source/
API ZIP/typed21+14/project/all35 raw fields/types passed; own replacement12207128174
and disposition12209851124. Indexed107-member hosted archive SHAb97670d9733322847845c305d8e9fb6114673957f9348180f2bed9e156048902.
Exactly13 runtime deltas promoted; main all340 matches tested source67a75e3.
The remote facade is now separately implemented in worktree
/home/marshall/src/worktrees/s3drive-external-clean-reader, branchcodex/external-clean-reader,
own source12a2f2a7d836aab518c7b0d60675234d6470ef97/all342. SDK1 sourcea8cc73e
passed2state32.01s/3SDK30.05s, Clippy failed large_enum_variant; runner1153483
CLOSED101, all342 unchanged. SDK2 compact-box repair9913dd8 passed2state31.39s/
3SDK30.55s/workspaceClippy35.04s; runner1176459 CLOSED0/all342 unchanged.
SDK3 changes only the SDK fixture, requiring a second provider HEAD after a gated
valid root response and refusing late access revocation/ETag change. Source12a2f2a
passed3SDK30.61s/Clippy2.89s; runner1179683 CLOSED0/all342 unchanged. All failed/
passing source maps/archives/logs/runners/results retained, same-prefix SDK1/2/3.
[Exact contract/evidence](docs/src/external-clean-reader.md).
Full gate1 runner1181132/child1181137 CLOSED0 at18:06:20Z, all342 unchanged.
Publication24/171.66s,state496+1ignored/258.82s,reader26+1nativeignored/69.06s,
37core/62S3/fmt/workspaceClippy/build/91Markdown/87chapter entries passed.
One-use local archiver49337 DONE; never rerun. Indexed376-member local archive
SHAbe0848b254fb9eb9a300fd7d0bd21f4deef4e9fbd940c496b2e2ba913e774b08 includes
exact342, all SDK failed/passing contexts and independently verified parent340
local+hosted archives. Same-prefix gate1 result/log and candidate1 frozen map/source.
No local Cargo remains from these owned runs; verify actual processes before next.
Own CI39298010083 attempt1 all3 succeeded. Complete raw/source/API ZIP and
independent typed/project/all35 fields/types passed. Indexed109-member hosted
archive SHA95f70fe3f45d554cac275f7325b51644c54012a43e237544fd5c50a2752ac10c.
Raw review/checker55074/comparator+archive55849 DONE, never rerun. Exactly nine
runtime deltas promoted; canonical main matches accepted source12a2f2a/all342.

New isolated atomic empty-manifest creation candidate: worktree
/home/marshall/src/worktrees/s3drive-empty-manifest-create, branchcodex/empty-manifest-create,
source0c7f8b72bcea830760f437036d201eb9c077ea1b/schema39/all349 frozen.
Core2/state9/HTTP1 focused cases and workspace all-target Clippy passed.
Full local gate1253500/child1253506 and own CI39302692507 attempt1 are running.
Exact status/log/frozen inputs: /tmp/s3drive-empty-create-{gate1,candidate1}-*.
Check live PID/startticks/results before touching runtime; one shared local Cargo.
No S3 upload or native creation enabled. Authenticated publisher remains next.


Next native prerequisite investigation: source/open namespace journals use compact
content references and already share Storage, but their constructors reject sparse
state. FS lifecycle pre-callback work still flushes whole-generation files. Prefer
one typed opt-in SQL composition, keeping legacy logical writers isolated. Empty
file creation may bootstrap a real canonical empty manifest and atomically publish
its namespace entry, avoiding a fabricated manifest identity for metadata-empty
nodes. This is a source-reviewed option, NOT a declared API or implemented phase;
validate canonical root, authority receipt/history and create/recovery semantics
before code. Retention cuts and all retry namespaces still require their contracts.


1. **Upload authorization batch** — worktree
   /home/marshall/src/worktrees/s3drive-mixed-upload-auth-batch,
   branch codex/mixed-upload-auth-batch. Runtime d4a46c5; published CI source
   2fc45fed5a15c1a24f0f9713ab7caca02569ec5b. Full local gate passed, all324 unchanged,
   normal four threads:23publication/446state1ignored/37core/22reader1nativeignored/
   62S3, formatting/Clippy/build/book. Own CI39276521247 attempt1 has book118142124084
   and Linux118142124394 and Windows118142124251 all succeeded. GET-only poller
   828884 CLOSED0; one-use checker/raw comparator/hosted archiver DONE, never rerun.
   Own source/API ZIP digests/independent typed21+14/project/all35 raw fields/types
   passed. Hosted104-member archive SHA
   d1bab01a5bf10a5c928f16ff70b0bc18f6718b8ac64af09b5e1e90e9fb34145f.
   At that batch checkpoint, all324 inputs matched its tested source exactly.
   [Contract/evidence](docs/src/mixed-upload-authorization-batch.md).
   Local673-member archive SHA67df76b1e22be23d738cc3ec9225b1921ac2223eab589171a91a663bec857b19
   is DONE; never rerun its archiver. This acceptance does not promote schemas36/37.

2. **Scoped operations** — combined source accepted, schema36/all329. Originala3028bf
   CI39267986023 succeeded, but its local gate failed. Repair worktree
   /home/marshall/src/worktrees/s3drive-scoped-source-operations-repair,
   branch codex/scoped-source-operations-repair, runtime170979e/head1ad9dc6.
   Full repair gate2 exited101:23publication passed/1 upload Timeout; exact case
   alone passed89.91s. Original and repaired frozen sources/logs remain unchanged.
   New combined worktree /home/marshall/src/worktrees/s3drive-scoped-source-operations-batched,
   branch codex/scoped-source-operations-batched, checkpoint6115775/schema36/all329.
   Accepted batch changes merged cleanly; the unsuccessful two-family SDK limiter
   was removed. Ordinary four test threads, old30s operations/180s child deadlines.
   [Combined contract](docs/src/scoped-batched-repair.md). Frozen candidate1 map/source
   /tmp/s3drive-scoped-batched-candidate1-frozen.json and -source.tar.gz.
   Full gate runner978816/child978818 CLOSED0 at16:29:24Z, all329 unchanged.
   Publication24/164.37s,460state1ignored/37core/22reader1nativeignored/62S3 plus
   format/Clippy/build/83MD/79chapters passed at normal four threads. Indexed348-
   member local archive SHAaa3ecd77ca9c1539ef612db05fb23d057bd1b92040eb123fdb1a81d5a4929143.
   One-use local archiver DONE, never rerun. No local Cargo remains from that run.
   Own accepted source44f0ee19077c7623465faf62834858821cd1ee47; CI39286027359
   attempt1 all3 SUCCESS: book118172159855/Linux118172161091/Windows118172161013.
   Observer1040546 CLOSED0/all_success. One-use checker/raw comparator/archives DONE;
   never rerun. Own source/API ZIP digests/independent typed21+14/project/all35 raw
   fields/types passed. Hosted104-member archive SHA
   5da274104d631b59c0d951917f617d11d6b7c19d3bfd765a90f7b009196527bd.
   All329 inputs matched at schema36 promotion. Schema37 combined acceptance
   below supersedes this runtime with all337 exact inputs.
   [Original scoped contract](docs/src/scoped-source-operations.md).

3. **Manual pin registry** — worktree
   /home/marshall/src/worktrees/s3drive-source-read-pins, branch codex/source-read-pins,
   runtime b336567/head5b6b270/schema37/all333. Based on unaccepted scoped repair.
   Component regression passed474state/1ignored/247.54s; focused crash/full-disk,
   metadata-only recovery, state Clippy and workspace compilation passed.
   [Component evidence](docs/src/source-read-pins.md), archive SHA
   b27862b4b6fad34932ca51fe05bc5d1d0910e1ffbbabd103247792b68003fb2b.
   Its one-use archiver is DONE. Manual token clones are metadata copies, not
   lifetime guards. Do not promote this schema ahead of its parent acceptance.

4. **Clone-sharing guards** — isolated worktree
   /home/marshall/src/worktrees/s3drive-source-read-guards, branch codex/source-read-guards,
   checkpoint eef4da2/schema37/all336. One shared pin and bounded pending cleanup
   survive clone drops, cancellation and all eight queued commands. Six focused
   cases passed38.18s; state all-target Clippy passed15.35s. Initial fixture and
   Clippy failures are retained under /tmp/s3drive-source-guards-*.
   Broad component runner881065/child881068 CLOSED0 at15:58:29Z:480state/1ignored/
   266.78s, all336 unchanged. See /tmp/s3drive-source-guards-state1-result.json.
   [Component evidence](docs/src/source-read-guards.md) includes a verified358-member
   archive SHA627cf14ad2f0bf8397a783e41a7a7c709aa8e1c01e949745e83c18755be1ba76.
   One-use archiver DONE, never rerun. No local Cargo remains from these runs;
   inspect actual processes before the next one. Managed EditedFile integration,
   full workspace/platform acceptance and parent integration remain required.

5. **Managed reader integration** — isolated codex/managed-reader-guards at
   /home/marshall/src/worktrees/s3drive-managed-reader-guards, checkpointb28ca7b,
   schema37/all337 frozen. Seven state guard cases passed48.89s; SDK3 lifetime case
   passed30.01s; workspace all-target Clippy passed35.45s. SDK1 fixture compilation
   and SDK2 double-prefixed cancellation-target failures are retained and fixed.
   Component runner931670 CLOSED0 at16:16:54Z, all337 unchanged:481state/1diagnostic
   ignored/298.56s and23SDK reader/1native-only ignored/46.92s. See
   /tmp/s3drive-managed-guards-component1-result.json. One-use component archiver
   DONE, never rerun. Verified355-member archive SHA
   4a1324c00285c6d887987e3cf2de0abc9e09ef5ad888b88c7a47564ee86ee8c6.
   [Managed reader evidence](docs/src/managed-reader-guards.md). This is not its full
   workspace/platform acceptance. After batch/scoped parent integration, adapt the
   new batch private chunk-read path to the guard and require fresh full/platform
   source acceptance. Main is now accepted schema38/all340.

6. **Managed-reader/batch integration** — isolated worktree
   /home/marshall/src/worktrees/s3drive-managed-reader-batched,
   branch codex/managed-reader-batched, checkpointefb2ab4/schema37/all337.
   Five-path merge from guard component8b29461; four paths match scoped6115775
   exactly, and preparation keeps guard reads in ordinary/private batch paths.
   No managed composition .local.read bypass remains. SDK admission limiter removed.
   Full gate1 runner1041992/child1041993 CLOSED0 at16:50:32Z, all337 unchanged.
   Publication24/168.58s,481state1ignored/23reader1nativeignored/37core/62S3 plus
   format/Clippy/build/87MD/80chapters passed at normal four threads. Verified348-
   member local archive SHA3927d8dda7400c685e17d7cb44c35f76acf5aee5c1c299c4fd75dccd224745d1.
   One-use local archiver DONE, never rerun. No local Cargo remains from this run.
   Own accepted source903ade67e21cd71bb7ab02716c098993d9f6e82f, CI39289349689
   attempt1 all3 success: book118185617231/Linux118185616746/Windows118185617331.
   Observer1101033 CLOSED0/all_success17:24:27Z. One-use checker45561 and comparator/
   hosted archive44680 CLOSED0; DONE, never rerun. Full raw/source/API ZIP/
   independent typed21+14/project/all35 raw fields/types passed. Hosted104-member
   archive SHAa5f64762435bcb81f33f7c25a26b6d9f773587bb127aa310d0efc23b4cf08e2b.
   Own artifacts replacement12204512424/disposition12205563584; no parent reuse.
   Exactly26 runtime deltas integrated; all337 inputs match tested source.
   [Current acceptance](docs/src/managed-reader-guards.md). Main schema37/all337.
   [Native boundary review](docs/src/native-chunk-integration.md) maps the actual
   config/writer/state-role/origin/unlinked-root gaps; the new SVG was rendered and
   visually inspected. It is a proposed bridge, not driver activation or GC.

One local Cargo at a time. All worktrees share /home/marshall/src/s3drive/target.
Inspect actual PID/startticks or terminal result, not only a lock/status file.
Keep failed exact sources/maps/logs. Owned runner success requires unchanged inputs.
Normal push uses task-local gh credential helper; do not change global credentials.
No subagents are authorized for the current continuation.

## Remaining work toward the full goal

Managed edited readers/preparations now share accepted guards. Before reclamation,
define clean-anchor witnesses and close every source/preparation/root/physical retry
namespace before deleting records or bytes. See [retention plan](docs/src/mixed-retention-plan.md).
No arbitrary dirty-view seal or copied hash proves a safe cut.

Implement safe fresh external-base adoption only for proven clean/no-unresolved
local state with exact-head CAS and preserved old readers. Define metadata-empty
and legacy origins. Enable the chunk pipeline in actual FUSE/WinFsp create/write/
truncate/flush/save-replace/rename/delete/open-unlinked lifecycles; native callbacks
must preserve uncertainties and dirty data. GC and mounted chunk writes are absent.

Still required: independent-host partitions/deployment; authority continuity,
fencing and restore; real managed Wasabi/large files; physical MX and Windows10/11
sign-in/reboot/SSH behavior; packaging/distribution/licensing. No user startup,
root/credential, hosting or Turso settings have been changed on this workstation.

## Provider authorization boundaries

Ignored local configuration is /home/marshall/src/s3drive/s3drive.local.toml.
Wasabi aliaswasabi, bucket s3drivetest, regionus-east-2, profilewasabi-s3drive.
Never print keys or configuration source. Prior three-object Unicode scope and
six-key scope s3drive-conformance/d147830ba5257d762e14c3962cde60f9/ completed and
were cleaned. The latter consumed all14 PUT attempts on2026-10-16 at23:36:18Z.
These approvals do not authorize new mutations. A later optional plan was prepared
but NOT approved/executed; pointer /tmp/s3drive-next-wasabi-plan-location.txt.
Any new managed live test needs a concrete bounded plan and explicit authorization.
Do not extend expiry or treat old capability evidence as permission.

## Historical record

The previous handoff is preserved byte-for-byte in
[the complete pre-refresh archive](docs/archive/HANDOFF-before-source-guards-refresh-20261019.md).
SHA-256: a1bae36a18bb5984809249bd2a5d1658727efeefaba8fa3d3a274c33fd15f017.
It contains historical source/job/artifact IDs, rejected explanations, prior
archives and provider plan details. Its old live/private notes are superseded by
this handoff and AGENTS.md. Before: 2708 lines; after: 154 lines.
