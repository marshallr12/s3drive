# Populated ordinary-store retry migration

Accepted ordinary-store migration prerequisite, 2026-10-23. **Migration itself
does not reclaim history; native migration remains unsupported.** Builds on the accepted [retry/checkpoint contract](retry-checkpoint-safety.md).

## Explicit migration boundary

`migrate_retry_safety` is a distinct, explicitly requested state-library action.
The original empty-downstream `enable_retry_safety` keeps its stricter refusal.
Neither SDK/native profiles nor startup invoke migration. This prerequisite was
additive behavior at schema49; the separately accepted collector uses schema50.

The migration acquires the single owner's immediate SQLite transaction, validates
the full source/preparation/root/physical forest, and accepts only ordinary plans
with exact published receipts and source acknowledgments. Every populated target
must currently be a validated clean rebase. All physical generations must belong
to the verified terminal preparation trees; an unrelated staged generation is a
blocker even if it looks harmless. Incomplete preparations, claimed/unknown
attempts, dirty heads and native/owned composition remain refused unchanged.

Only the existing permanent source-scope and downstream-retry mode rows are
inserted. Original source/request BLOBs, plan IDs, generations, publication IDs,
physical operations, receipts and pins are not translated, rewritten or deleted.
A legacy random ID cannot be converted into a scoped token without changing its
identity. Exact known legacy retries therefore keep using their saved identities;
unknown raw requests are fenced. New requests require tokens for the current
source epoch. Reader handles and clones retain their original view and bytes.

The transaction revalidates all original source/plan state and terminal evidence
before commit. A crash before commit leaves both modes unchanged; a crash after
commit preserves both. Retrying migration is idempotent. Cancellation cannot
leave a partial mode transition. No network action, fresh send permission,
provider deletion or credential acquisition follows from migration.

## Acceptance and evidence

Required dedicated fixtures cover all-original-SQL-value preservation, exact
legacy preparation/root replay, conflicting identities, unknown raw admission,
restart, fresh scoped editing, active cloned readers, incomplete and claimed
physical outcomes, unrelated staging, missing/corrupt terminal evidence, and
process death at five migration boundaries. Native-owner refusal is separately
retained. Full Linux/Windows workspace/native and source/artifact review remain
mandatory. Local overlayfs/tmpfs cannot supply durability acceptance and its guard
must remain unchanged. Final candidate workspace Clippy and build pass, and111 local documentation
links pass. The full local gate stops at four existing storage-dependent authority
cases. The focused suite stops at the unchanged persistent-storage guard (16
failures, two child-entry controls pass). Neither is durability acceptance.
Standard Linux/Windows verification and independent exact-source review passed
in the [accepted combined candidate](transactional-local-retention.md#accepted-exact-candidate-evidence).
The earlier local failures remain non-acceptance evidence, not relaxed guards.

## Separate cleanup boundary

Migration alone retains all legacy identity rows. It does not make legacy plans
eligible for the scoped observational checkpoint, free staging or database pages,
or solve retained-history quota exhaustion by itself. After a new scoped clean
publication, the separately accepted [transactional collector](transactional-local-retention.md)
can retire eligible legacy history with independently validated anchors, permanently
closed retry identities and transactional revalidation. [Physical snapshot tokens](physical-snapshot-retention.md)
protect independent staging readers; source-view pins alone never authorize deletion.

Native original-open/epoch-history and namespace/Close retry contracts remain an
unimplemented boundary. Populated native migration and automatic mounted cleanup
remain refused; ordinary acceptance must not be generalized to those stores.
