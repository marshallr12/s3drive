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
