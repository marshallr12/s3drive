# Wasabi integration target

The user selected **Wasabi** as the first hosted provider on 2026-10-16.
The user supplied test bucket `s3drivetest` in `us-east-2` (N. Virginia).
The local configuration and profile `wasabi-s3drive` are verified for signed
LIST, HEAD and ranged GET. With explicit approval, three isolated test objects
were created, read byte-for-byte through the CLI and a real Linux FUSE mount,
then deleted. The test prefix was empty after cleanup. This is scoped read-only
acceptance, not proof of every provider capability.

## Configuration

For this test bucket, use `https://s3.us-east-2.wasabisys.com`, including the
hyphen before `2`. Wasabi lists this endpoint for US East 2 (N. Virginia).
[Wasabi region endpoints](https://docs.wasabi.com/docs/service-urls-for-wasabis-storage-regions)

Save the local configuration as `s3drive.local.toml` in the repository root
(ignored by Git). A single-source configuration still requires `[[sources]]`:

```toml
version = 1
mode = "read-only"

[[sources]]
alias = "wasabi"
bucket = "s3drivetest"
region = "us-east-2"
endpoint = "https://s3.us-east-2.wasabisys.com"
prefix = ""
profile = "wasabi-s3drive"
path_style = true
list_url_encoding = "form"
```

The form-decoding setting is required by the observed Wasabi listing behavior:
its `EncodingType=url` response encoded the test key `a%2Fb + café.txt` as
`a%252Fb+%2B+caf%C3%A9.txt`. Without this setting the spaces would appear as plus
signs and lookup would fail. A live fixture exposed the difference; regression
tests preserve literal `+` and percent sequences while decoding spaces once.
This is evidence for this endpoint, not an assumption about every S3 provider.

The `prefix` filters object keys inside a bucket; it is not a setting to create
in the Wasabi console. Empty or omitted means the entire bucket. For example,
`prefix = "documents/"` exposes `documents/report.pdf` as `wasabi/report.pdf`.
A nonempty prefix must end with `/`.

Credentials live outside the repository, in `~/.aws/credentials` on Linux or
`%USERPROFILE%\.aws\credentials` on Windows. This is an INI file:

```ini
[wasabi-s3drive]
aws_access_key_id = YOUR_ACCESS_KEY
aws_secret_access_key = YOUR_SECRET_KEY
```

Preserve existing profiles when editing it. On Linux use private permissions
(`chmod 600 ~/.aws/credentials`). Never put real keys in TOML, command arguments,
chat or diagnostic logs. `profile` selects a named credential section; it does
not define a bucket. Add another `[[sources]]` block per additional bucket or
prefix, with its own unique `alias` and endpoint/region. Reuse the profile when
the same credentials have access, or select another named credentials section.

The checked-in [Wasabi template](https://github.com/marshallr12/s3drive/blob/main/examples/wasabi.toml)
remains a generic example; its placeholder bucket must be replaced.

Wasabi documents AWS Signature Version 4 authentication and use of AWS SDKs with
its S3 endpoint. S3Drive uses the Rust SDK with an explicit profile, endpoint,
region and path-style setting. Path-style URLs are also documented by Wasabi.
This is a design basis, not a replacement for running our actual reader against
Wasabi. [Wasabi API overview](https://docs.wasabi.com/apidocs/api-guides),
[documented object URL forms](https://docs.wasabi.com/docs/defining-a-bucket-policy-for-public-access)

```bash
cargo run --locked -- config-check s3drive.local.toml
cargo run --locked -- list s3drive.local.toml wasabi --max-keys 100
cargo run --locked -- stat s3drive.local.toml wasabi/example.bin
cargo run --locked -- read s3drive.local.toml wasabi/example.bin --offset 17 --length 4096
mkdir -p "$HOME/S3Drive"
cargo run --locked -- mount s3drive.local.toml "$HOME/S3Drive"
```

The network commands access the configured account. Use a dedicated test prefix
and read-only credentials for initial acceptance. Scope access to listing that
prefix and reading its objects; version-pinned reads may also require permission
to read object versions. S3Drive does not create buckets, change policies, upload
fixtures or enable public access. `read` writes raw bytes; redirect to a local
file when comparing a binary object. Linux mount instructions and limitations
are in [the implemented mount contract](linux-mount.md).

## Acceptance record — 2026-10-16

The [machine-readable evidence](evidence/wasabi-2026-10-16.json) records the
source file hashes, binary hash, platform, results and cleanup. Test namespace:
`s3drivetest/s3drive-conformance/20261016-lxrfkg8p/`. Bucket versioning was
`Disabled`. The current tested source includes the explicit form-decoding fix.

| Behavior | Current Wasabi evidence | Remaining check |
| --- | --- | --- |
| SigV4 and endpoint/region | Signed LIST, HEAD and GET succeeded | Credential expiration/permission failures |
| Delimiter listings and pagination | Three pages at max-keys=1, nested directory and literal/Unicode names | Folder markers and large tree workloads |
| Ranges and bytes | 524,288-byte pattern, 21-byte Unicode/literal-name object, empty file; exact SHA-256 and offset/tail/EOF comparisons | Broader sizes and network faults |
| Mounted filesystem | Real Linux FUSE sequential/concurrent reads and clean unmount | Native Windows against Wasabi |
| ETag preconditions | Reads supplied matching If-Match and received the expected bytes | Reject a stale ETag after a separately authorized replacement |
| Version reads | Unverified; this bucket has versioning disabled | Versioned test bucket and exact returned version checks |
| Writes, multipart, delete and rename | CLI fixture creation/cleanup only; S3Drive exposes no mutations | Separate safe-write phase conformance |

The binary fixture is `bytes(range(256))` repeated 2,048 times, SHA-256
`33bc8aab40703678c3ebe94d2dd8f2afff285dd901f9234e841e4679f8204fd5`.
The literal-name fixture contains `literal S3 key bytes` followed by a newline.
To reproduce, select and authorize a new disposable prefix, use create-only
PUTs for these files, set that prefix and form decoding in a separate local
configuration, run `list --max-keys 1`, `stat`, range reads and the Linux mount
commands above. Compare with the local fixture bytes and remove only the owned
test keys (or exact returned version IDs). Do not run mutation probes at startup.

Wasabi documents downloading a particular version with `get-object --version-id`.
That supports investigating immutable-version handles, but does not establish
our exact combined `Range`, `If-Match` and `versionId` response contract.
[Wasabi version downloads](https://docs.wasabi.com/docs/how-to-download-all-versions-of-all-files-within-a-bucket)

An S3 compatibility statement cannot establish every concurrency or failure
behavior. The reader remains strict: if Wasabi omits or disagrees with required
range/revision metadata, return an error and investigate; never silently read
an unpinned replacement. Mutation probes remain separate from mount startup.
