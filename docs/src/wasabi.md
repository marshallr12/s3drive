# Wasabi integration target

The user selected **Wasabi** as the first hosted provider on 2026-10-16.
The user supplied test bucket `s3drivetest` in `us-east-2` (N. Virginia).
Local configuration is being prepared; its credential profile has not yet been verified.
No account has been accessed and no Wasabi compatibility claim has been verified.

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
```

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

## Acceptance record to complete

Record the source commit, date, Wasabi region/endpoint, bucket type/versioning,
nonsecret source configuration fingerprint, test object manifest and local OS.
Keep account/bucket identifiers in a private evidence file if necessary.

| Behavior | Current Wasabi evidence | Required check |
| --- | --- | --- |
| SigV4 authentication and correct region | Documented by provider; untested by S3Drive | Successful signed LIST, HEAD and GET using the selected profile |
| Delimiter listings and continuation tokens | Unknown | Multiple pages, empty folder marker, Unicode and literal percent keys |
| Ranges and exact Content-Range/length | Unknown | Compare sequential and random reads against known local bytes |
| ETag preconditions | Unknown | Old open handle fails or stays on its pinned version after a controlled replacement |
| Version reads | Provider documents version-ID download; untested by S3Drive | Repeat with versioned objects, check returned version and permissions |
| Mounted filesystem access | Unknown | Traverse the Wasabi source through Linux and Windows adapters |
| Conditional writes, multipart, delete and rename | Unknown; not implemented | Separate disposable-prefix tests during writable phases |

Wasabi documents downloading a particular version with `get-object --version-id`.
That supports investigating immutable-version handles, but does not establish
our exact combined `Range`, `If-Match` and `versionId` response contract.
[Wasabi version downloads](https://docs.wasabi.com/docs/how-to-download-all-versions-of-all-files-within-a-bucket)

An S3 compatibility statement cannot establish every concurrency or failure
behavior. The reader remains strict: if Wasabi omits or disagrees with required
range/revision metadata, return an error and investigate; never silently read
an unpinned replacement. Mutation probes remain separate from mount startup.
