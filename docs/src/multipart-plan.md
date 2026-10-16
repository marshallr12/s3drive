# Multipart uploads: research and implementation plan

**Researched 2026-10-16. Proposed; not implemented or live-tested.** Multipart
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
