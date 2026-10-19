# Acknowledgment and clean base epochs

Declared implementation contract, 2026-10-19. **Local and own hosted/native/source acceptance passed.**
W5a permits consecutive publications from one frozen source. W5b1 must now make
an exactly published head the next source base without rewriting old views.
No payload reclamation or native activation is part of this slice.

![Draft exact-head acknowledgment and retained epochs](images/mixed-clean-epochs.svg)

One acknowledgment names its local operation ID and publication plan. The private
owner validates the retained exact root record, full physical/preparation evidence
and immutable source view in one transaction. An unknown, rejected, reservation-
only, wrong-owner or corrupt publication cannot acknowledge data. If that exact
published view is still the file head, derive the next binding from its validated
publication receipt and root descriptor. Keep the same volume/node/store/source/
configuration scope. Add an immutable rebase transition to the existing parent
chain and atomically update the file's active binding and head. Its ranges are
base coverage of the published logical length. Do not copy old patch ranges into
the new binding. New edits use that new base; old immutable reads keep their old
binding, bytes, IDs, requests and hashes.

If a newer head exists, acknowledge only the historical published view and save
which head was observed; leave the active binding/head untouched. Exact replay
returns the first result even after more edits. Never upgrade a historical result
into a later clean transition, invent a second operation ID, or clear newer data.
A different acknowledgment operation for an already acknowledged plan conflicts.
Acknowledgment operations share the sparse-mutation identity namespace, including
historical acknowledgments that created no view. Reusing one for a new write,
resize, initial view or another acknowledgment must fail explicitly.

Extend existing root records with an optional separately versioned acknowledgment;
omit the field on old records to preserve literal bytes and hashes. Schema35 marks
the new record/view formats without rewriting old rows. Metadata-only recovery
may return existing acknowledgment evidence but cannot create it or change source
heads. It must still resolve saved schema33/34 requests without migration or
source BLOB reads. New acknowledgment fields under older schemas are refused.

Source inspection validates each rebase against its exact retained acknowledgment.
Use a bounded metadata-only helper that never recurses into source/preparation
validation. Full root validation separately checks source/preparation/physical
evidence. Validate both directions: no acknowledgment can claim a missing or
changed rebase, and no rebase can exist without its exact acknowledgment. A
scoped current-head API can select a newer base epoch while the existing exact-
binding head API keeps rejecting stale callers. Neither grants S3 permission.

Admission and SQL failures preserve the previous head, binding, all old bytes and
root receipts. Acknowledgment may fail locally after a remote commit; this cannot
be reported as a clean flush, and retry uses the original acknowledgment ID.
Configured limits cannot prevent replay of a committed acknowledgment. New clean
view admission still respects source limits. Retention phases must reserve or
reclaim the required capacity before enabling continuous native writes.

Done evidence: consecutive edit/publish/acknowledge cycles read the new base,
reuse unchanged chunks and retain old reads; newer-head historical acknowledgment
and concurrent writes; exact replay across lost replies/restart; identity conflicts;
actual SQLite-full, attached trigger and self-consistent graph corruption refusal;
kill boundaries covering view/operation/head/root/role/validation/commit; unchanged
old literal rows across migration; same-owner async APIs; real SDK/coordinator
integration and separate full local plus own hosted/native/source gates.

W5b2 pins/explicit retry retirement and W5b3 bounded local reclamation remain
required. All existing patch BLOBs, preparations, physical staging and journals
remain retained in W5b1. S3 object garbage collection, explicit legacy/empty
origins and W6 mounted write/truncate/append/flush/lifecycle gates remain separate.


Initial local evidence: workspace check passed. All17 inherited publication tests
passed. Eleven new state cases passed, including11 actual killed-owner boundaries,
SQLite-full rollback/retry, migration preservation/refusal, self-consistent graph
corruption, recorded acknowledgment recovery without source BLOBs, damaged metadata
refusal, operation collisions, narrowed limits, triggers and owner write/ack races.
The actual SDK/coordinator scenario passed four sparse3MiB+17 edit/publish/ack cycles
across restarts, preserving all old reads, reusing unchanged chunks after quiescent
acknowledgment, preserving a newer head after a historical one, and adopting an
empty manifest base. It used13 owned loopback PUTs in74.36s and checked an8KiB
acknowledgment-future budget. The first SDK harness assertion incorrectly included
preceding authenticated reads in the no-RPC interval; the observation is now scoped
to the acknowledgment itself. Failed source/log retained; no assertion suppression.
Full local and own hosted/native/source acceptance remain required.


Cross-machine clean-base adoption remains required before W6. This slice derives
bases only from our own validated publication receipts. A later authoritative
remote manifest must also be adoptable when the local head is proven clean and
has no unresolved publication or dirty descendant. That separate observation
path must use fresh authenticated metadata/S3 identity, preserve old reads and
CAS against the exact expected local clean head. It must never treat a remote
change as permission to overwrite pending local edits, or disguise an external
publication as our own root receipt. Current `current_head` is metadata selection,
not automatic remote reconciliation. Legacy and metadata-empty origins still
require explicit formats.

The first complete local gate retained all324 inputs and stopped at one Clippy
style diagnostic: an unnecessary unit expression in the new rebase-format match
arm. Removed that expression without changing the predicate or suppressing the
warning. Exact failed source/map/log retained. A fresh complete gate is required.


The second full gate retained324 unchanged inputs and passed all23 SDK/publication
cases, but two old schema29/30 migration fixtures failed: their mutation setup
predates the publication tables queried by the new identity guard. The guard now
explicitly recognizes those pre-publication versions; a current-schema missing
table still fails closed. Both failed migration cases passed afterward, as did
all12 acknowledgment cases including a new current-schema missing-table test.
An initially misqualified test-only module path was corrected; its failed source
and compile log are retained. A fresh complete gate follows these final changes.


## Complete local gate (2026-10-19)

Final source `652f93806349a729f603301d3669005685c5f40d`, schema35/all324
unchanged inputs, passed the full local gate at13:48:18Z:446state tests (one
ignored),37core,22reader (one native ignored),62S3 and23publication cases, schema
fixtures, formatting, all-target Clippy, build,80Markdown links and76book chapters.
All12 new acknowledgment cases and the actual SDK four-cycle/empty-base scenario
passed. Log SHA-256
`9484b5e6c0dc27087a33bbb6dbddbbc0f8704df75f8c38861357f859fda851c5`.

The [indexed local archive](evidence/mixed-epochs-local-20261019.tar.gz) contains1004
verified members,2542124bytes, with all324 final inputs and both exact failed
324-input sources plus retained failure/repair logs. SHA-256
`9c46f7b89c352a5b3b97297c39c7fd5aca4980b4683d85bdc6254d07acbcb48b`.
[Dated evidence](evidence/mixed-epochs-20261019.json) records exact scope and failures.
Own Linux/Windows/book/native/source acceptance is still required. Local success
is not mounted chunk-write, provider or physical session/reboot acceptance.


## Own hosted acceptance (2026-10-19)

Exact source `a3574998138d0e4dfa2c62a28c2ada1e18099732` passed
[its own Linux/Windows/book cohort](https://github.com/marshallr12/s3drive/actions/runs/39263559455),
attempt1, all three jobs. Linux446/Windows441 state tests (one ignored each),
37core,22reader (one native ignored),62S3 and23publication passed, including all12
new acknowledgment cases and11 actual epoch-kill boundaries. Actual legacy
FUSE9/74.58s, WinFsp7/92.63s, manifest-remount1.65s/2.79s and paired
no-systemd24.21s/Windows managed startup80.60s passed. These native regressions
ran on Ubuntu24.04 image20261012.334.1 and Windows Server2025 image20261007.264.1.

Complete raw review, exact324 working/committed source hashes, own API ZIP SHA,
independent typed21 replacement+14 disposition records, project checks and all35
raw records' fields/JSON types passed. Own artifacts12193040182/12192258972 were
used; parent artifacts were refused. The [indexed hosted archive](evidence/mixed-epochs-hosted-20261019.tar.gz)
contains104 verified members/392261bytes, SHA-256
`2dbf5b3c542b57f7dff302cea67ccb210a38f4dcac08768b20539dd1fc37de9a`.

All long-running tests completed. Three disposable Windows fixture profiles stayed
locked: dummy credentials were removed; final profile deletion was deferred to VM
destruction. Two unused WinFsp delay-import warning groups and action/toolchain
notices are retained. Ordinary native-test ignores were exercised by separate
native legs; the diagnostic scaling study remains ignored. The checker, raw
comparison and archive are complete one-use operations and must not be rerun.

This accepts local clean epochs; it does not activate native chunk writes or
reclamation, nor establish Wasabi or physical workstation reboot/sign-in/SSH
acceptance. W5b2/3, fresh authoritative external-base adoption, explicit origins
and W6 remain. Goal ACTIVE/progress.
