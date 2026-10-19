# Deferred canonical manifest integration

Source review dated2026-10-19. Proposal only, not implemented or a resolved
wire/API contract. Reviewed against
66d78db. Keep native delete-on-close/save-replace flags unsupported until proven.

Observed boundaries:

- `namespace/publication/empty.rs` creates node, canonical payload and upload history
  atomically, but has no removal owner. Its artifact is `Published(payload)`.
- `namespace/removals.rs` recognizes `CreateDeferredEntry` as the initial owner/fence
  operation and its artifact is `Removal(fence)`. That operation creates a
  metadata-empty node and cannot be silently used as a sparse manifest origin.
- `namespace/creation.rs` only defines metadata-empty creation receipt shapes.
- Native lifecycle calls `CreateDeferredEntry` in `s3drive-fs/src/managed/writes/lifecycle.rs`.
- Mixed root reservations still use `detached:false, removal:None` in
  `sparse_views/preparation/publication/journal.rs`; owned-unlinked writes require
  an exact validated removal fence, not merely a boolean switch.

Design choices still to resolve:

1. Prefer evaluating an explicit new deferred canonical-manifest mutation rather
   than treating an unchecked create followed by prepare-removal as atomic. It
   needs exact parent/name/store/owner guards, stable operation identity, canonical
   64-byte physical root proof, and a receipt representing both publication and
   the original removal fence. Reuse the same authority SQL transaction; schema41
   retains the 64-table snapshot bound, so do not add a table casually.
2. Preserve old operation and intent JSON/BLOB bytes. New optional fields or
   record formats require explicit old-schema downgrade refusal and feature
   negotiation, plus tests proving historical exact-request recovery before fresh
   feature/policy admission. No decided API or schema42 claim is made here.
3. The creation SDK uses its own durable journal, while `OpenInput.admission`
   expects a namespace journal input/outcome. A typed same-owner import of an
   exact verified creation receipt may be needed. It must validate original local
   physical metadata and original authority request; never fabricate an outcome
   from a path or a newer node. Shared receipt-shape checks should avoid divergent
   definitions of canonical nodes/payloads/fences.
4. Owned-unlinked publication must bind the exact owner, fence and current removal
   event in the original saved reservation request. Final removal must remain
   blocked while root/physical outcomes or local source acknowledgment are
   unresolved. Preserve old handles and newer dirty heads through recovery.
5. Do not increase the old 8MiB whole-generation constant as a substitute for a
   native sparse engine. Native integration needs canonical manifest opening,
   sparse read/write/resize, exact flush/local acknowledgment and the owner-bound
   namespace/open capability. Source retention and complete retry-forest closure
   remain separate requirements before a usable long-running drive/GC.

Required future evidence: atomic node/payload/history/fence killed-owner points,
SQLITE_FULL rollback, same-name/cross-store races, lost PUT/metadata replies without
resend, exact old-session replay, native FUSE/WinFsp delete-on-close and save/replace,
and separately approved real-provider and physical startup tests. Existing
library or whole-generation native success cannot stand in for those gates.

The [current bridge plan](native-chunk-integration.md) and its architecture image
show the broader native integration. This proposal does not activate callbacks
or grant S3 permissions. Keep both coordinator hosting options open; it does not
provision a service or select a Turso backend.
