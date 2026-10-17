# Durable managed local edits

**P6m3b1 implementation, 2026-10-17.** This joins local logical-file heads to
[immutable S3 publication](managed-publication.md). It is a library/storage
checkpoint underlying [version-4 existing-file editing](managed-write-plan.md). Version-3 native mounts remain
[read-only](managed-native.md); no real bucket, credential, write policy or
startup registration is changed by this phase.

![Logical edits remain local until an exact generation is linked to a physical publication; settlement preserves a newer head.](images/managed-edits.png)

## Identity, admission and local sealing

The client-local `logical_files` row is indexed by volume ID and node ID. Its
frozen binding includes the original authoritative node, selected store, and
actual credential/configuration binding from the SDK executor. The local journal
target is a separate domain-derived identity; it is never an authorized S3
object target. Existing direct heads and physical blob generations retain their
original meaning. A rename or a new generated object key does not identify a
new logical file.

`Publisher::bind_edit_file` fetches authoritative metadata and binds that snapshot.
It does not download base bytes or prove an S3 read. An exact original binding,
or a live node matching this client's last verified publication receipt, reuses
the original local target. New credentials, changed store routing, or an external
revision cannot silently replace the stored base. The version-4 coordinator authorizes/hydrates partial edits and provides explicit verified clean-base observation; this lower-level method still accepts only exact bindings. Dirty or credential rebinding remains explicit.

`Publisher::stage_local` durably records an edit ID, expected local parent,
size and SHA-256, then stages immutable chunks and seals the local generation.
No metadata reservation or S3 request occurs in this operation. Multiple local
edits can be coalesced before selecting one for remote flush. Replaying the exact
intent recovers its builder; changing its parent/size/hash under the same ID fails.
Two builders can race from one parent, but only one can advance the local head.
The other builder and its bytes remain available for inspection.

The full byte hash is checked inside the sealing transaction, together with the
journal's manifest/retention/head changes. A mismatch rolls back sealing and head
advancement. All operations share the existing bounded SQLite owner and directory
lock. Schema 10 originally adds `logical_files`, `logical_edits` and the exclusive publication
operation registry. Migration preserves schema-9 data and refuses conflicting
unknown table shapes without advancing the version. An authority database cannot
host local edits, and a database containing only a logical binding cannot become
an authority.

## Publication and settlement

1. Select one sealed logical generation. `Publisher::stage_edit` persists the
   complete reservation request, publication ID and frozen SDK target binding in
   its logical record **before** physical staging or network effects. The request
   must match its bytes, store and current locally verified base revision.
2. Reserve and publication operation IDs are each exclusively associated with
   that logical edit. They cannot be shared with another local generation.
   Retries keep the exact original request; neither a new global sequence guard
   nor an external file revision may silently replace it.
3. Stage a distinct create-only physical root and use the existing single-attempt
   SDK/authority protocol. A physical PUT, its verification and metadata CAS have
   separate recovery records. Lost outcomes retain their IDs and bytes.
4. `Publisher::flush_edit` requires the verified physical flush and matching
   metadata publication receipt. It then commits the logical edit receipt and
   file settlement pointer in **one local transaction**. A missing physical or
   metadata receipt cannot settle the file.
5. Settlement of generation A leaves a newer head B intact. The file's next
   reservation may use A's confirmed authority revision because that revision
   came from this client's bound receipt. A later explicit flush of B does not
   adopt an unverified external edit. Replaying A cannot rewind B or the file's
   latest settled base.

Logical pending enumeration uses ancestry coverage and validates retained records,
including covered rows. Schema 11 adds a separately hashed verified-base observation without changing original identities or legacy hashes. It does not erase physical attempts that may still need
reconciliation. Generic staged generations remain retained; managed callers must
use the logical pending view for logical dirtiness. Corrupted or missing settlement
markers fail explicitly rather than turning into a clean cache miss.

A crash after metadata commit but before local logical settlement is recoverable
from the same physical/metadata receipt. `flush_edit` can finish that local
settlement without another PUT. Disk full at this final transaction returns an
error and retains the pending logical edit, even though S3 and metadata already
committed. Factory/policy freshness still governs admission of new physical work;
this is not a new permission to upload or silently rebind credentials.

## Limits and acceptance

Complete generations are at most 8 MiB. Four local staging/physical staging
operations across publisher clones bound payload admission. Logical records are
limited to 4,096 files and 4,096 edits, with at most two exclusive publication
operation IDs per edit. Existing staging byte/generation quotas also apply.
There is no payload reclamation. Integrity checks may reread whole local files;
this is a bounded initial implementation, not a large-file performance result.

The phase acceptance commands are:

```sh
cargo test -p s3drive-state --locked
cargo test -p s3drive-cli --test managed_publication --locked
bash scripts/gate.sh
```

Storage tests kill an owner at nine exact boundaries: intent commit, builder
allocation/link, before/after seal commit, before/after publication-input commit,
and before/after logical settlement commit. Other cases cover full-disk rollback,
ignored SQL updates, stale parents, credential/base mismatch, quotas, exclusive
publication IDs, corrupted retained rows, schema migration and role isolation.

The SDK/TCP fixture kills a client after S3 and authority commit while a newer
local head exists. A local predecessor is coalesced without upload. Reopening settles
the original generation without another
PUT, then publishes the newer bytes. A subsequent external metadata rename makes
a prepared revision stale; its losing blob and local bytes stay retained. An empty
local generation is also retained without upload. Exactly three immutable PUTs
cover two successful publications and one losing publication. These are owned
loopback fixtures, not live managed Wasabi acceptance.

Native FUSE/WinFsp and startup regression gates remain separate and necessary
because the SQLite schema and shared publisher changed. They do not certify
managed writable callbacks. P6m3b2 must now connect local edit views, patch/truncate,
remote fsync and recovery to those callbacks. P6m3b3 adds portable names,
create/rename/replace/unlink and real application save workflows. Authority
supervision, remote/Turso deployment and workstation reboot acceptance remain
separate requirements.
