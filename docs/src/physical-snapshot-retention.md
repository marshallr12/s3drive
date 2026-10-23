# Physical snapshot lifetime protection

Candidate, 2026-10-23. **Not accepted; no deletion or bounded-disk claim.**
This closes a concrete gap in the [local retention plan](mixed-retention-plan.md):
physical staging `Snapshot` handles previously depended on sealed generations
never being reclaimed. Source-view pins did not protect those independent handles.

## Same-owner capability

Each private State owner now keeps a bounded-by-live-generations weak registry of
physical reader tokens. Snapshot acquisition validates the exact sealed generation
and complete manifest before registering its token in the same serialized worker
command. Concurrent clones share a strong token. Repeated independent acquisition
for one generation shares its registry entry. Expired entries are pruned at every
acquisition and observation, so completed readers do not accumulate tombstones.

Reclamation remains absent. The observational retry checkpoint now reports exact
protected physical generations for all retained preparations on its target. A
live physical reader makes that checkpoint blocked even when no source-view pin
exists. Acquiring or releasing the last physical reader invalidates an earlier
checkpoint. A future collector must revalidate in its own serialized transaction;
an earlier unblocked observation is never deletion permission.

The registry has no asynchronous release, command-queue dependency, permanent
SQLite read transaction or timestamp lease. Losing a reply drops only the
unexposed result token. Existing clones retain their protection. A cancelled
queued request cannot expose an unregistered reader. Concurrent drops can only
make a checkpoint conservatively overprotect, never authorize too much deletion.
New acquisitions cannot race the owner's active command.

These tokens intentionally are not durable: physical snapshots are process-local
handles bound to the original worker channel. Closing that owner first closes all
its readers, even if an old clone is still alive, before releasing the database
lock. After process death, a replacement can acquire the exclusive ownership lock;
no old process-local reader can read through the new owner's channel. Restart
neither deletes physical bytes nor reuses an old capability. This differs from
persisted source-reader pins and does not weaken their independent contract.

## Acceptance

Dedicated cases cover4096 transient acquisitions beside a held clone, exact
checkpoint invalidation, concurrent cloned reads, no SQL changes, lost running
reply, invalid generation admission, owner closure/replacement and actual killed
owner with a held clone. Two registry-only tests and workspace Clippy pass locally.
The storage/worker tests need the unchanged standard Linux/Windows durable-storage
gates; the local overlayfs/tmpfs guard remains unchanged. Full workspace/native,
source/log/artifact and compatibility regression review remains required.

Schema49 and all stored formats are unchanged. No cleanup API, provider action,
SDK/native profile activation, quota increase or deadline relaxation is introduced.
Compact authenticated anchors, terminal retirement and measured DB/WAL/staging
boundedness still remain necessary before local reclamation can be accepted.
