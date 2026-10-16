# Research sources

Primary project/vendor documentation consulted on **2026-10-16**. Live pages
can change; versioned Rust API links identify the observed release where useful.
These references support the research, not claims of live integration testing.

## Filesystem surfaces and existing implementations

| Source | Why it matters |
| --- | --- |
| [fuser repository](https://github.com/cberner/fuser) and [0.18.0 API](https://docs.rs/fuser/0.18.0/fuser/) | Linux FUSE callbacks, dependency/features and maintenance context |
| [WinFsp project](https://github.com/winfsp/winfsp) and [license](https://github.com/winfsp/winfsp/blob/master/License.txt) | Windows runtime and its distribution terms |
| [winfsp-rs](https://github.com/SnowflakePowered/winfsp-rs), [API](https://docs.rs/winfsp/0.13.1+winfsp-2.1/winfsp/), [license](https://github.com/SnowflakePowered/winfsp-rs/blob/master/LICENSE.md) | Rust host/context APIs, delay loading, FFI and separate wrapper terms |
| [dokan-rust](https://github.com/dokan-dev/dokan-rust) | Alternative Windows adapter candidate |
| [Microsoft Cloud Files](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine) | Placeholder files, hydration, pinning, sync-root and Explorer integration |
| [Microsoft ProjFS](https://learn.microsoft.com/en-us/windows/win32/projfs/projected-file-system) | Why the projection API is less suited to slow remote stores |
| [Windows file naming](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file) | Reserved names and cross-platform path differences |
| [rclone mount](https://rclone.org/commands/rclone_mount/) | Existing cross-platform mount, cache modes, delayed uploads and cache ownership |
| [Mountpoint semantics](https://github.com/awslabs/mountpoint-s3/blob/main/doc/SEMANTICS.md) and [configuration](https://github.com/awslabs/mountpoint-s3/blob/main/doc/CONFIGURATION.md) | Rust S3 filesystem precedent and explicitly limited semantics |
| [s3fs-fuse](https://github.com/s3fs-fuse/s3fs-fuse) | Existing direct-object interoperability and cache/rename tradeoffs |
| [Amazon S3 Files](https://docs.aws.amazon.com/AmazonS3/latest/userguide/s3-files.html) | Current AWS-managed filesystem alternative |

## Object semantics and transport

| Source | Why it matters |
| --- | --- |
| [AWS S3 consistency](https://docs.aws.amazon.com/AmazonS3/latest/userguide/Welcome.html#ConsistencyModel) | Single-key atomicity and cross-key limitations |
| [S3 key names](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html) | Byte limits, case sensitivity and prefix hierarchy |
| [GetObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html) | Ranges, revision conditions and version reads |
| [HeadObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html) | Metadata and permission-dependent missing-object errors |
| [PutObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html) | Object replacement and write preconditions |
| [Conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html) | Create/replace conflict behavior |
| [CompleteMultipartUpload](https://docs.aws.amazon.com/AmazonS3/latest/API/API_CompleteMultipartUpload.html) | Completion conditions, uncertain outcomes and response parsing |
| [RenameObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_RenameObject.html) | S3 Express-specific rename rather than generic portability |
| [S3 metadata](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html) | ETag and metadata interpretation |
| [AWS Rust SDK](https://docs.rs/aws-sdk-s3/1.152.0/aws_sdk_s3/) | First transport candidate |
| [AWS Rust endpoints](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/endpoints.html) and [credentials](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credproviders.html) | Custom endpoints, addressing and credential providers |
| [object_store](https://docs.rs/object_store/0.14.2/object_store/) and [S3 builder](https://docs.rs/object_store/0.14.2/object_store/aws/struct.AmazonS3Builder.html) | Alternative async storage abstraction and provider settings |
| [Apache OpenDAL](https://opendal.apache.org/docs/rust/opendal/) | Broader multi-provider alternative |

## Local state and tooling

| Source | Why it matters |
| --- | --- |
| [SQLite WAL](https://www.sqlite.org/wal.html) | Local-host requirement and checkpoint behavior |
| [SQLite synchronous](https://www.sqlite.org/pragma.html#pragma_synchronous) | Durability settings and limits |
| [rusqlite 0.40.2](https://docs.rs/rusqlite/0.40.2/rusqlite/) | Actual scaffold DB dependency; resolved source inspected locally |
| [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) | Shared workspace settings and dependencies |
| [mdBook](https://rust-lang.github.io/mdBook/guide/creating.html) | Book structure and local authoring workflow |

## How to update this research

Refresh the load-bearing vendor contract before implementing an operation.
Record provider-specific findings with an exact environment and test result.
Keep test failures and unsupported features visible in the support matrix.
Never replace a failed integration check with a documentation citation: the
two are different types of evidence.
