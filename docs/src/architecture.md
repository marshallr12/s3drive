# Proposed architecture

The diagram describes the target system. P0 added configuration, evidence
types, SQLite initialization and explicit adapter stubs. P1 adds the read-only
S3 transport. P2 adds the shared read-only namespace/handle engine and Linux
adapter; P3 adds Windows WinFsp. Optional clean caching and the local staging
library and local file-edit plans are implemented. Writable mount integration and conditional remote
commit remain planned. See platform contracts for native acceptance scope.

![Local applications reach the Rust filesystem engine through FUSE or WinFsp; its S3 client reads remote bucket data. Local SQLite and cache have separate roles.](images/architecture.png)

The blue read/cache path is implemented. New opens authorize with S3 before
SQLite cache access; a hit avoids the data GET. Amber external bulk files,
writable mount integration and coordination components remain planned. The
local staging/journal library is implemented separately from mounted writes.
[Startup and remote sessions](startup-and-sessions.md)
include MX Linux without systemd. [Turso coordination](turso-coordination.md)
evaluates an optional shared metadata authority, separate from each host's local database.


## Crate ownership and process model

`s3drive-core` owns configuration validation and path resolution.
`s3drive-fs` owns the shared read-only namespace, inode identities and open
handles. It depends on the S3 reader but not on FUSE or Windows types. OS
adapters translate its errors and project metadata into native structures.

`s3drive-s3` translates read operations to the AWS SDK and retains
ETags/version IDs as opaque validators. Its current implementation exposes
paginated list, head and bounded range reads; mutation APIs arrive
only with their precondition and recovery tests. Avoid designing a broad
generic VFS trait before the first real adapter reveals the necessary contract.

`s3drive-state` owns SQLite migrations, persistent identity, cache metadata,
local staged generations and eventual remote upload jobs. Its clean cache uses
BLOBs up to 1 MiB so bytes and integrity metadata commit together. P5a1 stages
complete generations as bounded SQLite chunks, with local intent in the same
transaction boundary. Optional external bulk files remain future work. The
cache and journal now share one bounded SQL worker through `worker::Storage`.
Mounted reads use its cache handle; staged writes are a library API until the
remaining write phases pass. No SQL transaction spans a network request. A local process lock must prevent two daemon instances from owning
the same volume/cache; SQLite locking alone does not coordinate cache files.

The platform crates own only OS callback translation, reply lifetimes, mount
setup and teardown. A long-lived Tokio runtime and bounded workers will serve
network work. Do not create a runtime per callback, block the runtime on SQLite,
or reply after a kernel request has been canceled. Keep callbacks independent
of credentials and object signing.

The core `s3drive` mount process runs in the foreground. A detached per-user
launcher now provides Linux desktop/SSH startup without requiring systemd.
Windows uses an account-owned password-backed scheduled task and a shared NTFS
folder mount; native tests verify reads across distinct authentication sessions.
Both launchers provide start/status/stop/remove. A future local control endpoint
can carry status, unmount, pin, retry and conflict actions.
A tray UI should consume that same API rather than implementing storage logic.

## Multiple buckets

Each source has a stable alias, endpoint, bucket, region and optional prefix.
The initial root is a virtual directory of aliases. Resolving
`documents/reports/a.txt` under prefix `desktop/` produces the literal key
`desktop/reports/a.txt`. Bucket roots and aliases are synthetic and immutable
through filesystem calls. Separate drives can later reference separate configs.

Do not merge same-named paths across sources. Cross-source rename returns
`EXDEV` or the Windows equivalent; a file manager may explicitly copy then
delete. No transparent failover, replication or balancing is promised. Different
providers can have different credentials, latency and capabilities.

Implemented storage cache keys include the credential/configuration binding,
exact object key, size and revision. P4a binds successful opens to the actual
credentials used for HEAD and all reads through that handle; new opens resolve
credentials again. Configuration or credential changes select a different
namespace, and every new open still requires online authorization. SQLite
schema 6 retains the binding registry, unbound legacy rows, clean snapshots,
staged generations/local intent and edit operation keys. It adds the
[remote-attempt journal](remote-journal.md) with single-use send claims and
uncertain-result recovery; network execution remains subsequent work.
The engine enforces online authorization before cache lookup.
This is not a provider-global account identifier. See the implemented
[provider contract](provider-contract.md) and [migration](storage-and-recovery.md).

## Direct objects versus managed filesystem layout

| Model | Benefit | Cost |
| --- | --- | --- |
| Direct objects, selected initially | Existing keys remain readable by independent S3 tools | Rename, file metadata and multi-client behavior are constrained |
| Managed layout with immutable file/chunk IDs and manifests | Namespace edits can be separated from data movement | Requires a durable metadata protocol, garbage collection, recovery and interoperability tooling |
| Gateway with authoritative shared metadata | Can serialize namespace operations across clients | Adds an always-available service and deployment responsibility |

A local SQLite file is not a cross-machine coordinator. A hosted transactional
service such as Turso could supply shared authority; see the separate
[Turso evaluation](turso-coordination.md) for the proposed protocol and evidence limits. Uploading that file to
S3 after each change does not produce a safe distributed database. If managed
mode is chosen, design either a single-writer service with explicit failover or
a thoroughly specified conditional-manifest protocol. Do not introduce either
as an incidental optimization to direct mode.
