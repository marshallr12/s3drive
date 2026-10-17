# Deferred owner rename and move

**Implementation contract, 2026-10-17.** Build on atomic deferred creation and
[measured Windows rename behavior](windows-deferred-reference.md). Native
Windows delete callbacks remain unsupported during this authority/client phase.

`MoveDeferredEntry` retains the original fence and frozen source/store binding.
It requires a deferred source, exact source/destination/parent guards and the
previous ownership event ID. Source identity change, optional unreserved target
tombstone, deferred event pointer, immutable event and exact result commit in one
SQLite transaction. Ordinary moves still require clear reservations; a pending
source or reserved replacement destination is refused. Deferred destination
replacement needs further reference/native evidence before lifecycle integration.

Prove the identity chain from the original create/prepare through each committed
move; never accept arbitrary changes to the original name/parent. Replay an old
create, prepare or move without restoring an earlier identity or epoch. Preserve
payload/store and fenced publication before and after moves. Directory children
follow stable node IDs. Support Unicode/case-only names, cross-parent moves,
nonempty source directories, guarded ordinary replacement and explicit conflicts.

The client retains its original ownership input and advances only the latest
receipt/event after exact acknowledgment. An unknown move blocks affected nodes,
publication and cancellation until the same request is recovered. Do not extend
the identity-preserving unknown-transition cancellation exception to moves.
Moves consume ordinary capacity and preserve both promotion/final-close slots.

Acceptance includes corruption, ignored SQL/full-disk rollback, quota, stale or
foreign fences, cycles, parent/target races, late receipts and killed owners.
Use real TCP and SDK/SQLite recovery with Unicode, binary/empty content, retained
replacement bytes and no duplicate PUTs. Require full local gate and terminal
exact-tip hosted Linux/Windows/book native/reference/startup CI before private
publication. No real provider mutation or workstation mount bypass is included.

## Local verification (2026-10-17)

The full repository gate passed with 197 Linux state tests, nine authority HTTP
tests and thirteen managed SDK cases, plus formatting, strict Clippy, build,
documentation links and mdBook build/chapter tests. Schema 16 and configuration
6 remain unchanged; old request/result serialization and hashes are preserved.
The new move's fence and prior-event ID use indirection in memory to keep request
enums bounded, without changing their serialized fields.

Four SDK process stages recover lost file/directory moves and ordinary target
replacement using the same IDs and fences. They retain a child and a previously
opened target snapshot, publish binary/empty content with exactly six PUTs, keep
all six immutable objects, and reconcile lost metadata publication without
another PUT. Local bytes staged during an unknown move remain dirty across
process death: remote publication waits for exact move recovery, then uses the
retained generation. A source already dirty before move admission must first
be published; refusing that move preserves its data. Late results cannot rewind
the moved identity or a newer fence, and replayed empty-file cleanup cannot
delete a newly reused name.

State tests cover ignored effects, actual SQLite-full rollback at authority and
client, killed authority ownership, both closing quotas, corrupt/cyclic event
references, Unicode/case-only and cross-parent moves, nonempty source
directories, owner publication between moves and mode transitions. TCP clients
race normalized destination names and replay the exact result after restart.
These are loopback fixture results, not new Wasabi conformance evidence.

Require terminal all-three hosted CI for the exact commit before publication,
including existing real mounts/startup and fourteen NTFS/four upstream/nine
malformed-fixture checks. Next complete deferred replacement reference proof and
the durable native Windows per-open/node-wide lifecycle; native deletion remains
unsupported until that acceptance gate passes.
