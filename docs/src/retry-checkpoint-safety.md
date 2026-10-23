# Retry fencing and checkpoint state prerequisite

Candidate contract,2026-10-23. **Not accepted; no history reclamation.**
Based on accepted main86d1e0e. This is a bounded state-library prerequisite, not a
claim that populated/native owners can yet retire history. See the broader
[retention plan](mixed-retention-plan.md).

## Activation and retry identity

Schema49 adds one empty STRICT singleton mode table. Existing source/preparation/
root/staging bytes are not rewritten. The default remains disabled. Explicit
activation atomically enables the existing source epoch fence and the downstream
retry fence. Activation refuses existing downstream preparation/staging/root or
native-composition history before changing either mode: it must not strand an
old operation by changing its identity contract. Source-only history may remain.
No SDK, native profile or configuration activates this mode automatically.

A preparation token contains the exact source-binding epoch and caller nonce.
Domain-separated hashes derive its operation, generation and root-publication
identity. The caller persists the token before an uncertain request and supplies
matching request identities; mismatches are rejected, never silently rewritten.
The retained input records the token. Exact known retry precedes current-epoch
admission and returns the same plan without replacing the head. An unknown token
from a closed epoch, a raw fresh preparation and a changed generation/root ID
cannot become a new operation. Reusing a nonce under a deliberately new epoch is
explicitly a different operation.

Physical staging IDs retain the existing exact plan/root/position/object hash.
In opt-in mode a fresh generic staging operation must match a retained scoped
current-epoch transfer. An unknown remote intent similarly requires that exact
current-epoch generation. Known mapped generations and remote outcomes remain
available for exact recovery; a claimed/unknown outcome is never a new send
permit. Refusal precedes source-binding registration as well as generation
admission. None of this creates credentials or provider authorization.

## Checkpoints are observations, not cut-edge permissions

A checkpoint is a private, non-deserializable capability obtained by reading one
SQLite snapshot. It independently reloads the complete source graph, original
requests, exact acknowledgment/published root, semantic coverage, preparation
objects and verified physical dependency receipts. The clean anchor must be the
current clean-rebase head, bind the original published view and a different new
epoch, and match the ordinary replay validator. Missing ancestors remain errors;
a copied hash or supplied anchor is not accepted as a substitute.

The proof records terminal plans, blocked plans and protective reader views for
the closed epoch. Only scoped, acknowledged, fully verified ordinary plans meet
its narrow terminal condition. Unfinished plans and unknown outcomes stay blocked.
Native/owned provenance remains an independent protection, not something a
source-only proof may retire. Every same-target read pin remains protective,
including a reader of the anchor itself. A newer dirty head invalidates the
checkpoint rather than being overwritten or treated as published.

Verification reloads all retained evidence and compares the capability. There is
no permanent SQLite read transaction, new tombstone per operation, history
removal, actual cut edge, staging deletion, S3 deletion or disk compaction. The
proof's unblocked result supplies necessary local conditions only. A later GC
implementation must define and validate severed edges, durable retirement and
recovery together; this candidate does not grant permission to remove anything.
Releasing a read pin is not history GC. Retained byte growth remains unbounded
within the existing finite admission quotas, and exhaustion still preserves data.

## Verification requirements and current evidence

The candidate adds adversarial state tests for exact replay/restart, stale epoch
and changed identities, raw physical/root admission, clean versus dirty heads,
reader pin invalidation/release, unknown claimed transfers, damaged/missing
anchors, killed owners and actual SQLITE_FULL rollback. Activation refusal must
leave the original owner unchanged and allow its default flow to finish.

Local final-source workspace compilation/build and all-target Clippy passed.
An initial gate caught Clippy style defects; those were corrected, not suppressed. The first focused durable run failed at the
existing tmpfs guard (eight cases; child entry alone passed). This is not passing
durability evidence. The cloud filesystem guard remains unchanged. Local documentation links pass (110 Markdown files), but mdbook is not installed
on this workstation; the hosted book job remains mandatory. The full local gate
also fails at existing storage-dependent authority cases. Final-source standard
Linux/Windows Actions and full evidence review are still required.

Actual48/49 compatibility must use separately built accepted48 and candidate49
executables, prove old48 refuses fresh/upgraded49 without changing database or
sidecar bytes, and preserve the complete original48 schema/data dump apart from
the one verified empty additive table. Existing genuine46/47 and47/48 tests remain
separate. The structural catalog allowlist accounts for exactly the new table;
source/plan/blob/admission quotas, corruption gates and deadlines do not change.
Complete native regressions and raw typed artifact/source review remain mandatory.

## Still open

- Safe opt-in upgrade of populated downstream and native owners.
- Native/owned original-open and epoch-history retirement proof integration.
- Durable permitted cut edges and transactional history severance.
- Actual local history reclamation and bounded database/WAL/staging measurements.
- Subsequent native/provider/deployment acceptance; no default activation here.
