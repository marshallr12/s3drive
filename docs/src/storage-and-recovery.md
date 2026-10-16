# Cache, SQLite and recovery

P4a implements credential bindings and SQLite migration. The file cache,
exclusive ownership, eviction and upload journal below remain proposed designs.
An initialized database is not an implemented or proven upload journal.

## Implemented SQLite state and migration

`s3drive-state` uses `rusqlite` with bundled SQLite. Initialization marks a
database with an application ID and `user_version = 2`. Schema 1's reserved
`cached_objects` table had only source aliases, with no credential binding.
Migration 2 renames it to `legacy_cached_objects_v1` and preserves every row for
inspection. None of those rows is adopted into an active cache namespace.

The new `source_bindings` table maps a 32-byte credential/configuration digest
to its configuration fingerprint. Registration is idempotent for the same pair;
a conflicting fingerprint is rejected without changing the existing binding.
The caller must obtain the digest from a successfully opened S3 handle.
Registration itself does not authenticate anyone or grant offline access.
See the [credential-binding contract](provider-contract.md).

Fresh initialization and migration from schema 1 run inside an immediate
transaction, including application/version markers. A migration failure rolls
back table renaming and the version update. Foreign databases, unsupported
schema versions and missing expected columns fail explicitly. The schema does
not yet include cache data or upload jobs.

The connection uses WAL, `synchronous=FULL`, foreign keys, and a five-second
busy timeout. Tests cover restart, settings, binding isolation/rebinding refusal,
shape constraints, legacy-row preservation and migration rollback. The CLI's
`init-state` now initializes or migrates to schema 2. Mounts do not yet register
bindings or use this database; integration belongs to the next cache phase.
The caller must supply a private local directory. Cache-directory permissions,
Windows ACLs and exclusive process ownership still need implementation.

SQLite documents that WAL participants must share a host; keep state on a real
local disk outside the mounted tree. `FULL` synchronization is a useful building
block but cannot make a database transaction atomic with an unrelated cache
file or remote object. [SQLite WAL](https://www.sqlite.org/wal.html),
[SQLite synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous)

## Planned state layout

| Local resource | Purpose | Lifetime |
| --- | --- | --- |
| `metadata.sqlite3` with WAL/SHM files | Source identity, inode mapping, cached revisions, jobs and conflicts | Persistent; migrate explicitly |
| `clean/<generated-id>` | Revision-specific downloaded chunks/files | Evictable when unpinned and unused |
| `staging/<generated-id>` | Modified bytes not yet safely committed remotely | Never evict while dirty or unresolved |
| `volume.lock` | Single local daemon ownership | Held for daemon lifetime |
| Diagnostic logs | Redacted operational evidence | Rotated and size-limited |

Future schema tables should build on `source_bindings` with volume ownership,
`nodes` (stable IDs and parent links), `cache_extents`,
`upload_jobs`, `upload_parts`, `conflicts`, and `provider_capabilities` with
probe provenance/expiry. These additional entities are not shipped migrations.
Do not persist remote credentials. A source alias alone is not a safe cache ID.

Use a single SQL worker initially. Cache and upload tasks exchange bounded
messages with it; no transaction waits on S3. Keep enough inode tombstones to
avoid reusing a kernel-visible ID while handles or lookup references survive.
Preserve schema migrations and write recovery tests for each version change.

## Reads and local disk pressure

Resolve source/key and authorize a fresh handle with an online HEAD, then fetch
only missing ranges. Key cache entries by credential-bound source identity,
exact object key, revision and size. Store a separate checksum for local data
integrity. Coalesce adjacent
reads, cap read-ahead and concurrent requests, and deduplicate in-flight reads.
Start with a simple bounded whole-file cache for small files and an explicit
large-file policy; add sparse range caching when measurements justify it.

Only clean, unpinned, unreferenced data is evictable. Reserve space before
accepting staged writes. If dirty data consumes the budget, fail additional
writes with a disk-full error and show pending upload state. Do not delete dirty
data to make a cache quota appear satisfied. Keep the metadata DB and emergency
recovery margin outside the ordinary clean-cache budget.

A fully cached file could later be offered offline with an explicit access and
stale-data policy. The first cache requires online authorization on new opens;
it must report failure when that authorization cannot be obtained.
An uncached range must fail while offline. Persisted directory metadata is not
evidence that its listed file contents are locally available.

## Proposed upload state machine

```text
clean -> dirty -> prepared -> uploading -> remotely-committed -> clean
                     |            |
                     +------------+--> retryable / conflict / needs-reconciliation
```

1. Serialize mutations per inode. Keep a base revision and allocate a generated
   staging path. Build the next complete file generation; untouched ranges must
   come from the same base revision, not a newly overwritten remote object.
2. On a local durability barrier, flush staged bytes and the necessary parent
   directory metadata, then commit a journal record referring to that durable
   generation. Implement the OS-specific equivalent on Windows. Never record
   a supposedly durable generation before its bytes are durable.
3. Upload an immutable snapshot of that generation. New writes go to a newer
   generation. Persist multipart upload ID, part numbers/checksums, expected
   revision and operation ID so recovery can reconcile progress.
4. Commit the replacement with a verified remote precondition. A remote success
   response is the strict durability boundary. A lost response is an uncertain
   outcome, not proof of either success or failure.
5. Reconcile uncertainty using remote revision/operation metadata and checksums
   when supported. If the result cannot be identified, retain local bytes and
   require resolution. Do not overwrite a concurrent edit during replay.
6. Commit the resulting remote revision locally, then release only the matching
   staged generation. A later generation remains dirty.

Multipart completion can contain an error even within an HTTP 200 response;
use the SDK's parsed result. Preserve conditions across completion and follow
operation-specific retry rules rather than blindly resubmitting an entire job.
[CompleteMultipartUpload API](https://docs.aws.amazon.com/AmazonS3/latest/API/API_CompleteMultipartUpload.html)

Orphan staging files, jobs with missing bytes, remote success before local
acknowledgment, interrupted multipart transfers and failed cleanup need distinct
recovery paths. Cleanup may abort only uploads the application owns. Never
globally clean up a bucket's multipart uploads.

## Durability choices

The recommended writable default is **remote durability on explicit flush**:
`fsync`/`FlushFileBuffers` waits for publication of the requested generation and
reports failure if it cannot complete. Ordinary writes are buffered. Unmount
must report pending/failed operations; it cannot silently convert them to clean.

An optional future offline write-back mode may acknowledge **local durability**
after the bytes and journal survive restart. That is a different user contract:
loss of the computer before upload loses pending edits. The UI/control API must
show pending bytes, retry state, conflicts and last successful remote commit.
Never change the meaning of flush implicitly when the network goes down.

ETags are opaque comparison values, not universal content hashes. Store a
separate application checksum when integrity verification is required. Versioning
helps recovery but does not prevent lost updates or provide distributed locks.
[S3 object metadata](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html)

## Recovery acceptance cases

Kill the daemon before/after every file sync, journal commit, upload part,
remote completion, local acknowledgment and cleanup. Restart without network,
then reconnect. Check that every acknowledged durable generation is either
available remotely or preserved locally according to its selected mode.
Run the same cases with disk exhaustion, permission failures, corrupt staging,
expired credentials and an external writer. P0 does not exercise these cases.
