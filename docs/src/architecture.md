# Proposed architecture

The diagram describes the target system. P0 added configuration, evidence
types, SQLite initialization and explicit adapter stubs. P1 adds the read-only
S3 transport. P2 adds the shared read-only namespace/handle engine and Linux
adapter; P3 adds Windows WinFsp. Cache and writes remain planned. See the
platform contracts for native acceptance scope.

```text
Local applications
       |                               |
Linux kernel / FUSE                Windows / WinFsp
       |                               |
s3drive-fuse                      s3drive-winfsp
       +--------------+----------------+
                      |
            Shared filesystem engine
       namespace | handles | locks | errors
                      |
        +-------------+--------------+
        |                            |
  Local cache files            SQLite state worker
  clean / staged data          metadata / recovery journal
        |                            |
        +-------------+--------------+
                      |
       Async scheduler / provider adapter
          bounded queues, retries, cancellation
                      |
         S3 endpoint A       S3 endpoint B
         bucket/prefix       bucket/prefix
```

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

`s3drive-state` owns SQLite migrations, persistent identity, cache metadata and
eventually upload jobs. Large payloads belong in cache files, not database BLOBs.
Use one database worker rather than holding SQL transactions across network
requests. A local process lock must prevent two daemon instances from owning
the same volume/cache; SQLite locking alone does not coordinate cache files.

The platform crates own only OS callback translation, reply lifetimes, mount
setup and teardown. A long-lived Tokio runtime and bounded workers will serve
network work. Do not create a runtime per callback, block the runtime on SQLite,
or reply after a kernel request has been canceled. Keep callbacks independent
of credentials and object signing.

Initially `s3drive` runs in the foreground to make errors and shutdown visible.
Later add a systemd user service on Linux and a per-user agent on Windows. A
local control endpoint can carry status, unmount, pin, retry and conflict actions.
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

Cache identity must include a stable volume/source ID plus endpoint, bucket,
prefix and revision. A config change must not make old cache bytes appear under
another account or bucket. P0's reserved `cached_objects` table is not populated
by the application; add source identity and invalidation in P1/P4 before use.

## Direct objects versus managed filesystem layout

| Model | Benefit | Cost |
| --- | --- | --- |
| Direct objects, selected initially | Existing keys remain readable by independent S3 tools | Rename, file metadata and multi-client behavior are constrained |
| Managed layout with immutable file/chunk IDs and manifests | Namespace edits can be separated from data movement | Requires a durable metadata protocol, garbage collection, recovery and interoperability tooling |
| Gateway with authoritative shared metadata | Can serialize namespace operations across clients | Adds an always-available service and deployment responsibility |

A local SQLite file is not a cross-machine coordinator. Uploading that file to
S3 after each change does not produce a safe distributed database. If managed
mode is chosen, design either a single-writer service with explicit failover or
a thoroughly specified conditional-manifest protocol. Do not introduce either
as an incidental optimization to direct mode.
