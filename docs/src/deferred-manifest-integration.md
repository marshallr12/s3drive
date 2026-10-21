# Deferred canonical manifest integration

Updated2026-10-21. The initial authority transaction, creation client and same-owner open-journal
provenance bridge are independently accepted. The prospective closing budget,
sparse owned-unlinked publisher and native lifecycle integration remain future
work. Keep delete-on-close/save-replace flags unsupported in the chunk profile.

| Boundary | Current evidence |
| --- | --- |
| Canonical node, initial payload/history and original fence/event | [Authority/state contract](atomic-deferred-manifest-create.md),3e670915/schema42, full local and own Linux/Windows/book/native regression acceptance |
| Original client owner, physical root proof, exact request/outcome and restricted recovery | [Creation client](deferred-creation-client.md),9381456/schema43, full local and own Linux/Windows/book/native regression acceptance |
| Verified creator to same-owner namespace/open journal | [Verified import](creator-provenance-import.md),3c38ad1/schema44, full local and own Linux/Windows/book/native regression acceptance |
| Sparse publication after unlink or owner-bound replacement | Planned; exact fence/current event and unresolved-effect barriers required |
| Native v8 deferred/open-unlinked/delete/save-replace callbacks | Unsupported until their library and FUSE/WinFsp recovery gates pass |

The creation client and namespace/open journal use distinct durable records.
An ordinary namespace Input or historical receipt cannot stand in for the
creation journal's independently checked staging/upload witness. The importer
reads the exact successful owner-bearing creation from the same SQLite worker,
retains its original request/receipt and preserves source/store/owner controls.
Generic namespace submission cannot bypass it. Local activation checks retained
deferred history; native code still needs current remote ownership checks.
Historical replay cannot rewind known cancellation or adopt a successor epoch.

Owned-unlinked publication must save the original owner, fence and current
removal event in the exact reservation request. Final removal must stay blocked
while root/physical outcomes or local source acknowledgment remain unresolved.
Preserve existing handles, exact old reads and newer dirty heads through retry
and recovery. Increasing the old8MiB whole-generation bound cannot substitute
for the sparse chunk engine. Retention and complete retry-forest closure remain
separate prerequisites for safe GC and a usable long-running drive.

The import/admission crash, SQLITE_FULL, provenance and schema gates are recorded
in its accepted contract. Subsequent evidence must cover prospective closing
admission, sparse owned-unlinked publication, lost replies without resend, and native
FUSE/WinFsp delete-on-close/save-replace with large sparse Unicode files. Keep
real-provider conformance, physical MX/Windows10/11 sign-in/reboot/SSH/tmux,
independent-host partitions/restore, packaging and deployment gates separate.

The [native bridge architecture](native-chunk-integration.md) shows the wider
system; the current [creation diagram](images/atomic-deferred-canonical.svg)
separates the client's local SQLite/S3 verification from the credential-free
Rust authority's metadata SQLite transaction. Optional Turso and both hosting
choices remain open. This plan provisions no service and grants no provider
mutation or startup-installation permission.
