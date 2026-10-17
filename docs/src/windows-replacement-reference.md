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
the resolved Rust wrapper and explicit legacy mode. The reference host permits
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

Measure the same seven cases with the pinned runtime's POSIX capability enabled
before selecting the native bridge profile. Do not assume that advertising the
capability is enough: immediate unlink, old-context I/O and extended disposition
must be implemented and natively tested before enabling that profile in S3Drive.
