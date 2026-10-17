# Atomic owned-target replacement

Design date: 2026-10-17. **Implementation in progress; native enablement remains
gated.** The [measured replacement profiles](windows-replacement-reference.md)
require safe displacement of a destination which still has open file objects.

Extend `MoveEntry` and `MoveDeferredEntry` with optional `destination_removal`:
the target's exact owner/intent fence, latest immutable event ID and expected
deferred/pending mode. A claim requires a distinct guarded destination. A deferred
source must have the same owner. No claim preserves the original clear-target
contract and original serialized request/hash bytes. This is a schema22 semantic
extension; existing schema21 rows migrate without rewriting their proof bytes.

One authority transaction validates both identities, parents, modes and event
chains; tombstones the displaced node; closes its reservation; updates the source
name; and persists the exact two-node receipt. A deferred source retains its fence
and appends its identity-change event. The target appends a committed event to its
own chain, using the displaced-node side of the same receipt. Payloads, stores and
stable IDs never transfer between nodes. Empty-directory and cycle/type rules
remain enforced. No S3 object is deleted.

The client must retain the original ownership proof and storage/source binding
for every claimed reservation. Before RPC it freezes source and target entries,
the exact claim and operation ID. Unknown work blocks both nodes. Acknowledgment
atomically adopts the moved source and detached target, closes target ownership
and advances source ownership when applicable. Replay is historical bookkeeping
and must not close a later epoch or rebase a replacement. Ordinary replacement
cannot consume protected capacity reserved for final cleanup.

Existing target handles retain their own data and may publish detached late I/O.
Later cleanup/Close uses the target stable ID, never its former pathname. It must
not cancel or remove the source at that name. The native bridge must deliver and
persist actual callback facts; this metadata operation fabricates no callbacks.

Acceptance includes both target modes and ordinary/deferred sources; Unicode,
old-target late I/O and replacement survival; stale modes/events/revisions,
foreign ownership, dirty generations and provider-binding changes; SQL-full,
ignored writes and corruption; protected quotas, schema21 hash preservation;
lost replies, killed clients and exact SDK recovery without replayed PUTs. Keep
strict 21-profile and 14-NTFS reference checks plus existing OS mount/startup
gates. Native profile selection, POSIX early cleanup and full callback integration
remain subsequent required work.


## Local verification (2026-10-17)

The repository gate passed with 253 state tests, ten authority HTTP tests and
seventeen SDK cases, plus lint, builds and book checks. Twelve new state tests
exercise both target modes and source types, exact target transitions, failed
writes and actual SQLite-full, reserved closing quotas, corrupt closure proof,
retained dirty target data and schema21 hash preservation. A concurrent HTTP
race produces one replacement winner; authority restart replays exact success
and rejection without a second namespace effect.

Three SDK processes cover all four source/target combinations. They retain the
eight original immutable blobs and four detached-target late-write blobs, with
PUT counts 8/12/12, no DELETE and no replay. Lost rename and publication replies,
dirty target heads, source successor epochs and old receipt replay are checked
against actual SDK/HTTP/local SQLite behavior. This uses an owned loopback
provider fixture. It is not new Wasabi conformance or native callback acceptance.
Require exact-tip terminal hosted native/reference/startup gates and both typed
source-bound reference artifacts before private publication.
