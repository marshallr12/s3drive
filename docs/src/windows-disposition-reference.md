# Windows deletion reference contract

**Research and acceptance probes, 2026-10-17.** This phase verifies the operating
system behavior needed before designing deferred reservations. It does not
enable S3Drive deletion or treat a reference filesystem as S3 acceptance.

![Native callback lifetime is separate from the metadata reservation state. Deferred reservations and the native bridge are proposed; pending-owner publication and tombstones are implemented foundations.](images/disposition-reference.svg)

## Evidence that changes the design

WinFsp 2.1's upstream `create_test` deliberately opens a **nonempty** directory
with delete-on-close, closes that handle, then opens and deletes its child. It
subsequently opens/closes the empty directory with that flag and expects the
directory to disappear. The test has NTFS and WinFsp disk/network modes.
[Pinned reference test](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/tst/winfsp-tests/create-test.c),
lines 122–151 and 202–213; checked 2026-10-17. Our inference: the existing
ArmRemoval emptiness check and denial of new opens cannot represent the entire
delete-on-close handle lifetime. A distinct earlier state is needed if our
native reference checks confirm this behavior.

The pinned driver reasserts node-wide pending state during cleanup of a
delete-on-close descriptor, then uses remaining handle count to choose deletion.
Cleanup completion decrements that count; Close is separate.
[Driver lifecycle](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/src/sys/file.c),
lines 745–783 and 832–939; checked 2026-10-17. Do not infer shared disposition by
OR-ing local handle flags, or assume the deleting cleanup uses the arming handle.

Extended disposition requires the driver's POSIX support flag, and WinFsp 2.1
explicitly refuses FILE_DISPOSITION_ON_CLOSE. That is separate from the ordinary
open-time delete-on-close flag.
[Disposition implementation](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/src/sys/fileinfo.c),
lines 1342–1393; checked 2026-10-17. S3Drive currently does not advertise POSIX
unlink/rename, so an extended-flags workaround is not the chosen solution.

## Explicit native reference gate

`scripts/test-windows-disposition-reference.ps1` runs on the prepared disposable
Windows CI VM, using the already digest-pinned WinFsp 2.1 test archive. It first
runs eight Rust NTFS reference probes from the audited native test boundary,
then the upstream `create_test` on its own WinFsp disk/network memfs backends in
legacy unlink/rename mode. The CLI flag is mapped into memfs configuration by
[the pinned test harness](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/tst/winfsp-tests/memfs-test.c),
lines 29–53; [default modes and argument handling](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/tst/winfsp-tests/winfsp-tests.c),
checked 2026-10-17. No service, credentials or real S3 objects are changed.

Rust probes cover repeated true/false disposition, deletion at a nonarming
final handle, late retained-handle I/O, deferred second opens, nonempty versus
empty directory cleanup, and ordinary nonempty deletion refusal. Three probes
record unresolved observations rather than inventing expectations: cancellation
through another context, legacy cancellation of delete-on-close, and new child
creation during a deferred directory lifetime. They emit structured
`S3DRIVE_DISPOSITION_REFERENCE` JSON in CI logs. Those observations must be
reviewed before specifying their authority transitions.

All paths are owned temporary fixtures. Linux compiles the portable workspace
but does not execute these Windows probes. A missing runtime, missing selected
upstream test, failure, or timeout fails acceptance. The eight reference probes
are separate from S3Drive's seven real native cases on each OS.

## Deferred protocol plan, not implemented

After native observations are reviewed, specify an operational reservation
which protects stable identity/name while allowing the required open sharing.
Determine child-creation and nonempty final-cleanup behavior from evidence.
Keep pending-open denial as an explicit later transition. Preserve the accepted
owner-fenced content publication and retained-conflict paths.

Promotion must preserve the fence and bind immutable transition evidence.
Current pending-row verification treats revision advancement without payload
change as corruption; add a proper transition proof rather than weaken that
check. Cancellation and commit must use exact owner, node, parent and payload
guards. Unknown cancellation cannot trigger automatic re-arming. Missing
cleanup during volume teardown must remain inspectable retained uncertainty.

Next add the authority protocol, then persistent per-descriptor and node-wide
client lifecycle state, diagnostics and recovery. Native callbacks wait for
normal/deferred opens, cancellation, multiple contexts, late I/O, volume loss,
Unicode and concurrent-client acceptance. Windows Server CI is not Windows
10/11 desktop reboot acceptance; actual workstation startup remains separate.
