# Creator provenance import proposal

Source review2026-10-21 against deferred client9381456/schema43. This is a
proposal; no import or native lifecycle flag is implemented by this chapter.
The [creation client](deferred-creation-client.md) persists the original owner,
canonical root proof and combined authority receipt. Its receipt is historical,
not an active open admission.

## Observed integration boundary

`namespace_client::opens::OpenInput.admission` currently contains a namespace
`Input`. Activation loads the exact acknowledged namespace journal row and
reduces its retained removal history. The creation client's separate journal
cannot supply that row today. `namespace_client::Input::validate`, removal-history
reduction, open targeting/activation and session recovery currently recognize
metadata-empty deferred creation. Merely adding a mutation to their matches
would allow a generic namespace submission path to claim a canonical creation
without checking the independent local upload/staging evidence.

The chunk mount already binds a stable client owner and exact volume/store/
source/fingerprint controls to one SQLite worker. That is the proposed import
boundary. Import must read the creation record from that same database; a caller
must not supply a receipt, inferred pathname, replacement owner or another
storage connection as proof.

## Proposed independently testable phase

1. Expose a dedicated chunk-capability import operation, taking the original
   creation operation ID. Check the chunk key and retained identity, exact source
   controls, acknowledged format2 creation request/outcome and the independent
   staging/upload witness. Refuse ordinary creation, an unknown/rejected creation,
   damaged proof or a foreign owner/store/source/volume.
2. Atomically retain the exact original request and combined successful receipt
   as a complete namespace journal record. Never insert a pending surrogate or
   synthesize a metadata-empty create. Preserve existing quotas and validate the
   inserted row through the normal loader before committing. Exact repeat import
   returns the same record; a changed operation binding remains a conflict.
3. Keep fresh generic namespace `begin` from admitting this canonical mutation.
   Validate imported records against their original creation journal on load as
   well as on import, so a self-consistent namespace row/hash rewrite cannot
   replace the independent provenance. Share canonical receipt-shape validation
   with the creation journal instead of duplicating its definition.
4. Extend removal-history and open admission/activation targeting to recognize
   the imported combined creation. Bind admission to the original fence and
   owner; activation still requires the current acknowledged deferred event.
   Historical import must never resurrect an owner after cancellation or adopt a
   successor epoch. Require import before a new creation-bound open is admitted.
5. Preserve old namespace/open JSON, hashes and rows. Evaluate a new compatibility
   marker without new tables; refuse the new imported shape under an older
   marker before migration/relabeling. Keep the 64-table snapshot bound.

An import may finish before open admission. Its complete durable namespace row
and original creation record must permit exact recovery after a crash between
those operations. No active handle may be inferred from that intermediate state.
The exact public method and marker remain proposals until implementation review.

## Required evidence and following work

Use public same-owner worker APIs for successful import and open activation,
restart, queue/deadline/cancellation behavior and exact deduplication. Kill owners
before/after import commit and admission; exercise real SQLITE_FULL rollback,
quota pressure, wrong source/key/owner/session, missing or corrupted independent
proof, retained old bytes and actual old-binary refusal. Verify that generic
namespace admission cannot bypass the importer and that original versus
successor owner histories stay separate. Run the full local and own hosted
Linux/Windows/book/native regression gates before promotion.

Sparse owned-unlinked publication still needs exact original fence/current event
binding in reservations, dirty-view preservation and unresolved-effect barriers
before final removal. Native delete-on-close/save-replace, cold metadata recovery,
retention/GC, physical sign-in/reboot/SSH/tmux and newly approved real Wasabi
acceptance remain later work. This proposal grants no provider mutation or
service/startup installation permission and selects no Turso hosting option.
