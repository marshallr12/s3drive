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

## Measured NTFS results, 2026-10-17

The reference step at `53f09b1a832cbdebc54d9e0c3d9e318e8f5b8004`,
[run 39041403340](https://github.com/marshallr12/s3drive/actions/runs/39041403340),
produced a source-bound artifact on **Microsoft Windows Server 2025 Datacenter**
at 14:41:31 UTC. All eight previous records matched. The
[frozen fourteen-case fixture](evidence/windows-disposition-reference-20261017.json)
contains every observed scalar and preserves the original eight-case provenance.
The four upstream legacy cases also passed before the artifact was written.
These are reference-step results; publication additionally requires the final
exact-commit workflow and unchanged native/startup acceptance.

| Case | Observed result |
| --- | --- |
| Deferred file rename | Rename succeeded before cleanup. Old name disappeared and the destination existed. Cleanup made the retained handle pending and new opens failed with error 5; retained I/O worked, and the destination disappeared after the final close. |
| Deferred nonempty directory rename | Rename succeeded without pending state. The child data survived cleanup at the destination; the old name remained absent. |
| Exclusive new file with delete-on-close | Create succeeded and another open was initially allowed. Cleanup made that reader pending and denied new opens with error 5. Existing data stayed readable; the file disappeared after all closes. |
| Multiple delete-on-close opens | First cleanup set pending. Ordinary cancellation through the second open cleared pending and allowed reopening. Its later cleanup reasserted pending and denied reopening; retained I/O worked and the final close removed the file. |
| Extended on-close cancellation | The NTFS call succeeded. Cleanup did not set pending; reopening succeeded and the file remained after all closes. This does not override WinFsp's documented ON_CLOSE rejection in the selected mode. |
| Duplicate handle | Closing the first alias did not set pending and reopening worked. Closing the duplicated alias triggered pending and denied reopening with error 5. The independent reader retained its bytes until its final close removed the file. |

Implementation consequences are **inferences and next requirements**. Deferred
rename must retain stable node/fence identity while changing namespace placement;
cleanup must follow the node instead of deleting its old pathname. Create and
deferred reservation must be atomic. A nonempty renamed directory must preserve
its child. Track an open file object separately from node-wide disposition and
duplicated aliases; ordinary cancellation cannot erase another live DoC
obligation. No observed NTFS result establishes WinFsp deferred parity.

The final checker requires fourteen unique cases, the measured OS profile and
every exact observation field. Its portable PowerShell test rejects nine
malformed observations, including missing/duplicate/unknown cases, wrong
backend/OS, extra/renamed fields and changed null/boolean values. The temporary
eight-golden/six-measured allowance is removed. Final hosted CI must repeat all
fourteen observations before this checkpoint can be published.

Next prioritize atomic deferred create and same-fence owner rename/move, then
the native bridge. Before enabling replacement of a deferred target, also prove
that its old cleanup cannot delete the replacement's name, including both
source and target being deferred. This is additional acceptance work, not
permission to leave ordinary native rename or DoC permanently unsupported.


Local final gate passed (exit 0) with 173 state tests, seven authority HTTP
tests and 11 SDK cases plus formatting, lint, build and book checks. PowerShell
syntax passed for all three scripts. The checker accepted the actual measured
artifact and rejected all nine malformed observations. The first measurement
run completed all-green; final publication requires exact-tip CI repeating the
now-frozen fourteen-case check, four upstream cases and native/startup gates.
