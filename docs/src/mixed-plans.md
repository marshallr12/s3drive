# Durable mixed-tree preparation

Implementation date: **2026-10-19**. W3b in `codex/mixed-plans` adds schema32 and
a managed producer for durable preparation of one immutable edited view. Focused
state/SDK/authority tests and the full local gate passed; own combined platform
gates are pending. This phase performs no uploads, root publication, clean settlement
or native write activation.

![A Rust plan controller uses a private SQLite worker to persist cursor, tagged dependencies and deduplicated BLOBs. It authenticates original metadata through the shared Rust authority and reads immutable source objects using the S3 client. Publication and native writes remain later phases.](images/mixed-plans.svg)

The additive schema stores preparation inputs/progress, object records and a
shared content-addressed BLOB pool. Inputs pin a local operation key, exact view,
new object generation, original root descriptor and verified root-body identity.
The view already binds the original content CAS reference, file/store/route,
configuration and credential generation. Neither newer local heads nor changed
remote metadata can silently replace those inputs. Existing persisted view,
operation and dirty-block bytes are preserved by migration.

The cursor distinguishes input coverage from bottom-up page construction. Input
steps admit one changed chunk, a whole unchanged subtree, or a structural zero
run. A page step builds at most128 child entries or skips an already covered
subtree. Sparse64GiB coverage needs one zero input record; the resulting balanced
tree has517 page records. Neither path allocates or downloads the file. Changed
chunks contain at most1MiB. Validation scans bounded admitted metadata and BLOBs;
these limits do not establish constant command latency.

Object tags distinguish new data, structural zeros, inherited witnesses and new
pages. Repeated witness paths reference shared BLOB IDs rather than copying the
same leaf body128 times. The new root uses a new object identity. An old root can
be reused as a child only when its level, offset and complete length match the
new tree. Partial old data objects cannot be inherited as shorter or longer
objects. A changed final chunk verifies the complete original data object before
truncating, masking or adding zeros. Shrink/regrow never restores removed bytes.
A zero run ending inside a chunk before a local edit falls back to bounded
composition, preserving the edit.

Raw BLOB SHA-256 identifies local bytes. Manifest page references instead use the
core format's domain-separated page digest. The initial focused run caught an
incorrect substitution through actual root verification; the corrected tests
verify the complete mixed graph, including old/new pages and data, through the
core manifest verifier. The failed run and source snapshots are retained in the
dated evidence; no failing source is promoted.

Each transaction inspects the retained source/preparation state, constructs its
exact expected result, writes new rows/progress and compares the complete result
before committing. Counts, types, BLOB sizes and total admission are checked
before allocation. Corruption returns an error with no automatic repair or
cleanup. Preparation tables do not support application triggers: attached
triggers are refused before writes, including triggers that could target generic
staging. SQLite-full and process death roll back uncommitted rows/cursors; a
committed lost reply is recovered by the original operation and cursor.
Narrowing admission quotas does not hide old work or defeat exact replay.

Default preparation admission is16plans,32768objects,8MiB metadata and64MiB
BLOB bytes; hard bounds are256plans,262144objects,32MiB metadata and256MiB BLOBs.
These are separate from existing sparse-view admission. There is no reclamation
or local clean acknowledgment in this phase. Exact immutable views, original
inputs, newly prepared data and conflicting/unknown work remain retained.

`Preparation::dependency` returns a privately constructed checked dependency,
not a send permission. New data and holes are compared with the exact pinned
view through verified composition or authenticated whole-chunk hashes. A
correct-root offline plan containing false base bytes or a false hole remains
an error at this boundary; it cannot certify source content merely by being
self-consistent. New page evidence is structural: W4 must still require ordered
child receipts/provenance before publishing a parent. Raw state dependencies and
ready root metadata are not substitutes for that check.

The state API validates offline intent/integrity. A self-consistent caller-supplied
root is not authenticated by its constructor or local hashes. The managed
producer obtains context from the actual authorized pinned base. Reopen asks the
Rust authority for that exact historical version, verifies its root and compares
the full original binding/context before allowing progress. Tests deliberately
persist a self-consistent false root claim and show that managed reopen refuses
it without reading data. Principal rotation cannot adopt another principal's
private dirty plan. An inherited proof establishes reachability, not current
data health, retention permission or a successful upload.

The private-owner client shares the source view's connection, kernel lock, bounded
queue and worker; it never opens another database from an async callback. The
managed `Preparation` shares reader active/snapshot limits. Each step has one
overall reader deadline covering local validation, authority/provider I/O and
cursor admission; it acquires no nested reader slot. SQL transactions end before
network waits. The complete preparation loop is cancellable and resumable with
per-step deadlines. A lost local begin reply can be inspected by its saved local
operation key without credentials; that lookup grants no authority operation
ownership or network send permission. Later progress still reopens the exact
base. No shared-authority request is reconstructed from a bare operation lookup.

Focused controls cover mixed Unicode/binary data, unchanged-reference reuse,
reopening after replacement/rename/removal and local/authority restart, both
127/128 and16383/16384 boundaries, sparse growth, partial final chunks, zero runs,
empty manifest roots, malformed proof/binding/tag/coverage, quotas, SQLite-full,
corrupt/missing/oversized BLOBs, migration collisions, orphan role refusal,
credential rotation, shared limits/cancellation/deadlines and independent SQL
progress during blocked S3 reads. Owned subprocesses kill begin/append/page work
at14 boundaries before and after commit, then inspect and replay exact results.
These tests use owned fixtures, not Wasabi or a physical workstation reboot.

The existing full-snapshot planner, uploader proofs and source receipts remain
unchanged. W4 must admit new dependencies before parent pages/root using the
original CAS and operation identities, with source-bound write policy and exact
unknown-result recovery. W5 must settle only the exact prepared view while
preserving newer heads and pins. Legacy/metadata-empty origin admission and W6
actual FUSE/WinFsp chunk writes remain required, followed by provider and physical
sign-in/reboot/SSH/tmux acceptance. Local preparation alone is never file flush
success.

[Dated evidence](evidence/mixed-plans-20261019.json) retains focused results and
the initial failed run. The first corrected five-test run passed in60.16s;
refusal tests5/1.27s, killed-owner/helper tests2/8.37s, path dedup1/5.24s,
orphan refusal1/0.04s and two-level paths1/0.15s passed. The final three owned
SDK/authority/controller cases passed in15.76s, including false root/data/hole
claims, semantic dependency verification and private credential isolation.
The full local gate passed:408state and22reader tests (one ignored each),
37core and62S3 tests, formatting/Clippy/build,76Markdown link checks and72book
chapters. All300 frozen runtime inputs match. Own combined acceptance is pending.

The first full gate passed formatting/workspace Clippy but found the existing
authority-snapshot validator did not allow the three new empty local tables.
The narrow schema adaptation keeps all nonempty client/preparation tables
forbidden. Nine snapshot state tests and two real offline CLI tests passed,
including refusal of each new nonempty table. The failed gate, input map and old
validator are retained. The fresh full gate passed on the changed source.

The [local evidence archive](evidence/mixed-plans-local-20261019.tar.gz) contains
all300 runtime inputs, focused and full logs, failed source snapshots, reviewed
diagram and a verified member digest index. Its SHA-256 is
`363badd8492a7f492ad8fcf209933969ec3c4175b2cb745ee6b5faa304d2588f`.

Exact source `e140508b19aef4082f9565990402852d4f27cdb4` is published for its
[own Linux, Windows and book run](https://github.com/marshallr12/s3drive/actions/runs/39225921755).
Acceptance is pending; preceding source artifacts do not cover this implementation.
