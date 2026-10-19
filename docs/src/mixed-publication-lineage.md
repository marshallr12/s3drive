# Repeated edited-file publications

Design and implementation contract, 2026-10-19. **Implemented in an isolated
worktree; full local and own hosted acceptance pending.**
The user selected immutable chunks plus manifests. Small edits should reuse
unchanged chunks. Ordinary drive names remain metadata in the shared authority.

W4b2 fixes a sparse view to its original authenticated manifest. Its root CAS
requires that original payload reference. After a successful publication, a
second edit still composes against that frozen source; the authority now points
to the first published root. Treating every such change as foreign would prevent
a second flush. Accepting any current payload would overwrite unseen edits.

W5a allows only an exact retained local predecessor. A new root request records
its predecessor plan ID. That plan must have a validated publication receipt,
the same full sparse binding/store/original root, and a view that is a strict
ancestor of the new view. The current authority payload must exactly equal that
receipt's published payload, including reservation, upload and revision identity.
A rename can advance the node revision without changing this content identity.
The predecessor receipt sequence cannot exceed the saved request sequence or
current node revision. These strict sequence edges also rule out metadata cycles.

The journal recomputes this admission in the transaction that saves the exact
reserve. The managed layer checks it before preparing new remote work. Neither
layer accepts a caller's assertion that a payload is ours. New requests still
need fresh source authorization, feature/store/route checks and create policy.
Known saved requests recover first and keep their exact operation IDs and CAS;
no failed or unknown operation is silently retried against a different base.

Original schema33 records retain format1 and omit the new optional field when
serialized. New lineage records use format2. Schema34 refuses old binaries
before they can misinterpret this new intent format; migration does not rewrite
existing views, source bytes, operation rows, checks, requests, receipts or hashes.
Restricted recovery validates the complete retained predecessor graph using
metadata only; normal opening also checks the actual local view ancestry.
It also recovers existing format1 requests in schema33 without migration, even
when normal source opening would fail. Schema33 metadata containing a new
predecessor edge is refused. It still cannot admit requests, repair source bytes
or invent a publication.

This slice preserves all local dirty bytes and immutable view IDs, including a
newer head created while an older view is publishing. It is not a clean receipt
and does not reclaim capacity. A subsequent W5b phase must establish clean base
epochs, define open-view/retry retention and atomically retire only unreferenced
local payloads after every associated unknown operation is resolved. It must
support repeated windows beyond existing dirty/history quotas without increasing
those bounds or silently discarding offline edits. S3 object garbage collection
is separate and must account for shared snapshots and remote clients.

Required evidence: consecutive real SDK/authority publications from one frozen
source, full final bytes, newer-head preservation, restart and exact replay,
unknown/conflicted/missing/foreign/nonancestor refusal, source-corrupt metadata
recovery, literal old-record migration preservation and SQL rollback. Full local
and own Linux/Windows/native regression gates precede promotion. Native chunk
writes, clean flushes, new-file origins, Wasabi and physical reboot/session checks
remain separate acceptance requirements.


The initial16 state cases and SDK scenario passed. The state suite includes28
actual owner-kill boundaries; the SDK scenario uses12 owned loopback PUTs and
performs three root publications across reopen, exact lost-reply recovery and
foreign-writer refusal while preserving a newer head. The full gate must also
cover the newly added schema33 pending-publication recovery case. Retained test
harness failures and scope are recorded in [dated evidence](evidence/mixed-lineage-20261019.json).
