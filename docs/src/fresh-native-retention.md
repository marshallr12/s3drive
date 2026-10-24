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

The candidate regression fixtures include a private shared-runtime capacity test:
retain an independent cloned edited-file reader, close 256 native opens, observe
backpressure, release the last reader and reopen through the ordinary filesystem
API without an explicit collector call. It also checks exact bytes after a cold
restart. This test has its own bounded subprocess deadline and does not alter
any existing native deadline. State tests separately pin an older physical root
that must actually disappear after release and validate rollback including both
permanent opt-in mode rows. These describe required checks, not passed results.

Development runs have exposed performance blockers, not acceptance: early
shared-cycle runs exceeded their existing step deadlines, and the retained-open
pressure check detected six duplicate full-history scans during one pinned
refusal. The current candidate reuses a privately constructed validation scope
only inside one immutable read transaction. Frontier, mode, every retained open
and session, native semantic edges, source/preparation rows and compact terminal
witnesses remain validated. No proof survives a write or another invocation.
Late-corruption regressions include a coherently resealed unselected native edge
and changed frontier, session, identity and both permanent mode rows. The
256-open pressure check also retains an earlier compact anchor. These fixes
still require exact-source hosted execution under unchanged limits.

The next runtime-pressure attempt also shares validated owner/session facts and
four prepared SQL statements within each read-only retained-open scan. Every
open and event is still reread and replayed, and unused sessions are still
checked. Standalone mutation readers do not reuse that context. Deterministic
counters require all 256 histories and every event to be visited while preparing
only four scan queries and validating the shared owner/session once. Additional
late-corruption cases cover open owners, event hashes, resealed unknown or
volume-mismatched sessions, and an unused damaged session. Runtime progress
reports open and Close cost separately; its deadline remains unchanged.

A separate diagnostic job now measures individual state primitives, not total
filesystem operations. In run40073081377, at 16 retained opens, ordinary
session/record/admission reads performed four full scope scans, native Close
performed twelve, and completion performed ten. The controller invokes several
of these primitives per filesystem open or Close, so these timings cannot be
substituted for the mounted gate. The next candidate consolidates each
format-two mount requirement into one explicitly owned read transaction, or
one read-only interval of the caller's existing transaction. It returns only
the validated binding, never the shared scope. Tests require fresh validation
after a write even within the same outer transaction, preserve the caller's
transaction ownership, and check that the read-only path changes no rows.

The explicit-snapshot change reduced diagnostic work but did not pass runtime
pressure: Linux reached 162 retained opens before the unchanged 900-second
limit. The next change shares only the adjacent eager isolation check and
source inspection inside one transaction. It keeps eager rejection for empty
foreign stores, every inspection after a mutation, and all reader pin checks.
Deterministic diagnostics require heads to use two scopes, guard acquisition
four, and guard release three; a pin corrupted after the initial read must
still fail fresh validation and roll back exactly. Runtime acceptance remains
pending.
