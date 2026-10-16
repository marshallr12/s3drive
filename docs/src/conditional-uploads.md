# Conditional uploads and the disposable-key probe

**P5b1, implemented 2026-10-16:** a private single-attempt transport and explicit
`probe-writes` CLI can test a provider in a reserved namespace. Mounted drives
remain read-only. A [local attempt journal](remote-journal.md) now records durable
claims and uncertain outcomes. A [quarantined SDK executor](upload-executor.md)
now exercises those records with local HTTP fixtures. Production construction,
remote flush, multipart writes and dirty-data cleanup remain subsequent P5b work.

## Provider contract and evidence

AWS documents `If-None-Match: *` for creating an absent key, and `If-Match` for
updating the object whose current ETag matches. Failed preconditions return HTTP
412; concurrent operations can also return 409. These are conditions on the
current object, not a compare-and-swap against a version ID.
[AWS conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html),
[PutObject request and response contract](https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html)

These probes use distinct replacement bodies. They do not establish that ETags
are unique change counters or detect an intervening change that returns to the
same ETag. Strict generation fencing would require a different publication
protocol; do not infer it from an ETag match.

Wasabi describes broad S3 compatibility and points to AWS documentation. That
statement does not provide endpoint-specific evidence for our upload guards.
The [scoped Wasabi live probe](wasabi.md#conditional-put-acceptance--2026-10-16)
passed all ten checks on 2026-10-16, with cleanup. That evidence covers sampled
unversioned single-PUT conditions at the recorded source; it does not verify
multipart, application deletion or arbitrary providers. No compatibility
statement or successful read enables mounted writes.
[Wasabi S3 API reference](https://docs.wasabi.com/apidocs/wasabi-api)

The transport uses the resolved AWS SDK 1.152.0 `if_match`/`if_none_match` APIs.
It pins one resolved credential generation for the session, sets the SDK's
maximum attempts to one, and uses `RequestChecksumCalculation::WhenRequired`.
Known-length bodies avoid optional streaming checksum framing in the local TCP
tests. This is implementation evidence, not a claim about every provider's
checksum support. Each request has a 30-second deadline. The private single-PUT
transport accepts at most 8 MiB; probe payloads are only 4 KiB or empty.

## Unknown results need observation

The transport distinguishes a success response with a valid revision,
conditional rejection, and an uncertain result. A missing/malformed receipt,
timeout, closed connection or service error may follow an applied write. The
SDK must not silently replay that request. Dropping the caller can similarly
lose a result; it cannot undo a server-side commit.

Probe requests attach a random-run-derived operation marker and the payload's
SHA-256 in user metadata. Observation checks the exact key, markers and length,
then performs a complete bounded conditional GET and validates its revision,
metadata and SHA-256. It does not infer content from an ETag. A literal `null`
version is treated as mutable; it is not an immutable version selector.

An absent object or a rival marker does not authorize an automatic retry. Our
test model produces the same absence after a request that never committed and
after a committed request followed by another writer's deletion. Replaying the
create in the latter case would undo that deletion. Future application recovery
must retain the unresolved intent and local bytes unless a matching remote
operation can be established or the user explicitly resolves it. This phase
provides the transport contract; the [local journal](remote-journal.md) implements
durable states. P5b2b supplies the separately tested, production-gated executor.

## Reviewable probe plan

`probe-writes plan` performs no credential resolution or provider request. It
creates a new TOML file containing a random 128-bit namespace, source fingerprint,
bucket/endpoint/region, exact keys and operation limits. It contains no credentials.
Plan/report paths refuse existing files. Linux files are created mode 0600;
Windows inherits the destination directory's ACL. Store these metadata artifacts
in a private location. Credentials remain in the existing local profile.

Each plan reserves exactly six keys below the configured source prefix:

| Key | Purpose |
| --- | --- |
| `create.bin` | Conditional create and rejection when already present |
| `replace.bin` | Matching, stale and deliberately wrong ETag replacements |
| `race-create.bin` | Two concurrent create attempts, requiring exactly one winner |
| `race-replace.bin` | Two concurrent updates against the same ETag |
| `nested/日本語 %2F + café.txt` | Exact Unicode, literal percent, space and plus key through create/replace/read |
| `empty` | Empty-object create and verified read |

The namespace is `s3drive-conformance/<random-id>/`, relative to the source
prefix. Maximum: six stored objects, 4 KiB per object, fourteen PUT attempts.
The scripted bodies and ordinary request overhead are below 1 MiB of transfer.
The race checks sample concurrent requests; they are not a proof of every
possible execution or provider-wide linearizability.

Execution regenerates the entire plan from the source and random ID and checks
the supplied canonical SHA-256 confirmation. Editing a key and calculating a
new digest cannot authorize an arbitrary target. Source changes require a new
plan. Before any PUT, the probe requires an unversioned bucket, an empty prefix,
and a missing HEAD result for every exact key. It changes no bucket policy,
versioning, lifecycle, credential, startup registration or mount setting.

The test namespace must be reserved for this probe while it runs. Cleanup checks
each exact key against known operation markers and complete fixture bytes before
issuing a plain `DeleteObject`. This is explicit fixture disposal in a reserved,
unversioned namespace; it is **not** the application's concurrent delete protocol
or an unconditional fallback from a failed application guard. Foreign, corrupt
or versioned objects are retained and reported. Cleanup never deletes arbitrary
prefix contents. It also checks the prefix is empty afterward.

The CLI reserves its report file before network work and records a startup
comment. It writes the completed result and syncs the file after cleanup. An
interrupted/partial report is not capability evidence. The separate immutable
plan allows `cleanup` to inspect and dispose owned fixtures after interruption;
it never issues a PUT. All provider operations still need valid credentials and
the corresponding permissions. Reports never include raw SDK errors or secrets.

## Commands

Prepare and review first:

```bash
cargo run --locked -- probe-writes plan s3drive.local.toml wasabi \
  --output s3drive.probe-plan.local.toml
```

After authorization for the printed bucket, namespace and actions, use the
printed `plan-digest` as the confirmation:

```bash
target/debug/s3drive probe-writes run s3drive.local.toml wasabi \
  --plan s3drive.probe-plan.local.toml --confirm PRINTED_PLAN_DIGEST \
  --report s3drive.probe-run.local.toml
```

For an interrupted run, review retained fixtures and use a new report path:

```bash
target/debug/s3drive probe-writes cleanup s3drive.local.toml wasabi \
  --plan s3drive.probe-plan.local.toml --confirm PRINTED_PLAN_DIGEST \
  --report s3drive.probe-cleanup.local.toml
```

The commands run the explicitly selected source. Never infer authorization for
a new live probe from the earlier three-object Wasabi read fixture approval.
Only a complete passing report at the recorded source/credential generation
provides this probe's evidence; it does not automatically enable an upload API.

## Verification and next work

The reviewed plan with digest
`98914e5210466c6c1e074ea36184ea580d20c93749363e18650e5ddf14906de6`
was explicitly approved and executed once. Its fourteen PUT attempts are
consumed; all six owned fixtures were removed and the prefix was empty.
The [sanitized result](evidence/wasabi-conditional-put-2026-10-16.json) preserves
the evidence without local credentials. A new live mutation run needs its own
reviewed scope; this record does not enable application writes.

Real SDK/TCP tests cover enforced and ignored guards, stale/incorrect ETags,
two-writer races, exact Unicode/empty bytes, lost successes, missing receipts,
service errors, cancellation, malformed/oversized input, full checksum checks,
foreign cleanup refusal and preflight rejection for occupied/versioned scopes.
CLI tests cover offline planning, private create-new files, source/plan binding,
confirmation, edited-key refusal and existing-report preservation.

P5b2a adds durable attempt/uncertain/verified states, random operation IDs and
verified-parent guards in the local journal. P5b2b adds the journal-driven SDK
executor. P5b still needs a production write policy/factory,
unresolved-attempt/supersession resolution, explicit credential rebinding,
multipart recovery, flush semantics, and reference/pin-aware acknowledgment
cleanup. Direct `If-Match` is an ETag guard, not a version-ID fence. Preserve
these limits when deciding whether direct mode or managed metadata is needed.
