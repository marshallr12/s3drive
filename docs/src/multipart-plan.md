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

## Offline immutable plans (candidate implementation, 2026-10-18)

`s3drive-state::multipart` now implements the offline preparation boundary on
the existing storage owner. Obtain a planner with `Storage::multipart(options)`;
provide an operation `Id`, sealed generation, preferred size and explicit
`s3drive-core::multipart::Limits`. There are no provider defaults. `freeze`
durably records the complete original input before hashing. `prepare` produces
whole-file and ordered part SHA-256 commitments. `info` and `for_generation`
recover the recorded identity after an unknown local reply. These APIs do not
create an upload ID or issue initiation, part-send, completion or flush permits.

| State | Persistent content | Recovery |
| --- | --- | --- |
| Frozen | Caller-supplied ID, sealed generation/manifest, exact target/configuration/credential-generation binding, original guard and ancestor operation/state-hash receipt, explicit geometry/limits and part quota reservation | Reuse the same ID/input; changed input conflicts; transient hashing restarts from zero |
| Hashing | Frozen row remains; hash state and at most10,000 part digests are transient | Each owner command reads/verifies/hashes at most1MiB, then releases the queue for other commands; cancellation retains the frozen row and sealed bytes |
| Ready | Whole SHA-256, all ordered part rows and a domain-separated plan digest commit atomically | Recover exactly the same artifact; partial rows never look ready; preparing again rehashes rather than selecting a new guard |

Freeze and finalization validate their exact saved rows before committing, in
addition to checking affected-row counts. Six trigger controls exercise ignored,
deleted and corrupted SQL effects; no false Frozen result or partial Ready
artifact can be committed by those faults.

Schema24 adds `multipart_plans` and `multipart_parts`. They remain client-local
tables, forbidden when populated in an authority database or snapshot. Migration
preserves legacy remote intent/state hashes; a conflicting new table rolls back
instead of being adopted. The shared read-only guard-selection extraction keeps
the single-PUT admission and error order, including its quota check. Existing
single-PUT, managed publication, logical-edit and native8MiB bounds remain.

Planning defaults reserve at most64 plans and100,000 total parts and admit at
most4,096 extents per generation. Explicit resource ceilings are4,096 plans,
1,000,000 total parts and65,536 extents. Reservations include Frozen plans.
These limits bound local work/metadata; they do not prove a provider capability.
Payload reads retain complete manifest validation, so the measured repeated-scan
cost still exists. Admission/finalization validate retained metadata and ancestry;
the1MiB step bound and an interleaving test do not prove maximum-catalog latency
or fairness. No metadata proof survives across owner commands.

A Ready artifact commits to the generation's **expected immutable bytes**. Each
consumed chunk was verified when read; a final manifest check does not certify
that previously read media cannot later corrupt. Sealed-generation public
mutators refuse modification, and current retention preserves the data. Future
send-range reads must revalidate current bytes against the staging and plan
commitments; future reclamation must honor plan/generation/anchor references.
The corruption-after-read test explicitly demonstrates this boundary. An offline
hash plan cannot be presented as an uploaded object or remotely durable file.

Focused controls cover a16MiB asynchronous plan with intervening owner reads,
nonaligned parts, sparse inherited/new data, unchanged input across restart and
newer receipts, five kill/transaction boundaries, true `SQLITE_FULL`, independent
part/extent quotas, malformed digest/ID cells, legacy error precedence, migration
and both new-table authority exclusions. The initial state regression exposed
two synthetic legacy fixtures that retained new tables while lowering the schema
number; they were corrected without weakening production collision refusal.
The final focused suite passed16 tests. The full local gate then passed292 state
tests (one diagnostic study intentionally ignored),48 authority tests and the
workspace/build/lint/book checks on unchanged frozen source. Source-bound
platform acceptance remains a separate requirement, recorded in the
[dated evidence](evidence/multipart-plans-20261018.json).
The unchanged64aa073 Windows repeat failed paired startup; this offline feature
does not correct that transport failure. No real provider requests occurred.

Exact-sourcec7fca78 CI39130994866 passed Linux/book and all16 new offline planning
tests on Windows, but the Windows job failed its broader ordinary-account startup
fixture. That fixture still required `schema=23` while the binary reports24; the
stale assertion rejects successful initialization output. The next candidate
corrects the expected version and adds a local/early-book contract check tying
both native schema expectations to the Rust constant. This does not replace a
new source-bound Windows acceptance run, prove another failure is absent or fix
the separate historical transport issue. The failed run's success artifact
checker is ineligible; no runtime promotion follows.


## Next journal and streaming phases (proposal, 2026-10-18)

Current source inspection found independent8MiB boundaries in
`s3drive-state/migrations/006.sql`, `remote::payload`, managed publication and
the filesystem's whole-file edit implementation. A multipart sizing result does
not relax any of them. Do not enlarge the single-PUT table's limit and pass an
oversized buffer into its existing executor.

The offline candidate above supplies a separately versioned immutable planning artifact with the sealed
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

## Staged range cost and planning boundary (investigated 2026-10-18)

An isolated experiment at production source `64aa073` read sealed16,32 and64MiB
generations through1MiB ranges. Every returned byte and the complete streamed
hash matched an independently generated pattern; reads made no SQLite changes,
and a tail read plus manifest check passed after reopening. The test module,
registration patch, complete final log, Clippy log and source hashes are retained
in the [reproduction archive](evidence/staged-range-study-20261018.tar.gz), with
[structured results](evidence/staged-range-study-20261018.json). This measures
`Journal::read`, not the asynchronous queue, a native mount or an upload.

| Sealed size | Extents | Data reads of1MiB | Scan progress callbacks | Scan time, instrumented debug fixture |
| --- | --- | --- | --- | --- |
|16MiB|16|16|11,610|1.365sec|
|32MiB|32|32|37,394|2.731sec|
|64MiB|64|64|132,738|5.459sec|
|16MiB|64|16|36,162|1.370sec|
|16MiB|256|16|134,370|1.416sec|

Callbacks were registered at interval1. Scan measurements include exact-byte
comparisons, SHA-256 and final EOF/bounds/head/manifest checks; times are not
production latency guarantees. Holding size and data-read count constant exposes
the metadata cost as extent count grows. A separate zero-length read also
validated the complete manifest:571 callbacks at16 extents,6,571 at256 extents.
No elapsed-time threshold or maximum-quota acceptance follows.

The source explains this behavior: every range calls `validate_manifest` before
reading its intersecting chunks, and those chunks separately recheck their
payload digests. `copy_next_at` also scans its parent manifest for each piece.
`seal` verifies all payloads inside one synchronous SQLite transaction.
Consequently, small payload buffers alone do not bound command duration or
prove fair service for other queued work.
[Range validation and sealing source](https://github.com/marshallr12/s3drive/blob/64aa073a6ecbfc6cc27739c6c767663143811ffd/crates/s3drive-state/src/journal.rs)

The worker snapshot holds generation metadata and an owner clone; its reads
recheck the expected manifest and call the same range reader. It does not
implement a per-generation reclamation pin. Current sealed generations cannot
be aborted and their chunks remain retained, but a future collector must add
explicit plan/snapshot references before reclaiming them. The pending-scan
optimization's proof lifetime ends with one synchronous command; it cannot be
copied into a cache spanning asynchronous reads without a separate integrity
and lifetime argument.
[Worker snapshot source](https://github.com/marshallr12/s3drive/blob/64aa073a6ecbfc6cc27739c6c767663143811ffd/crates/s3drive-state/src/worker.rs#L449)

### Planning contract derived from the study (implemented offline candidate)

The following contract was derived at baseline64aa073 and is implemented by the
offline candidate above. Executor admission, network operations, reclamation and
large native editing remain future phases.

Freeze a durable planning input **before** the long hashing phase, under the
existing owner and an idempotency key supplied by the caller. It must include
the generation/manifest, target and credential/configuration binding, original
guard and ancestor receipt, exact geometry/limits, and reserved plan/part quotas.
Repeated admission with that key must return the same input or an explicit
conflict. Cancellation or a lost local reply must not allocate a replacement
identity or discard the input and sealed bytes.

Hash bounded ranges while allowing other owner commands between chunks. Whole
and current-part SHA-256 states can advance together without collecting the
file in memory. The first format can restart hashing from byte zero after
process death; do not persist an opaque dependency-specific hasher state.
Recompute from the frozen input, rather than choosing a new guard on resume.
The measured repeated-manifest cost must be addressed or explicitly bounded by
admission policy before promising large-catalog fairness. Any optimized cursor
needs its own immutable-byte, corruption and future-reclamation contract;
skipping validation is not an optimization acceptance criterion.

Keep the finalizer private. After verifying all bytes, atomically publish the
whole hash, complete ordered part hashes and a versioned plan digest as one
ready artifact. Revalidate its frozen bindings and reject changed input;
partial parts or disk-full rollback must never look ready. A ready artifact is
data, not an initiation or completion permit. Later send admission must resolve
both single-PUT and multipart target ownership atomically and preserve the
original guard if it has become stale.

At the study baseline, `remote::prepare` could not be reused to obtain that guard for a large
generation: it first builds and hashes its bounded single-PUT payload. Its later
transaction selects the verified ancestor and checks head, target and credential
binding. Extracting shared admission must preserve these checks and original
receipt hashes, rather than merely copying the final guard field.
[Current preparation source](https://github.com/marshallr12/s3drive/blob/64aa073a6ecbfc6cc27739c6c767663143811ffd/crates/s3drive-state/src/remote.rs#L281)

A separate planning schema also affects migration and database-role validation.
At the study baseline, schema23 was shared by local and authority state. Snapshot validation
accepts only explicitly named tables and refuses nonempty client-local tables
in an authority artifact. Add new local planning tables to the explicit checks
with migration/snapshot tests; do not broadly permit unknown tables or weaken
client/authority separation.
[Migration source](https://github.com/marshallr12/s3drive/blob/64aa073a6ecbfc6cc27739c6c767663143811ffd/crates/s3drive-state/src/lib.rs),
[snapshot role checks](https://github.com/marshallr12/s3drive/blob/64aa073a6ecbfc6cc27739c6c767663143811ffd/crates/s3drive-state/src/authority_snapshot/native.rs#L142)

Acceptance must include exact plans across restart and changed-input conflicts;
mixed inherited/new chunks, sparse holes and nonaligned part boundaries; corrupt
or short chunks, quota exhaustion, process death and SQL-full rollback before
each ready-publication boundary; preserved legacy receipts and role separation;
and no network permission from any partial or ready record. The full local gate
and later executor/native/provider gates remain separate. The isolated study
changed no production source, upload limit, schema or real-provider permission;
the subsequent offline candidate changes local schema and planning APIs only.
