# Immutable local patches and write versions

Implementation date: **2026-10-19**. W2b2b in `codex/sparse-patches` implements
local patch bytes, immutable write/resize versions and exact operation replay.
Focused controls and the full local gate passed. The changed source still
requires its own combined Windows/Linux native and reference-artifact gates. Mounted chunk writes and remote publication are still disabled.

![The private Rust worker commits immutable patch bytes, views, operation results and a head in SQLite. Retained versions prevent tail resurrection. Planned composition connects to the existing manifest reader, Rust S3 client, shared Rust authority and S3-compatible service.](images/sparse-patches.svg)

A write names its exact parent view, operation key, offset and binary payload.
A resize names the exact parent, operation and new size. An unknown operation
must match the current file head. The transaction inserts immutable patch blocks,
new view and operation result, then compares and updates the head. The complete
post-write state must match the exact expected additions/head change, including
literal old record/request BLOBs. SQL failures and ignored/modified trigger
effects roll back; no unconditional overwrite or implicit rebase exists.

A known operation compares the original arguments before new-head admission or
soft quotas. Exact replay returns its original view, even after later edits.
Changing its parent, data, offset, size or operation kind conflicts. Earlier views
remain readable after reopening. The explicit `head(binding)` method inspects
the current local head; it does not rebase a write or authorize remote content.

The root request/record format and IDs remain compatible with schema29/30.
Literal independently generated vectors protect the old serialized shape and
hash domain, including the original88-byte base descriptor. Children use a new
request/record format and domain. Validation reconstructs every child's canonical
extent map from its request and parent and requires one complete chain to each
head, rejecting forks, cycles, missing parents and orphan versions. Old JSON
whitespace is preserved byte-for-byte, not silently rewritten into a new encoding.

Writes replace intersecting ranges and add explicit zeros across holes. Resize
clips the old map or adds zeros. A shrink followed by growth cannot expose the
removed remote or local tail; older pinned views keep their own bytes. Compatible
adjacent spans merge. A64GiB sparse file needs metadata for its actual extents,
not a64GiB buffer. Empty writes are refused by this storage API; a future native
adapter must handle a zero-length callback explicitly without a mutation.

Schema31 adds `sparse_view_blocks`, with nonempty BLOBs at most1MiB keyed by raw
SHA-256. Equal bytes deduplicate; incoming bytes are still compared for an
existing key. Every block must be referenced by retained write intent. Validation
checks row counts, scalar types, per-cell lengths and aggregate byte limits in
one SQLite snapshot before copying BLOBs. Full block hashes are checked before
returning any local slice, including when corruption lies outside that slice.
The private hashes detect inconsistencies; they do not authenticate arbitrary
rewriting of an owned database or confer provider permissions.

`View::range` returns metadata. `Journal::read` and its bounded worker counterpart
return base/zero placeholders and digest-checked local byte pieces, capped at8MiB
in total logical range. No S3 request occurs. **W2c must reopen and authorize the
exact historical base, compare actual size/store/route and compose these pieces
before enabling filesystem reads from edited chunk views.** A declared base size
is not proof. Local mutation success is not a root publication or clean receipt.

| Admission resource | Default | Fixed ceiling |
| --- | ---: | ---: |
| Files | 64 | 256 |
| Views | 512 | 4096 |
| Operation results | 512 | 4096 |
| Total retained extents | 8192 | 32768 |
| Binding + record + request BLOB bytes | 8MiB | 32MiB |
| Retained patch bytes | 64MiB | 256MiB |

An individual view has at most4096 extents, a record at most1MiB, and a binding or
request at most4096 bytes. The patch table has a32768-row ceiling. Mutations and
range requests are capped at8MiB and logical files at64GiB. Views and operations
currently correspond one-to-one but have independently configurable admission
limits. Narrowed quotas retain existing bytes, older-view reads and exact replay;
new work fails explicitly. No reclamation/settlement exists in this phase. The
current validator hashes all retained patch bytes per validated snapshot; these
are correctness and resource bounds, not a performance acceptance claim.

The new table participates in authority/legacy/publication role guards and
snapshot preflight, including refusal when only an orphan patch row exists.
Generic cache/staging remains available for future mixed object uploads. Existing
complete-snapshot receipts are not accepted for sparse views. Schema30 migration
still validates initial rows before the additive patch table exists. Populated
schema30 BLOBs and version rollback on a table collision are tested. Both Windows
schema expectations move to31 with the Rust state version.

Focused controls include48 overlapping edits against an independent byte model,
all retained views after restart, exact older-result replay, eight-block bounded
binary writes/deduplication, six quota scenarios, sparse64GiB shrink/regrow,
11 new corruption/fork/cycle cases, eight transaction-trigger cases and actual
SQLite-full rollback/retry. Six real process-death boundaries distinguish aborted
work from a committed result with a lost caller; concurrent worker writes keep
exactly one head and recover after owner close/reopen. Existing24 corruption
cases and role/migration controls also pass. These use owned local fixtures.

The migration-only source `8141996` did not gain hosted acceptance: its Linux
[run39205603137](https://github.com/marshallr12/s3drive/actions/runs/39205603137)
failed an existing reader-fixture byte-count assertion. The fixture increments
its counter after the response body and async shutdown, so a completed client
read can precede accounting. The new source waits for owned response handlers at
measurement boundaries and adds a deliberately delayed post-body accounting
control. The exact nine-chunk assertion remains unchanged. All12 portable reader
tests passed (one native test ignored); the new combined source still needs its
own real platform/native gates. Neither the failed parent's partial results nor
older native artifacts certify schema31.

See the [dated evidence](evidence/sparse-patches-20261019.json). W2c verified
composition, W3 changed-object/mixed-tree preparation, W4 exact-CAS publication,
W5 settlement preserving newer heads, legacy/metadata-empty recovery and W6 real
native chunk writes remain required. No provider uploads or credentials changed.


Full local gate2 `40129` passed with all284 frozen inputs unchanged:393state
(one ignored),37core,12reader (one native ignored),62S3 and all format/Clippy/
workspace build/73MD links/69book chapter checks. Gate1 stopped at a new
collapsible-if lint; the failed map/log/exact source are retained, and only the
let-chain cleanup changed before the fresh full run. The
[indexed local archive](evidence/sparse-patches-local-20261019.tar.gz) retains44
verified members, including failed focused/full runs, the failed parent's exact
reader fixture and Linux log, final sources, literal vectors, source maps and the
reviewed diagram. No earlier hosted artifact certifies this changed source.


Published source `d37096c` matches all284 locally gated runtime inputs and includes
accepted main ancestry. Its
[own Windows/Linux/book run](https://github.com/marshallr12/s3drive/actions/runs/39209163191)
is in progress. Main remains accepted base-view/schema29. This publication does
not establish native chunk writes, combined platform acceptance, provider behavior
or physical workstation reboot/session acceptance.
