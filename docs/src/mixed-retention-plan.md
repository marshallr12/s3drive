# Bounded local retention for managed chunks

Research and design proposal,2026-10-19. **Reclamation is not implemented. Pins and managed-reader guards are now accepted;
see [combined acceptance](managed-reader-guards.md).**
This follows [clean epochs](mixed-clean-epochs.md) and
[scoped source operations](scoped-source-operations.md). It is required before
continuous mounted chunk writes can exceed the current retained-history quotas.
A successful publication alone does not make its local dependencies disposable.

## Retry/checkpoint prerequisite (2026-10-23, accepted state contract)

The [bounded state contract](retry-checkpoint-safety.md) adds explicit downstream
retry fencing and revalidated observational checkpoint evidence, without deletion.
It deliberately refuses populated/native-owner activation and still requires all
ancestor rows. Full history severance, native-owner upgrade and bounded local
storage remain open. The accepted retained-history performance improvement at
main86d1e0e does not satisfy these retention gates.

## What the current code requires

Inspection of `sparse_views/ledger.rs`, `preparation/transfers.rs`,
`preparation/publication/ledger.rs` and the managed `reader/composition` modules
found these dependencies. Paths below are relative to their crate's `src/`.

| Retained data | Current reason it must remain | Boundary needed before deletion |
|---|---|---|
| Source views and original requests | One chain is reconstructed from the current head; derived spans require their parents | Validated clean anchors and explicit permitted cut edges |
| Original write blocks | Every historical write request contributes block references, even when later spans overwrite them | Retire the request and its required history, or preserve its blocks |
| Open edited reads | Accepted schema37 `EditedFile` and preparation clones share a SQLite read guard and snapshot permit | Same-owner source pin shared by clones |
| Preparations and object rows | Plans retain their source view and complete dependency/transfer proofs | Durable plan/recovery pin and explicit terminal retirement |
| Physical staging and upload intents | Transfer rows map exact staging generations and physical receipts; unknown attempts may still recover | Exact terminal receipt and no remaining recovery/source dependency |
| Root records and checks | Acknowledgment/rebase and publication lineage cross-check retained records | A validated compact anchor format and closed retry identities |

Deleting only `sparse_view_blocks`, keeping arbitrary hashes as tombstones, or
raising quotas would not satisfy these relationships. Existing validators must
continue to reject unexplained missing rows rather than treating them as GC.

## Proposed first slice: explicit pins, with no deletion

Add a bounded same-owner pin capability for an exact validated source view.
Acquisition, head selection where applicable, and durable pin registration must
share the SQLite transaction. A pin grants retention only; S3 reads still require
the existing authority, credential and content checks. Never hold that transaction
open while waiting for network I/O.

`EditedFile` clones should share one pin, as they already share a reader snapshot
permit. Preparation/upload/root recovery needs durable dependency pins that survive
a process restart. A pin must distinguish its purpose, source owner, exact view,
base scope and owner session. Caller-supplied IDs are intent, not existing pins.
Fresh acquisition must refuse an already retired view; cancellation must either
leave a retrievable committed pin or perform no admission.

For transient read pins, propose a persisted exclusive-owner session identity.
Only after acquiring the existing kernel ownership lock may a replacement owner
mark the previous session obsolete. Time passing, a missed heartbeat, sign-out,
SSH disconnect or a lost RPC reply cannot prove a reader is gone. This preserves
the requested tmux/remote-session lifetime. A failed asynchronous drop/release
must preserve the pin; explicit release or proven owner replacement recovers it.
No deletion is enabled by this first slice, even after release succeeds.
Pin admission must account for retained bytes and metadata and reserve space for
the active write window and its acknowledgment. If a new long-lived read would
consume that reserve, refuse acquisition before exposing the handle; never evict
an existing pin. The online window size must fit within the configured cache
budget after pin reservations rather than raising that budget implicitly.

## Proposed cut points and remaining protocol work

Start with clean rebase anchors, not arbitrary materialized dirty views. A clean
rebase has deterministic base coverage derived from its exact published receipt.
Old dirty reads can retain their original records from the nearest validated
clean anchor or initial origin. This could let a long-lived old reader preserve
its own bounded segment while newer clean epochs stop retaining all intervening
segments. A historical acknowledgment with a newer dirty head cannot establish
that cut point.

The exact anchor witness format remains unresolved. It must bind the original
view/request identity, exact acknowledgment and published root, current file scope
and permitted cut edge. A copied row hash alone is insufficient: current child IDs
bind the operation/request, while span validity is established by replaying the
parent. New validators must explicitly derive clean coverage and cross-check the
anchor against retained evidence. Do not accept a missing ancestor as an anchor.
Do not use a proposed anchor as a generic flush receipt or fresh global head.

Source-operation scoping closes only one retry namespace. Before forgetting other
records, specify preparation, root and physical-operation retry closure as well.
For example, forgetting an old preparation's random operation ID must not allow
that ID to admit a different preparation against a newer view. Unknown remote
outcomes and pending physical attempts stay pinned. Boundedness cannot be achieved
by retaining a growing tombstone for every completed operation forever.

The proposed order is: pins without GC; exact retry/anchor/terminal-retirement
formats with adversarial validation; then one-owner transactional retirement
across the source, preparation, root and physical layers. Keep local SQLite outside
the mounted tree. Neither local retirement nor its receipt authorizes deleting S3
objects. Shared Rust/Turso authority hosting remains a separate decision.

## Disk use and acceptance

SQLite ordinarily keeps deleted database pages available for reuse; shrinking the
file requires separate handling. `VACUUM` can require up to twice the original
file's size in free space, so it cannot be the emergency response to a full cache.
[SQLite VACUUM documentation](https://www.sqlite.org/lang_vacuum.html).
Long-lived SQLite read transactions can also prevent WAL checkpoint progress;
a retained file-read handle must therefore hold a logical pin, not a permanent
SQLite transaction. [SQLite WAL documentation](https://www.sqlite.org/wal.html).

The resolved bundled dependency is `rusqlite0.40.2` / `libsqlite3-sys0.38.2`, whose
local header reports SQLite3.53.2. SQLite documents the WAL-reset fix in3.51.3
and later; this resolved bundle includes a later version. This is dependency
inspection, not a power-loss acceptance claim.
[SQLite WAL-reset notes](https://www.sqlite.org/wal.html).

Acceptance must measure database, WAL and physical staging file sizes across many
publish/acknowledge/retire windows, rather than counting only live SQL rows.
Cover long-lived cloned reads; concurrent pin/retirement; process death before and
after pin/retirement commits; failed release; restart with unknown publication;
full-disk rollback; wrong-owner/copied/self-consistent forged anchors; narrowed
quotas; old-token replay after retirement; and retained Unicode/binary/empty reads.
Online writes must progress beyond the current history limits with bounded local
storage. Offline exhaustion must return an explicit error while preserving dirty
data. Complete separate Linux/Windows native and provider gates before activation.


Status2026-10-19: the [guarded-reader component](managed-reader-guards.md) passed
its scoped state/reader checks and is being combined with verified batching under
a new full gate. It does not implement dirty-window admission reservations, clean
anchors, retry retirement or reclamation. The [native integration source review](native-chunk-integration.md)
also identifies the existing role exclusions between source views and mounted
namespace/open journals; those guards must remain until a validated composition
and recovery protocol replaces them.
