# Windows installer

## Status and scope (2026-10-27)

The new x64 MSI is **under hosted validation**, not an accepted or published
release yet. It installs the experimental command-line application, session
helper, documentation, MIT license and collected dependency notices. S3Drive is
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
collects source notices and builds current/prior-fixture MSIs. On its disposable
Windows runner it tests:

- missing WinFsp refusal without payload installation;
- previous-package install and current-package upgrade;
- every installed payload hash and source manifest;
- installed CLI version, offline configuration parsing and durable initialization;
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
S3Drive is MIT; WinFsp and all dependencies retain their own licenses.

Primary references reviewed 2026-10-27:
[WiX major upgrades](https://docs.firegiant.com/wix/schema/wxs/majorupgrade/),
[registry search](https://docs.firegiant.com/wix/schema/wxs/registrysearch/),
[WixUI](https://docs.firegiant.com/wix/tools/wixext/wixui/), and
[Windows Installer reboot control](https://learn.microsoft.com/en-us/windows/win32/msi/reboot).
