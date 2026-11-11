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

## Office rerun with delete-on-close (`run5`, approved 2026-11-10)

**Status: run on 2026-11-10 (evening, local; 2026-11-11 01:22 to 02:26 UTC) and
stopped by the owner after Word. Delete-on-close works and Word saves reach the
provider intact, but Word could not reopen its document within seven minutes
and a cold read took about seven minutes per 4.6 MB file. Excel was not run.**
See [Results](#results-2026-11-10). This is phase 3
of [delete-on-close on the version 8 writer](sparse-delete-on-close.md) and the
separate approval that [Cleanup and limits](#cleanup-and-limits) requires.
The plan below was written from a cloud session before the run.

- **Build.** A private MSI built on MiniAir from `6992084` or a later commit of
  branch `claude/relaxed-dirac-z0oi0d` whose hosted CI passed
  ([run 42871136250](https://github.com/marshallr12/s3drive/actions/runs/42871136250),
  ten of ten jobs). Hosted CI does not publish an MSI. The owner uninstalls the
  `68cfab8` build and installs the new one; record the MSI and installed binary
  SHA-256 and check that they match the build.
- **Write scope.** Bucket `s3drivetest`, managed root
  `s3drive-physical-20261108/run5/.s3drive/`, inside the existing writable
  prefix and write policy. Nothing outside it is written or deleted.
- **Bound.** At most 200 object PUTs and 100 MiB uploaded, from throwaway files
  only. The owner's approval named no figures; this carries over the `run4`
  bound, which the failed saves barely used. What `run4` actually uploaded was
  not recorded, so list the prefix first and confirm the total stays inside the
  original 500-PUT / 256 MiB bound (140 objects and 41.7 MB on 2026-11-08).
  Stop and ask before exceeding either figure.
- **Local components.** A fresh loopback authority (`authority-5`, its own
  volume) on `127.0.0.1:43131`, a store plan in `store-plan-5`, state in
  `managed-state-5` and a version 8 `component-retention-v1` drive in `drive5`,
  all under `%LOCALAPPDATA%\S3Drive\physical-20261108\`. Mount at
  `%USERPROFILE%\S3DriveRun5` (the path must not pre-exist). The owner runs
  every command; the assistant does not handle the provider keys.
- **Probe evidence.** The 2026-11-08 conditional-PUT report expires seven days
  after it was written. Run before 2026-11-15, or get a new probe approved.
- **Checks, in order.**
  1. The two `CreateFileW` probes from `run4`: the delete-on-close create must
     now succeed, and its file must be gone after the handle closes.
  2. Word, then Excel: create, save, edit-save, Save As, close and reopen.
     Record the wall time of each save, the mount's CPU-seconds, and the state
     database and write-ahead log sizes before and after.
  3. If a save still fails, take a Process Monitor trace of that one save
     before changing anything, so the failing call is observed, not inferred.
  4. Look for leftover Office temporary files (`~WRD*.tmp`, `~$*`, eight-hex
     names) in the folder after each application closes.
  5. Cold remount, then SHA-256 read-back of every saved document against a
     copy saved outside the mount.
  6. PUT count and bytes under the `run5` root, per save where it can be told
     apart.
- **Not covered.** Startup tasks, sign-out and reboot, and anything on another
  machine.
- **Cleanup.** The `run5` keys are deleted with the rest of the prefix, after
  the owner has reviewed the results, as above.

### Results (2026-11-10)

Outcome by check:

| Check | Result |
|---|---|
| 1. `CreateFileW` delete-on-close probe | Passed |
| 2. Word create, save, edit-save, Save As | Files written; save times not cleanly measured |
| 2. Word close and reopen | **Failed**: not open after at least seven minutes |
| 2. Excel | Not run (owner stopped the run after Word) |
| 3. Process Monitor trace | Not taken: no save failed |
| 4. Leftover temporary files after Word closed | None |
| 5. Cold remount, SHA-256 read-back | Bytes identical; **868.72 s for two 4.6 MB files** |
| 6. Provider usage under `run5` | 36 objects, 13.16 MiB (bound 200 / 100 MiB) |

So phase 3 confirms the delete-on-close change on a physical machine and a real
provider, and shows that Office is still not usable on this build for two
other reasons: the cost of Word's open sequence, and the cost of a cold read.
Neither was diagnosed in this run. Details, in the order they happened:

- **Build.** MSI built on MiniAir from `6992084` (clean tree, the CI installer
  job's release command). `d138b5b`'s own hosted run 42898618988 was still in
  progress at build time, so it was not used; it passed later, and differs from
  `6992084` in documentation only. MSI SHA-256
  `b09a099238509069499554014cb0eff3ceb70e19dea835e60382fd2330c5a812`, binary
  `ce807c46f9b4be9823ccfad16b40759ac000a77eaac7e4fd0f412cc20ba02bc2`.
- **Install.** The owner uninstalled the `68cfab8` build and installed this one
  (Windows Installer status 0 for both). The installed binary's SHA-256 matches
  the build.
- **Probe evidence.** The report's `observed_unix_seconds` is 2026-11-08
  19:20 UTC, about two days old at the start of the run; no new probe.
- **Prefix before the run** (read-only listing run by the owner, 2026-11-11
  01:22 UTC): 141 objects and 43,762,364 bytes (41.74 MiB) under
  `s3drive-physical-20261108/`. Of these `run3` holds 131 objects (32.25 MiB),
  the first volume 9 (9.48 MiB), `run4` one object of 64 bytes, and `run5`
  none. The whole `run5` bound on top of that is 341 objects and 141.74 MiB,
  inside the original 500-PUT / 256 MiB bound. The listing counts objects, not
  PUT requests.
- **Authority.** `authority-5` on `127.0.0.1:43131`, `portable-collision-v1`,
  volume `267d24e3acd2e8d30ac1c6a2cca7122e`.
- **Store and drive.** Store `27d713285d0eed0ee540bd508c2627b7`, root
  `s3drive-physical-20261108/run5/.s3drive/` (plan SHA-256
  `4d4650c28e109c2b1d7e001c4e35f3688d7ed110411eb3ee4c3e2cb6f446e414`,
  registered), version 8 `component-retention-v1` drive in `drive5`, mounted
  read-write at `%USERPROFILE%\S3DriveRun5`.
- **Check 1, `CreateFileW` probes: passed.** Both creates ask for
  `GENERIC_READ | GENERIC_WRITE | DELETE`, `CREATE_NEW` and
  `FILE_ATTRIBUTE_TEMPORARY` on an empty file in the mount root.

  ```text
  plain-temporary create=ok exists_while_open=True exists_after_close=True ms=2989
  plain-temporary removed=True
  delete-on-close create=ok exists_while_open=True exists_after_close=False ms=2289
  listing-after:
  ```

  The delete-on-close create, refused with Win32 error 19 on `run4`, is
  accepted, and its name is gone once the handle closes. The folder was empty
  afterwards.
- **Cost of the two probe files.** From the baseline to 24 s after the probe
  the mount went from 0.31 to 8.8 CPU-seconds, the state database from
  651,264 to 729,088 bytes and the write-ahead log from 45,352 to 4,218,912
  bytes. That is two empty files created and removed, so about the same per
  file as on `run4` (about 7 CPU-seconds and 3 MB of log for one).
- **Check 2, Word saves: the files were written; times are not clean.** A new
  document saved as `run5-word.docx`, an edit saved with Ctrl+S, and a Save As
  (the name came out as `run5-word.-copydocx.docx`, a typing slip) all left
  their files in the folder. Whether Word showed any message during the saves
  was not recorded. Snapshots taken by hand before and after each save:

  | Step | Span between snapshots | Mount CPU-seconds | State database (bytes) | Write-ahead log (bytes) |
  |---|---|---|---|---|
  | Save 1 (new file) | 71 s | 8.8 → 17.42 | 729,088 → 3,969,024 | 4,218,912 → 5,108,832 |
  | Save 2 (Ctrl+S) | 66 s | 54.58 → 79.97 | 13,389,824, unchanged | 5,108,832, unchanged |
  | Save As | 67 s | 103.53 → 142.73 | 13,533,184, unchanged | 5,108,832 → 5,174,752 |
  | Word closed (141 s later) | | 209.09 | 13,533,184 | 5,174,752 |

  The spans include the owner's time in the dialogs, and each "after" snapshot
  still listed a `~WRD` temporary file, so the saves were probably still
  finishing; the spans are not save times. The owner gave no stopwatch
  figures. The mount kept working between saves: 37 CPU-seconds between save 1's end and
  save 2's start, 66 after the Save As.
- **Check 4, leftovers after Word closed: none.** During the run the folder
  listed `~$n5-word.docx`, `~WRD0000.tmp`, `~WRD0002.tmp`, `~WRL0003.tmp` and
  `~WRD0004.tmp` at various snapshots. After Word closed only the two
  documents remained.
- **Check 2, Word reopen: FAILED.** Opening `run5-word.docx` from Explorer
  froze the Explorer window; the first sample of the frozen state was at
  01:44:40 UTC. Word's process started at 01:49:43 and a second Word process
  at 01:50:56; both showed only the splash screen ("Opening - Word"). At
  01:51:37, at least seven minutes after the open was requested, the document
  had not appeared and the owner stopped the attempt. The mount was working
  throughout, not idle: 209 CPU-seconds at 01:40:02, 541 at 01:44:40 and 645
  at 01:51:37, with the database and log still being written. So this is
  slowness, not a deadlock, but whether the open would ever have completed
  was not observed. A 4.6 MB document that does not open in minutes is
  unusable. No Process Monitor trace was taken: the plan asks for one on a
  failed save, and this was an open. By the time Word was closed (01:53:36)
  the mount had used 716 CPU-seconds.
- **Plain reads are fast, so the cause is Word's open sequence, not reading.**
  Straight after Word was closed, a script read both documents in full
  through the same mount (4,599,470 and 4,599,486 bytes) in 6.59 seconds
  for the pair, copy and hash included. Both copies are valid Word packages
  (38 entries) with the typed text. What Word asks for while opening, and which of those requests
  is slow, was not observed.
- **Unmount.** Two operator slips first, neither with any effect on the data:
  a second `mount` onto the still-mounted path was refused twice with Win32
  error 1005 ("The volume does not contain a recognized file system"), a
  misleading message for a path that is already mounted; and Ctrl+C was
  pressed in the authority's window, stopping it while the mount was idle.
  The authority was restarted on the same volume with `--expect-volume` at
  01:58:54 UTC. Ctrl+C in the mount window then unmounted at once, with no
  message; the process was gone, the mount point removed and the state
  write-ahead log checkpointed away by 01:59:12 (database 13,533,184 bytes).
  The mount had run for 33 m 55 s and used 717 CPU-seconds. The remount below
  therefore also follows an authority restart.
- **Check 5, cold remount and read-back: bytes correct, far too slow.** The
  remount printed "Mounted" at once (process started 02:00:18 UTC). Both
  documents then hashed identical to the copies taken before the unmount
  (`mismatches=0`):

  | File | SHA-256 | Result logged (UTC) |
  |---|---|---|
  | `run5-word.-copydocx.docx` | `4c1a4114c3bbecbee1ebfdb36d781cc8c26a093f86d33deacd67c673fbb5e818` | 02:07:50, MATCH |
  | `run5-word.docx` | `b72b798de03ec09666cc653b6c4b249364944637d227b78248eeb95552c90ca5` | 02:15:21, MATCH |

  The whole read-back took 868.72 seconds, so each 4.6 MB file took about
  seven minutes to read cold, against 6.59 seconds for both
  while their bytes were still local. Over that time the mount used 78
  CPU-seconds at a constant rate (about 2.8 per 30 s), held one HTTPS
  connection, and its process I/O counter rose by about 0.5 MiB per 30 s;
  the state database was not written. Whether that is the object arriving at
  about 17 KB/s or the overhead of many small ranged requests was not
  measured. The copies are not independent of the mount (Word saved straight
  into it), so the match shows the remount returns what the mount held, and
  the package check above shows those bytes are valid documents.
- **Check 6, provider usage after Word** (read-only listing, 02:26 UTC):
  36 objects and 13,803,398 bytes (13.16 MiB) under the `run5` root, inside
  the 200-PUT / 100 MiB bound. The whole prefix holds 177 objects and
  54.90 MiB; the other roots are unchanged. By size the 36 are twelve 1 MiB
  objects, three of 405,166, 405,182 and 407,042 bytes, three each of 145,
  162 and 469 bytes, and twelve of 64 bytes. Four 1 MiB objects plus one tail
  add up exactly to each saved document (4,599,470 and 4,599,486 bytes) and
  to a third generation of 4,601,346 bytes. So the three saves published
  three whole 4.6 MB generations, about twelve objects and 4.6 MB each, and
  no object is attributable to Word's temporary files or to the two probe
  files. This is inferred from sizes; the listing counts objects, not PUT
  requests, and the keys were not mapped to files.
- **Excel: not run.** The owner stopped the run after the Word results. Whether
  Excel's save gets past the refusal that stopped it on `run4` is untested.

**Open after this run** (proposals, none started):

- A Process Monitor trace of one Word open on a version 8 drive, to see which
  request costs minutes. Plain reads of the same files were fast.
- The cold read path: five objects per document took about seven minutes.
  Per-request overhead is suspected from the constant slow rate; not measured.
- Clean save times need a scripted or traced save, not hand-run snapshots.
- The `mount` error for an already-mounted path (Win32 1005) is misleading.

**Left in place.** The `run5` drive, state, store plan and authority
directories under `%LOCALAPPDATA%\S3Drive\physical-20261108\`, the `6992084`
MSI installation, and all objects under the prefix. Scripts, logs and the two
document copies are in `%TEMP%\s3drive-msi\run5\`, unpublished. No startup
task, sign-out, reboot or key cleanup was done.
