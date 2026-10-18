# Multipart uploads: research and implementation plan

**Researched 2026-10-16; limits refreshed 2026-10-18. Upload execution remains
proposed and not live-tested.** Multipart
support must extend the single-PUT journal before files above its 8 MiB bound
can upload. A passing `PutObject` probe cannot verify conditional completion.
The completed six-object Wasabi approval does not cover multipart operations.

## Provider limits must remain separate

| Provider documentation | Current documented limits | Implementation implication |
| --- | --- | --- |
| Amazon S3 | Maximum object reported as 48.8 TiB; at most 10,000 numbered parts; nonfinal parts 5 MiB–5 GiB; final part has no minimum; listing pages up to 1,000 | Use an explicit provider profile and pagination. Do not turn the rounded object-size figure into an assumed exact byte limit |
| Wasabi | Five-terabyte objects; single HTTP PUT up to five gigabytes | Do not inherit Amazon's larger object limit for Wasabi. Clarify exact byte units and prove supported bounds before advertising them |

Sources: [AWS multipart limits](https://docs.aws.amazon.com/AmazonS3/latest/userguide/qfacts.html),
[Wasabi object limits](https://docs.wasabi.com/docs/en/working-with-buckets-and-objects).
These are documentation claims, not this implementation's accepted limits.

Wasabi also documents automatic removal of incomplete uploads after roughly
31 days, with incomplete parts counted as active storage. Resumption therefore
cannot assume an upload ID remains usable indefinitely; cleanup and expiration
must be visible in recovery state.
[Wasabi multipart behavior](https://docs.wasabi.com/docs/how-does-wasabi-handle-multipart-uploads)

## Completion is the publication boundary

AWS exposes `If-Match` and `If-None-Match` on `CompleteMultipartUpload`.
Its response can begin with HTTP 200 and later contain an embedded error; the
SDK parses that body and applies its configured error/retry behavior. The
executor must keep mutation attempts at one and inspect the SDK result.
`NoSuchUpload` can mean an invalid, aborted **or completed** upload. It cannot
prove that publication never happened. AWS prescribes re-initiation and part
reupload after a 409; our application must first preserve the conflict and
resolve the original intent explicitly, rather than silently adopting a new
base or making a new publication attempt.
[CompleteMultipartUpload API](https://docs.aws.amazon.com/AmazonS3/latest/API/API_CompleteMultipartUpload.html)

The resolved Rust SDK `aws-sdk-s3` 1.152.0 exposes completion builders
`if_match`, `if_none_match` and `upload_id`, and part builders `part_number` and
`upload_id`. Their resolved local source was inspected. Their presence does not
establish Wasabi support for those headers or responses.

## Proposed durable protocol

The following design extends the existing local journal; it is our proposal,
not a provider guarantee.

1. **Plan immutable bytes first.** Bind the sealed local generation, complete
   desired length/hash, exact target, runtime credential generation and original
   guard. Select a part size that covers the file within the provider's part
   count/size limits. Persist offsets, lengths and part hashes as an immutable
   plan. A configured provider limit or local quota failure preserves the data.
2. **Record initiation uncertainty.** Persist an initiation attempt before
   `CreateMultipartUpload`; persist a valid returned upload ID before parts.
   A lost initiation response may leave an orphan. Do not guess an upload ID
   from a key alone or delete unrelated uploads. Design bounded discovery and
   explicit owned-orphan cleanup before adding automatic initiation replay.
3. **Keep part identity fixed.** Persist part number, offset, bytes/hash and
   receipt state under that upload ID. Every part send must use those immutable
   bytes and the same pinned binding. Any replay policy must be proved for
   exact part identity; a blanket retry rule from single PUT is insufficient.
   Keep network work outside SQLite and stream bounded chunks from staging.
4. **Close the part set.** Record a complete ordered list of accepted part
   receipts and its digest. Resolve or drain outstanding part sends before
   publication or cleanup. An upload ID or ETag alone is not full-content proof.
5. **Claim completion once.** A durable claim must precede the single guarded
   `CompleteMultipartUpload`. Persist the exact upload ID, ordered list and
   guard. A timeout, cancellation, lost receipt, embedded error or missing
   upload ID after completion cannot grant another publication permit.
6. **Observe the published operation.** Establish matching operation metadata,
   full object length/hash and revision without inferring content from the
   multipart ETag. Large-object verification needs a bounded streaming protocol
   and, where supported/proven, trustworthy full-object checksum semantics.
   Absence or a rival remains unresolved. Preserve newer local descendants.
7. **Separate abort and acknowledgment.** Cleanup may address only the known
   owned upload ID after its send workers have quiesced. Abort uncertainty must
   retain records and local bytes. Reference/pin-aware remote acknowledgment
   and reclamation remain a separate contract; abort is not file deletion.

Use separate SQLite tables for immutable multipart plans, initiation/upload-ID
state and per-part attempts/receipts, with manifest/intent hashes and retained
foreign-key references. Reuse the one owner/queue/process lock. Bound the number
of jobs, parts and concurrent buffers, and paginate inspections. Do not expand
the current single-PUT record's body bound or load whole large files into RAM.

## Acceptance before enabling large-file writes

Local SDK/TCP tests must exercise lost initiation IDs, wrong/reordered/duplicated
parts, short/corrupt bodies, changed credentials, part deadlines, process death,
and local disk-full/ack failures. Completion tests need 200-with-error bodies,
409/412, lost success followed by `NoSuchUpload`, an absent/rival published key,
and two independent journals racing against the same guard. Count actual
publication requests and prove no completion replay after an uncertain outcome.

Design the abort/running-part race explicitly and test expiration without
deleting unrelated upload IDs. Include Unicode/literal keys and provider-limit
boundary calculations without allocating multi-terabyte fixtures.

Finally prepare a **new**, concrete live plan with exact keys, multipart IDs,
operation/byte limits and owned-part cleanup. It will need fresh user
authorization; this document performs no provider requests or policy changes.
Passing small single-PUT checks cannot substitute for this acceptance.

## First implementation phase: sizing only (candidate, 2026-10-18)

Add a pure `s3drive-core::multipart` planner before adding durable attempts.
Its done criterion is a bounded, immutable part geometry that covers the exact
nonempty object once, with contiguous positive parts, consecutive numbering,
the configured minimum for nonfinal parts and maximum for every part. Select
the greater of the requested preferred size, configured nonfinal minimum and
the size needed to fit the part-count limit. Raise the preferred size when
needed; reject an object that cannot fit instead of truncating or silently
changing the configured limits. An empty file stays on the single-PUT path.

Limits are explicit inputs, with no provider defaults or capability claim.
The client bounds its plans to10,000 parts and signed64-bit lengths compatible
with its SQLite staging model. These are implementation ceilings, not exact
Wasabi service limits. Validate malformed limits and use division/remainder
arithmetic that cannot overflow at the largest accepted object length. Part
lookup and lazy iteration must avoid allocating a payload or a per-part vector.
Test final-short/exact-multiple parts, boundary growth past the count limit,
impossible layouts, the signed64-bit boundary and exhaustive small geometries.
Run the full local gate and preserve every current unsupported upload path.
See the [dated implementation record](evidence/multipart-sizing-20261018.json).

This sizing phase does not bind or persist bytes, generation, credentials,
guards or part hashes, and cannot issue a send permit. The next journal phase
must freeze those bindings and hashes before initiation. The filesystem's
current whole-file edit/read buffers also need replacement with bounded staged
range composition; a valid size plan alone cannot enable large-file writes.
Follow with initiation/part/completion uncertainty, streamed verification and
owned-abort acceptance before changing the8MiB write bound. No new Wasabi
request or mutation permission is implied.

## Next journal and streaming phases (proposal, 2026-10-18)

Current source inspection found independent8MiB boundaries in
`s3drive-state/migrations/006.sql`, `remote::payload`, managed publication and
the filesystem's whole-file edit implementation. A multipart sizing result does
not relax any of them. Do not enlarge the single-PUT table's limit and pass an
oversized buffer into its existing executor.

First add a separately versioned immutable planning artifact with the sealed
generation ID/manifest, exact target and credential/configuration binding,
original guard/ancestor receipt, whole-body SHA-256, explicit limits/geometry,
and ordered per-part SHA-256 values. Bound both plan and total part-row quotas.
Use the existing retained staged chunks and private SQLite owner. Hash verified
ranges in bounded buffers; keep a long hash scan from monopolizing all owner
commands. A prepared plan grants no network permission. Repeated preparation
must recover the identical recorded plan; differing inputs require explicit
resolution, without replacing or rehashing an uncertain attempt into a new one.
Test sparse zero extents, mixed inherited/new chunks and nonaligned boundaries,
corrupt/short bytes, schema conflicts, disk-full rollback, process death during
planning and preservation of existing generations/receipts.

Before initiation is possible, design shared target admission across both
single-PUT and multipart attempts. The existing unique blocked-target index
covers only `remote_intents`; a second unrelated table must not let two upload
strategies acquire conflicting permission. Check both kinds atomically at
prepare/claim, validate retained history at reopen and preserve old operation
hashes and unknown outcomes during migration. Completion/acknowledgment and
abort need their own state transitions and fault tests; a complete plan alone
cannot be treated as a verified upload.

Separately replace whole-file editing with bounded staged range composition:
copy untouched immutable ranges, retain exact parent revision/manifest, stage
only changed chunks and represent holes without allocating their logical size.
Reads and append/truncate need the same bounds. A newer local descendant must
not be discarded when an ancestor upload or logical metadata commit settles.
Integrate large native files only after both the streaming engine and durable
multipart executor satisfy their acceptance contracts.
