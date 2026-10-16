# Existing tools and Rust libraries

This is desk research, not a performance benchmark or a claim that any provider
has passed S3Drive conformance tests. Versions below are observations on
2026-10-16, not compatibility guarantees.

## Existing solutions

| Option | What it contributes | Fit for this project |
| --- | --- | --- |
| [rclone mount](https://rclone.org/commands/rclone_mount/) | Linux and Windows mounts; Windows uses WinFsp; VFS cache modes trade disk space for application compatibility | Best behavioral baseline and an immediate existing-tool option; its implementation is Go |
| [Mountpoint for Amazon S3](https://github.com/awslabs/mountpoint-s3) | Rust implementation targeting high-throughput S3 access on Linux | Study its filesystem contract and error handling; it is not a generic desktop filesystem |
| [s3fs-fuse](https://github.com/s3fs-fuse/s3fs-fuse) | Native S3 objects, disk caching, random writes/appends and copy-based renames | Useful interoperability reference; different implementation language and platform approach |
| [Amazon S3 Files](https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-files.html) | AWS-managed shared filesystem, built using EFS, with NFS access and bucket synchronization | Relevant current alternative for AWS workloads; not a Rust desktop client for arbitrary S3-compatible endpoints |

rclone's `writes`/`full` cache modes persist pending changes locally and upload
after close and a delay. Its documentation also warns against overlapping
instances sharing a VFS cache. Use it to define comparison workloads, not to
infer that all POSIX operations are transactional over S3.
[rclone VFS documentation](https://rclone.org/commands/rclone_mount/)

Mountpoint deliberately limits filesystem semantics. Its detailed current
documentation distinguishes ordinary buckets from S3 Express One Zone rename
support. Prefer this specific contract over older blanket descriptions that
say all renames are impossible.
[Mountpoint semantics](https://github.com/awslabs/mountpoint-s3/blob/main/doc/SEMANTICS.md)

## Linux and Windows surfaces

| Surface | Rust entry point | Assessment |
| --- | --- | --- |
| Linux FUSE | [`fuser` 0.18.0](https://docs.rs/fuser/0.18.0/fuser/) | Recommended first adapter; implement filesystem callbacks in userspace |
| Windows virtual volume | [`winfsp` 0.13.1+winfsp-2.1](https://docs.rs/winfsp/0.13.1+winfsp-2.1/winfsp/) | Recommended drive-letter direction, subject to binding and distribution review |
| Windows Cloud Files | [Cloud Filter API and Windows.Storage.Provider](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine) via generated Windows bindings | Alternative for a registered sync folder with hydration, pinning and Explorer status |
| Windows Dokan | [dokan-rust](https://github.com/dokan-dev/dokan-rust) | Fallback virtual-filesystem route; evaluate current wrapper/driver version alignment before adoption |
| Windows ProjFS | [Projected File System](https://learn.microsoft.com/en-us/windows/win32/projfs/projected-file-system) | Lower priority: Microsoft directs slow remote-store scenarios toward Cloud Files |

The `fuser` project documents a Linux build without the `libfuse` feature,
although FUSE kernel access and appropriate mount helpers remain operational
requirements. Its repository currently says contributions should use issues or
forks rather than pull requests; evaluate maintenance expectations before
pinning the adapter release. [fuser repository](https://github.com/cberner/fuser)

The WinFsp wrapper documents delay-load setup and warns that FFI soundness is
difficult to prove. Treat a small Windows native spike and an unsafe-code review
as required engineering work. [winfsp-rs](https://github.com/SnowflakePowered/winfsp-rs)

## S3 transport candidates

| Library | Strength | Decision |
| --- | --- | --- |
| [`aws-sdk-s3` 1.152.0](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/) | Direct S3 operation APIs; first-party AWS implementation | Selected for P1 reads; mutation and recovery behavior still require verification |
| [`object_store` 0.14.2](https://docs.rs/object_store/0.14.2/object_store/) | Async object API, range reads, streaming and conditional puts | Strong alternative if it exposes every required precondition and multipart recovery detail |
| [Apache OpenDAL 0.59.4](https://opendal.apache.org/docs/rust/opendal/) | Many storage services and composable layers | Prefer if expanding beyond S3 becomes a near-term requirement |

The AWS SDK supports custom endpoints and path-style addressing configuration.
That enables an S3-compatible implementation; it does not prove compatibility
with every vendor. Test signing region, pagination, range responses, checksums,
multipart completion and error translation against each target.
[AWS Rust endpoint configuration](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/endpoints.html)

`object_store` exposes `PutMode` and conditional updates; its S3 builder also
has provider-specific settings. Investigate whether its public upload APIs
provide the persistent upload IDs and completion conditions needed by the
recovery design before selecting it.
[ObjectStore conditional writes](https://docs.rs/object_store/0.14.2/object_store/),
[S3 builder](https://docs.rs/object_store/0.14.2/object_store/aws/struct.AmazonS3Builder.html)

P0 linked no network or filesystem bindings. P1 now links `aws-sdk-s3`,
`aws-config` and Tokio for the read-only adapter, plus SHA-256 configuration
fingerprints and percent decoding for list responses. Native filesystem
bindings remain deferred. All dependencies are locked in `Cargo.lock`.
The local components use `serde`, `toml`, `url`, `clap`, `rusqlite` with bundled
SQLite, and `tempfile` for isolated tests. SQLite operations should run on a
dedicated worker once the filesystem engine exists.
