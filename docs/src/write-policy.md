# Explicit application write policy

**P5b2c, implemented 2026-10-16:** the single-PUT library executor can now be
constructed from an explicit source-bound policy and complete local probe
evidence. Existing configurations still accept only `mode = "read-only"`.
There is no ordinary upload command, writable mount, remote flush or automatic
payload reclamation. No real application write policy has been installed for
the user's bucket, and this phase makes no live provider requests.

The [approved Wasabi probe](wasabi.md#conditional-put-acceptance--2026-10-16)
establishes sampled conditional-PUT behavior for its recorded source and
credential generation. Its fourteen PUT attempts were consumed and its six
fixtures removed. Keeping that report does not authorize more probe uploads,
ordinary bucket edits, or multipart tests.

## Three separate local inputs

`uploads::WritePolicy::from_toml(source, policy, plan, report)` accepts bounded
UTF-8 TOML documents. Each is limited to 32 KiB. Unknown fields, unsupported
versions, malformed digests, an altered plan, an incomplete/failed report and
future-dated evidence fail with sanitized errors. It does not resolve
credentials or contact S3.

The plan must regenerate exactly from the selected source and its run ID.
The report must match its plan digest and configuration fingerprint, include
all ten named successful checks in order, and have no errors or remaining
fixtures. A cleanup-only report cannot establish acceptance. The timestamp
must follow plan creation and not exceed the current wall clock. The policy
selects the SHA-256 of the **exact full report file**, including any comments
and whitespace. The committed sanitized JSON evidence summary is not this
full report and cannot replace it.

These files are trusted local owner input. The hash protects against selecting
or accidentally editing the wrong report; it is not a signature or independent
attestation. Someone able to replace both policy and evidence can fabricate
them. The ordinary `Capabilities::Verified` enum is still insufficient, and
S3 credentials and bucket permissions remain the provider's access control.

The repository's `examples/write-policy.toml` is a separate document,
not a new table inside the mount configuration:

```toml
version = 1
source_fingerprint = "REPLACE_WITH_64_LOWERCASE_HEX_SOURCE_FINGERPRINT"
report_sha256 = "REPLACE_WITH_64_LOWERCASE_HEX_OF_EXACT_FULL_REPORT_FILE"
writable_prefix = "documents/"
max_object_bytes = 4096
operations = "create-only"
object_metadata = "s3drive-markers-only"
evidence_max_age_seconds = 86400
```

Both placeholder digests intentionally fail validation. The fingerprint is in
the full report and plan; the report hash can be calculated locally with
`sha256sum` on Linux or `Get-FileHash -Algorithm SHA256` on Windows. Do not send
credential files or full local evidence to chat or commit them to Git.

`writable_prefix` is relative to the configured source prefix. For a source
prefix `desktop/`, the policy above permits file keys under
`desktop/documents/`. It does not permit `desktop/documents2/` or the directory
marker itself. A nonempty prefix must end in `/` and follow the same path
component rules as source prefixes. An explicitly empty string allows the
source root; it is never the default. Unicode, spaces, plus signs and literal
percent text remain exact UTF-8 bytes, without URL decoding or normalization.
Reserved `s3drive-conformance/` namespaces cannot be application targets.

The byte limit must be 1..8,388,608; zero-byte files are allowed. Wasabi's live
guard probe used bodies up to 4,096 bytes. The larger 8 MiB transport bound has
local SDK/TCP evidence, not live Wasabi acceptance at that size. No policy
enables multipart or claims a provider-wide size guarantee.

`create-only` grants absence-guarded creation. `create-and-replace` also grants
ETag-guarded replacement. There is no unconditional overwrite mode. The
mandatory metadata setting acknowledges the present transport contract:
it writes S3Drive's operation and SHA-256 user metadata. It does **not** preserve
arbitrary existing user metadata, content type, tags or other object settings.
Preserving or configuring those attributes needs a separate contract before
writable desktop workflows. There is no rename/delete/ACL/retention API here.

Check a separately prepared policy without contacting Wasabi:

```bash
cargo run --locked -- upload-policy-check s3drive.local.toml wasabi \
  --policy s3drive.write-policy.local.toml \
  --plan s3drive.probe-plan.local.toml \
  --report s3drive.probe-run.local.toml
```

This reads local files only and does not change them, load credentials or
construct an executor. Success verifies local structure and binding evidence;
it does not assert that credentials still work, that evidence is fresh enough
for a new claim, or that the provider has not changed behavior.

## Construction and recovery

`Executor::open(reader, storage, policy)` first checks the configuration
fingerprint, then pins the resolved credentials and compares their generation
digest with the full report. A different alias, endpoint, region, bucket,
source prefix, profile, access key, secret or token cannot silently adopt that
evidence or existing local history. Opening makes no S3 request or mutation;
credential discovery may use the configured credential provider. Explicit
rebinding remains future work.

`executor.target(key)` creates a bound local staging target inside the policy.
The application stages and seals immutable bytes through the existing local
library, then explicitly calls `submit`. Prefix, full key, byte limit and
freshness are checked before preparing a new remote intent. The selected guard
is checked before claim; a denied replacement can leave an unclaimed Prepared
record, which may be inspected or explicitly cancelled. Freshness is checked
again after local payload hashing, immediately before requesting the claim.
This is admission expiry, not cancellation of a request already authorized
and in flight.

Evidence age is explicitly bounded by 1..2,592,000 seconds (30 days) in the
policy. The application never refreshes evidence, runs probes or switches
credentials automatically. An expired policy still constructs an executor for
recovery. Already-claimed intents can be observed through `submit` or
`reconcile` even if the new policy narrows the prefix or size limit. They can
never acquire another claim. Prepared intents cannot send under an expired
policy; `reconcile` only inspects them. Clock rollback before the recorded
observation also denies new admission.

All existing single-use, exact-observation, conflict retention and four-slot
bounds in the [executor contract](upload-executor.md) still apply. Local tests
exercise the public factory, strict parsing, complete wire-generated probe
evidence, changed credentials, prefix escapes, size/guard denials and recovery
under expired/narrowed policy. These tests do not authorize cloud mutations.
Multipart, uncertain-intent resolution, credential rebinding, flush,
acknowledgment cleanup and writable FUSE/WinFsp acceptance remain subsequent
phases.
