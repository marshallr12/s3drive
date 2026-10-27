# Windows installer

## Status and scope (2026-10-27)

The x64 MSI has **hosted candidate acceptance** at `1593ac9` in
[run40553716413](https://github.com/marshallr12/s3drive/actions/runs/40553716413),
not a merged or publicly published release. It installs the experimental command-line application, session
helper, documentation, GPL-3.0-only license and collected dependency notices. S3Drive is
not a desktop settings GUI. Windows 10/11 physical-machine acceptance, Office
application workflows, sign-in/reboot and live-provider acceptance remain separate.

The MSI uses Windows Installer's per-machine Program Files location and requires
administrator approval. It does not download or install WinFsp, change PATH,
register a service or startup task, create credentials, open a database, contact
S3, migrate a store, delete user data, or restart Windows. The app's existing
feature flags, quotas and runtime defaults are unchanged. It is unsigned during
private testing; a hash is provenance, not publisher signing or a reason to bypass
a Windows security warning. No public binary is uploaded by this CI job.

## Installation and first use

1. Obtain an approved installer and verify its recorded digest/source. Public
   distribution and signing are separate release decisions.
2. Install the official [WinFsp 2.1 runtime](https://winfsp.dev/) separately,
   accepting its own terms. The MSI checks the registered x64 runtime DLL before
   initial installation. It does not prove driver health or physical reboot
   readiness, and does not repair a broken WinFsp installation.
3. Close S3Drive mounts and supervisors cleanly. Preserve pending/uncertain edits;
   do not force-kill them merely to install or upgrade. Double-click the MSI and
   follow its Windows Installer prompts. The minimal UI is authored but not
   interactively GUI-tested by the silent hosted gate.
4. Run `& "$env:ProgramFiles\S3Drive\s3drive.exe" --help` in PowerShell.
   A full executable path is intentional: no PATH modification occurs.
5. Keep configuration, tokens, credentials and local SQLite/cache/journal data in
   private user-owned locations **outside Program Files and outside the mount**.
   Follow [managed bootstrap](managed-bootstrap.md) and
   [startup/session setup](startup-and-sessions.md). Configuration checking is
   offline and does not validate a bucket, authorize writes or install startup.

No writable provider configuration is supplied. Do not use real data for the
first physical acceptance run. Store/schema changes remain explicit application
operations, never MSI custom actions.

## Upgrade, repair and removal

MSI product versions use three numeric fields. Increment the package version for
a new distribution; a different binary must not be silently shipped under the
same version. Major upgrade identity is stable, product identity is stable per
version, and downgrade is rejected. Upgrade removes the earlier managed payload
inside the installation transaction so a failed new installation can roll back.
The separate hosted failure fixture adds the official WiX deferred-failure action;
it is absent from the ordinary installer. The native test verifies old payload,
registration and user data after the failed upgrade, then upgrades normally.
The hosted prior-version fixture packages the same binary as `0.0.9`; it verifies
installer mechanics, **not migration from an actual older database or release**.

Use Windows Installed Apps to repair/uninstall S3Drive. Removal deletes only MSI
owned files and registration. Unknown files in the install directory, user
configuration, databases, dirty bytes and WinFsp remain. Remove any separately
registered startup task explicitly using its session-management instructions
before uninstalling; the MSI does not silently edit an existing user task.

Never manually delete the state directory to fix an installer error. A process
holding application files may cause an MSI files-in-use prompt or a reboot-required
exit code. The package disables automatic Restart Manager shutdown and suppresses
reboot; stop the owning application safely and resolve the condition yourself.
Exit code 3010 is not treated as a successful completed acceptance test. No
reboot is performed or authorized by this installer build workflow.

## Troubleshooting

- Missing WinFsp: install/repair the official x64 runtime and retry. A registry
  entry without the required runtime DLL does not satisfy the package search.
- Access denied: per-machine installation requires elevation. Do not weaken
  application directory ACLs or store private data there as a workaround.
- Downgrade blocked: retain the newer installation; do not remove your database.
- Files in use: close the mount/supervisor safely, retaining pending writes, then
  retry. Do not accept forced data loss or silently restart active applications.
- MSI failure: collect a verbose log with `msiexec /i package.msi /l*v install.log`.
  Review logs before sharing; local paths/usernames may be present.
- Unknown publisher/security warning: do not bypass it automatically. Private
  unsigned CI packaging is not a trusted public distribution.

## Verification matrix

The additive standard GitHub Actions job builds the exact locked release binary,
collects notices from the Windows-resolved dependency graph and builds current/prior-fixture MSIs plus a separate rollback-failure fixture. On its disposable
Windows runner it tests:

- missing WinFsp refusal without payload installation;
- previous-package install, injected failed-upgrade rollback and current upgrade;
- every installed payload hash and source manifest;
- installed CLI version, offline configuration parsing and durable initialization;
- actual WinFsp Git save/replace/rename/delete and interrupted-owner cold recovery
  using the MSI-installed CLI against the owned loopback fixture;
- repair, repeated install, downgrade refusal and reinstall;
- uninstall even when the independent driver was removed;
- preservation of configuration, a database, binary dirty-data sentinel and an
  unknown user file inside the installation directory;
- absence of the owned executable and MSI registration after removal.

Existing full Linux/Windows/native/recovery/soak/compatibility/book gates remain
required. Portable Python tests check packaging/refusal rules but cannot replace
native MSI or filesystem tests. No archived installer or raw private test directory
is uploaded; hosted logs expose only synthetic fixture evidence and digests.

Before claiming real-use acceptance, separately verify interactive installation,
UAC/cancel/files-in-use behavior, WinFsp version and mount readiness, sign-in and
physical reboot, Office open/save/replace/rename/delete, actual storage durability,
and the bounded provider plan. Test on disposable files and record exact versions.

## Build and licensing notes

Build scripts use pinned WiX 4.0.6 and its matching UI extension from the official
NuGet registry. Tooling is pinned for the private reproducible recipe, not a claim
of current upstream support; re-review builder lifecycle/security before public
release. MSI bytes themselves are not claimed reproducible across builds.
The build refuses a dirty tracked checkout, unsafe notice sources, missing license
metadata/files and a mismatched binary/tool version. Notice inventories and hashes
are included; package-level automated collection still needs a distribution review.
S3Drive-owned source is GPL version 3 only. The locked `winfsp`/`winfsp-sys` Rust bindings
declare GPL-3.0 separately from the driver. Public redistribution remains held for compliance
review, including source obligations. All dependencies retain their own licenses.

Primary references reviewed 2026-10-27:
[WiX deferred failure injection](https://docs.firegiant.com/wix/tools/wixext/util/),
[WiX major upgrades](https://docs.firegiant.com/wix/schema/wxs/majorupgrade/),
[registry search](https://docs.firegiant.com/wix/schema/wxs/registrysearch/),
[WixUI](https://docs.firegiant.com/wix/tools/wixext/wixui/), and
[Windows Installer reboot control](https://learn.microsoft.com/en-us/windows/win32/msi/reboot).

## Exact candidate evidence (2026-10-27)

Source `1593ac9508a9d583da000c63a7ac040256703815`, tree
`44cc76b008cdda67a4efb05ec2e072d2ec60be2c`, passed all ten jobs in
[run40553716413](https://github.com/marshallr12/s3drive/actions/runs/40553716413).
Review covered all ten full logs, 835 exact source blobs and all six artifact
digests/reports, including schema46–50 compatibility and 35 Windows reference
observations. Workspace totals were Linux1143 / Windows1135; original and
extended native gates were retained unchanged.

The installer job proved missing-WinFsp refusal, installation, executed deferred
failure followed by rollback, normal upgrade, repair, repeat installation,
downgrade refusal, uninstall without WinFsp, reinstall and final uninstall.
User configuration, unsaved bytes, a real SQLite database and an unknown file in
the install directory remained unchanged. The MSI-installed executable passed
native Git save/replace/rename/delete and interrupted cold remount in16.22s.
The prior-version MSI uses the same binary, not a historical released binary.
The production MSI has no injected failure action. Its private test digest was
`0163d8266e4f075fe83cb72f7d2614cb79b2eaba25f785f8ecb1d7d5d677184c`;
no binary artifact was published.

Original shared/debug/release300-save times were617.49/769.91/158.88s on Linux
and546.31/855.22/303.56s on Windows. Windows debug retained344.78s below its
unchanged1200s limit; this is not a hardware-independent performance promise.
Both OSes passed400 component cycles and200 cold SQLite integrity checks.
Additional1000-save/four-file64KiB300-save workloads were469.54/280.69s on Linux
and1175.29/566.77s on Windows.

Earlier installer attempts are not accepted evidence:40545722462 exposed missing
upstream crate notices,40548138606 exposed Windows checkout newline conversion
of pinned notice bytes, and40552374768 exposed a duplicate WiX UI property.
Pinned upstream notices, binary-preserving attributes and single ownership of
the WiX property corrected these failures; no acceptance assertion was waived.

This documentation-only successor does not change runtime, installer authoring,
tests or CI. Its exact-source CI and any subsequent merge/main verification are
separate from the accepted candidate evidence above. Real-machine GUI/UAC,
files-in-use, Office, reboot, live-provider, signing and public redistribution
acceptance remain incomplete. GPL selection does not waive dependency/source
obligations or authorize a public binary release.
