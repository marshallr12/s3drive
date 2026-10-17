# Deferred replacement reference

**Research and measurement phase, 2026-10-17.** Session recovery is accepted at
`27697fd`, [CI39059948714](https://github.com/marshallr12/s3drive/actions/runs/39059948714).
Before integrating native deletion, measure replacement of an open destination
with ordinary, deferred, pending, cleared and duplicated delete-on-close handles.
Old target cleanup must not delete a replacement source identity.

The WinFsp 2.1 header describes legacy kernel restrictions on replacement of open
targets. Its upstream tests distinguish legacy and POSIX rename support. Those
sources do not establish deferred-target behavior or NTFS parity for this app;
run explicit measurements with the selected runtime before choosing the bridge
policy. Sources checked 2026-10-17:
[WinFsp API](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/inc/winfsp/winfsp.h),
[upstream rename tests](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/tst/winfsp-tests/info-test.c).

Seven probes run on native NTFS and on a disposable in-memory WinFsp host using
the resolved Rust wrapper and explicit legacy/POSIX capability profiles. The reference host permits
file replacement when Rename reaches user mode, retains old contexts by stable
identity, and records whether Rename was forwarded. It is not S3Drive, a
production filesystem, or evidence that the S3Drive bridge is enabled. The
observations include pending flags, Win32 errors, old-context late I/O and bytes
at the destination after old-target cleanup and final close. Only owned temporary
paths are used; no S3 client/provider or workstation setup is touched.

Initial measurement records actual outcomes without inventing a golden fixture.
Download the exact-source artifact, review all typed fields and the actual
Windows/runtime versions, freeze it, and require strict fixture comparison plus
malformed-data rejection checks in a new exact-tip hosted run before publication.
Keep the original fourteen NTFS/four pinned-upstream/native/startup gates intact.
Then use the measured legacy contract for the full native lifecycle bridge;
do not replace it with SetDelete-only support or permanent application refusals.

## Three-profile measurement (2026-10-17)

Source `0ba71c6`, [measurement run39062561017](https://github.com/marshallr12/s3drive/actions/runs/39062561017),
artifact12112350631, observed19:30:42UTC on Windows Server2025 Datacenter and
WinFsp `2.1.25156.ddca7bd`. All original fourteen typed records matched unchanged.
The [frozen twenty-one records](evidence/windows-replacement-reference-20261017.json)
retain every observation and original provenance; outcomes are not invented from
the API header. The POSIX profile advertises the capability but uses the same
ordinary Win32 rename calls as the other profiles.

| Destination at replacement | NTFS | WinFsp legacy | WinFsp POSIX capability |
|---|---|---|---|
| Closed target | Replaced | Replaced; callback reached | Replaced; callback reached |
| Ordinary open target | Replaced | Refused before callback | Replaced; callback reached |
| Deferred target, including cleared/duplicate handles | Replaced | Refused before callback | Replaced; callback reached |
| Pending target | Refused, error5 | Refused before callback | Replaced; callback reached |

Successful replacement preserved the new source after old-target cleanup and
allowed late I/O through retained old-target handles. When both source and target
had DoC handles, closing the source's handle removed the new destination on NTFS
and the POSIX reference host. Pending-state observations also differ; the POSIX
reference host is not identical to NTFS. Neither profile is enabled in S3Drive
by these tests. Choosing the POSIX profile requires atomic displacement of owned
deferred/pending targets, correct early deletion while other handles remain,
stable-ID cleanup, retained I/O and extended-disposition acceptance. Do not select
legacy merely to hide the missing replacement behavior, or claim parity from
this disposable host. Require strict replay of all21 records' fields and malformed-data
checker tests in the final exact-source CI before publication.


## Initial measurement (2026-10-17)

Source `8abc469`, [measurement CI39062113865](https://github.com/marshallr12/s3drive/actions/runs/39062113865),
Windows Server2025 Datacenter, WinFsp `2.1.25156.ddca7bd`, observed19:25:26UTC.
The [original fourteen records](evidence/windows-replacement-legacy-measurement-20261017.json)
are retained unchanged. NTFS replaced ordinary/deferred/cleared/duplicated open
targets and preserved old-target I/O; old-target cleanup did not remove the new
source. Legacy WinFsp rejected every open-target replacement with Win32 error5
before Rename reached the reference host. A closed-target control succeeded.
A pending target refused replacement on both backends. With both files deferred,
NTFS removed the new destination when the source's DoC handle finally closed;
legacy WinFsp had refused the move and subsequently removed the original names.
These are reference observations, not S3Drive native deletion acceptance.

This finding required measuring the same seven cases with the pinned runtime's
POSIX capability enabled, as recorded above. Advertising the capability is not enough: immediate unlink, old-context I/O and extended disposition
must be implemented and natively tested before enabling that profile in S3Drive.


## Local verification (2026-10-17)

The final repository gate passed: 241 Linux state tests, nine authority HTTP,
sixteen SDK cases, formatting, Clippy, builds and book checks. The replacement
oracle accepts the measured21 records and rejects twelve malformed variants.
Required CI now uses strict fixture comparison, not measurement mode. Schema21
also prepares void-callback capture/reconciliation without enabling a native
bridge. SDK tests preserve five intentional immutable PUTs, including late data
after non-deleting cleanup, HTTP-unavailable callback facts, queued deletion
priority and exact owner recovery. Schema20 migration preserves original hashes;
a missing-cleanup Close remains missing in separate abandonment proof.

Require terminal exact-tip all-three hosted native/reference/startup acceptance
before private publication. Full native bridge/profile support remains work.
