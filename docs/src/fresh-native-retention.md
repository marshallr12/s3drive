# Fresh linked-native retention (work in progress)

Status: implementation candidate, not accepted. No hosted or actual mounted
acceptance is claimed by this document. The previously accepted default and
`owned-unlinked-v1` profile remain unchanged.

Alpha databases are disposable. This work does not convert old mounted stores
or promise backward compatibility. An explicitly selected
`linked-retention-v1` profile requires a fresh owner and establishes mount
binding format 2, source-scoped identities and preparation retry mode atomically
before its first operation. Existing format-one stores are refused, not deleted.

## Bounded scope

The candidate targets repeated edits, flushes, actual Close and remount of
existing linked manifest files. Creation, rename, removal/unlink, replacement,
owned/deferred publication and retained session recovery are not supported by
this profile. Their original journals remain protective; this is not completion
of the whole owned/unlinked native lifecycle or general namespace retention.
No provider object is deleted, and no automatic upgrade or default activation
is introduced.

Open and session identities include an owner-bound monotonic admission ordinal.
Only atomic admission consumes the next ordinal. Exact retained records can be
replayed; after retirement an old identity expires rather than being fabricated
as a current success or admitted anew. A concurrent candidate for an already
consumed ordinal must fail. There is one session per live runtime and bounded
retained open/session capacity, including retired opens held by readers.

Preparation, generation and root-publication IDs derive from the exact original
open and source epoch. Collection requires a current clean head, terminal roots,
completed actual Close on all target opens, and no logical or physical reader.
Clones retain the same reader protection. Dirty, uncertain or conflicting work
is never discarded. Long-held readers can impose backpressure rather than
forcing a cut or raising quotas.

## Independent terminal evidence

The compact publisher proof retains the exact Close/freeze/completion chain
suffix and its original retired-open seal, not an unbounded open-event journal.
A separate bounded per-target witness in the permanent mount binding binds its
digest, selected plan/generation, target, clean view and admission frontier.
Native anchors and witnesses must form an exact bijection; missing witnesses
are not repaired from anchors. Every original selected and unselected candidate
is validated before the transaction, and the surviving graph is validated after
source, physical and open retirement. All changes commit together.

A no-effect closed linked open with no source epoch or preparation can expire
without retaining a historical result, because it has no surviving dependency
and its ordinal cannot be reused. A failed or unknown Close is not eligible.

## Pending acceptance

The local workstation uses overlayfs/tmpfs; its durable-state test guard remains
unchanged and refuses those filesystems. Local compilation is not durable or
native acceptance. Required hosted evidence includes Linux and Windows repeated
cycles beyond history limits, DB/WAL/staging bounds, stale identity rejection,
coherently rehashed forged proofs, reader/clone lifetime, concurrency, process
kill and actual SQLITE_FULL rollback, plus real FUSE/WinFsp debug and release
save/Close/remount coverage. Existing whole-sequence deadlines remain unchanged.
