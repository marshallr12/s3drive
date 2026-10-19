# Empty-manifest client integration

Source review: 2026-10-19. **Local intent journal accepted; network/native integration remains proposed.**
The [atomic authority operation](atomic-empty-manifest-create.md) is accepted
at source29eb237/schema39 after its own complete local and hosted gates. It deliberately cannot prove an S3 upload:
the client must make and verify that upload before submitting the metadata request.
This work must lead into normal native create/write/flush, rather than introduce
another disconnected write profile.

## Reuse and limits found in the current sources

The accepted [S3 executor](https://github.com/marshallr12/s3drive/blob/12a2f2a7d836aab518c7b0d60675234d6470ef97/crates/s3drive-s3/src/uploads.rs)
already provides source-bound create policy checks, exact staging targets,
single-use send claims and observation after uncertainty. Repeated submission of
an already claimed generation observes its original object; it does not acquire
another PUT permission. Reuse that executor for the canonical64-byte page body.
The logical file is empty, but the staged/uploaded object is not.

The [local namespace journal](https://github.com/marshallr12/s3drive/blob/12a2f2a7d836aab518c7b0d60675234d6470ef97/crates/s3drive-state/src/namespace_client.rs)
accepts a fixed list of older namespace mutations; CreateEmptyManifest is not
one of them. Its constructor also enforces legacy-role restrictions. Merely adding
the mutation to that list does not establish safe coexistence with sparse views,
manifest publication and Windows open-handle recovery.

The [shared storage owner](https://github.com/marshallr12/s3drive/blob/12a2f2a7d836aab518c7b0d60675234d6470ef97/crates/s3drive-state/src/worker.rs)
already owns the connection, lock and bounded command queue. Generic immutable
staging can serve physical objects. Keep creation records and staging on that
same owner; do not open a second connection against its private directory or
relax legacy logical-writer isolation. The [native bridge review](native-chunk-integration.md)
identifies the separate typed composition and lifecycle changes still required.

## Required durable sequence

1. Bind the exact authority volume, configured store, S3 route, local credential
   generation and create-operation ID. Negotiate atomic-empty-manifest-v1 before
   any fresh upload permission. Refuse unsupported or changed bindings explicitly.
2. Persist the full creation intent: parent/revision, portable name, expected
   sequence, operation IDs and immutable store identity. Derive the canonical root
   locally with node and root generation equal to the create-operation ID. Retain
   a stable, domain-separated staging-operation key; a lost local reply must not
   allocate another object, node or name.
3. Seal precisely the canonical page bytes under the exact derived target.
   Link the generation to its creation intent durably. Before a fresh send, apply
   the existing policy and source checks; use create-only conditional upload and
   full-byte verification. Reject nonmatching remote bytes or identity.
4. Persist the verified physical receipt and then the exact CreateEmptyManifest
   request, including its ETag/version and upload-operation binding, before RPC.
   These are distinct stages: the final metadata request cannot be manufactured
   before a real physical proof exists. SQLite failure after upload retains the
   original generation for observation/recovery.
5. Submit/recover only that saved metadata request. Validate the complete returned
   node, canonical manifest, placement, publication history and operation receipt.
   Save the receipt without substituting a newer live node. Name collision or a
   changed parent is an explicit retained outcome; never choose a new name or
   silently replace the competitor.
6. Initialize the ordinary authenticated manifest edit view through the typed
   native bridge. Historical creation success alone does not prove the name is
   still live: subsequent removal or rename must remain visible as such. Bind
   filesystem handles to the actual node and content epoch, not just a pathname.

Known local receipts should replay before narrowed quotas or fresh feature probes.
A metadata-only recovery facade must work without S3 credentials or source bytes
once the exact metadata request is saved. Earlier unknown physical outcomes still
need the original source-bound observer. Neither path may invent a provider receipt,
resend a claimed PUT, change an admitted parent/name, or erase retained uncertainty.
Orphan uploads remain retained until a separately verified cleanup protocol exists.

## Acceptance tied to the native outcome

Use deterministic SDK/TCP and authority fixtures for fresh creation, Unicode and
empty content, same-name races, absent feature before PUT, wrong route/credentials,
root corruption, policy expiry, lost local replies and physical/metadata responses,
restart and cancellation. Kill the real local owner around staging/link/proof/
request/receipt boundaries and verify SQLITE_FULL rollback and quota behavior.
Exercise historical receipt recovery after rename/removal without resurrecting it.

Then test the composed native path on actual FUSE and hosted WinFsp: create,
write beyond8MiB, sparse extension/truncate, read-your-writes, flush, reopen,
save/replace, rename/delete, open-unlinked handles, owner death and remount. The
same existing-file chunk path must be used by created files. This integration
must preserve the old read-only and whole-generation profile regression suites.
A real Wasabi write test needs a fresh concrete user-approved fixture scope;
physical MX Linux and Windows10/11 sign-in/reboot checks remain separate.


## Declared durable-intent implementation boundary (2026-10-19)

Worktree codex/empty-manifest-intent is based on exact repaired parent29eb237;
parent hosted acceptance is complete; this child phase requires its own gates before promotion. This first implementation
boundary is a client-local SQLite creation journal on the existing Storage owner.
It must support intent admission, exact staging-generation linkage, preparation
of the authority request from a verified local remote-intent receipt, and exact
metadata receipt/rejection recovery. It grants no network or native callback rights.
The subsequent managed facade must use these records and the existing Executor,
then compose with the ordinary native edit path described above.

Proposed schema40 adds bounded creation records and a checked membership projection,
without modifying old request or outcome bytes. Bind volume, create operation,
parent/revision, name, sequence, store, source and configuration fingerprint. Derive
canonical bytes/key and a stable staging-operation key from those identities.
Prepare the final metadata request only from the linked generation's verified
create-only remote record, matching exact target,64-byte body SHA and proof fields.
Known request/receipt recovery cannot require source bytes or credentials.

Known exact retries precede narrowed admission quotas. Fresh and changed-request
reuse, role adoption, triggers, corrupt membership/history or false SQL effects
must fail explicitly and preserve every existing row. Keep generic physical staging
available, allow the declared sparse source owner, and reciprocally exclude old
logical, namespace/open and complete-generation publication roles. This does not
remove their current isolation checks or enable native composition prematurely.

Acceptance requires canonical independent parent vectors; Unicode; exact replay
and changed binding refusal; same-owner async races/lost replies; actual process
death around admission/link/request/receipt; real SQLITE_FULL and bounded quotas;
old-schema byte preservation and migration rollback; source/authority isolation;
self-consistent corrupt proof/receipt rejection; and metadata-only replay after
staged-source loss. The network facade and native acceptance remain required
before a mounted-create capability can be reported.


## Local journal candidate implemented (2026-10-19)

The isolated codex/empty-manifest-intent candidate implements the first boundary
in Rust module manifest_creation, schema40. Existing Storage supplies one bounded
worker/connection/lock. Input binds every create field, physical store and local
source identity; canonical64-byte page, object key and staging-operation key are
derived. Admission reserves bounded record-growth capacity. Known exact intent
replays before narrowed admission quotas; changed input under an admitted operation
conflicts without changing earlier rows.

A sealed generation must use that exact staging key/target and canonical bytes.
Only its verified create-only remote journal record can prepare the final authority
request. The physical proof snapshot, exact request and later complete receipt or
terminal rejection are retained. Metadata replay checks the retained staging header
and operation binding without rereading staged bodies or acquiring credentials.
This has been tested after staged-body corruption and restart; it is not permission
to delete staging headers, operation bindings or other retry namespaces.

The membership projection refuses orphan/missing rows, self-consistent rewritten
request/proof fields, oversized values and unexpected role rows. Reads bound each
value before Rust allocation. SQL triggers are refused before admission or replay.
New role reciprocally excludes old logical, namespace/open and complete-generation
publication journals; sparse dirty views and generic staging can coexist on the
one private owner. Old source/authority isolation and all existing bytes remain.
Migration uses additive040.sql without IF NOT EXISTS; collisions roll back the
transaction and version marker. Test-only old-schema setup removes only the new
empty tables rather than permitting production migration to adopt collisions.

Fourteen focused cases passed in1.17s, including eight actual killed-owner
boundaries, actual SQLITE_FULL rollback/retry, a deterministic lost committed
worker reply, real authority receipt validation, corrupted staged-body recovery,
sparse coexistence in both orders, bounded corruption checks and version fences.
Thirty-five existing migration cases passed37.04s. Workspace all-target compilation
passed30.27s before the later tests and fixture updates. Initial E0624/E0277
visibility/error-conversion compile failures and an E0061 test call were retained;
all assertions remain in place. The first workspace Clippy run rejected two nested
conditionals; the style-only repair passed workspace all-target Clippy32.25s. Full local and own hosted
source/native/artifact gates remain required before promotion.

These are local intent tests: verified physical records are model fixtures, not
new live S3 uploads. The authenticated S3/authority facade, restricted recovery
bootstrap for otherwise unreadable source state, typed namespace/open composition,
and real native chunk-create/write acceptance remain necessary. Native callbacks,
startup settings, credentials and provider write permissions are unchanged.


## First full cohort and snapshot integration repair (2026-10-19)

Frozen source0ec7cfd600aeaba0b445f6b3842c7386a200dfdd/all354 failed its full
local gate at19:39:01Z (exit101, every input unchanged). CLI snapshot round-trip
failed with Snapshot(Validation): the offline authority snapshot validator did not
recognize the two new empty client tables. Own CI39312392108 Linux failed the
same test; book passed. Windows failed earlier at locked-profile cleanup in the
coordinator-supervision fixture,77.91s; its primary failure cause was obscured. A
later green cohort would not establish that earlier Windows failure's cause.

[Indexed first failed cohort](evidence/empty-intent-failed1-20261019.tar.gz):13
verified members/902503bytes, SHA-256
`36ca0c7555201edc2cff0949e2c81337eafe9ca19775b2702e0dcf9ac48cec83`.
It retains exact354 source, frozen map, complete local log/runner and all three raw
hosted logs/API identities. One-use failed archiver99420 DONE; never rerun.

The production repair adds only manifest_creation_role and manifest_creations to
the snapshot's fixed local-table allowlist. Their rows must still be empty. Unknown
tables, views, nonempty local tables, active owners and invalid artifacts remain
refused; the64-table bound and restore prohibition are unchanged. A new native
state test verifies each populated creation table blocks snapshotting without
clearing client rows or producing an artifact. Tests of a latest-version seed use
SCHEMA_VERSION; literal old39/downgraded/future-marker expectations stay exact.

Original CLI snapshot cases passed2/0.24s; snapshot library cases passed10/1.43s,
including real schema8 and the new client-table isolation control. Repaired
workspace all-target Clippy passed9.40s. The repaired354-input source requires
a new complete local and own hosted source/native/artifact cohort before promotion.
Earlier pending statuses are historical. No network facade or native chunk-create
capability has been enabled.


## Repaired cohort running (2026-10-19)

Frozen [sourcebb17161](https://github.com/marshallr12/s3drive/commit/bb17161ecd60d31b34dfc90b341b5ea4d3d6d899),
schema40/all354, is undergoing a fresh full local gate and own
[CI39313836156](https://github.com/marshallr12/s3drive/actions/runs/39313836156),
attempt1. Book118270052958 succeeded; Linux118270053086 and Windows118270053014
remain running. Full acceptance is pending; accepted canonical runtime stays
source29eb237/schema39/all349. No source, provider or native write capability
from this candidate is reported as accepted.


## Complete repaired local gate (2026-10-19)

Sourcebb17161ecd60d31b34dfc90b341b5ea4d3d6d899/schema40 passed the full local
gate at19:57:58Z, all354 frozen inputs unchanged. Publication24/158.91s,
state520 (one diagnostic ignored)/252.87s, reader26 (one native-only ignored)/67.87s,
39core/62S3, formatting/workspace all-target Clippy/build and book checks passed.
The log records93 Markdown files and89 chapter-test entries; ordinary four
threads and unchanged operation bounds were used.

The [indexed local archive](evidence/empty-intent-local-20261019.tar.gz) contains
403 verified members/23660725bytes, SHA-256
`a66a26b0e73fa50afe0267e13ca8f7669cd9a2c5a19ec37cc15a9bd8a42e2d09`.
It retains final354 source, full log/map/runner, all focused and failed source
contexts and independently indexed parent349 local/hosted and first failed-cohort
archives. Full gate log SHA-256
`5423ef96e3169cdfef6fdad9f940a88f80174cadb226a44a57982ec9e7f1cc2b`.
One-use local archiver63176 DONE; never rerun.

Own CI39313836156 attempt1: book118270052958 and Linux118270053086 succeeded;
Windows118270053014 remains running. Complete own raw/source/native/artifact
acceptance remains required. Parent349 is accepted. These tests do not enable
a network creation publisher or native chunk-create/write callbacks; source
proofs in the new state-journal tests remain explicitly modeled local fixtures.
Earlier pending local statuses above are historical.


## Own hosted acceptance (2026-10-19)

Sourcebb17161ecd60d31b34dfc90b341b5ea4d3d6d899/schema40/all354 passed
[CI39313836156](https://github.com/marshallr12/s3drive/actions/runs/39313836156),
attempt1: book118270052958/Linux118270053086/Windows118270053014 all succeeded.
Complete raw logs, exact354 working/committed inputs, own API ZIP digests, independent
typed21+14 records, project oracles and all35 raw fields/JSON types passed. Own
replacement12215769525/disposition12218526631 artifacts were used.

The [indexed hosted archive](evidence/empty-intent-hosted-20261019.tar.gz) contains
119 verified members/441408bytes, SHA-256
`5aa41539617cd261118f87668dcb75a0679c601331e3180fcc60204970cd53bf`.
One-use raw reviewer/source checker52968/comparator+archiver5863 DONE; never rerun.
Observer1442770 CLOSED0/all_success20:13:21Z. All seven slow Linux and three slow
Windows cases completed;15 interleaved reference completions were checked.

Linux520/Windows515 state cases (one diagnostic ignored each),26reader (one
native-only ignored),24publication/39core/62S3 passed. Actual existing native
regressions passed FUSE9/75.65s, WinFsp7/84.86s, manifest ranges/remount1.63s/1.60s,
and paired no-systemd24.10s/Windows78.49s. Actual images: Ubuntu24.04 test image
20261018.339.1 (book image20261012.334.1), WindowsServer2025 windows-2025-vs2026
20261007.264.1. Three locked disposable profiles had dummy credentials removed;
final deletion deferred to VM destruction. Tooling deprecations and unused WinFsp
delay-import warnings were retained/reviewed. Earlier Windows cleanup failure's
primary cause remains unverified; this success does not retrospectively explain it.

This accepts the local creation-intent journal, schema migration/isolation and
snapshot compatibility repair, with existing native regressions. New state-journal
physical receipts are model fixtures. Authenticated S3/authority creation, restricted
metadata recovery, typed namespace/open composition, native chunk writes, retention,
real Wasabi mutations and physical MX/Windows10/11 sign-in/reboot remain separate
work. Earlier pending statuses above are historical.
