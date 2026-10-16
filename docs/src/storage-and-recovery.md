# Cache, SQLite and recovery

P4a implements credential bindings and SQLite migration. P4b1 adds private local
directory ownership. P4b2a implements the bounded clean-cache storage library and
worker. P4b2b connects that cache to read-only mounts. New offline opens and the
writable mounts remain unsupported. The [local staged-generation and edit libraries](staged-writes.md)
implement pending local intent, patch/truncate and pinned-base hydration;
upload execution remains disabled.
An initialized database is not an implemented or proven upload journal.

## Implemented SQLite state and migration

`s3drive-state` uses `rusqlite` with bundled SQLite. Initialization marks a
database with an application ID and `user_version = 5`. Schema 1's reserved
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
schema versions and missing expected columns fail explicitly. Schema 4 preserves
schema-3 clean snapshots and adds staged chunks, manifests,
local heads and pending intent. It does not execute remote upload jobs.
Schema 5 adds operation-key recovery for edit plans, preserves older manifests,
and binds new operation keys into manifest domain v2. It adds no remote jobs.

The connection uses WAL, `synchronous=FULL`, foreign keys, and a five-second
busy timeout. Tests cover restart, settings, binding isolation/rebinding refusal,
shape constraints, legacy-row preservation and migration rollback. The CLI's
`init-state` now initializes or migrates to schema 5. Mounts with an explicit
`[cache]` table register authorized source bindings and
use this database for clean snapshots. Mounts without that table keep direct
S3 range reads.
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
is local but disappears on reboot. It can hold disposable clean cache data;
new staging mutations/receipts are refused on known tmpfs storage.

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
read. Cache-enabled session fixtures additionally reopen preserved state after
shutdown. Mounted cache behavior has its own native acceptance corpus below.

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
mounted reads retain direct revision-pinned range reads.
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

## Implemented mounted cache (P4b2b)

Caching is opt-in for `mount`; the standalone `list`/`stat`/`read` tools continue
to access S3 directly. Add one top-level table to the same TOML as `sources`:

```toml
[cache]
directory = "/home/YOUR-USER/.cache/s3drive/default"
max_bytes = 67108864
max_file_bytes = 1048576
max_entries = 4096
```

Use an absolute path for this computer. On Windows, a TOML literal string can
hold `directory = 'C:\Users\YOUR-USER\AppData\Local\S3Drive\cache'`.
Create its parent first. The private final component is created by S3Drive;
existing directories must meet the ownership contract above. Neither `~` nor
environment variables are expanded inside TOML. Defaults apply when the three
limits are omitted. Omit the whole table to disable caching. `config-check`
validates syntax/limits without creating state or contacting S3.

Before initializing the cache, the CLI rejects overlap between the cache and
mount trees, resolving existing ancestor aliases. State must remain on supported
local storage. Each mount owns its cache directory exclusively; separate mounted
processes need separate directories. Buckets within one mount share its quota
and remain isolated by their source/credential bindings.

Every file open resolves credentials and completes an online HEAD before a
cache address can be constructed. First read then checks the exact binding,
key, revision and size. A hit returns checksum-verified bytes. A miss downloads
the complete eligible file through the handle's conditional range GET, then
commits it locally before replying. Sixteen fixed lock stripes coalesce
same-revision misses. Four slots bound concurrent cache operations and whole-file
downloads. No SQL transaction spans a network request, and the potentially large
SDK read futures are heap-pinned for native callback stack use.

A successful cached read retains an immutable `Pinned` snapshot in that open
file context. Later reads through it can continue after remote replacement,
revocation or loss of network; they retain the bytes authorized for that open.
A new open must authorize again and sees a new revision or an error. An already
open handle that has not yet cached its bytes still needs a conditional GET;
replacement can therefore produce a stale-handle error. This avoids mixing
revisions without promising immediate revocation of already-open cached bytes.
Restart the mount after editing static AWS profile files; refreshable providers
retain their own refresh behavior.

Files above the threshold use direct range reads. Zero-length/EOF reads return
without downloading or adding an empty cache entry. If clean quota is fully
pinned, the first requested read is served from the verified download and that
handle switches to direct conditional range reads; no unaccounted snapshot is
retained. A real SQLite disk-full error maps to Linux `ENOSPC` or Windows
`STATUS_DISK_FULL`. Other failed SQL operations produce I/O errors. Committed
entries survive a failed insertion/eviction transaction. Disable caching or
resolve the local error before retrying; the database is never silently reset.

Adapters close the worker only after native callbacks stop. Successful unmount
waits for SQLite and its process lock to close. Startup helpers preserve cache
data on removal. The native corpus on both platforms exercises warm hits while
GET is denied, concurrent misses, Unicode/NFC/NFD separation, fully cached old
snapshots, new revisions, denied/deleted/offline new opens, graceful restart and
forced process death, corruption/missing-entry refetch, changed static secrets,
pinned quota bypass, cancellation during a stalled fill, and an injected SQLite
insertion failure. Storage tests separately induce real `SQLITE_FULL`. These are deterministic local fixtures;
live Wasabi cached-read acceptance is still separate work.

## Planned state layout

| Local resource | Purpose | Lifetime |
| --- | --- | --- |
| `metadata.sqlite3` with WAL/SHM files | Bindings, clean BLOBs, staged chunks/manifests and local pending intent; later remote jobs/conflicts | Persistent; migrate explicitly |
| `clean/<generated-id>` (planned) | Large revision-specific chunks/files | Evictable when unpinned and unused |
| External staging files (optional future backend) | Current P5a1 generations use non-evictable SQLite chunks | Never evict while dirty or unresolved |
| `volume.lock` | Single local daemon ownership | Held for daemon lifetime |
| Diagnostic logs | Redacted operational evidence | Rotated and size-limited |

Schema 4 also includes `staged_generations`, `staged_chunks`, `staged_extents`,
`retained_staged_chunks`, `staged_heads` and `write_journal`; see the staged-generation chapter.
Schema 5 also adds `staged_operations` for local intent recovery. Future
schema tables should build on these with volume ownership,
`nodes` (stable IDs and parent links), `cache_extents`,
`upload_jobs`, `upload_parts`, `conflicts`, and `provider_capabilities` with
probe provenance/expiry. These additional entities are not shipped migrations.
Do not persist remote credentials. A source alias alone is not a safe cache ID.

The implemented cache and journal APIs share a single SQL worker and bounded
command queue; see the [async owner contract](staged-writes.md#shared-async-owner).
Future upload tasks should also exchange bounded messages with it; no transaction waits on S3. Keep enough inode tombstones to
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

1. Serialize mutations per inode. Keep a base revision and build a complete
   immutable generation (P5a1 uses SQLite chunks); untouched ranges must
   come from the same base revision, not a newly overwritten remote object.
2. P5a1 publishes its complete manifest, retained payload references, pending
   intent and local head in one FULL SQLite transaction. An external-file
   backend would need durable payload/parent-directory ordering before SQL
   publication. Neither local boundary is a remote flush acknowledgment.
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
