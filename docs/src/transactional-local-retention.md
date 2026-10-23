# Transactional ordinary-store local retention

Candidate, 2026-10-23. **Not accepted until exact-source hosted gates pass.**
This schema50 state-library slice follows [populated ordinary migration](populated-retry-migration.md)
and [physical reader protection](physical-snapshot-retention.md). It is explicit,
local-only and unavailable to native/owned owners. No SDK/native wiring, default
activation, provider deletion or deployment is included.

## Contract

`publication::Journal::retire_checkpoint` and its owner client consume an exact
observational checkpoint, revalidate it inside the serialized owner's immediate
SQLite transaction and return only the retained historical clean-view identity.
They never manufacture a fresh remote receipt. Every target preparation must have
complete verified physical publication, lineage and source acknowledgment. Dirty
heads, conflicting or uncertain work, persisted source pins and live physical
snapshot clones prevent retirement. A pin blocks the entire target conservatively;
this does not yet collect around long-lived protected history.

The transaction retains one clean source anchor and one independently checked
physical root per target. Its compact witness binds the original source identity,
preparation input, original root page, terminal publication, acknowledgment and
owner identity. Loading it rederives the exact clean rebase, verifies canonical
identities and quota bounds, then verifies the retained physical receipt and root
bytes independently. A hash alone is not a severance proof. Missing, changed,
foreign-owner or inconsistent evidence fails closed. Ordinary source edges still
require every ancestor; only this exact validated anchor can terminate a chain.

Retirement removes only the fully validated target history: old source operations,
views and unreferenced blocks; preparations and unreferenced blobs; publication
checks/records; and retired physical operations, receipts, generations, extents and
unreferenced chunks. Foreign dependencies abort rather than being cascaded away.
The retained root supplies independent evidence across restarts; replacement by a
later clean anchor also checks readers of that earlier root. No filesystem unlink,
remote mutation, VACUUM or timestamp lease is used. SQLite may reuse freed pages;
this is not a promise to shrink a preexisting oversized database file.

The permanent retry mode remains enabled. Removed old source/preparation/root/
physical identities cannot be admitted as fresh work because their old epoch or
reserved operation namespace remains fenced. An exact repeated retirement can
return the currently retained historical anchor after a lost commit reply; it
cannot produce a new receipt. Once that anchor is superseded, its retired plan is
missing rather than resurrected. There is no accumulating per-operation tombstone.

Populated legacy ordinary stores must first pass the explicit migration proof.
Their legacy history can be retired only after a new scoped clean publication;
legacy admission is never reopened. Native/owned populated stores remain refused,
as their namespace/open/Close and SDK migration contracts are not implemented.

## Acceptance gates

Disposable fixtures exercise300 publish/acknowledge/retire cycles, exceeding the
prototype's256-plan and512-view limits without raising either limit. They assert
one retained staging root, one source anchor, no old plans or payloads, database
and WAL ceilings, and restart every31 cycles. Measurements are emitted for review;
passing compilation alone does not establish boundedness.

Adversarial cases cover stale retries at every layer, cloned old-root readers,
concurrent read/retire, dirty and uncertain work, migrated legacy history, empty
files, actual SQLite full-disk rollback, killed owners at six transaction boundaries,
missing/corrupt/forged/foreign-owner anchor evidence and corrupt physical evidence.
All original corruption, quotas, deadlines, native and durable-storage gates stay.

Schema50 adds exactly two initially empty tables. Genuine separate schema49/current
executables must prove additive upgrade with complete old data preserved and old
refusal without byte changes. A separate old executable also opens a reclaimed
fixture and must refuse unchanged. Existing46/47,47/48 and48/49 checks remain.
Workspace Clippy/build and schema checks pass locally. Durable runtime acceptance
requires standard Linux/Windows runners because the workstation's overlayfs/tmpfs
fails the unchanged storage guard. Full logs, exact sources and artifacts must be
reviewed before merge. Private failed-run evidence stays unpublished.
