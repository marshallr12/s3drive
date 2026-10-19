# Observing another writer's clean file version

Source review and proposal, 2026-10-19. This is not an implemented API or a
claim of native filesystem acceptance. Immutable chunks and manifests are the
user-selected representation. Another writer can publish a new manifest while
this machine still has an older local clean head; local acknowledgment of our
own upload does not solve that case.

## Existing boundaries

The local binding records intent, not authenticated remote bytes. Its target is
(volume, node); its scope also includes store, source identity, credential
fingerprint and object-root route. The existing clean rebase derives from our
own exact publication receipt. Inventing that receipt for a remote observation
would conflate two different operations. See the reviewed
[binding/model source](https://github.com/marshallr12/s3drive/blob/903ade67e21cd71bb7ab02716c098993d9f6e82f/crates/s3drive-state/src/sparse_views/model.rs)
and [publication acknowledgment source](https://github.com/marshallr12/s3drive/blob/903ade67e21cd71bb7ab02716c098993d9f6e82f/crates/s3drive-state/src/sparse_views/preparation/publication/settlement.rs).

The source ledger retains immutable views, operations and patches. Preparation
and publication have separate retained records. An acknowledged historical plan
must not permanently block observation simply because its row still exists;
an unresolved preparation, claimed upload, unknown outcome or rejected work must
not disappear merely because a remote version was observed. See
[source ledger](https://github.com/marshallr12/s3drive/blob/903ade67e21cd71bb7ab02716c098993d9f6e82f/crates/s3drive-state/src/sparse_views/ledger.rs),
[preparation ledger](https://github.com/marshallr12/s3drive/blob/903ade67e21cd71bb7ab02716c098993d9f6e82f/crates/s3drive-state/src/sparse_views/preparation/ledger.rs)
and [publication ledger](https://github.com/marshallr12/s3drive/blob/903ade67e21cd71bb7ab02716c098993d9f6e82f/crates/s3drive-state/src/sparse_views/preparation/publication/ledger.rs).

## Proposed first slice

1. Acquire a guard for the exact local head and identify whether its derivation
   is clean. Initial full-base views and validated clean rebases are candidates;
   a write or resize remains dirty even if its bytes happen to equal the base.
2. Refuse fresh adoption when work for this file is unresolved. Define and test
   the preparation/publication correlation explicitly. Retained, acknowledged
   historical work may remain; do not silently cancel other states.
3. Obtain a fresh authenticated authority observation and verify the referenced
   manifest through the existing reader. Require the same file and scope for
   this first slice. Route, store and principal migration remain separate work.
4. Commit a distinct immutable observation record and new clean head in one
   SQLite transaction, conditional on the expected old local head and current
   work state. Permit changed file length. Preserve old views, read pins and all
   operation outcomes. Hold no SQLite transaction across network I/O.
5. Reconcile a lost local reply using the exact admitted operation before any
   new remote request. Reject reuse with a different observation. Repeated polls
   of an unchanged binding must not grow history without bound.

The observation proves an authenticated remote version at observation time.
It cannot promise that the remote head stays unchanged until or after the local
commit. A later write still needs the normal exact authority CAS and explicit
conflict handling. A local record alone cannot establish remote authenticity.

## Local implementation status

The isolated [local intent phase](external-clean-local.md), source67a75e3/schema38,
implements the SQLite half of this proposal. Focused tests and state Clippy passed;
its full local and own Linux/Windows gates are running. Main remains accepted
schema37. The distinct record/domain, exact scoped retry, unchanged result without
admission, immutable View result and terminal-work predicate are declared there.
This does not authenticate a new remote version or complete this proposal.

The next managed facade should recover known local admission before network I/O,
retain an old-view guard, preflight local cleanliness/work state, load the live
node and independently verify its manifest root, then recheck the live payload
and pinned provider revision before the local CAS. Use one existing operation
deadline and the reader's current active/snapshot bounds. Neither a historical
lookup alone nor a publicly retained candidate proves a fresh live observation.
Existing code has separate [live opens](https://github.com/marshallr12/s3drive/blob/903ade67e21cd71bb7ab02716c098993d9f6e82f/crates/s3drive-managed/src/chunks/reader.rs)
and [historical authorization](https://github.com/marshallr12/s3drive/blob/903ade67e21cd71bb7ab02716c098993d9f6e82f/crates/s3drive-managed/src/chunks/reader/composition.rs).
This facade sequence is a source-reviewed proposal, not an implemented operation.

## Earlier open decisions

The following questions motivated the declared local phase; its contract now
settles the local choices, while authenticated facade behavior still needs tests.

## Decisions needed before implementation

Choose a distinct record format/hash domain and migration behavior, including
old binaries refusing new state. Specify the no-change result and retry-token
semantics. Define the exact terminal-work predicate without requiring perpetual
quiescence of historical records: an old pinned view may acquire preparation
work after adoption. Decide whether the API returns a durable local receipt or
also a newly guarded reader, avoiding a post-commit pin-quota failure being
reported as if no adoption occurred. None of these choices is settled here.

## Required acceptance

Independent byte/model checks must cover larger, smaller, empty and Unicode-named
remote versions; dirty and no-op local edits; a concurrent local edit or competing
adoption; foreign scope and denied/corrupt/stale remote data; pending/unknown work
and acknowledged history; exact retry before and after restart; retained readers;
quota, real SQLite-full rollback, injected SQL effects and process death around
every durable boundary. SDK-backed tests must prove fresh authorization and no
network during known local recovery. Run the full local gate and this source's
own Linux/Windows cohort before promotion. Native chunk writes, retention/GC,
provider mutation checks and physical sign-in/reboot remain separate gates.

See [native integration boundaries](native-chunk-integration.md) and
[bounded retention](mixed-retention-plan.md) for the surrounding prerequisites.
