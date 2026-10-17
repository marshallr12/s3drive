# Windows deletion reference contract

**Research and acceptance probes, 2026-10-17.** This phase verifies the operating
system behavior needed before designing deferred reservations. It does not
enable S3Drive deletion or treat a reference filesystem as S3 acceptance.

![Native callback lifetime is separate from the metadata reservation state. Authority reservations, pending-owner publication and tombstones are implemented foundations; the native deletion bridge is proposed.](images/disposition-reference.svg)

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

## Initial eight-case reference gate

The initial `scripts/test-windows-disposition-reference.ps1` gate ran on the
prepared disposable Windows CI VM, using the digest-pinned WinFsp 2.1 test
archive. It ran eight Rust NTFS reference probes from the audited native test
boundary, then the upstream `create_test` on its own WinFsp disk/network memfs backends in
legacy unlink/rename mode. The CLI flag is mapped into memfs configuration by
[the pinned test harness](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/tst/winfsp-tests/memfs-test.c),
lines 29–53; [default modes and argument handling](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/tst/winfsp-tests/winfsp-tests.c),
checked 2026-10-17. No service, credentials or real S3 objects are changed.

Rust probes cover repeated true/false disposition, deletion at a nonarming
final handle, late retained-handle I/O, deferred second opens, nonempty versus
empty directory cleanup, and ordinary nonempty deletion refusal. Three probes
initially recorded observations rather than inventing expectations: cancellation
through another context, legacy cancellation of delete-on-close, and new child
creation during a deferred directory lifetime. They emit structured
`S3DRIVE_DISPOSITION_REFERENCE` JSON in CI logs. The measured NTFS fixture below now pins those results; WinFsp equivalence
still needs independent evidence before its authority transitions are fixed.

All paths are owned temporary fixtures. Linux compiles the portable workspace
but does not execute these Windows probes. A missing runtime, missing selected
upstream test, failure, or timeout fails acceptance. The eight reference probes
are separate from S3Drive's seven real native cases on each OS.

## Protocol consequences and remaining native work

These observations motivated the implemented [deferred authority](deferred-removal.md)
and [cancellation recovery](mode-cancellation-recovery.md). Deferred ownership
allows reads and child operations, while pending-open denial is a later exact
same-fence transition. Schema 16 binds immutable transition evidence instead
of weakening payload/identity corruption checks. Unknown cancellation cannot
trigger automatic re-arming; dirty data and uncertain work remain inspectable.

The [additional reference phase](windows-deferred-reference.md) extends the
gate to fourteen frozen NTFS cases and four selected upstream legacy cases.
Next implement atomic deferred create and owner-fenced rename/move, then the
persistent per-open and node-wide native lifecycle, diagnostics and recovery.
Native callbacks still wait for normal/deferred opens, cancellation, multiple
contexts, late I/O, volume loss, Unicode and concurrent-client acceptance.
Windows Server CI is not Windows 10/11 desktop reboot acceptance; actual
workstation startup remains separate.


## Observed NTFS results (2026-10-17)

The first reference run `6e5fef7`,
[CI 39032499844](https://github.com/marshallr12/s3drive/actions/runs/39032499844),
passed on Windows Server 2025. Its [structured trace](evidence/windows-disposition-reference-20261017.json)
is retained with the six later cases in the expanded reference fixture; the
gate compares every field before
accepting a later run. These are measured NTFS results, not a claim that WinFsp
matches every cross-context detail.

| Case | Observed result |
| --- | --- |
| Deferred file open | Both handles report no pending deletion before cleanup; another open succeeds. Closing the DoC handle makes the retained handle pending and new opens fail with Win32 error 5. |
| Nonarming final handle | Retained read/write/flush succeeds; the file is absent after its final close. |
| Nonempty deferred directory | Open/close succeeds, preserving its child. A later empty-directory DoC cleanup removes it. |
| Child creation while deferred | Child creation succeeds; the nonempty directory and child remain after cleanup. |
| Cross-context ordinary cancellation | Both handles report not pending, reopening succeeds before and after arming-handle cleanup, and the file remains after both close. |
| Legacy cancellation of DoC | It reports not pending before cleanup, but cleanup reasserts pending; reopening fails and final close removes the file. |
| Ordinary nonempty disposition | Refused with HRESULT 0x80070091; child bytes remain. |
| Repeated true/false | Repeated cancellation succeeds and bytes remain after close. |

The pinned WinFsp legacy disk/network create_test also passed, confirming the
nonempty deferred-directory behavior on those reference backends. The driver
sources distinguish shared pending state and per-file-object flags; do not
extrapolate NTFS query results to every WinFsp handle. The [additional NTFS results](windows-deferred-reference.md#measured-ntfs-results-2026-10-17)
now cover deferred rename, new-file DoC, multiple DoC opens and duplicate aliases.
Deferred replacement and WinFsp parity still require acceptance before the full
native bridge. The authority must support these operations rather than
permanently freeze names as a shortcut.
