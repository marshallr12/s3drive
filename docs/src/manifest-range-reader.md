# Verified manifest range reader

After controller acceptance, add a separate managed manifest reader that opens
an authenticated authority node and retains its exact root. The current
[managed Reader](https://github.com/marshallr12/s3drive/blob/41b759aeb09511f2e27956c8c6a0110bfe283dcb/crates/s3drive-managed/src/reader.rs)
intentionally refuses manifests and retains whole-file buffers. Preserve that
explicit refusal and the old format while building the bounded reader; do not
raise native limits or replace its snapshot type with an unverified lazy view.
The existing [codec verification](https://github.com/marshallr12/s3drive/blob/41b759aeb09511f2e27956c8c6a0110bfe283dcb/crates/s3drive-core/src/manifest.rs)
and [role-specific object mapping](https://github.com/marshallr12/s3drive/blob/41b759aeb09511f2e27956c8c6a0110bfe283dcb/crates/s3drive-core/src/manifest/objects.rs)
provide the structural/hash primitives; they do not authenticate a caller's
descriptor or confer S3 authorization.

Bind the expected volume/name policy, feature and store route through the
authenticated Rust metadata client. Obtain the manifest descriptor from that
client's validated live Node response, never an arbitrary caller-supplied root.
Validate payload/store/node/volume/generation context and derive the root key
through `Namespace`. HEAD the root using the mapped S3 reader, require the exact
published size/ETag/version, read its bounded complete bytes and verify both its
ordinary physical SHA and the descriptor's domain-separated page hash. They
are different hash meanings. Keep that root immutable for the open handle;
rename, replacement or a later root must not silently retarget it.

For each bounded range, traverse only intersecting authenticated child entries.
Verify each complete bounded page against its parent's hash, identity, start,
length and decreasing level before following any reference. Derive data/page
keys with the pinned file namespace and require the mapped store. Use the
existing S3 pinned HEAD/range-GET handle for each fetched object, require its
expected size, and verify the entire data chunk before exposing its subrange.
Only authenticated Hole entries produce zeros; missing, denied, truncated or
changed objects are errors. No missing-object fallback or whole-file scan.

Declare finite output, concurrent-read, traversal and temporary-buffer budgets
before implementation. Keep read lengths within the current S3 reader's 8 MiB
bound and data verification within the codec's 1 MiB chunk bound; check EOF and
arithmetic before allocation. An initial reader without shared persistent cache
can establish integrity and online authorization first. Any later cache must
preserve credential/configuration isolation and require current authorization;
cache presence never grants access. No new database is needed for this slice.

Real-SDK/authenticated-authority acceptance must cover empty/sparse/binary/Unicode,
unaligned ranges, cross-chunk and cross-page reads, EOF and limits; request counts
must show that opening/reading a small range does not fetch the whole tree/file.
Corruption outside the returned portion of a fetched chunk must still fail.
Test wrong hash/domain/context/store/role, malformed pages and depth, changed
ETag/version, missing/denied/truncated/oversized responses, cancellation and
concurrent read capacity. Prove pinned old-root reads after newer publication
and rename while new opens see the new root. Existing native callbacks, legacy
whole-file readers and mounted-format activation remain separately gated.

## Concrete reader contract and footprint (proposal, 2026-10-18)

Read-only source inspection found an additional dependency boundary: the root
`ReadHandle` already captures one credential generation, while the original
`S3Reader` may use a provider that supplies a different generation on a later
open. The new file handle must open child pages/data through the root handle's
captured provider. Add a narrow `ReadHandle::open_related(key)`-style method
that performs a fresh scoped HEAD with those captured credentials; do not
expose credentials or let a caller substitute metadata/configuration. Keep the
root handle alive for the file snapshot. Expired captured credentials fail
explicitly; a new file open can obtain newly resolved credentials. This follows
the existing [open implementation and rotating-provider control](https://github.com/marshallr12/s3drive/blob/474601edd5582ebed9316d04cf4524db6d72694b/crates/s3drive-s3/src/reader/tests.rs).

Do not assume that editing an INI file hot-reloads an existing SDK provider.
The resolved `aws-config` 1.12.0 profile provider caches parsed profile files
after its first load. The planned rotation control should extend the existing
injected `RotatingCredentials` fixture and inspect signed dummy requests, rather
than depend on profile-file rereading. This is a dependency-source finding,
verified 2026-10-18, not a new S3Drive reload feature.
[AWS SDK profile-provider documentation](https://docs.rs/aws-config/1.12.0/aws_config/profile/struct.ProfileFileCredentialsProvider.html).

Proposed public surface: a reader under `s3drive_managed::chunks`, configured
from an authenticated metadata client, expected volume and at most 16 mapped
stores; `open_file(node_id)` returns an opaque cloneable pinned file, whose
`read_range(offset, length)` returns a complete verified bounded buffer or an
error. Expose its pinned Node/logical length for callers. The legacy managed
Reader and `FileSnapshot::bytes()` retain their current meanings. No arbitrary
caller-supplied descriptor constructor or mounted callback dispatch is added.

Distinguish two empty-file states. An authenticated live `Kind::EmptyFile` Node
without a payload has no S3 object to authorize; validate its optional placement
and return a pinned zero-length view using metadata authorization alone. A
published empty manifest is a `Kind::File` with a real root page and must still
perform the root HEAD/GET/hash checks at open. Missing payload on `Kind::File`
is an integrity error. Legacy one-object payloads remain explicitly unsupported
by this new reader rather than being mistaken for empty files. Deleted nodes
fail new opens; an already pinned empty/manifest view retains its identity.

Initial finite budgets, per shared reader family:

- Four active open/read operations, acquired without an unbounded queue; a
  clone shares the same permits. One permit covers the complete operation.
- 256 retained pinned file snapshots; clones of one snapshot share its retained
  permit. Acquire permits before metadata/provider work; errors and cancellation
  release them. These are library ownership bounds, not total SDK/RSS bounds.
- At most 8 MiB output and one complete 1 MiB data buffer per active range read.
  Read pages within the 10,432-byte codec bound and retain only the bounded
  authenticated path/current page needed for traversal. No whole-tree cache.
- One overall 30-second open/read deadline, including all metadata/provider
  awaits, in addition to existing per-request deadlines. Return no partial buffer
  after integrity failure or timeout. Zero-length/EOF reads make no network call.

Require the verified root page's level to equal the level encoded in the
validated descriptor ordinal. The authority's manifest length/ordinal policy
admits at most level 2 (64 GiB logical length), even though the general codec
admits level 7. A root with a different actual level must fail. Child verification
then strictly decreases level while checking exact covered ranges. An 8 MiB
unaligned read intersects at most nine chunk intervals; conservatively budget
27 visited pages and nine complete data objects per read. Count visits and
fetched bytes explicitly, skip authenticated holes by their recorded lengths,
and fail on exceeded bounds. These are proposed algorithm budgets, not measured
request counts or new file-size support. See the current
[authority descriptor policy](https://github.com/marshallr12/s3drive/blob/474601edd5582ebed9316d04cf4524db6d72694b/crates/s3drive-state/src/namespace/publication/manifest.rs).
Do not require a reused child/data reference's generation or ordinal to equal
the current root generation or logical chunk index: the authenticated reference
names its own immutable object. Keep role/store/file namespace checks intact.

Expected write footprint: additive S3 ReadHandle method and its existing
credential/scope/HEAD-denial/future-size controls; new managed chunk-reader child
module and an additive module export; a dedicated real-SDK/authenticated-authority
integration test with owned dummy S3 fault/gate fixture; this book/evidence.
Prefer a new focused read fixture over changing the widely shared write fixture
merely to inject arbitrary page faults. No state migration, wire/codec vector,
native callback, write-policy or credential-file change is needed. Before coding,
inspect the resolved dependency APIs used by deadline/semaphore/test gates and
verify that the chosen traversal satisfies the stated visit/buffer budgets.

The S3 helper's meaningful controls must show related HEAD and GET retain the
root's access key/session token after an injected provider rotation, independent
new opens use the new generation, and denied/missing related objects never reuse
cached authorization. Keep the future's inline size bounded as the existing
S3 open control does; size checks cannot substitute for native stack acceptance.
The managed tests must additionally prove both physical and page hash checks,
actual root-level refusal, all-or-error cross-chunk corruption, explicit request
and byte counts, capacity at controlled barriers, cancellation/released permits,
and preserved old-root reads after newer publication/rename. No new API is
implemented by this contract. The full managed reader follows acceptance of
the controller cohort. Its additive pinned-credential S3 helper can be developed
as independent preparation in an isolated checkout from the accepted S3 baseline:
it does not depend on source settlement or alter the controller checkout's frozen
files. It still needs focused tests, its full local gate and source-bound hosted
acceptance before integration.
The [dated source-audit evidence](evidence/manifest-range-reader-contract-20261018.json)
records the proposed budgets and resolved dependency hashes.


Implementation status (2026-10-18): isolated draft on `codex/manifest-range-reader`,
from helper status checkpoint `9564a53`. Controller `474601e` is accepted.
Helper `fef9442` still needs its own hosted acceptance before this reader is
published or branches are integrated. The published helper checkout stays frozen.
No native callback or legacy reader changes belong in this draft.

The implementation may expose a bounded `ReadOptions` deadline shorter than the
default maximum of 30 seconds. Zero and values above that maximum fail before
requests. Active-operation, snapshot, byte and traversal limits stay fixed.
This also permits controlled integration tests of the overall deadline without
changing SDK deadlines or relaxing default production bounds.
