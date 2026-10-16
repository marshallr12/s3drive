# Security and operations

The [provider contract](provider-contract.md) records the security boundaries
already implemented in P1. P2 adds an owner-only read-only Linux mount with
non-executable files and signal cancellation. The cache/service requirements remain
future work.

## Credentials and endpoints

Use the selected SDK's credential-provider mechanism rather than a custom
signer. The AWS Rust SDK documents environment, shared configuration and
workload credential providers. Credential expiry and refresh must be tested
for long-running mounts. Separate credentials per source where needed; do not
assume the process-wide default identity is correct for every bucket.
[AWS Rust credential providers](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html)

The committed config contains placeholders and optional profile names only.
The parser rejects unknown fields and endpoint userinfo/query/fragment.
HTTPS is required except for the explicit numeric-loopback testing option.
Parsing errors report a byte range instead of echoing the TOML source.
P1 constructs the S3 client separately from SDK shared endpoint settings:
source endpoint, region and addressing are authoritative. Explicit profiles use
the profile credential provider, avoiding unrelated environment credentials.

For local integration tests, `allow_http_loopback = true` permits HTTP only for
a numeric loopback address with `path_style = true`. Certificate verification
remains enabled for HTTPS. Remote URLs,
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
