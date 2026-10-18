# Wasabi integration target

The user selected **Wasabi** as the first hosted provider on 2026-10-16.
The user supplied test bucket `s3drivetest` in `us-east-2` (N. Virginia).
The local configuration and profile `wasabi-s3drive` are verified for signed
LIST, HEAD and ranged GET. With explicit approval, three isolated test objects
were created, read byte-for-byte through the CLI and a real Linux FUSE mount,
then deleted. The test prefix was empty after cleanup. This is scoped read-only
acceptance, not proof of every provider capability.

The separately approved [conditional-PUT probe](#conditional-put-acceptance--2026-10-16)
also passed later that day. Its six fixtures were removed and its prefix was
empty afterward. Ordinary CLI uploads and mounted writes remain disabled. A separate
[source-bound library policy](write-policy.md) is required to construct an executor;
no real application policy has been installed.

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
| ETag preconditions | Read validators passed; later single-PUT probes rejected stale/wrong ETags with HTTP 412 | Versioned reads and live network-fault recovery |
| Version reads | Unverified; this bucket has versioning disabled | Versioned test bucket and exact returned version checks |
| Conditional single-PUT create/replace | [Ten live checks passed](#conditional-put-acceptance--2026-10-16), including concurrent create/replace, Unicode and empty objects | Application write policy, recovery and remote flush |
| Multipart, application delete and rename | Unverified; fixture disposal is not a concurrent application delete protocol | Separate capability probes and safe-write conformance |

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

## Conditional-PUT acceptance — 2026-10-16

After explicit user approval, the `conditional-put-v1` plan ran **once** against
`https://s3.us-east-2.wasabisys.com`, bucket `s3drivetest`, under
`s3drive-conformance/d147830ba5257d762e14c3962cde60f9/`. The run started at
23:36:18 UTC with code commit `8eeda5b3b336047ad37f66b39e6c81c98c0c501c`.
The completed script used its fourteen PUT attempts, with payloads up to 4 KiB
and six exact keys. Preflight required an unversioned bucket and empty scope.

All ten checks passed: create, duplicate-create rejection, matching replacement,
stale/wrong-validator rejection, concurrent-create and concurrent-replace races
with one winner each, Unicode/literal-key replacement, empty-object round trip,
and receipt-independent observation using metadata plus a complete SHA-256 GET.
The literal key included `nested/日本語 %2F + café.txt`. Rejections returned 412.

Research refresh2026-10-18: Wasabi's current
[bucket/object documentation](https://docs.wasabi.com/docs/en/working-with-buckets-and-objects)
says object names containing four-byte UTF-8 characters, such as emoji, are
unsupported. The live literal-key sample above uses only one-, two- and
three-byte UTF-8 characters; it does not establish emoji-key support or disprove
that documentation. This is a provider documentation claim, not a new live
result. The chosen managed layout keeps logical names in the Rust authority's
database and forms immutable payload keys from volume/node/operation IDs under
the selected store root. Thus logical filenames do not need to become literal
S3 keys; an emoji in a configured physical store root still needs separate
validation. Managed logical emoji names and native/provider behavior retain
their separate acceptance scopes. No additional Wasabi request was made.

Cleanup verified ownership and exact bytes before deleting only the six fixture
keys. The final prefix listing was empty, with no errors or remaining keys.
The [sanitized evidence](evidence/wasabi-conditional-put-2026-10-16.json) records
checks, scope, code/binary/report hashes and these limits. The complete
credential-generation-bound report remains in the ignored mode-0600
`s3drive.probe-run.local.toml`. No raw credentials are in the evidence.

This approval has been consumed; do not rerun uploads with the same plan or
infer authority over ordinary bucket contents. It establishes sampled
unversioned single-PUT behavior for the recorded source. It does not establish
multipart conditions, versioned buckets, conditional application deletion,
rename, live fault recovery, strict generation fencing or provider-wide
linearizability. The probe deliberately forgetting a receipt is not a real
network fault. Native Windows against Wasabi and application uploads still
need their own acceptance. Existing mounts remain read-only.

## Prepared next-provider option (2026-10-18; not executed)

A fresh six-key plan was generated with the existing offline CLI. Preparation
made zero provider requests and resolved no credentials. Its source and bounds
are in the [sanitized preparation record](evidence/wasabi-next-plan-20261018.json);
the private plan's local path is in HANDOFF. It permits no action without new
explicit approval. The previous fourteen-attempt approval remains consumed.

This is a prepared renewal option, not a finding that renewal is necessary.
Before selecting evidence for a write policy, inspect its original observation
time, source/credential binding and explicitly chosen maximum age. Preparing a
new plan neither validates current bucket state nor executes a capability check.

Managed-filesystem acceptance needs a separate concrete runner and approved
application prefix. The implemented `WritePolicy::permits_full_key` rejects
application keys in the reserved `s3drive-conformance/` namespace. Provider
capability evidence and permission to mutate application objects are separate;
successful probe cleanup does not authorize a new managed-volume upload.
