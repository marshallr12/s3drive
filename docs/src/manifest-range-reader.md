# Verified manifest range reads

Implementation and source review date: **2026-10-18**. The managed library now
has a bounded manifest reader. Its five focused integration controls passed;
the combined full local gate passed. Its source-bound Linux/Windows/book cohort
has also passed. Mounted chunked files, editing and migration remain later work.

![Authenticated metadata pins a root; bounded traversal verifies pages and complete chunks before returning a range.](images/manifest-range-reader.svg)

## Opening a file

`chunks::Reader::open` takes an authenticated metadata client, expected volume
and at most 16 mapped S3 stores. It checks the manifest feature, volume identity,
name policy and each store's route fingerprint. `open_file(node_id)` obtains
live namespace metadata and validates the node, placement and published payload.
It derives the root key from that authenticated descriptor; callers cannot
substitute a descriptor or object metadata.

The root receives a fresh S3 HEAD. Its physical length, ETag and any published
version must match. The reader fetches the complete bounded root page, checks
its physical SHA-256, then verifies the domain-separated manifest page hash,
volume, file identity, logical length and root level. Opening fetches no child
page or data chunk. The returned cloneable `chunks::File` retains its Node,
logical length, verified root and S3 handle. A rename or newer publication does
not redirect an existing file snapshot. Opening does not download the logical file.

An authenticated live `EmptyFile` without a payload returns a zero-length view
without an S3 request. A published empty manifest has a real root and still
requires HEAD/GET/hash verification. A `File` without a payload is an integrity
error. A directory, deleted node or legacy one-object payload is refused by this
new reader. The existing managed `Reader` and `FileSnapshot::bytes()` retain
their existing behavior; native callback dispatch is unchanged.

## Reading and verification

`File::read_range(offset, length)` returns a complete verified buffer or an
error. It clips to the pinned logical EOF; zero-length and EOF reads require
no network request. Traversal follows only intersected child pages, verifying
each complete page's digest, volume/file identity, start, length and decreasing
level. Every reference in a loaded page must belong to the bound store, including
references outside the requested range. Namespace and role derivation keep page
and data keys scoped to the file.

Each data chunk receives fresh HEAD authorization and a revision-pinned GET.
The reader verifies the **whole chunk** before copying its requested subrange.
Corruption outside that subrange therefore fails the read. Only authenticated
hole entries produce zeros. Missing, denied, changed, oversized, malformed or
truncated objects remain errors. A later chunk failure discards the private
output prefix; no partial buffer escapes.

A new root may refer to an unchanged chunk from an older generation. The reader
uses the authenticated reference's generation and ordinal rather than requiring
that they equal the current publication or logical chunk index. It has no
persistent cache, eviction policy, garbage collector, clean acknowledgment or
write side effects. Local SQLite continues to hold namespace/journal state
outside the mount; this reader adds no database or schema.

## Credentials and finite budgets

Child pages and data use the root handle's
[`ReadHandle::open_related`](pinned-related-reads.md). It preserves the captured
credential generation and source configuration while performing fresh scoped
HEAD authorization for each key. Expired or denied captured credentials fail;
a new independent open may resolve another generation. Editing an INI file is
not a promised hot-reload mechanism: the resolved `aws-config` 1.12.0 profile
provider caches parsed profile files. This dependency finding was verified on
2026-10-18. [AWS profile-provider documentation](https://docs.rs/aws-config/1.12.0/aws_config/profile/struct.ProfileFileCredentialsProvider.html).

Budgets are shared by a reader and its clones:

| Resource | Limit and behavior |
| --- | --- |
| Active open/read operations | 4; excess returns `Busy` before metadata/S3 requests |
| Retained file snapshots | 256; clones share one retained permit |
| Requested/output range | 8 MiB; oversized requests fail, including at EOF |
| Complete data buffer | One chunk, at most 1 MiB, per active range |
| Manifest page | 64–10,432 bytes; no whole-tree cache |
| Traversal | 27 page visits and 9 complete data objects per read |
| Overall operation deadline | 30 seconds, including metadata and provider awaits |

`ReadOptions` can choose a shorter positive deadline; zero and values above
30 seconds fail before requests. Errors, timeout and cancellation release owned
permits. These are library ownership and traversal bounds, **not total SDK/RSS
or native stack guarantees**. Per-request SDK bounds also remain in effect.

The authority's current descriptor policy allows 64 GiB logical length and a
root at level 2. The reader requires the actual verified root level to equal
the descriptor ordinal's level, even though the general codec permits deeper
trees. An unaligned 8 MiB range can intersect nine chunks. Rewalking a path of
at most three pages for each interval gives the conservative 27-visit budget.
The deepest controlled test makes 54 HEAD/GET requests for two child pages plus
one data object across each of nine intervals; cached root visits make no S3
request. This is a correctness observation, not a throughput benchmark.

## Evidence and remaining acceptance

The dedicated `chunk_reader` integration binary uses the actual AWS SDK for
HEAD/GET and an authenticated Rust authority backed by temporary SQLite. S3
objects are seeded in owned loopback memory. It proves neither an S3 upload nor
provider conformance. A child process owns each synthetic credential environment;
listeners, workers, barriers, output collection and child teardown are bounded.
No real credentials or new Wasabi mutations are involved.

The five focused controls passed in **4.68 seconds**, including the child entry
control and four parent-owned scenarios:

- Sparse, binary, empty and Unicode files; two mapped buckets; cross-page reads;
  old-root reads after rename/new publication; reuse of an older chunk; unaligned
  maximum ranges and a sparse 64 GiB, level-2 tree with exact request counts.
- Malformed metadata and descriptor bindings; both root hash domains; wrong root
  level, child identity/length/level/role; foreign-store references; corruption
  outside the requested slice and late failure after a valid first chunk.
- Denied/missing objects, oversized HEAD, changed revision/version, wrong range,
  truncated/extra body; repair followed by a successful verified read.
- Active-operation and snapshot capacity at barriers; clones retain their permit;
  cancellation frees capacity; a shorter overall deadline interrupts a stalled
  read and permits a later successful read.

The first focused run passed four controls and failed the transport fixture:
it expected a GET version mismatch without first supplying an expected version
in HEAD. The fixture now supplies that revision. Production validation was not
weakened. The deepest-tree and older-chunk positive controls were then added;
the second focused run and focused Clippy passed.

Accepted prerequisites are controller source `474601e` and related-read source
`fef9442`, each with its own Linux/Windows/book cohort. They are combined in this
reader branch before its full gate. Its acceptance uses its own hosted cohort;
prerequisite artifacts do not certify its changed source. Native chunk lifecycle,
write/recovery integration, migration, real provider behavior and physical reboot
acceptance remain separate phases. See the [implementation plan](implementation-plan.md),
[dated implementation evidence](evidence/manifest-range-reader-20261018.json)
and [contract audit](evidence/manifest-range-reader-contract-20261018.json).

The combined local gate completed with exit status 0: 12 publication/source
controls, five reader controls, 62 S3 tests, 365 state tests / one ignored,
formatting, Clippy, workspace build, 68 documentation files and 64 chapters.
All 256 frozen source hashes remained unchanged. One existing retained-open
quota control exceeded a minute and then passed; no performance conclusion is
inferred. The [14-member indexed local archive](evidence/manifest-range-reader-local-20261018.tar.gz)
retains logs, source, self-review and checks; every member was verified. Its
SHA-256 is `a1737729817e21d78c356c699987e1d4129f128172c71363007b04d9bb4a2ffd`.

Published source `88d02f7` is on `codex/manifest-range-reader`. Its
[own Linux/Windows/book cohort](https://github.com/marshallr12/s3drive/actions/runs/39171113280)
completed successfully. The clean exact remote and all 256 runtime hashes were verified;
repository visibility remains PUBLIC and main was not changed. All three raw
logs were reviewed; its own finalized reference artifacts passed
independent strict 21/14 records, project oracles and all 35 raw field/type
comparisons. The [22-member hosted archive](evidence/manifest-reader-88d-hosted-20261018.tar.gz)
retains the exact source, logs, payloads and checks. Every indexed member was
verified. Archive SHA-256:
`994942870840d5efd3082d61dd3533e89dbf0bd154844f5b3d2d93b7b37428ac`.

Both platforms passed all five reader controls and 62 S3 tests. Existing native
regression also passed, including Linux no-systemd startup and independent
Windows logons. Those controls use the existing mounted format. They do not
activate or certify mounted manifests. Windows retained three locked disposable
fixture profiles until VM destruction after dummy credential removal; delay-import
and Node notices remain in the logs. Native manifest integration follows separately.
