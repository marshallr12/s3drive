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

Main runtime remains W5b1 clean base epochs, schema35, all324 frozen runtime inputs.
Accepted integration: 0ff32cf72f8dfc8006d4f599c6104255160f3c75.
Accepted hosted source: a3574998138d0e4dfa2c62a28c2ada1e18099732.
CI39263559455 attempt1: book118097200982, Linux118097201820 and Windows118097201387
all succeeded. Local full gate3 passed with all324 unchanged. Own raw logs/source,
API ZIP digests, independent typed21+14/project/all35 raw fields and JSON types
were checked. Native legacy FUSE9/WinFsp7, manifest ranges/remount and paired
no-systemd/Windows startup passed. Actual images: Ubuntu24.04 20261012.334.1;
WindowsServer2025 windows-2025-vs2026 20261007.264.1. These are not native chunk
write, Wasabi or physical desktop/reboot proofs.

[Accepted epoch evidence](docs/src/mixed-clean-epochs.md) includes indexed local
archive SHA9c46f7b89c352a5b3b97297c39c7fd5aca4980b4683d85bdc6254d07acbcb48b
and hosted archive SHA2dbf5b3c542b57f7dff302cea67ccb210a38f4dcac08768b20539dd1fc37de9a.
Completed one-use acceptance/archivers must NEVER be rerun. Current canonical
publication proof: /tmp/s3drive-canonical-clean-epochs-publication.json. Ordinary
later documentation commits do not change the accepted runtime.

The accepted path includes authenticated manifest reads, immutable local patches,
mixed plans/dependency uploads, exact root publication/recovery, local lineage and
clean epochs. Mounted writes still use the older bounded whole-generation path.
Offline authority snapshots support create/inspect, not writable restore/failover.
Earlier failed Windows transport cohorts are retained; later success does not prove
their cause. Never equate portable unit tests with an actual mount or flush.

## Active isolated work and exact next actions

1. **Upload authorization batch** — worktree
   /home/marshall/src/worktrees/s3drive-mixed-upload-auth-batch,
   branch codex/mixed-upload-auth-batch. Runtime d4a46c5; published CI source
   2fc45fed5a15c1a24f0f9713ab7caca02569ec5b. Full local gate passed, all324 unchanged,
   normal four threads:23publication/446state1ignored/37core/22reader1nativeignored/
   62S3, formatting/Clippy/build/book. Own CI39276521247 attempt1 has book118142124084
   and Linux118142124394 success; Windows118142124251 is in progress at the latest GET
   (final cache-save step). Poll the same job; never restart on an observation
   timeout or HTTP502. GET-only poller828884/startticks143026538 is tracked in
   /tmp/s3drive-upload-auth-batch-hosted-poll2-{running,result}.json.
   First watcher826436/826437 exited1 on HTTP502, not a workflow failure.
   Prepared checker/raw comparator/hosted archiver have NOT executed. Complete
   raw book/Linux logs are retained; download Windows only when terminal.
   Review the exact cohort, independently check own artifacts and source, then
   integrate only if accepted. [Contract/evidence](docs/src/mixed-upload-authorization-batch.md).
   Local673-member archive SHA67df76b1e22be23d738cc3ec9225b1921ac2223eab589171a91a663bec857b19
   is DONE; never rerun its archiver.

2. **Scoped operations** — schema36/all329 remains UNACCEPTED. Originala3028bf
   CI39267986023 succeeded, but its local gate failed. Repair worktree
   /home/marshall/src/worktrees/s3drive-scoped-source-operations-repair,
   branch codex/scoped-source-operations-repair, runtime170979e/head1ad9dc6.
   Full repair gate2 exited101:23publication passed/1 upload Timeout; exact case
   alone passed89.91s. After batch acceptance integrate it and restore normal SDK
   admission if the full regression permits; require a fresh full gate and own
   hosted cohort. Never reuse the original prototype's platform artifacts for
   changed sources. [Contract](docs/src/scoped-source-operations.md).

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

One local Cargo at a time. All worktrees share /home/marshall/src/s3drive/target.
Inspect actual PID/startticks or terminal result, not only a lock/status file.
Keep failed exact sources/maps/logs. Owned runner success requires unchanged inputs.
Normal push uses task-local gh credential helper; do not change global credentials.
No subagents are authorized for the current continuation.

## Remaining work toward the full goal

Connect every managed reader to clone-sharing guards before reclamation. Define
clean-anchor witnesses and close every source/preparation/root/physical retry
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
