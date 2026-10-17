# Deferred create, rename and handle reference

**Research and measurement plan, 2026-10-17.** Cancellation recovery is accepted
at `0c8f6c7`, [CI 39039582208](https://github.com/marshallr12/s3drive/actions/runs/39039582208).
Before implementing atomic deferred create and owner-fenced moves, extend the
[existing Windows reference](windows-disposition-reference.md) with six probes.
No production callback, schema, configuration or real provider changes belong
to this reference phase.

| Case | Observation needed |
| --- | --- |
| Deferred file rename | Rename before cleanup, pending state, retained binary I/O, final source/destination lifetime |
| Deferred nonempty directory rename | Renamed child path/data and directory lifetime after cleanup |
| Exclusive new file with delete-on-close | Create and reserve must be one authority operation; measure sharing and cleanup |
| Multiple delete-on-close opens | Cancellation between two cleanups, repeated pending state and retained final I/O |
| Extended on-close cancellation | Record native HRESULT and lifetime separately from legacy cancellation |
| Duplicate handle | Distinguish a duplicated handle from an independent open file object |

WinFsp documents cleanup per kernel file object; duplicated handles share that
object. Operations may still arrive between cleanup and close, and cleanup
cannot report failure. The durable lifecycle must preserve that distinction.
[WinFsp API](https://winfsp.dev/doc/WinFsp-API-winfsp.h/), checked 2026-10-17.

Microsoft's extended disposition structure includes an ON_CLOSE flag to set or
clear the on-close state. That does not establish support in our selected
WinFsp legacy mode, which the previous reference chapter explicitly excludes.
This probe records NTFS behavior only; it is not a workaround for the driver.
[Microsoft extended disposition](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-_file_disposition_information_ex),
checked 2026-10-17. Inspect the resolved windows 0.61.3 structure/flag/function
before adding its synchronous test-only FFI call under the native audit boundary.

The pinned upstream suite also has ordinary `rename_test`, `rename_open_test`
and `delete_pending_test` with disk/network memfs variants. Run each explicitly,
alongside the already accepted `create_test`, and fail on a missing selector,
nonzero status or timeout. Those cases do not prove deferred rename parity.
[Pinned info tests](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/tst/winfsp-tests/info-test.c),
checked 2026-10-17.

First measure all fourteen NTFS cases while checking the eight existing golden
records exactly. The six additions have explicit case identities but no guessed
values. Review the source-bound JSON artifact, then freeze every observed field
and require all fourteen records in the final acceptance script. A measurement
run alone cannot complete this phase. Artifact upload uses a pinned action and
retains only observations, source commit, OS, time and upstream test names.
[Artifact action contract](https://github.com/actions/upload-artifact/blob/ea165f8d65b6e75b540449e92b4886f43607fa02/README.md),
checked 2026-10-17.

Acceptance additionally requires the repository gate and terminal exact-tip
Linux/Windows/book CI with the existing seven actual S3Drive native cases per
OS and startup gates unchanged. No NTFS trace or upstream memfs result enables
S3Drive deletion. Then implement atomic deferred create/owner-fenced move and
the persistent per-open/node-wide lifecycle, followed by native deletion tests.
