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
