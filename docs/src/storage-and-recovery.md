# Cache, SQLite and recovery

P4a implements credential bindings and SQLite migration. P4b1 adds private local
directory ownership. P4b2a implements the bounded clean-cache storage library and
worker. Mount integration, offline access and the upload journal remain planned.
An initialized database is not an implemented or proven upload journal.

## Implemented SQLite state and migration

`s3drive-state` uses `rusqlite` with bundled SQLite. Initialization marks a
database with an application ID and `user_version = 3`. Schema 1's reserved
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
includes clean snapshots in schema 3, but no upload jobs.

The connection uses WAL, `synchronous=FULL`, foreign keys, and a five-second
busy timeout. Tests cover restart, settings, binding isolation/rebinding refusal,
shape constraints, legacy-row preservation and migration rollback. The CLI's
`init-state` now initializes or migrates to schema 3. Mounts do not yet register
bindings or use this database; integration belongs to the next cache phase.
`State::open_managed` and `init-state` now acquire a private local directory and
retain its process lock until the SQLite connection closes. The internal
unmanaged schema helper is no longer a public API.

SQLite documents that WAL participants must share a host; keep state on a real
local disk outside the mounted tree. `FULL` synchronization is a useful building
block but cannot make a database transaction atomic with an unrelated cache
file or remote object. [SQLite WAL](https://www.sqlite.org/wal.html),
[SQLite synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous)

## Implemented local ownership boundary

`init-state --directory PATH` creates the last directory component when needed;
its parent must already exist. Relative paths are resolved against the working
directory, while parent traversal, symlinks and Windows reparse points are
refused. Existing directories must already be private. Unrecognized entries
cause refusal rather than adoption or deletion. The directory currently permits
only the database, known SQLite sidecars and `volume.lock`. Small clean payloads
are SQLite BLOBs; external payload directories are not yet supported.

On Linux, the directory must belong to the effective user with mode `0700`.
Ancestors must belong to that user or root and prevent other accounts from
replacing entries; root-owned sticky directories such as `/tmp` are supported.
Known files must be owned regular files with one link. Files from the earlier
SQLite bootstrap are tightened to `0600` only after those checks and successful
database application/schema validation, inside the
already-private directory. Symlink and hard-link targets are never changed.
The initial filesystem allowlist is ext2/3/4, btrfs, xfs, f2fs and tmpfs. Network,
FUSE, overlay and other unverified filesystem types are refused. Tmpfs state
is local but disappears on reboot; choose persistent local disk for a cache.

On Windows, new directories have a protected DACL granting the account and
SYSTEM inheritable full control. Existing entries must have a non-null DACL
with only supported allow rules for the account, SYSTEM or Administrators,
and grant the account full control. The owner must also be one of those
principals. This does not attempt to isolate data from privileged administrators.
Only local fixed NTFS volumes are supported. Open directory handles deny
delete sharing and remain alive for every ancestor, preventing path replacement
while the state is open. File hard links and reparse points are refused.
The ACL is supplied during directory creation, not applied afterward to an
initially public directory.
[Windows directory creation](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createdirectoryw)

The process opens the stable `volume.lock` file and uses Rust's nonblocking
exclusive file lock before opening the database or its sidecars. A contender
fails explicitly. The lock file is never deleted during normal shutdown;
closing the handle or process death releases the kernel lock. SQLite closes
before ownership is released. These locks coordinate cooperating S3Drive
processes, not hostile software running as the same user or administrator.
[Rust file locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock)

Tests cover competing processes, aliases, forced process termination followed
by reopening committed state, private permissions, symlinks/junctions, file
hard links, broad Windows ACLs, unknown entries and preserved unrelated bytes.
The native Windows session fixture additionally initializes and reopens state
from separate ordinary-account logons and denies a different account's file
read. These are state ownership tests; mounted file data caching is not enabled.

## Implemented clean-cache storage library (P4b2a)

`cache::Cache` owns a private `State` on one dedicated OS thread. Its async API
sends at most eight queued commands; a full queue returns an explicit error.
SQLite and checksum work stay off Tokio's executor. Operations have a 30-second
caller deadline. Cancellation before execution skips the command; cancellation
once an operation is running may leave a complete clean entry, never dirty
user data. `close()` acknowledges only after SQLite closes and the directory
lock is released. Dropping all clients also drains and stops the worker.

Schema 3 adds `clean_blobs`. Each row contains a generated SHA-256 cache address,
source binding, byte length, an independent SHA-256 payload checksum, the complete
small-file BLOB and an LRU counter. Address hashing uses tagged, length-prefixed
fields: credential/configuration binding, configuration fingerprint, exact UTF-8
key, ETag, version ID and size. Composed/decomposed Unicode keys remain distinct.
No object key becomes a local path; neither credentials nor raw object names
are stored in this table. Source registration rejects fingerprint rebinding.
The caller must obtain these inputs from an authorized `ReadHandle`; this
storage API does not authenticate access by itself.

The default library policy permits 64 MiB of logical payload, 4,096 entries and
files up to 1 MiB. Limits are validated before creating state. The API allows a
smaller threshold and byte/entry budgets, bounded above by 1 MiB per file, 1 GiB
of logical payload and 65,536 entries. Larger files return an ineligible result;
the future mount integration will retain direct revision-pinned range reads.
Empty objects consume an entry even though they use no payload bytes.

Each lookup checks size/source/shape before allocating a BLOB, verifies its
checksum and returns an immutable shared buffer. A `Pinned` snapshot prevents
its row's eviction while the worker remains open. Concurrent acquisitions share
that buffer. Eviction chooses the least recently used unpinned entries; if
all candidates are pinned, insertion returns `None` without deleting anything.
Insertion and any required eviction commit in one transaction. Damaged payloads
are discarded as clean-cache misses. A damaged SQLite database or failed SQL
operation is an explicit error, not permission to reset the database.

On restart, surviving snapshots are verified on demand; a reduced budget is
enforced before any entry is pinned. Logical quotas exclude SQLite page/index
space, source-registry metadata, WAL and temporary buffers. Freed pages are
reused and the database does not automatically shrink. This is not a hard limit
on total disk usage. Actual disk-full errors propagate and the transaction rolls
back, preserving prior entries. New opens still require online authorization;
no offline filesystem behavior is enabled by this library.

**Storage decision, 2026-10-16:** small payloads live in SQLite so metadata,
checksum and bytes have one transactional boundary. Separate files and durable
staging remain the plan for bulk payloads and writes. SQLite's published BLOB
comparisons are hardware/size dependent and do not establish performance on
our target workstations; this choice is about a verifiable first recovery
boundary, not a speed claim. [SQLite BLOB comparison](https://sqlite.org/intern-v-extern-blob.html),
[SQLite WAL](https://sqlite.org/wal.html)

Tests cover source/revision/size/Unicode separation, checksum/length corruption,
missing rows, LRU and empty-entry quotas, pins, restart with reduced limits,
bounded queue saturation, and executor responsiveness during a real SQLite
writer lock. `PRAGMA max_page_count` induces a real `SQLITE_FULL` error and proves
that eviction rolls back with insertion. Child processes are killed after a
committed payload and during an uncommitted eviction; reopening preserves the
committed bytes. This tests the SQLite durability boundary, not hardware power
loss. [SQLite page limit](https://sqlite.org/pragma.html#pragma_max_page_count)

**Still required before mounted caching is claimed:** configure and connect this
worker to the shared filesystem, authorize every new open before cache access,
coalesce concurrent misses, bound downloads, and run cache-hit/revision/denied
open tests through real FUSE and WinFsp mounts. Current mounts remain uncached.

## Planned state layout

| Local resource | Purpose | Lifetime |
| --- | --- | --- |
| `metadata.sqlite3` with WAL/SHM files | Implemented bindings and small clean BLOBs; planned inode mapping, jobs and conflicts | Persistent; migrate explicitly |
| `clean/<generated-id>` (planned) | Large revision-specific chunks/files | Evictable when unpinned and unused |
| `staging/<generated-id>` | Modified bytes not yet safely committed remotely | Never evict while dirty or unresolved |
| `volume.lock` | Single local daemon ownership | Held for daemon lifetime |
| Diagnostic logs | Redacted operational evidence | Rotated and size-limited |

Future schema tables should build on `source_bindings` with volume ownership,
`nodes` (stable IDs and parent links), `cache_extents`,
`upload_jobs`, `upload_parts`, `conflicts`, and `provider_capabilities` with
probe provenance/expiry. These additional entities are not shipped migrations.
Do not persist remote credentials. A source alias alone is not a safe cache ID.

The implemented clean-cache API uses a single SQL worker. Future upload tasks
should also exchange bounded messages with it; no transaction waits on S3. Keep enough inode tombstones to
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
