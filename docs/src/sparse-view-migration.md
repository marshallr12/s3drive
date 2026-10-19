# Preserving base views for editable versions

Research and implementation date: **2026-10-19**. W2b2a is the schema migration
prerequisite for patch mutations. It does not enable writes or claim that local
base metadata authenticates remote content. See the [base-view architecture](sparse-base-views.md).

Schema29 admits one initial view per file. Its UNIQUE constraint on
`sparse_views.target` must be removed before immutable child versions can share
the same file identity. Existing view IDs, request/result BLOBs, state hashes,
operation keys and file heads must survive unchanged. Creating another target
for the same file would evade conflict detection and is not the chosen design.

SQLite documents a transaction-based table rebuild for changes to UNIQUE
constraints and warns that rename order matters. Its foreign-key rules also
apply to DROP TABLE. These are important constraints on this migration, rather
than evidence that a particular rebuild works in S3Drive.
[SQLite ALTER TABLE](https://www.sqlite.org/lang_altertable.html),
[SQLite foreign keys](https://www.sqlite.org/foreignkeys.html).

An experiment against the resolved rusqlite/bundled SQLite build found that
copying the view table, dropping it and renaming its replacement failed at
commit with foreign-key error787, despite a clean final foreign-key check.
The tested alternative copies the three related tables into temporary tables,
empties their cyclic references, rebuilds the view table and restores the exact
rows in one transaction. Foreign keys stay enabled, with checks deferred only
until commit. Migration30 uses this sequence and explicitly checks the final
foreign keys and complete decoded records. It neither disables foreign keys
nor edits SQLite's schema catalog.

State opening now explicitly enables foreign keys before starting migrations;
it does not rely on a compile-time default. Before rebuilding, the migration
validates old records and refuses triggers attached to the four sparse-view
tables. This prevents copy-time DELETE/INSERT triggers from changing data.
Any SQL/validation failure rolls back the transaction and schema version.
Both Windows fixture expectations move to30 with the Rust schema version.

The focused controls construct actual schema29 DDL with populated cyclic
references, compare every persisted sparse BLOB before/after reopening, check
old operation replay, and exercise SQL constraints. Target uniqueness is removed;
view identity uniqueness and the target foreign key remain. A mid-copy temporary
name collision rolls back the first copy and preserves the conflicting table;
an attached persistent trigger is refused without executing it. Successful and
failed migrations leave no deferred-FK setting behind.

This is W2b2a only. The validator and public API still admit initial base/zero
views, so unsupported child records remain refused. W2b2b must add immutable
patch blocks, multiple-version validation, exact parent/head mutations and
receipts, quotas, truncation/growth rules, older-view reads and crash recovery.
W2c must compose authenticated remote bytes with verified local patches. Mixed
publication, settlement and native chunk-write acceptance remain separate work.

The [dated evidence](evidence/sparse-view-migration-20261019.json) records the
experiment and controls. Validation status: all12 final focused tests passed on
this Linux workstation; the first full gate exposed an allocator-dependent older disk-full fixture. A
receipt-update pressure trigger now forces actual SQLITE_FULL, retains the
original dirty/receipt assertions and checks that pressure rows roll back. The
focused repaired test and full local gate6893 passed. All276 frozen inputs were
unchanged:380state tests (one ignored),37core,11reader (one native ignored),62S3
and all format/Clippy/build/72MD link/68book chapter checks. The
[indexed local archive](evidence/sparse-view-migration-local-20261019.tar.gz) retains
21 verified members, including the failed gate and exact older test, experiment,
repaired fixture, source maps and successful gate. This source still requires
its own combined Windows/Linux native acceptance; no provider acceptance is claimed. No S3 requests or credential changes were made.

## Proposed patch-mutation contract

The next slice will identify a mutation by operation key and exact parent view,
plus write offset/data or new size. An unknown operation must compare that parent
with the current file head in the same transaction. A known exact operation must
return its original result even after newer edits; changed arguments must return
a conflict. No implicit rebase or overwrite fallback is proposed.

Each file will retain one initial root and a bounded linear chain of immutable
children. Validation must establish the entire chain to the current head, reject
forks/orphans/cycles and reproduce each child's canonical extent map from its
parent and request. Existing root serialization and hash domains stay unchanged.
This requires a new child request/record format rather than reinterpreting old
bytes. Reading an older view must continue to use that older view's map.

Writes replace only intersecting extents and use explicit zeros across holes.
Truncation discards the removed tail in the child map; later growth adds zeros,
so remote or local tail data cannot reappear. Adjacent compatible extents should
merge. Proposed patch blocks are at most1 MiB, keyed and checked by their full
byte digest before returning any slice. Individual mutations/ranges remain
bounded at8 MiB and logical files at64 GiB. File, version, operation, extent,
metadata and retained dirty-byte quotas must be separate; exhausted quotas may
refuse new work but must preserve existing bytes, pinned views and exact replay.

Patch BLOBs, new immutable records, operation results and the head change must
commit together. The post-write state must equal the old state plus precisely
the intended additions/head change, including when SQL triggers ignore or alter
writes. Process-kill, SQLite-full, late replay, overlapping writes, shrink/regrow,
corruption and worker-owner controls remain necessary. These paragraphs are a
proposal for the next implementation, not current behavior or acceptance.


Published source `8141996` includes accepted main ancestry and matches all276
locally gated runtime inputs. Its
[own Windows/Linux/book run](https://github.com/marshallr12/s3drive/actions/runs/39205603137)
is in progress. This is publication evidence, not hosted acceptance. Main remains
on accepted base-view runtime/schema29; this follow-up schema30 source needs its
own complete native and reference-artifact checks.
