# Coordinator backups and recovery

Research/design dates: **2026-10-17–18**, America/New_York. **Offline snapshot
prototype; Windows acceptance pending. Restoration and failover remain proposals.**
Pinned startup and subsequent transport diagnostics are accepted through f8c4f58;
see [their scoped evidence](authority-startup.md). No real database,
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

## Resolved Rust API constraints

The locked rusqlite0.40.2 source was inspected locally on2026-10-17. Its
`Backup::new` borrows source and destination connections; `step` distinguishes
completion from busy/locked/more work. `run_to_completion` has no overall retry
deadline. Cleanup calls SQLite's finish function from `Drop` and discards its
return value; this version has no public `finish` method. At that research
checkpoint the workspace enabled bundled SQLite without the backup feature;
the unpublished prototype now also enables backup, hooks and limits.
[Recorded API/source fingerprint](evidence/rusqlite-backup-api-20261017.json).

The online docs.rs source endpoint could not be retrieved during this check;
these are observations of the actual resolved build input. Before choosing a
snapshot mechanism, inspect SQLite's completion guarantees and determine how
safe APIs establish completion, validation and close/sync errors. Do not invent
a wrapper method, use an unbounded convenience loop or bypass this question
with raw FFI. The prototype below uses checked `step` completion and subsequent
independent validation; the initial API research alone enabled no feature.

## Proposed delivery contract

1. **Snapshot and inspect.** First implement an explicit maintenance snapshot
   under exclusive authority ownership, then bounded online snapshots through
   the owning service. Do not expose arbitrary host output paths to ordinary
   namespace clients. Keep database ownership while SQLite reads; do not make
   an independent raw-file copier bypass it. Inspect resolved rusqlite backup
   APIs/features before implementation; the prototype uses the resolved safe API
   and explicitly enables its required features.
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


## Next implementation stage

[P6m4d1](implementation-plan.md#p6m4d1--private-offline-snapshots-and-inspection-prototype-acceptance-pending)
is an unpublished library prototype on `codex/authority-snapshots`, now rebased
from a7994e6 onto accepted diagnostic main f8c4f58;
experimental offline snapshot/inspection CLI commands are now present locally;
no restore/promotion command exists. These changes remain unpublished.
The artifact uses distinct snapshot filenames so ordinary startup refuses it.
Local2026-10-18 state check/Clippy and nine filtered tests passed, including
WAL-only data, exact schema8 receipts, full disk, cancellation and eight actual
killed-process boundaries. This is Linux fixture evidence, with no source-data
loss or automatic promotion. Two real local CLI round-trip/refusal cases also passed. Windows/ACL,
native acceptance remain. The combined f8c4f58-plus-snapshot local gate passed
on2026-10-18 with274 state tests, two snapshot CLI cases, two Hyper I/O, ten
HTTP, six TLS,17 SDK and seven managed cases plus lint/build/book checks.
Actual Windows snapshot acceptance is next.

The source is opened read-only under the existing owner's cooperative lock,
without migrations or namespace changes. Ordinary WAL/SHM bookkeeping can occur;
source operation/receipt bytes remain identical. The prototype's validation
profile checks SQLite integrity and authority identity, not complete semantic
history or freshness. Inspecting a complete snapshot always reports continuity
unproven and restore unauthorized. No real provider or user metadata was copied.


## Experimental offline CLI (unpublished, 2026-10-18)

The local prototype exposes `s3drive authority-snapshot create` and `inspect`.
Both require an explicit `--expect-volume` and `--name-policy`; creation also
requires the stopped authority's `--directory` and a new `--output` directory.
The output parent must exist. An active owner, wrong identity, missing state,
nested destination or existing artifact refuses; the command never stops a
coordinator, replaces output or starts a listener. Neither command accepts an
S3 configuration, provider credential or authorization token.

For example, after stopping an owned test coordinator, a Linux invocation is:

```sh
s3drive authority-snapshot create --directory /private/authority --output /private/snapshots/new-copy --expect-volume ORIGINAL_VOLUME_ID --name-policy portable-collision-v1
s3drive authority-snapshot inspect --directory /private/snapshots/new-copy --expect-volume ORIGINAL_VOLUME_ID --name-policy portable-collision-v1
```

Replace `ORIGINAL_VOLUME_ID` with the original32-character volume ID. These
examples do not create a hosting account or install/stop a real user service.
Defaults are512MiB maximum standalone database bytes and30 seconds cooperative
work; `--max-bytes` permits4KiB through8GiB and `--timeout-seconds`1–300. Work
runs on a blocking worker. Ctrl+C requests cooperative cancellation and waits
for that worker; a stalled kernel I/O call is not interruptible by this deadline.
One real Linux SIGINT check passed below; Windows console-control behavior
remains a separate check.

Success prints only the validated manifest and explicit false continuity/restore
authorization flags. It does not print namespace records or input file contents.
A cancellation/error after marker publication may be CompletionUnknown; retain
the original source and partial output and inspect that exact artifact. There
is no automatic retry, new destination selection or cleanup of uncertain output.
CLI tests use Unicode private paths, exact receipts, duplicate-output refusal,
active/wrong/missing/client sources, size/deadline argument bounds, nested output
and a private corrupt-manifest marker, without real credentials or S3 operations.
The full local gate passed (274 state tests plus CLI/lint/build/book); actual
platform acceptance and deployment drills remain.


## Linux cooperative cancellation evidence (2026-10-18)

The [dated CLI observation](evidence/authority-snapshot-sigint-20261018.json)
used the locally built, unpublished defc8b9 prototype. Its
[Linux-only reproducer](evidence/authority-snapshot-sigint.py) creates/stops a
private loopback fixture authority, then creates/drops disposable zero-filled
padding so the original valid namespace has128MiB of free pages. The fixture
contains no user configuration or S3 data. Caller must supply and verify the
executable; the evidence binds binary and Rust/input source hashes.

After the snapshot directory appeared, the test sent SIGINT to its own unreaped
child process. The CLI reported cancellation with no success stdout, left no
completion manifest, and exited2.919ms after the signal in this single observation.
The source database SHA-256 stayed identical, partial inspection refused, and
a retry at that same destination refused without replacing it. All owned
processes were reaped before disposable-fixture cleanup.

This validates one real Linux cancellation path. It does not guarantee latency,
prove Windows Ctrl+C, interrupted kernel I/O, physical power-loss durability,
restore continuity or actual provider/mount recovery. The source gate had already
passed; this heavy manual fixture is separate from the portable gate.


## Locked-source refusal evidence (2026-10-18)

The [separate Linux CLI experiment](evidence/authority-snapshot-busy-20261018.json)
held an independent raw SQLite EXCLUSIVE writer on the stopped fixture
authority in DELETE journal mode. The snapshot command refused in109.197ms
within its configured2-second cooperative budget, printed no success and
created no destination. Source SHA-256 stayed unchanged. After the lock was
released, an explicit control invocation using the same arguments succeeded,
and offline inspection matched creation with both restore/continuity flags false.

The [reproducer](evidence/authority-snapshot-busy.py) tests SQL source locking
beyond the cooperative application owner lock. It uses only a disposable
private loopback/SQLite fixture. It does not prove Windows locking, backup
contention at the destination, interrupted kernel I/O or a universal latency
bound. As in the implementation, its checksum file descriptors are opened only
when its own SQLite connections are closed, preserving POSIX lock behavior.


## Prepared Windows acceptance (runtime pending, 2026-10-18)

The existing disposable ordinary-account coordinator workflow now includes
`scripts/test-windows-snapshot-worker.ps1`. It first checks that a live
coordinator refuses snapshot creation without creating output. After normal
coordinator removal and a successful original-identity check, the same account
must create and inspect the artifact, preserve the source database's SHA-256,
refuse replacement and report both continuity and restore authorization false.
The fixture tests a readable concurrent file handle, temporarily widened
Everyone-read ACL with exact descriptor restoration, an owned hard link, a
source junction and a case-alias nested output. Positive inspection controls
follow reversible faults. It uses the existing fresh test account and fixtures;
no real user account, credentials, provider, startup entry or restore is involved.

PowerShell5.1 lists `HardLink` and `Junction` among filesystem `New-Item` types.
This API check and local PowerShell parsing do not establish Windows filesystem
behavior. [Microsoft New-Item documentation](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.management/new-item?view=powershell-5.1)
(read2026-10-18). Actual hosted Windows execution, existing native/reference/
startup regressions, and source-bound logs are required before acceptance.
Windows console cancellation, physical power loss and target Windows10/11
remain separate evidence requirements.


## Windows console cancellation follow-up (prepared; runtime pending, 2026-10-18)

The resolved Tokio1.53.3 `ctrl_c` implementation registers a Windows Ctrl+C
listener; the Linux SIGINT experiment does not exercise it. A future actual
Windows fixture must isolate its console before generating a control event.
Microsoft documents that `CTRL_C_EVENT` cannot be targeted to a nonzero process
group; a zero group broadcasts to processes sharing the caller's console.
`CTRL_BREAK_EVENT` has different delivery semantics and would not test this
CLI's Ctrl+C path. [GenerateConsoleCtrlEvent](https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent)
(read2026-10-18).

The prepared fixture creates a new console owned only by a disposable helper and
its snapshot child, install a helper-local handler, and signal only that
isolated console after output creation. Never attach to or broadcast into the
runner's existing console. Do not set the inheritable ignore-Ctrl+C attribute: a
child would inherit it and the test could measure forced termination instead
of cooperative cleanup. Changing console attachment resets handlers, so the
helper must register after creating/attaching its private console.
[SetConsoleCtrlHandler](https://learn.microsoft.com/en-us/windows/console/setconsolectrlhandler)
(read2026-10-18).

Use a valid stopped authority with sufficiently many disposable pages, assert
non-success and no completion manifest, preserve source bytes and partial
output, refuse same-destination replacement, and reap every owned process
before cleanup. A helper deadline must fail, never become a cancellation PASS.
The source cannot be padded through a raw file while SQLite is active. This is
a prepared acceptance design only; no console helper or Windows Ctrl+C result
is included in candidate f9a52e2. The follow-up files now exist locally but
remain unverified on Windows.


The prepared helper is `scripts/test-windows-snapshot-console.cs`. Windows
PowerShell5.1 compiles it as a console application inside the existing
disposable ordinary account. This compiler path is intentional: current
PowerShell7 no longer supports the executable output types.
[PowerShell output assembly types](https://learn.microsoft.com/en-us/dotnet/api/microsoft.powershell.commands.outputassemblytype?view=pscore-6.0.0),
[Add-Type executable-output limitation](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/add-type?view=powershell-7.3)
(read2026-10-18). The helper detaches only itself, calls `AllocConsole`, and
checks `GetConsoleProcessList` for exactly itself before launch and itself plus
its retained child before any signal. Unexpected membership fails; it never
uses `AttachConsole`. [AllocConsole](https://learn.microsoft.com/en-us/windows/console/allocconsole),
[GetConsoleProcessList](https://learn.microsoft.com/en-us/windows/console/getconsoleprocesslist)
(read2026-10-18).

The test-only Rust padding mode retains the production directory-owner lock
while a separate SQLite connection creates/drops128MiB of zero-filled scratch
pages. All connections close before source hashing. It does not add namespace
operations. A local8MiB positive fixture passed, preserving the original
identity/sequence and byte-identical operation input/outcome; the C# source
compiles with warnings treated as errors under the installed Mono compiler,
and changed PowerShell scripts parse locally. The parent must also pass an
offline original-identity authority check after cancellation, proving source
ownership can be reacquired. These checks establish fixture
preconditions and syntax only. The new code is outside the still-running
f9a52e2 candidate. Its full local gate subsequently passed with274 state tests,
eight restart/worker cases and the existing snapshot CLI/HTTP/TLS/SDK/managed/
lint/build/book checks; the final changed PS scripts passed fresh AST parsing,
and the helper compiled as C#5 against .NET4.5 with warnings treated as errors.
Exact-source hosted Windows execution is still required. Helper or child deadline, forced termination, lost membership,
completion before the observation boundary, or missing cancellation error must
fail acceptance. Owned-account cleanup remains mandatory on failure.
