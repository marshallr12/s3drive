# Implemented read-only provider contract

P1 adds the AWS SDK transport now used by the P2 Linux mount. Its exposed storage
operations are LIST, HEAD and ranged GET only. It does not write objects, create
buckets, probe mutations or alter policies. Provider capabilities remain unknown
until separately tested.

## Configure and use

Copy the example to `s3drive.local.toml` (ignored by Git), replace bucket and
endpoint names, and select a credential profile if appropriate. Each source's
explicit region, endpoint and addressing mode control its S3 requests.
Ambient `AWS_ENDPOINT_URL`, `AWS_ENDPOINT_URL_S3` and region settings do not
override those fields.

With `profile` configured, the SDK profile credential provider uses that profile
rather than silently substituting process environment keys. Profiles can
themselves define credential sources/roles/processes as supported by the SDK.
Without an explicit profile, the SDK's default credential chain applies. No
credentials belong in the drive TOML or repository.

```bash
cargo run --locked -- list s3drive.local.toml documents --max-keys 100
cargo run --locked -- list s3drive.local.toml documents reports/ --max-keys 100
cargo run --locked -- stat s3drive.local.toml documents/reports/example.txt
cargo run --locked -- read s3drive.local.toml documents/reports/example.txt --offset 0 --length 4096
```

`list` returns one page, with an opaque `next-token` when more entries exist.
Pass the original token using `--token`, keeping the same source and directory.
Displayed names/tokens use Rust string escaping so control characters cannot
silently alter terminal output; unescape the displayed token before passing it
back. A machine-readable listing format is deferred. `stat` prints metadata;
`read` writes only file bytes to stdout. Maximum read length is 8 MiB per call.

For a local test service, this explicit exception is supported:

```toml
endpoint = "http://127.0.0.1:9000"
path_style = true
allow_http_loopback = true
```

The option accepts numeric IPv4/IPv6 loopback hosts only. A hostname such as
`localhost`, a LAN IP, and non-loopback HTTP remain rejected. HTTPS certificate
verification is unchanged. Tests bind an ephemeral port rather than assuming
9000 is free. The snippet does not start a service.

## List semantics

The SDK requests `ListObjectsV2` with delimiter `/`, URL-encoded keys and a page
size between 1 and 1,000. Returned keys/prefixes are percent-decoded exactly
once, validated as UTF-8, and checked against the requested prefix. Literal
percent escapes are preserved. `list_url_encoding = "percent"` is the default
and leaves unescaped plus signs literal. Providers such as the tested Wasabi
endpoint use form-style spaces; select `list_url_encoding = "form"` to decode
raw `+` as space and `%2B` as literal plus. Decoding is never guessed from a
filename. The same dialect applies to common prefixes and source containment.
Continuation tokens are opaque
and are never percent-decoded by application code. Missing/repeated tokens on
truncated pages, missing encoding declarations, invalid sizes and out-of-prefix
keys fail as invalid responses.

The API returns objects and directory prefixes separately; it does not yet
resolve file/directory or case-folding collisions for an OS mount. A folder
marker can appear as an object. Listing does not promise a multi-page snapshot.
The entry-count limit is checked after SDK XML parsing; there is not yet a
custom byte limit on the SDK's raw XML response allocation. Do not treat this
as a hardened arbitrary-host fetcher.
[ListObjectsV2 reference](https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html)

## Open handles and reads

`open` resolves one credential generation, performs HEAD using it, and binds
those credentials, key, size, revision and reader into a handle.
No caller can swap the handle's metadata for another source's metadata. At
least an ETag or non-null version ID is required; the literal version ID `null`
does not prove immutability. Empty files with a validator are valid.

Each GET sends the handle's ETag condition and/or immutable version ID. The
adapter checks the returned Content-Range, content length and revision before
returning bytes. It reads the stream incrementally into a buffer capped at the
requested length, rejects a short/oversized body, clips reads at EOF, and avoids
network requests for zero-length/EOF reads. ETags are not content checksums.
No automatic reopen is performed after a revision conflict.

When the provider returns rotated credentials, they affect new opens. Existing handles retain their original
credentials for GET, so an SDK refresh cannot silently switch a handle to a
different account. If those credentials expire or are revoked, a subsequent
remote read can fail; the caller must close and reopen. The 30-second open
deadline includes both credential resolution and HEAD. Credential-provider
failures are sanitized configuration errors; stalled resolution is a timeout.

Resolving again does not make the SDK watch credential files. Its profile
provider parses those files once and caches the parsed contents. Restart the
mount after editing static credentials or profile definitions; refreshable
providers retain their own refresh behavior. The binding uses the credentials
actually returned, rather than inferring them from a profile name.
[AWS profile provider](https://docs.rs/aws-config/1.12.0/aws_config/profile/struct.ProfileFileCredentialsProvider.html)

Each operation has a 30-second overall deadline; range-read timing includes the
response body, not just receipt of headers. The SDK has a five-second connect
timeout and at most three attempts. Dropping a read future cancels it. These
defaults are currently fixed. The shared engine adds bounded optional caching
above this transport; the transport itself always performs the requested S3
operation. New offline filesystem opens remain unsupported.

## Errors and identity

Errors expose an operation, an error category and optional HTTP status. SDK
debug text, URLs, remote error messages and credentials are not included.
403 remains permission denied rather than missing; 404 is not found; 412 is
revision changed; 416 is an invalid range. Timeouts, transport failures,
throttling, service failures and malformed responses are distinct categories.

The reader has a versioned SHA-256 **configuration fingerprint** covering source
alias, bucket, region, prefix, canonical endpoint, credential-profile selection
and addressing/loopback/listing-decoding settings (fingerprint format v2). It
distinguishes reconfigured sources, but does not identify the authenticated
principal. Credentials behind the same profile/default chain can change.

P4a adds an opaque `CacheIdentity` available from a successfully opened handle.
It is a domain-separated SHA-256 digest of that fingerprint and the exact
access key ID, secret key and optional session token used for HEAD. Fields are
length-prefixed, including an explicit absent/present marker for the token.
Its Debug output is redacted. Persist only the digest in private local state;
never persist the raw credentials. This binding is neither a provider-global
account ID nor an authorization token. It distinguishes credential generations,
including secret/token changes under an unchanged profile or access key ID.

The mounted cache requires an online HEAD for each new open. A stored
binding cannot authorize access; successful HEAD establishes only the provider's
authorization for that object at that time. A newly issued credential generation
gets a cold namespace even if it belongs to the same account. Already-open fully cached handles retain their authorized immutable bytes
until release, including after revocation/network loss. New offline opens are
denied. Bindings exclude object key/revision/size; mounted cache keys include
all of these independently. See [SQLite migration](storage-and-recovery.md).

## Verification and remaining work

The local tests use real TCP sockets and the real SDK HTTP client with dummy
credentials. They cover pagination, exact key encoding, request signature
fields, version/ETag conditions, empty files/EOF, incorrect ranges, short bodies,
bad XML, missing tokens, source escape, HTTP error categories, canceled reads,
stalled bodies and configuration fingerprints. Additional binding tests cover
credential rotation between opens, unchanged credentials across reader
recreation, configuration/secret/token isolation, denied HEAD, sanitized
credential failures and discovery timeouts. The rotating-provider test inspects
the actual Authorization/token headers on each HEAD and GET. Workers use kernel-selected
ports, bounded waits and explicit teardown.

The HEAD future inside `open` is heap-pinned to avoid amplifying its large SDK
state through native callback stacks. A size-budget test guards the open future;
real OS mount tests remain necessary to prove runtime stack behavior.

A CLI subprocess test supplies isolated dummy credential/config files plus
conflicting ambient AWS settings. It checks that the configured profile,
endpoint and region are used. It does not read the user's credential files.

The fixture inspects signing headers; it does not independently validate the
SigV4 cryptographic signature. Separate [live Wasabi evidence](wasabi.md)
records authenticated LIST/HEAD/GET, pagination, exact bytes and Linux mounting
against user-approved isolated objects. Stale validators, versioned reads and
mutating provider capabilities remain unverified. The OS adapters now implement
mounting and native name projection. The credential binding and schema migration
support the optional mounted clean cache described in the storage chapter.
Upload recovery and filesystem mutations remain later work.
