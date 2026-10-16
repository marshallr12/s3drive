# Security and operations

These are implementation requirements for future network/mount phases, not a
claim that the scaffold already enforces runtime security policies.

## Credentials and endpoints

Use the selected SDK's credential-provider mechanism rather than a custom
signer. The AWS Rust SDK documents environment, shared configuration and
workload credential providers. Credential expiry and refresh must be tested
for long-running mounts. Separate credentials per source where needed; do not
assume the process-wide default identity is correct for every bucket.
[AWS Rust credential providers](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html)

The committed config contains placeholders and optional profile names only.
P0 rejects unknown fields, endpoint userinfo/query/fragment and non-HTTPS
endpoints. Parsing errors report a byte range instead of echoing the TOML source.
P1 must define precedence between explicit source settings and SDK environment
overrides so an ambient endpoint setting cannot silently redirect a source.

For local integration tests, add a narrowly scoped loopback HTTP test option
only when needed; never weaken certificate validation globally. Remote URLs,
object keys and profile names can be sensitive even when they are not passwords.
Redact authorization headers, session tokens, signed URLs and request bodies.
An SDK debug dump is not a suitable production log record.

## Access boundaries

Start read-only credentials with listing limited to the configured prefix and
object read access. Version-specific reads and encrypted objects may need
additional provider permissions. Add write/delete/multipart permissions only
for the writable phase and its dedicated test prefix. Do not create buckets,
change policies, enable versioning or alter encryption as mount side effects.

The initial drive is single-user. OS-visible file modes/security descriptors
must not imply that each local user receives distinct S3 authorization when all
requests use one credential. Shared mounts require an explicit access model.
Remote key names must never become unchecked local cache paths.

Store metadata and cached bytes in a private directory. P0 creates new Unix
state directories with mode 0700; existing directory permissions are not
repaired, and Windows uses inherited ACLs. Production bootstrap must inspect
ownership/ACLs and symlink boundaries before accepting the location. Disk
encryption is an operating-environment decision; plain SQLite does not encrypt
the database, and server-side S3 encryption does not encrypt the local cache.

## Operational behavior

Expose current mount/source status, last successful request, remote versus local
durability mode, queued/failed bytes, conflict count, cache budget and credential
expiry warnings. Report remote storage quota as unknown unless it is actually
known; enforce local cache headroom regardless of remote capacity.

Use bounded exponential backoff with jitter for retryable failures, a finite
per-operation deadline and cancellation. Distinguish authorization, missing,
precondition failure, throttling, local disk pressure and provider outage.
Retrying permission errors indefinitely hides a user-action problem.

For service shutdown, stop accepting new mutations, attempt the selected flush
policy, persist remaining state and unmount cleanly. A failed flush must remain
visible and recoverable. Provide diagnostics/export and explicit conflict
resolution before a writable release; avoid opaque retry loops.

## Costs and performance

Object-backed filesystems amplify requests: directory traversal, `stat`, antivirus
and thumbnail generation can cause many LIST/HEAD/GET operations. Large-file
edits may upload an entire replacement; rename by copy/delete may incur storage
operations and transfer costs. Egress and request pricing are provider-specific
and are not estimated in this research.

Measure request counts, bytes transferred, latency percentiles, read-ahead
waste, cache hit rate, memory use and local disk use. Use cold and warm reads,
many small files, a large directory, sequential and random reads, then writable
save/replace workloads. Compare with rclone under the same credentials,
dataset and cache budget. Record versions, configuration and host/network
conditions so results can be reproduced.
