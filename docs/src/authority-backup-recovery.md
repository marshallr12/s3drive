# Coordinator backups and recovery

Research/design date: **2026-10-17**, America/New_York. **Proposal, not an
implemented backup/restore or failover facility.** Startup acceptance remains
in progress; see [pinned restarts](authority-startup.md). No real database,
account, bucket or hosted service was changed for this research.

The authoritative SQLite database contains names, node identities, store
bindings, removal fences and exact operation outcomes. S3 holds immutable file
payloads, not a complete copy of this metadata history. Each machine's private
SQLite journal separately contains uncertain operations and dirty file bytes.
A coordinator snapshot therefore cannot replace client-journal preservation,
payload retention or a tested deployment recovery procedure.

## A valid snapshot is not proof of current history

An isolated Linux CLI experiment at source `7962058` created a consistent
sequence0 snapshot, then obtained an acknowledged sequence1 namespace mutation.
After stopping the original process, the experiment started the older snapshot
with the **same expected volume ID**. Startup accepted it at sequence0, and
lookup no longer found the acknowledged operation. Only one authority process
ran at a time. This is a measured boundary of the current identity pin, not
evidence of supported restoration or a Windows result.
[Recorded fixture result](evidence/authority-backup-boundary.json),
[dated Linux reproducer](evidence/authority-backup-boundary.py).

Our protocol analysis: an old copy can retain the correct volume/schema/root
while forgetting later outcomes. Sequence numbers and per-node guards are not
an external witness that the database has never rolled back. Reusing forgotten
operation IDs or accepting a new branch can invalidate clients' prior receipts.
Checksums, timestamps and the largest sequence visible in available backup
files cannot prove that no newer acknowledged history existed elsewhere.
Do not manually replace the live database with an older copy and call it a
normal pinned restart. The current CLI provides no restore-freshness guarantee.

## SQLite mechanisms

SQLite's Online Backup API produces a consistent database snapshot and supports
incremental copying. Changes through another source connection can restart the
copy, so continuous writes can prevent bounded completion. The checkpoint
sequence must be read from the **completed destination**, rather than inferred
from the time backup began. [SQLite backup overview](https://www.sqlite.org/backup.html).

The API uses distinct source/destination connections. Destination access must
remain exclusive while backup is active. A successful finish call alone does
not establish completion: a cancelled, incomplete backup can finish without a
permanent error. The implementation must observe `SQLITE_DONE` and check cleanup
results. Busy/locked steps require a bounded policy; I/O, memory and read-only
failures must fail the artifact. [SQLite backup API contract](https://www.sqlite.org/c3ref/backup_finish.html).

`VACUUM INTO` is another consistent-snapshot mechanism. It rewrites logical
content, uses a new or empty destination, and can leave an incomplete output if
interrupted. Current SQLite documentation describes output synchronization for
NORMAL/FULL synchronous mode. It is not a substitute for publishing a complete,
validated artifact. [SQLite VACUUM INTO](https://www.sqlite.org/lang_vacuum.html).

Do not copy a database file while transactions run or discard its recovery
journal. SQLite documents corruption risks from inconsistent backup/restore,
mismatched journals and raw file access that bypasses SQLite's locking rules.
[SQLite corruption guidance](https://www.sqlite.org/howtocorrupt.html).
Committed state can still reside in the WAL rather than the main file.
[SQLite WAL](https://www.sqlite.org/wal.html).

These sources were read2026-10-17. They establish SQLite copying behavior;
they do not establish S3Drive history continuity or deployment fencing.

## Proposed delivery contract

1. **Snapshot and inspect.** First implement an explicit maintenance snapshot
   under exclusive authority ownership, then bounded online snapshots through
   the owning service. Do not expose arbitrary host output paths to ordinary
   namespace clients. Keep database ownership while SQLite reads; do not make
   an independent raw-file copier bypass it. Inspect resolved rusqlite backup
   APIs/features before implementation; the workspace currently enables only
   its bundled SQLite feature.
2. **Publish a complete private artifact.** Create a new owner-private staging
   location outside all mounted trees. Never overwrite an existing backup.
   Include the complete authoritative database and an inventory with volume,
   naming policy, application/schema IDs, destination sequence, digest, creation
   time and tool/source version. Exclude tokens, TLS keys and S3 credentials.
   Validate SQLite integrity plus application invariants and exact receipts;
   close destination handles, synchronize data/manifest/directory as required
   by the platform, then publish without replacement. A crash, quota failure,
   uncertain publication or cancellation must not report success or erase the
   last valid artifact. Partial artifacts remain distinguishable and disposable.
3. **Restore only into quarantine initially.** Validate an artifact in a new
   private directory with no listener, client traffic, automatic supervisor
   start or overwrite of existing state. Preserve the old database and every
   client journal. Inspection reports integrity and checkpoint identity; it
   must not label those checks as proof of freshness or permission to serve.
4. **Prove continuity and fencing before promotion.** A later protocol must
   establish one writable authority and prevent an older history from reusing
   the current writer identity. Any external witness or recovery generation
   must itself survive database rollback. Its durability/acknowledgment order,
   unavailable-witness behavior and client authorization need a separate
   contract. A new generation fences old traffic but does not recover lost
   acknowledged metadata. If continuity cannot be proved, retain quarantine
   and require explicit data-loss/fork recovery; never rewrite frozen unknown
   requests, silently adopt a new volume or imply zero data loss.

No automatic restore/promotion or multi-writer failover should be enabled by
the first snapshot implementation. These are stages toward the full recovery
requirement, not a replacement for it. Keep SQLite-hosted and optional Turso
deployment options open. Turso backup/restore and leader behavior need their
own pinned implementation and failure acceptance; no cloud feature is assumed
to solve the application's rollback or uncertain-operation contract.

## Acceptance required before deployment claims

| Area | Required direct evidence |
| --- | --- |
| Snapshot contents | WAL-only commits, Unicode/binary/empty-file metadata, original store bindings and byte-identical exact operation inputs/outcomes survive copying |
| Crash and disk failures | Kill during copying, validation, data/manifest sync and publication; full disk/permission errors; no false success, partial promotion or replacement of the last valid artifact |
| Ownership and privacy | Competing owners, symlink/hardlink/reparse/path races and wrong ACLs refuse on real Linux and Windows; no secret/configuration disclosure |
| Quarantined restore | Corrupt/truncated/foreign/client/future-schema or manifest-mismatched artifacts refuse without modifying originals; no listener or supervisor starts |
| History rollback | Reproduce the dated same-ID/older-history example; later promotion must refuse it without sufficient continuity proof, including offline clients and lost acknowledgments |
| Split brain and failover | Real independent hosts/partitions, stale leaders and restored witnesses cannot both publish; preserve exact uncertain IDs and dirty bytes through reconnection |
| File availability | Recover referenced immutable payloads and retained local dirty bytes; metadata integrity alone cannot prove that file contents are available |

Physical reboot, independent hosts, actual target Windows versions, provider
acceptance and operational restore drills remain separate gates. No production
backup schedule, retention store, credential or recovery service was provisioned.
