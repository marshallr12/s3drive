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

`open` performs HEAD and binds a key, size, revision and reader into a handle.
No caller can swap the handle's metadata for another source's metadata. At
least an ETag or non-null version ID is required; the literal version ID `null`
does not prove immutability. Empty files with a validator are valid.

Each GET sends the handle's ETag condition and/or immutable version ID. The
adapter checks the returned Content-Range, content length and revision before
returning bytes. It reads the stream incrementally into a buffer capped at the
requested length, rejects a short/oversized body, clips reads at EOF, and avoids
network requests for zero-length/EOF reads. ETags are not content checksums.
No automatic reopen is performed after a revision conflict.

Each operation has a 30-second overall deadline; range-read timing includes the
response body, not just receipt of headers. The SDK has a five-second connect
timeout and at most three attempts. Dropping a read future cancels it. These
defaults are currently fixed, and queue/concurrency limits belong to the future
filesystem engine. No cache or offline data path exists yet.

## Errors and identity

Errors expose an operation, an error category and optional HTTP status. SDK
debug text, URLs, remote error messages and credentials are not included.
403 remains permission denied rather than missing; 404 is not found; 412 is
revision changed; 416 is an invalid range. Timeouts, transport failures,
throttling, service failures and malformed responses are distinct categories.

The reader has a versioned SHA-256 **configuration fingerprint** covering source
alias, bucket, region, prefix, canonical endpoint, credential-profile selection
and addressing/loopback/listing-decoding settings (fingerprint format v2). It distinguishes reconfigured sources, but
does not identify the current authenticated principal. Credentials behind the
same profile/default chain can change. Before persistent caching, add a
verified identity boundary and migrate the unused P0 cache table. This phase
does not read or populate that table.

## Verification and remaining work

The local tests use real TCP sockets and the real SDK HTTP client with dummy
credentials. They cover pagination, exact key encoding, request signature
fields, version/ETag conditions, empty files/EOF, incorrect ranges, short bodies,
bad XML, missing tokens, source escape, HTTP error categories, canceled reads,
stalled bodies and configuration fingerprints. Workers use kernel-selected
ports, bounded waits and explicit teardown.

A CLI subprocess test supplies isolated dummy credential/config files plus
conflicting ambient AWS settings. It checks that the configured profile,
endpoint and region are used. It does not read the user's credential files.

The fixture inspects signing headers; it does not independently validate the
SigV4 cryptographic signature. Separate [live Wasabi evidence](wasabi.md)
records authenticated LIST/HEAD/GET, pagination, exact bytes and Linux mounting
against user-approved isolated objects. Stale validators, versioned reads and
mutating provider capabilities remain unverified. The OS adapters now implement
mounting and native name projection. Persistent authenticated cache identity,
cache recovery and all filesystem mutations remain later work.
