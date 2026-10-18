# Exact chunk source acknowledgments

Implementation date: **2026-10-18**. This state-library slice adds an explicit
historical acknowledgment for one frozen chunked file snapshot. It follows the
[root publisher](chunk-root-publisher.md), whose root object and original file
snapshot have different identities and bytes. Mounted chunk callbacks remain
unfinished. Controller integration is being tested in a
separate slice below. The full local gate passed; this state source's
own source-bound Linux, Windows and book acceptance also passed.

![An exact snapshot acknowledgment is stored by the same SQLite owner that retains its chunk tree and authority receipt. A newer edit stays pending.](images/chunk-source-settlement.svg)

`Storage::chunk_publications().sources()` returns a facade on the same private
SQLite owner. `settle_source(plan_id)` accepts only an admitted plan ID and
returns an opaque `SourceReceipt`. `source_status(plan_id)` returns the complete
publication record, optional exact source acknowledgment and current sealed head.
It keeps unresolved requests and conflicts visible. A matching sealed head says
nothing about building drafts; neither API declares a whole file clean.

The acknowledgment binds the exact source generation and manifest to the complete
committed publication hash, root generation/checksum and historical authority
receipt. It requires an attached source and a successful durable publication
receipt. An unknown or rejected operation cannot become a source acknowledgment.
Callers cannot supply a previously returned root/tree receipt as sufficient proof.

## Persistence and recovery

Schema 28 adds `chunk_source_settlements` and `chunk_source_role`. Existing
publication JSON and hash domains remain unchanged. The new rows have separate
versioned integrity domains, bounded serialized cells and a retained count that
detects missing acknowledgment rows. Permanent reciprocal exclusion of legacy
logical/namespace/open state remains, including orphaned new tables. Authority
initialization, restart preflight and snapshot classification reject nonempty
client settlement state.

An immediate transaction reloads the frozen publication and source mapping,
checks the complete historical chunk receipt tree and validates same-target
ancestry to the current sealed head. It inserts the new proof and retained
count, then reloads the exact effects and repeats the publication/tree, source
lineage and isolation checks after the final write. No mutation or callback
intervenes between that final proof and commit. Repetition returns the identical
receipt. A lost reply requires inspecting/repeating the exact plan; it does not
authorize a replacement operation.

The added lineage walk admits at most 4,096 generations and 1,000,000 total
extents. Exceeding either returns a limit error and retains all history. These
are walk bounds, not a latency guarantee: existing complete publication/tree
validation has separate costs that need measurement before native workloads.
Transitive journal reads now bound the phase field before String conversion and
project chunk digests through a 32-byte substring before allocation. Valid digest
bytes, NULL-hole encoding and existing manifest hashes retain their meaning.

## What the acknowledgment means

The proof records a **historical commit**. It does not hash current local payloads
or query current S3 objects. Same-length local payload corruption after upload
does not invalidate that historical linkage; reading damaged bytes still fails.
Missing or inconsistent retained proof metadata does block acknowledgment.

Settlement of snapshot 21 does not settle snapshot 22 or any ancestor. A later
remote root does not rewrite snapshot 21's authority receipt. The generic staging
pending list, `write_journal.status`, physical remote flushes and physical
settlements remain unchanged. Generic physical upload intent on the exact source
snapshot is refused as mixed semantics. Callers must use the explicit chunk
status rather than infer a generic file flush.

## Verification scope

The focused state suite passed **31 controls**, including **16 new controls**, in
54.76 s. These use real SQLite and its metadata authority, with synthetic physical
upload acknowledgments; they do not establish real-provider behavior.

- Restart, concurrent first acknowledgment, lost local reply and process death
  after the role write, proof write and commit. Every killed child has an
  unsuccessful exit status and is reaped.
- First acknowledgment of an older snapshot after a newer remote root, local
  head and draft exist; acknowledging a newer snapshot leaves its ancestor
  unacknowledged. Generic pending history stays intact.
- Unknown/rejected publication, missing grandchild receipt, mixed physical
  intent, orphan tables, corrupt/oversized cells and schema migration collisions.
- Actual SQLite-full and ignored/deleted/rewritten SQL effects roll back the
  complete acknowledgment. The lineage boundary admits its exact limit and
  refuses the next generation without deleting the historical proof.

Independent static review checked the receipt binding and final immutable
transaction section. It identified the inherited allocation gaps and refinements
to first-acknowledgment/concurrency controls; those were addressed. Projection
inspection establishes the allocation bounds; the corrupt-cell tests establish
rejection, not measured peak memory.

The full local gate passed with all **252 frozen runtime/fixture hashes**
unchanged: 365 state tests / 1 ignored, all 16 new source controls, 59 S3 tests,
34 core tests, 50 authority tests, all 18 existing managed-publication tests,
8 chunk-publication tests, formatting, Clippy, build, 66 documentation files and
62 book chapters. The state suite took 562.46 s. Twelve long-test notices across
the workspace eventually completed; these times are gate observations, not a
benchmark. The [14-member indexed local archive](evidence/chunk-source-settlement-local-20261018.tar.gz)
retains the complete gate, focused controls, review, frozen source map, exact
implementation and runtime delta. Source `41b759a` is pushed on
`codex/chunk-source-settlement`, with all 252 hashes and the clean exact remote
tip verified. Public visibility was checked read-only before and after; main is
unchanged. Its [own hosted cohort](https://github.com/marshallr12/s3drive/actions/runs/39163298774)
passed all three jobs. All raw logs were reviewed. Its own finalized Windows
artifacts passed independent strict checks for 21 replacement and 14 disposition
records; all 35 raw log records matched every artifact field and type. Artifact
IDs were 12149243457 and 12149814008; no older payload was reused. The
[22-member indexed hosted archive](evidence/chunk-source-settlement-41b-hosted-20261018.tar.gz)
retains the exact source, logs, source-bound checker, artifact metadata/payloads,
strict comparisons and all 252 frozen hashes. Every indexed member was verified.

Linux's separate nine FUSE mount controls and no-systemd desktop/SSH supervisor
recovery checks passed. Windows passed its seven separate native controls,
32-flush HTTPS burst, disposition lifecycle, independent-logon coordinator and
paired drive startup/recovery. Windows state tests passed 360 / 1 ignored,
including all 16 new source controls. Retained warnings include the unused
WinFsp delay-import linker warning, Node/cache deprecations and three locked
fixture profiles left for disposable-VM cleanup after dummy credentials were
removed. Expected HEAD denial, retained content conflict and stopped-authority
errors remain in the logs. These checks cover the existing mounted format;
they do not activate mounted chunks or prove a physical reboot. See
the [dated evidence](evidence/chunk-source-settlement-20261018.json).

## Controller integration (local gate passed, 2026-10-18)

`s3drive_managed::chunks::{Publisher, Recovery}` now have an explicit
`resume_source(plan)` method returning the state's opaque `SourceReceipt`.
The existing `resume(plan)` still returns only a historical `RootCommit`.
The new method first completes or recovers durable root publication, then
invokes the same SQLite owner's source facade. The facade reloads and verifies
the exact publication; the controller does not substitute its returned root
commit for that proof. One controller-family permit spans both steps.

`Recovery::resume_source` needs the private local store, metadata client and
expected volume/store IDs. It constructs no S3 client and needs no S3 credentials
or current upload policy. A known root requires no metadata RPC; a prepared
unknown root replays only its complete saved Apply before local acknowledgment.
The publisher takes that historical branch before S3 policy or binding checks,
as its root-only API already did. If local acknowledgment fails after the root
was committed, the caller receives an error and can recover the same plan.

New real-SDK/dummy-S3 and authenticated-authority controls cover binary/Unicode,
sparse and empty sources, first historical acknowledgment after newer work,
missing/unprepared/wrong-scope refusal, actual killed-owner unknown/known-root
recovery with dummy credentials/policy removed and current payload damaged,
refused source insertion, expired/narrow policy, refused root-receipt storage,
same-family Busy and concurrent first acknowledgment. The parent counts all S3
requests during metadata-only recovery and checks unique immutable PUT keys.
The 12-control focused suite passed in 60.30 s. An additive damaged-record
refusal check passed separately in 0.90 s. The full local gate also passed:
12 chunk-publication controls, 365 state tests / 1 ignored, 59 S3, 34 core,
50 authority, 18 existing managed-publication controls, formatting, Clippy,
build and 62 book chapters. All 253 frozen runtime/fixture hashes matched.
The [12-member indexed local archive](evidence/chunk-source-controller-local-20261018.tar.gz)
retains the complete log, focused controls, exact source, implementation
self-review and frozen map. This was self-review, not independent review. One
existing retained-session quota test exceeded a minute and then passed; times
are gate observations, not a benchmark. Its own hosted cohort is still required.
See the
[controller evidence](evidence/chunk-source-controller-20261018.json).

No payload eviction/reclamation, role release, implicit migration, detached or
removal-owner publication, native chunk activation, fresh provider-health claim
or physical reboot acceptance follows. Controller acceptance requires the full
local gate and its own source-bound hosted cohort. The [controller contract](implementation-plan.md#next-controller-slice-explicit-source-acknowledgment-proposal-2026-10-18)
defines the distinct API, operation permit and subprocess recovery controls.
Verified manifest range reads and native
lifecycle barriers remain later requirements.
