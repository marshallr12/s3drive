# Related reads with pinned credentials

Implementation date: **2026-10-18**. This S3-library preparation supports the
planned lazy manifest reader. It does not enable a manifest reader or mounted
chunked files. Focused S3 controls, the full local gate and the source-bound hosted
cohort have passed.

An existing `ReadHandle` binds source configuration, credentials, key and the
revision obtained through its online HEAD. A manifest file needs multiple
objects: a root page, child pages and data chunks. Opening each through the
original provider could resolve another credential generation midway through
that file's lifetime. Related opens must preserve the original generation while
independently authorizing every requested object.

`ReadHandle::open_related(key)` opens another source-relative key through the
handle's captured reader. It performs a fresh scoped online HEAD and returns a
new pinned handle. The source prefix, bucket, endpoint, configuration and
credential generation stay bound; its key, metadata and object revision are
obtained anew. Chained related opens retain the same generation. Independent
`S3Reader::open` calls can resolve another generation from their provider.

The helper grants no permission beyond the related object's successful HEAD.
It does not accept substitute metadata or expose credentials. Missing/denied
objects remain errors; an earlier successful read does not bypass a later HEAD.
Callers must still authenticate manifest descriptors, derive allowed keys,
validate object metadata and verify hashes before returning file bytes. This
helper adds no cache, database, write policy, wire format or native callback.

The implementation delegates to the existing S3 open path with a boxed future.
That path validates keys before credential/provider work, bounds credential
resolution plus HEAD, requires a revision-bearing response and pins subsequent
range GETs. Related opens preserve those checks. Captured credentials do not
silently switch to newly supplied credentials if they expire or lose access.

## Verification scope

The focused real-SDK/local-TCP suite passed **62 tests** in 21.04 s, including
three new controls. All credentials are synthetic and services use owned
ephemeral loopback ports.

- After an injected provider rotation, root/child/grandchild HEAD and data GET
  requests retain the first dummy access key and session token. Independent new
  root/child opens and GETs use the second generation. Credential-binding hashes,
  exact bucket/prefix/key paths and pinned GET preconditions are checked.
- A related key is HEAD-authorized again after a prior successful read. Denial
  and absence return their typed errors; an invalid key makes no request.
- The unpolled related-open future stays within the existing 4 KiB inline-size
  budget. This is a future-layout check, not native runtime-stack acceptance.

The first focused run passed two controls and failed the third's fixture path
assertion. The locked SDK adds its `x-id=GetObject` query parameter; the fixture
now parses the request target and checks its exact bucket/prefix/key path.
Production code was unchanged by that correction. The rotation control also
now exercises a chained child open. The complete S3 suite subsequently passed.

Resolved `aws-credential-types` 1.3.0 provider implementations and `aws-config`
1.12.0 source were inspected before implementation. The SDK's profile provider
caches parsed profile files, so the generation test uses the existing injected
rotating provider instead of assuming INI hot reload.
[AWS profile-provider documentation](https://docs.rs/aws-config/1.12.0/aws_config/profile/struct.ProfileFileCredentialsProvider.html).

The full local gate passed with all 252 frozen runtime/fixture hashes unchanged:
62 S3 tests, 365 state tests / 1 ignored, 34 core, 50 authority, 18 existing
managed-publication and eight existing chunk-publication controls, formatting,
Clippy, build, 67 documentation files and 63 book chapters. One retained-session
quota test exceeded a minute and then passed; this is not a benchmark. The
[12-member indexed local archive](evidence/pinned-related-reads-local-20261018.tar.gz)
retains logs, exact source, the frozen map, dependency audit and implementation
self-review. Every indexed member was verified. Review was not independent.
See the [dated evidence](evidence/pinned-related-reads-20261018.json).

This phase is developed in a separate checkout from accepted source checkpoint
`20706cd`. It is independent of the source-controller change, whose own hosted
acceptance has passed. Tested helper source `fef9442`
is pushed on `codex/pinned-related-reads`; all 252 hashes and its clean exact
remote were verified. Its [own hosted cohort](https://github.com/marshallr12/s3drive/actions/runs/39167583915)
completed successfully on Linux, Windows and the book job. All three raw logs
were reviewed. Own finalized artifacts 12151153703 and 12151479241 passed
independent strict 21/14 typed-record checks, the project oracles and all 35 raw
field/type comparisons. Older artifacts were not reused. The
[22-member indexed hosted archive](evidence/pinned-related-fef-hosted-20261018.tar.gz)
retains the exact source, logs, artifacts and checks; every member was verified.
The archive SHA-256 is `15d38ecf5047cd5f67c70c37347a19b527e2476d77c5580654edfbf44c629d26`.

Both platforms passed all 62 S3 tests, including the three new controls. Existing
format native regression also passed: nine Linux FUSE controls and seven Windows
WinFsp controls, plus separate startup, flush and disposition checks. Windows
retained three locked disposable fixture profiles until VM destruction after
dummy credential removal; unused delay-import and Node deprecation warnings
remain in the logs. This does not certify mounted chunks, a physical reboot or
new Wasabi behavior. Controller acceptance is independent. Integration and
verified manifest-range reads follow; native chunk lifecycle remains later work.
