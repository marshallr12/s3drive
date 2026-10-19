# Repeated edited-file publications

Design and implementation contract, 2026-10-19. **Implemented in an isolated
worktree; full local gate passed, own hosted acceptance pending.**
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


## Next: clean epochs and bounded local retention (proposal)

A root publication is historical proof, so "clean" must name its exact immutable
view rather than whichever head happens to exist when a reply arrives. Keep a
per-file active base epoch separate from retained immutable views. Only switch
that epoch when the published view is still the current head in the same SQLite
transaction. Derive the new base descriptor from the validated publication
receipt, retaining the source/store/route bindings; do not adopt an arbitrary
current authority node. The new initial view reads from the published manifest.
Old view IDs, operation requests and their hashes remain unchanged in their old
epoch. A stale write parent cannot change the new head.

If a newer edit exists, record acknowledgment of the older publication without
switching the active epoch or clearing that edit. W5a permits that newer view to
publish against the acknowledged ancestor. A later quiescent acknowledgment can
then switch the base. The native engine must serialize write-head selection and
the epoch switch, while existing immutable reads keep their pinned view. An
external content change after our publication still produces a conflict on the
next new root request; local acknowledgment never proves global freshness.

Epoch switching alone does not reclaim bytes. Retention roots include the active
head, outstanding read/open pins, local retry records, preparation witnesses and
objects, saved/unknown upload requests, saved/unknown metadata requests and their
predecessor chains. Resolve unknown outcomes before retiring their evidence.
A process death is not evidence that a saved remote request never committed.
Deletion must be transactional across every affected local table and validated
before commit. Do not delete patch BLOBs underneath immutable records that still
refer to them, or remove generic staging chunks underneath retained receipts.
Disk-full and forced process death must yield either the complete old state or
the complete new state. Local retirement never authorizes S3 DELETE.

There is also a retry-identity constraint: the current random operation IDs have
exact replay semantics. Removing their rows and accepting the same ID for a new
request would break that promise. Bounded continuous operation therefore needs
an explicit retirement protocol, not just an LRU of old operations. One proposed
successor API uses a durable producer epoch and monotonic ordinal with a retained
retirement floor. Exact unresolved operations retain their results; explicitly
retired operations return a distinct retired result and can never admit new work.
Once that protocol is selected, old raw-ID mutation admission must be gated rather
than silently treating forgotten IDs as new. Existing immutable IDs are not
rewritten. This is a design proposal, not an implemented promise or an implicit
change to the current public API.

Proposed independently verifiable slices:

| Slice | Done criterion |
| --- | --- |
| W5b1 exact acknowledgment and active epochs | Atomic current-head comparison and clean base switch; historical acknowledgments preserve newer heads; old views/read pins stay valid; new edits prepare from the new published root |
| W5b2 explicit pins and retry retirement | Recovery-safe pin/session ownership and explicit producer retirement; missing replies never lose a live pin or admit an old operation again; unsupported legacy callers fail explicitly |
| W5b3 bounded local reclamation | Retire only proven unreachable views/preparations/physical staging and journals; repeated edits and publication run beyond configured history/dirty quotas with bounded retained bytes; pinned or uncertain work yields explicit quota refusal without losing data |

Acceptance must include a continuously edited large file, bounded online write
windows, offline quota exhaustion with all dirty data preserved, held old reads,
newer-head races, multiple processes after owner death, actual SQLite-full and
kill boundaries, exact retry/retirement and both native adapters. Raising the
256MiB hard dirty-data bound is not a replacement for those properties. Remote
object retention/garbage collection, explicit legacy/empty origins and W6 native
write lifecycle remain separate work.


A concrete W5b1 implementation candidate keeps the existing single immutable
parent chain. Introduce a separately versioned rebase transition whose parent is
exactly the published head and whose new binding describes that publication.
Its ranges start as base coverage of the published logical length. Ordinary
write/resize transitions still require equal bindings. Only the rebase transition
may change them, and the file table's active binding must equal the current head's
binding. Historical records validate against their own epoch and retain their
original serialized bytes and IDs. This avoids copying dirty spans into a new
binding or pretending that old base offsets name the new object.

The source validator can validate a rebase's publication reference through a
bounded metadata-only helper. That helper must never call source validation;
otherwise preparation -> source -> publication -> preparation would recurse.
Normal root validation still verifies preparation/physical evidence separately.
Admission reads the actual retained root record and compares its exact view,
receipt, source binding and new descriptor inside the final transaction. A
caller-supplied descriptor or receipt alone is insufficient.

The existing exact-binding `head` API should keep refusing stale callers. A new
scoped current-head facade can select the active epoch using stable volume/node
and unchanged store/source/configuration scope; it grants no read permission.
Readers continue to authorize their exact immutable source histories. Native
write callbacks must refresh through that facade under the same mutation/epoch
serialization boundary. This candidate does not yet remove old chain prefixes;
that remains W5b2/W5b3 work and cannot be hidden behind successful rebase tests.


The first complete local gate passed formatting/Clippy but stopped at the shared
CLI test subprocess: adding the large new scenario increased the dispatcher poll
frame enough to overflow the default stack in an existing source-flow test. Its
21 other publication cases passed. The new scenario now constructs its future
inside a separate non-inlined factory; stack limits and assertions are unchanged.
The failing existing case then passed on its default stack (5.69s). A fresh full
gate must cover this fix and all319 final inputs. Exact failed source and logs are
retained; the first gate is not accepted as green.


## Complete local gate (2026-10-19)

Final checkpoint `71242f5a8184d9c126a00a82a3a2c9a99d91409f`, schema34/all319
unchanged inputs, passed the full local gate at12:36:50Z. It passed434 state
cases (one ignored),37core,22reader (one native ignored),62S3 and22publication
cases, schema fixtures, formatting, all-target Clippy, workspace build,79Markdown
links and75book chapters. All five new lineage/old-schema recovery cases passed,
including saved schema33 Publish recovery with corrupt source and no migration.
The root-owner suite exercised28 actual killed-process boundaries. The successful
full log SHA-256 is
`b14a48b1e9ebd3bd583fed0add8d2ff57c3653f24d50d20cae9f3264b023eb3b`.

The [indexed local archive](evidence/mixed-lineage-local-20261019.tar.gz) contains663
verified members,1690240bytes, including all319 final inputs and all319 exact
failed gate1 inputs plus failure/repair logs and copied failed fixtures. SHA-256
`33d07eb2df7f6742dd7bc56a70d86cbb182400de6feb19e3b24e0f3256543359`.
This accepts local library behavior, not a native chunk filesystem. An own hosted
Linux/Windows/book/native/source cohort is required before runtime promotion.
W5b clean epochs/retention, explicit origins and W6 remain unimplemented gates.
