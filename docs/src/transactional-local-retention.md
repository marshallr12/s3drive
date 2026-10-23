# Transactional ordinary-store local retention

Accepted ordinary-store candidate, 2026-10-23. **Native/populated-native migration remains unsupported.**
This schema50 state-library slice follows [populated ordinary migration](populated-retry-migration.md)
and [physical reader protection](physical-snapshot-retention.md). It is explicit,
local-only and unavailable to native/owned owners. No native wiring, default activation, provider deletion or deployment is included.
The ordinary SDK bridge adds explicit scoped preparation and root
admission, not automatic migration or cleanup.

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
bytes and the original deterministic staged-operation-to-plan mapping independently.
A hash alone is not a severance proof. Missing, changed,
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

## Pre-acceptance review corrections

The first schema50 candidate339b535 had two review findings: unrelated inherited
witness page blobs were omitted from the collector's dependency union (the final
validator rolled the attempted cleanup back), and the compact anchor did not
independently bind its copied plan to the original physical staging operation.
The correction uses the complete validated object-blob dependency enumeration
and rederives that exact operation from the retained root and plan. New tests
cover a pending unrelated target with nested inherited witness pages and a fully
rehashed forged plan/source/anchor with unchanged physical evidence. These fixes passed structural review at970eca9 and the full accepted run below;
the earlier tree is not accepted.

## Explicit ordinary SDK bridge

`EditedFile::begin_scoped_preparation` derives exact preparation/generation
identities from an explicit epoch token. Known exact plans replay before fresh
network authorization; new plans retain the original source and policy checks.
`MixedPublisher::begin_scoped` requires that exact admitted token and derives its
root operation. Neither method changes defaults or activates retry mode. Explicit
local migration/checkpoint/retirement remains on the same state owner client.

An authenticated loopback SDK fixture starts with populated legacy ordinary
history, migrates it explicitly, runs three more scoped publication/retirement
cycles and reopens after each. It checks binary/Unicode reads, held EditedFile
clones, lost publication replies, exact no-RPC preparation replay and retired
recovery refusal without RPC. Modeled state receipts are not substituted for this
SDK test. The fixture uses disposable local authority/provider servers and dummy
profiles only. Its hosted gates passed in the accepted run below; no real-provider
acceptance is inferred and native populated migration remains blocked.

Initial Linux run40025691644 passed all13 selected retention cases, including
300 cycles with database peak1,474,560 bytes and WAL peak4,420,792 bytes; late
peaks matched. It subsequently failed an existing future-version corruption
fixture whose literal50 became the current schema. The fixture now derives
SCHEMA_VERSION+1 and still requires refusal. This failed run is historical partial
evidence only, not candidate acceptance; Windows and corrected full gates remain
required.

SDK candidate42eb926 passed its dedicated Linux state/SDK/native gates but full
workspace acceptance found two historical43/44 upgrade-oracle failures. Log
comparison showed no changed legacy definitions or rows: only the two exactly
empty schema50 tables were additional. The corrected oracle verifies their exact
DDL and emptiness alongside schema49's existing addition, then retains the full
original catalog comparison. It does not ignore arbitrary tables or data. The corrected full rerun passed below;42eb926 itself is not accepted.

## Accepted exact candidate evidence

Candidate `f77f75684436c0afbed0fba15797d9819ffa61b0`, tree
`0fccf3615a4879e3d85879a515448d28d0412b72`, passed independent review of
[run40030136888](https://github.com/marshallr12/s3drive/actions/runs/40030136888):
Linux1053/Windows1049 workspace tests in37 cohorts;20 retry-safety, seven
physical-reader and15 retention-filter cases per OS; the ordinary SDK fixture;
full native regression gates in debug/release;794 exact source files and six
artifact ZIPs. Genuine46/47,47/48,48/49 and49/50 executable compatibility passed,
including an actual old executable refusing a reclaimed store unchanged.

Both OS300-cycle fixtures measured DB peak1,474,560 bytes and WAL peak4,420,792
bytes, with identical late peaks and one retained physical root/source anchor.
These are bounded disposable-fixture results, not arbitrary-workload capacity or
native mounted-history acceptance. The original quotas, storage guard and
120-second native sequence deadline were preserved. Local Clippy/build/schema
and documentation checks passed; local durability acceptance was not substituted
for hosted evidence. Main verification remains pending at this documentation
revision. No real user data or provider objects were migrated or reclaimed.
