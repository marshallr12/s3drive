# Physical Windows + Wasabi acceptance plan (2026-11-08)

**Status: approved by the owner on 2026-11-08 and partly executed; results are
recorded at the end of this page.** The first draft was a proposal only.
The owner chose Wasabi over a local S3 emulator on 2026-11-08 for the first
physical-machine mount, startup and Office/Explorer test. Choosing Wasabi
selects this plan's direction; it does not approve the writes below.

## Machine and build

- MiniAir, Windows 11 Pro 10.0.26300, WinFsp 2.1 (2025 build).
- MSI built locally from main `feea0a9` (MSI SHA-256
  `d6206675489748cfcf579348a868bca61260a82d58c0c95ade627ad9baa8d5fc`, binary
  `d7b66779ec5cb4557a0ce172010d579ff1aa20be1f47b1606e57bc457d56b27c`).
  The owner installed it interactively on 2026-11-08: Windows Installer status 0,
  555/555 payload files hash-identical to the build, no PATH or startup changes.
  A system reboot was already pending before installation.

## Write scope requested

All writes go to bucket `s3drivetest`, endpoint
`https://s3.us-east-2.wasabisys.com`. Nothing outside the two prefixes below is
read for mutation, written or deleted.

1. **Fresh conditional-PUT evidence.** Write policies require a probe report
   no older than `evidence_max_age_seconds` (proposed 7 days), so the
   2026-10-16 report cannot be reused. One run of the existing
   `probe-writes plan` / `run` under a new
   `s3drive-conformance/<random run id>/` scope: at most 14 PUTs, six keys,
   bodies up to 4 KiB, preflight requires an empty unversioned scope, and the
   run deletes only its own six keys. Interrupted runs use `probe-writes cleanup`
   (no PUT).
2. **Managed test volume.** A new empty application prefix
   `s3drive-physical-20261108/`, managed root `managed/.s3drive/`, version 7
   (`--windows-disposition posix-v1`) writable profile, policy
   `create-and-replace`, `max_object_bytes = 8388608`. Proposed workload bound:
   at most 500 object PUTs and 256 MiB uploaded in total, from throwaway files only.

## Local components (no remote hosting)

- `s3drive authority-serve` on numeric loopback with private SQLite, token and
  plan directories under `%LOCALAPPDATA%\S3Drive\physical-20261108\`, outside
  Program Files and the mount.
- `managed store-plan` / `store-apply` / `configure --mode read-write`, and a
  directory mount at `%USERPROFILE%\S3DriveTest` (path must not pre-exist).
- Per-account startup via `scripts/windows-session.ps1 -Action Install` for both
  the authority and the mount. The owner types their Windows password into the
  local credential prompt; Claude never sees it.

## Owner prerequisites

- Add a `wasabi-s3drive` AWS profile on this machine with a key limited to
  `s3drivetest` (only `[default]` exists now). Claude must not handle the keys.
- Grant the account **Log on as a batch job** if Task Scheduler refuses with
  `0x80070569`.

## Checks

1. Probe report: all ten checks pass, final scope listing empty.
2. Mount: Explorer create/copy/rename/delete of throwaway files; Unicode name;
   a binary file over 1 MiB; cold remount and byte-exact read-back (SHA-256).
3. Office: create, save, edit-save and Save As of a Word and an Excel document
   in the mount; close and reopen. Record any WinFsp lifecycle differences.
4. Startup: sign out/sign in, then a full reboot; the mount returns without a
   terminal and the files hash-identical.
5. Stop/Remove the startup tasks; verify the uninstall path leaves user data.

## Cleanup and limits

The managed layout has no remote GC yet, so deleted files leave immutable
objects behind. After the owner has reviewed the results, delete exactly the
keys under `s3drive-physical-20261108/`, listing them first and confirming
the count with the owner before deletion. Separate approval is needed for any
rerun, a larger bound, another prefix, multipart or versioned-bucket checks.
This plan does not establish multi-host coordination, remote hosting, or
provider-wide behavior.

## Results so far (2026-11-08, in progress)

Owner approval was given in the project thread at 19:19 UTC. Only the
`wasabi-s3drive` profile was added by the owner (a root key, not a sub-user key;
the owner intends to rotate or delete it after testing). Claude never handled
key material.

**Probe.** `probe-writes plan`/`run` under `s3drive-conformance/7d38d354…/`:
all ten checks passed and the six fixture keys were removed. The report is the
write-policy evidence for the managed volume (`evidence_max_age_seconds` 604800).

**Managed volume.** Loopback authority `portable-collision-v1`, volume
`2e463bd1…`, store `4325a5fc…`, root `s3drive-physical-20261108/.s3drive/`,
version 7 `posix-v1` writable profile, mounted at `%USERPROFILE%\S3DriveTest`.
Scripted checks passed: file creation, a Unicode folder and file name, a 3 MiB
random file (read back SHA-256 identical, also after three forced process kills
and cold remounts), rename, delete and overwrite.

**Defects found** (each is a code change for a separate PR):

1. **Per-open cost grows with local history.** A Word *Save As* of a 3.4 MB
   document grew `managed-state/metadata.sqlite3` to 277 MB within about two
   minutes. Afterwards every file open costs 7–25 CPU-seconds, including the
   10-byte `hello.txt`, and a cold remount about 60–95 CPU-seconds. Word opens
   files dozens of times per save, so the save ran for more than eight minutes
   and appeared hung; Word retried with a second temp file. This is the
   version 7 whole-generation profile without local reclamation. It is not an
   infinite loop or a deadlock: operations complete serially. Two full memory
   dumps of the mount process (spinning, and later idle with a queued request),
   the matching PDB and a copy of the database are kept locally under
   `%LOCALAPPDATA%\S3Drive\physical-20261108\evidence\`, unpublished.
2. **Office and CopyFile creates are refused.** Direct `CreateFileW` probes on
   a fresh store show: a create requesting `MAXIMUM_ALLOWED` or `GENERIC_ALL`,
   or any mask containing `WRITE_DAC`/`WRITE_OWNER`, fails with
   `ERROR_WRITE_PROTECT` (19); a create with `FILE_ATTRIBUTE_TEMPORARY` fails
   with `ERROR_NOT_SUPPORTED` (50), and the adapter accepts only the NORMAL and
   ARCHIVE attributes on create and set-attributes. `CopyFile` (Explorer paste,
   drag-and-drop, `copy`, `Copy-Item`) always asks for `WRITE_DAC`; Excel writes
   its save temp file (for example `D1834730`) with the temporary attribute and
   reported "cannot access the file"; Word reported "a file permission error".
   Office lock files (`~$name`) also need the HIDDEN attribute. Plain creates,
   stream copies, `DELETE`, delete-on-close, renames and deletes work.
3. **All timestamps report as 1969-12-31** (zero), on every file and directory.

**Provider usage.** After the Word test, `s3drive list` under the application
prefix found nine objects totalling 9.5 MB: one object per closed file
generation (the 3 MiB file, two generations of Word's temp file, four small
files, the 0-byte document, one deleted file). After the scripted reproduction
on the `run3` store the prefix held 140 objects and 41.7 MB: the interrupted
4 KiB flushed-write run alone produced 131 objects, so **each flush of an open
file publishes a new whole-file generation**. Word does not flush per write, so
its save produced only two generations; applications that do flush will
amplify uploads as well as local history. Totals stayed within the approved
500-PUT / 256 MiB bound.

**Scripted reproduction of the Word save pattern** (`word-save-repro.ps1`,
kept outside the repository): on a *fresh* store, writing 3.4 MB as 4 KiB
sequential writes with a flush after each one did not finish within 240 s; the
mount process used 157 CPU-seconds and the local database grew by 33 MB before
the run was stopped. The cost is therefore per flushed write, not only a
consequence of an already large history. A plain 3 MiB `WriteAllBytes` of the
same size completes in well under a second.

**Operational note.** Claude's tools run inside the packaged (MSIX) Claude
desktop app, which redirects its writes under `AppData\Local` to the package's
`LocalCache`; the owner's own shell could not see the test state until it was
moved to the real path. `%TEMP%` is not redirected. Test state should be
created in a location both can see before registering scheduled tasks.

**Startup registration (not completed).** The install path worked up to
configuration validation of the installed helper. Registration then needs the
Windows account password for `TASK_LOGON_PASSWORD`, and the owner signs in with
a Windows Hello PIN and does not know the account password. This is defect 4:
the per-account startup design is unusable on a typical PIN-only Windows
sign-in; startup must run at sign-in from the interactive session without a
stored password (the Google-Drive-style default chosen by the owner on
2026-11-08). PR #2 adds that for the drive-letter mount; the coordinator
registration still needs the same treatment. Sign-out/sign-in and reboot
acceptance therefore wait for that work. Two registration directories left by
the failed attempt must be removed before any retry.

**Follow-up (2026-11-09).** Defect 2 has an adapter change with local real-mount
evidence, defect 3 is narrowed to a per-mount overlay, and defect 1 was measured
on the version 8 profile and is not fixed; see
[Windows desktop application requests](windows-desktop-requests.md).

**Not yet done:** scheduled startup tasks, sign-out/sign-in, reboot, and prefix
cleanup with the owner's confirmation of the key count. Office tests cannot pass
on this build and were stopped.

## Office rerun on version 8 (`run4`, approved 2026-11-09)

**Status: approved by the owner on 2026-11-09; not yet run.** This is the
separate approval that [Cleanup and limits](#cleanup-and-limits) requires for a
rerun and a new prefix.

- **Build.** The private MSI from `68cfab8` (pull request 3 head, hosted CI 20 of
  20 checks passed), installed by the owner after uninstalling the `feea0a9`
  build. Installed binary SHA-256 `b9773e23…d854`, matching the build.
- **Write scope.** Bucket `s3drivetest`, managed root
  `s3drive-physical-20261108/run4/.s3drive/`, inside the existing writable prefix
  and write policy. Nothing outside it is written or deleted.
- **Bound.** At most 200 object PUTs and 100 MiB uploaded for this rerun, from
  throwaway files only. This fits inside what remains of the original
  500-PUT / 256 MiB bound (140 objects and 41.7 MB used on 2026-11-08).
- **Local components.** A fresh loopback authority (`authority-4`, its own
  volume) on `127.0.0.1:43130`, a store plan in `store-plan-4`, state in
  `managed-state-4` and a version 8 `component-retention-v1` drive in `drive4`,
  all under `%LOCALAPPDATA%\S3Drive\physical-20261108\`. Mount at
  `%USERPROFILE%\S3DriveRun4`. The owner runs every command; the assistant does
  not handle the provider keys.
- **Probe evidence.** The 2026-11-08 conditional-PUT report is within its
  seven-day `evidence_max_age_seconds`; no new probe run.
- **Checks.** Check 3 above (Word and Excel: create, save, edit-save, Save As,
  close and reopen), then a cold remount with SHA-256 read-back. Record save
  times, the state database size and the PUT count and bytes under the `run4`
  root.
- **Cleanup.** The `run4` keys are deleted with the rest of the prefix, after the
  owner has reviewed the results, as above.

### Results so far (2026-11-09, in progress)

Authority volume `24866f09aede3a42787c2e74227a60ca`, store
`8471b0605a628507976268fec168dd58` (plan SHA-256 `c34b866a…dc9b0`, registered),
version 8 `component-retention-v1` drive mounted read-write.

- **Word: the first save failed.** Word reported "Disk c: is write protected".
  The mount process used no CPU for the attempt and the state database did not
  change, so the request was refused in the WinFsp adapter before any storage
  work.
- **Probable cause: delete-on-close.** Two `CreateFileW` probes on the mount,
  differing only in `FILE_FLAG_DELETE_ON_CLOSE`: the plain temporary create was
  accepted (and the file deleted again); the delete-on-close create failed with
  Win32 error 19, `ERROR_WRITE_PROTECT`. In the source, the adapter refuses
  `FILE_DELETE_ON_CLOSE` unless `native_lifecycle_enabled()`, which only the
  version 7 writer reports; on version 8 it is false. Delete-on-close is a
  documented unsupported path of the sparse writer
  ([native chunk writes](native-chunk-writes.md)). That Word's failing call is
  that create is inferred, not observed: no call trace of Word was taken. The
  real-mount desktop test passed because it never asks for delete-on-close.
- **Excel: the first save failed the same way** ("write protected"). Again the
  mount used about 0.1 CPU-seconds for the attempt and the state database did
  not change.
- **Cost of one empty file.** The accepted plain probe (create an empty
  temporary file, close, delete) took the mount from 0.4 to 7.6 CPU-seconds in
  about 20 s and grew the write-ahead log from 45 KB to 3.0 MB.
- **Conclusion.** Office saves cannot succeed on version 8 until the sparse
  writer supports delete-on-close. Version 7 supports it but keeps the per-write
  cost measured on 2026-11-08, and write coalescing applies to version 8 only.
