# Physical Windows + Wasabi acceptance plan (proposal, 2026-11-08)

**Status: proposal awaiting owner sign-off. Nothing in this plan has run.**
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
