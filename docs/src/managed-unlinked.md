# Retained unlinked files and namespace acknowledgment

**Library implementation, 2026-10-17 (P6m3b3b2).** This connects the
[guarded namespace transactions](managed-namespace-plan.md#guarded-authority-entries-p6m3b3b1-2026-10-17)
to local logical state and immutable S3 publication. It does not enable native
rename/unlink/rmdir or WinFsp delete cleanup; those callbacks still need separate
acceptance. Live Wasabi, remote authority/Turso and workstation startup settings
are unchanged.

![Exact namespace acknowledgment preserves dirty file identity; detached publication changes a tombstone's payload without restoring its name.](images/managed-unlinked.png)

## Atomic local observation

`Publisher::namespace_change_input` freezes compact preconditions for the source
and displaced target. Each includes stable identity, revision, parent, exact
name, kind, authoritative placement and a digest of the complete payload
reference. This preserves the existing 8 KiB namespace input bound without
duplicating long provider proof strings. Existing creation records omit the new
empty field and retain their serialized hashes. The controlling SDK binding is
still frozen; local file bindings and their credential fingerprints never change
when observing a namespace result.

`Publisher::change_namespace` persists the exact input before HTTP. Ignored or
deleted intent insertions cannot admit an RPC. A matching
successful receipt then commits **namespace outcome plus all affected local file
observations in one SQLite transaction**. Original local targets, retained edits,
heads and settlements stay intact. A displaced/removed file's observed base
becomes the retained tombstone. Readback and affected-row checks detect ignored
writes; a full disk or failed observation rolls back the whole acknowledgment.
An older result cannot rewind a newer verified observation or settlement. Semantic
rejections remain recorded and return an SDK error on both first application and
cached replay; an outer successful result never disguises a conflict.

A dirty source must be published before a move can expose it at the destination.
A dirty displaced or unlinked file can remain locally dirty when its base payload
matches the guarded input. Different retained content is a conflict, never an
implicit rebase. An unresolved prepared publication must be reconciled before a
new namespace effect. While a namespace effect is pending, new publication
preparation is blocked; local byte staging remains possible. Effects on the same
node recover before another namespace effect is admitted.

## Publication without resurrection

The additive `detached-content-cas-v1` authority capability accepts
`ReserveDetachedContent`. A default-omitted `detached` content-guard flag keeps
legacy live-file hashes intact. Detached reserve/publication require a tombstone
and the exact prior complete payload reference. A live node is refused. Live-file
reservations retain their previous behavior and cannot publish to a tombstone.

The actual SDK still writes a distinct immutable, create-only generation and
verifies its complete bytes. Metadata publication updates the old node's payload
while leaving `deleted=true`, name, parent and placement intact. A new file at the
same name has a different node ID and physical keys; publication cannot replace
it. Competing detached content publications conflict. No old blob or local
generation is reclaimed.

Logical preparation requires the detached guard to agree with the observed base.
Physical/metadata/logical receipt checks now accept a tombstone only for that
explicit guard. Settlement still preserves a newer head. Immutable original
bindings remain live snapshots for identity hashing; only a verified namespace
outcome installs a tombstone observation. New `Reader`/`Publisher::read` opens
continue to refuse deleted nodes. This library does not expose tombstones as paths.

## Verification and remaining native work

State tests cover atomic source/target observations, ignored receipt/file writes,
restart, retained dirty bytes/identity, full-disk rollback and exact retry,
publication/namespace ordering, input tampering, legacy serialization, detached
conflicts and name reuse.

The SDK/HTTP/SQLite test uses a real forwarding service to lose a committed unlink
reply, then kills the client. Restart replays the original intent without another
namespace effect. S3 then accepts binary bytes while its reply and observation
are lost; the client is killed again with a newer dirty head. Recovery verifies
and settles the original generation **without another PUT**, then publishes the
newer generation. Exactly two immutable blobs remain; the new file reusing the
name stays untouched. These are owned loopback fixtures, not live provider proof.

Native integration must still quiesce mutations/publication, capture verified
source/target states, recover pending namespace intents before serving paths,
and retain open-unlinked views. Windows needs a reversible delete-intent and
cleanup design, including failure recovery despite cleanup having no returned
error. Full temporary-file/write/fsync/replace application saves remain required.
