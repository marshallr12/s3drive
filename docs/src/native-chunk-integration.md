# Mounted chunk writes: the remaining integration boundary

Source review,2026-10-19. **Proposed implementation plan; native chunk writes are
not enabled.** The reviewed filesystem paths match accepted source
`2fc45fed5a15c1a24f0f9713ab7caca02569ec5b`. The accepted schema38 libraries add
retention guards and local clean-adoption intent but do not change these mounted callbacks.

![Existing mounted writer and proposed chunk bridge, with local SQLite, Rust S3 client, shared coordinator and S3 service](images/native-chunk-integration.svg)

## Verified code boundaries

| Boundary | Observed behavior | Required implementation |
|---|---|---|
| Backend selection | `fs/managed.rs` creates the chunk reader only when `config.writes.is_none()`; write configurations retain the existing publisher | Explicitly select and negotiate a supported mutation profile; preserve validation failures rather than silently falling back |
| Native dirty files | `fs/managed/writes.rs` keeps a complete original `FileSnapshot`; write/truncate assemble the complete new byte vector, with an8MiB maximum | Exact sparse view heads, ranged dirty edits and guarded reads; durable size/head updates before a successful callback |
| SQLite roles | `state/sparse_views.rs::isolation` refuses populated logical/client identity, namespace intent and Windows open journals | A validated state composition/lifecycle contract; removing the exclusion checks alone is not a bridge |
| New/legacy origins | `BaseFile::sparse_binding` requires manifest content; the current edited-view path cannot turn a metadata-empty or legacy payload into a manifest origin | Separate authenticated origin formats and recovery rules, including empty creation and save-by-replacement |
| Unlinked publication | `preparation/publication/journal.rs` persists `ContentGuard { detached: false, removal: None }` | Typed ownership/removal proof for retained unlinked files; preserve existing fences and conflicts |
| Remote refresh and history | Own publication acknowledgments are not proof of observing another machine's new clean base; local history currently remains retained | Explicit clean external-base adoption and validated bounded retirement, with old readers and unknown outcomes preserved |

Source evidence: [backend selection](https://github.com/marshallr12/s3drive/blob/2fc45fed5a15c1a24f0f9713ab7caca02569ec5b/crates/s3drive-fs/src/managed.rs#L122),
[whole-generation writer](https://github.com/marshallr12/s3drive/blob/2fc45fed5a15c1a24f0f9713ab7caca02569ec5b/crates/s3drive-fs/src/managed/writes.rs#L519),
[role exclusion](https://github.com/marshallr12/s3drive/blob/2fc45fed5a15c1a24f0f9713ab7caca02569ec5b/crates/s3drive-state/src/sparse_views.rs#L37),
[manifest-only sparse binding](https://github.com/marshallr12/s3drive/blob/2fc45fed5a15c1a24f0f9713ab7caca02569ec5b/crates/s3drive-managed/src/chunks/reader/composition.rs#L19),
and [linked root request](https://github.com/marshallr12/s3drive/blob/2fc45fed5a15c1a24f0f9713ab7caca02569ec5b/crates/s3drive-state/src/sparse_views/preparation/publication/journal.rs#L172).
These are code observations, not a claim that a proposed bridge has passed tests.

## Proposed sequence and evidence

Scoped-operation and guarded-reader source/platform gates are accepted; see
[combined evidence](managed-reader-guards.md). Next define clean external-base observation and a validated retention cut before
continuous large writes can exhaust the retained-history limits. See
[clean epochs](mixed-clean-epochs.md) and [retention dependencies](mixed-retention-plan.md).
A count-bounded transient pin registry does not reserve dirty-window capacity or
permit deleting preparation, physical-attempt or root records.

The next native design must explicitly decide how source edits compose with the
existing namespace and Windows handle ledgers. Prefer one SQL owner and a durable
atomic transition where possible. If a separate journal is proposed, its cross-
journal recovery protocol must be specified and tested before any callback can
acknowledge success. This decision remains open; the diagram's dashed bridge is
not an implemented database layout. Keep the existing role guards until the new
composition has reciprocal isolation and corruption/recovery tests.

Use a shared Rust filesystem layer for logical open/read/write/truncate/flush and
metadata, with Linux FUSE and Windows WinFsp adapters. Preserve the existing
append/constrained-write, share/delete, cancellation and ownership rules. A writer
must expose its durable local head to subsequent metadata/read callbacks, recover
that head before declaring a remount ready, and return conflicts without dropping
its bytes. Exact remote commit/recovery must retain its original request identity.
Flush must not report completion merely because chunks were uploaded.

A first driver profile needs explicit origins and a complete application lifecycle:
create empty, write beyond8MiB, sparse growth/truncate, flush, rename/save-replace,
delete with retained opens, close and remount. Add one platform-independent engine
scenario, then actual FUSE/WinFsp acceptance of the same scenario with Unicode
names and binary/empty data. Kill the source owner at durable boundaries, lose
commit replies, narrow policy, deny provider requests and race another writer.
Each case must preserve old reads and unresolved data and verify exact retry/no
unconditional-overwrite behavior. Actual sign-in/reboot/SSH acceptance on MX Linux
and Windows10/11 remains separate from hosted native regressions.

Do not activate the mounted path by increasing `MAX_BYTES`, enabling the existing
chunk reader in a writable profile, or substituting publisher types alone. Those
changes leave the observed ownership, origin and recovery boundaries unresolved.

## Empty-file bootstrap option under investigation (2026-10-19)

A real empty manifest could avoid treating a metadata-empty node as if it already
had an S3 manifest. This is a source-reviewed option, not a declared mutation/API
or an implemented native profile. Existing source facts:

- Namespace creation derives the node ID from the create operation and currently
  creates it with no payload. See [creation projection](https://github.com/marshallr12/s3drive/blob/67a75e3047a867e2499ef08ded64fe00b72a4aa9/crates/s3drive-state/src/namespace/creation.rs).
- Namespace preconditions and open records use file IDs, placement and content
  references rather than complete file buffers. They could be reused with typed
  source-state composition. Their constructors currently reject sparse state;
  that guard remains required. See [preconditions](https://github.com/marshallr12/s3drive/blob/67a75e3047a867e2499ef08ded64fe00b72a4aa9/crates/s3drive-state/src/namespace_client/changes.rs)
  and [open journal](https://github.com/marshallr12/s3drive/blob/67a75e3047a867e2499ef08ded64fe00b72a4aa9/crates/s3drive-state/src/namespace_client/opens.rs).
- Manifest history needs the retained reservation descriptor and publication in
  namespace_uploads. A new creation operation must preserve that history, not just
  place a payload on the live node. See [historical lookup](https://github.com/marshallr12/s3drive/blob/67a75e3047a867e2499ef08ded64fe00b72a4aa9/crates/s3drive-state/src/namespace/publication/history.rs).

The candidate sequence is: derive a canonical zero-length manifest and its actual
root bytes/key from the known volume, create-operation/node ID and selected store;
conditionally upload and verify that immutable root; then atomically create the
namespace entry with that manifest payload and its historical publication record.
The logical file has zero bytes; its manifest object has a real encoded body.
The existing authenticated manifest reader can then supply an ordinary initial
clean view. No fabricated reservation or invented provider receipt is needed.

This requires a new negotiated authority operation rather than changing old
CreateEntry semantics. The authority should derive the canonical root itself and
bind the name/parent revision, configured store, creator identity and exact upload
proof fields. An independent unrelated namespace change need not conflict when
the parent guard still holds, matching existing guarded creation. Persist the
node, payload/history and exact operation outcome in one transaction. A known
reply must replay historically after rename/removal/restart. Reject malformed
or foreign proofs and all ignored/partial SQL effects without half-creating a file.

A client must persist its exact physical and metadata intent before sends and
recover unknown results without a second PUT or different namespace request.
An uploaded root whose create conflicts may remain an owned orphan; do not delete
it while the authority outcome is uncertain. Feature negotiation, atomic collision
races, corruption, real SQLITE_FULL and killed-owner boundaries need their own
phase contract and tests before implementation. Actual provider uploads still
require a new concrete authorization scope; no live test follows from this option.

A broader empty-origin binding is another possible design, but would require new
historical authorization and preparation/publication semantics for a base with
no provider object. The real-manifest bootstrap is the next option to evaluate
because it can retain the existing manifest-only edit representation. Legacy
payload conversion, bounded retirement and unlinked writes remain separate work.
