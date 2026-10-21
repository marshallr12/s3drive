# Event-bound manifest authority

Research, design and acceptance date: 2026-10-21. Based on accepted main378c569;
the authority phase is accepted at schema46. No new native callback is enabled.

This first phase of the [sparse unlink plan](owned-unlinked-sparse-plan.md) extends
the Rust metadata authority. It extends the active `RemovalClaim` concept with `RemovalPublicationClaim`
(original owner/intent, current event operation ID and explicit publication mode)
as optional boxed `ContentGuard.removal_claim`.
The claim must match the guard's original `removal` fence and refer to a manifest.
Deferred/pending claims require an attached node. A committed claim requires a
deleted stable node and that exact committed event; cancellation has no mode. The new field is omitted when absent, preserving legacy JSON,
hashes and behavior. Legacy fence-only operations do not gain event guarantees.

Fresh reservation and publication must match the exact current event/mode,
original fence, expected payload and node revision. A transition away and back cannot revive a stale event paired with a current
node revision and unchanged payload.
`RemovalEpoch` is a new read-only authority request/reply that returns the node
and current claim in one read transaction. It does not authorize a native handle.
The server advertises `event-fenced-manifest-v1`; callers must require this before
new event-bound work. Saved exact requests/results retain idempotent historical
replay after transition, cancellation, replacement or restart.

Schema46 is a marker-only change, with no new DDL. A raw pre46 check
scans fixed bounded JSON metadata columns for field presence, including null,
before a temporary marker change. It never interprets user file bytes as metadata.
Genuine legacy records must upgrade byte-for-byte; every failed check rolls back.
An actual retained schema45 binary must refuse46 without changing the database.

Required acceptance: both modes, Unicode names, stale events paired with current node
revisions, cancellation and successor epochs, competing clients, lost replies,
immutable history/corruption, real worker termination, actual SQLite full, quota
rollback, old-shape bytes, pre46 marker laundering and genuine upgrade. Complete
gate commands and exact-source Linux/Windows/native regression evidence remain
separate requirements. No new SDK sparse-publisher call site or native lifecycle
flag is enabled by this authority phase. No S3 writes or provider claim occurs.

Authority implementation and verification are complete for this phase. The next
phases must bind sparse local preparation, physical claims and source settlement
to this event and preserve node-wide unresolved-effect barriers before final
removal. Existing whole-generation limits are not a substitute for that work.

## Historical workstation transfer checkpoint

2026-10-21: all 15 focused state tests and the authenticated loopback HTTP race,
lost-reply and restart test pass. State coverage includes eight real worker kills,
SQLiteFull rollback/retry, marker43/44/45 laundering refusal, unchanged legacy
bytes and genuine45 upgrade. Workspace/all-target Clippy with warnings denied
passes. Earlier compiler and test-assumption failures are retained in the
[portable development archive](evidence/manifest-event-checkpoint-20261021.tar.gz),
alongside successful runs and a per-file runtime digest map. Tests used owned
local fixtures; this does not establish S3, mounted-drive or physical Windows
behavior. At transfer, full gate, actual old45/new46 binary compatibility,
exact-source hosted/native regressions and phase acceptance remained pending.
The acceptance record below supersedes that checkpoint status.

The [cleanup audit](evidence/workstation-cleanup-20261021.tar.gz) records the
user-requested retirement of obsolete worktrees and branches. Main and this
active candidate remain; a dirty historical experiment and its references are
preserved. Deleted committed history also has a verified local recovery bundle,
whose path and digest are recorded in HANDOFF.md. The bundle is not in Git.

## Acceptance on suitable hosts

[Own CI 39650193633](https://github.com/marshallr12/s3drive/actions/runs/39650193633)
completed successfully on Linux, Windows and book jobs at source
`0eacebaac719b97df56ecf64f6e75b9ed101ba59`. This empty CI-trigger commit has the
same entire Git tree as transfer source `a6e91434`: all 751 downloaded source
files were compared byte-for-byte, and all 384 runtime/configuration/test inputs
remained unchanged throughout validation. No runtime repair or weakened guard
was needed. The production diff and all new tests were reviewed.

The workspace suites contain 37 cohorts each: Linux 921 passed/0 failed/15
ignored; Windows 917 passed/0 failed/31 ignored. Both include all 15 event-manifest
state tests (including the subprocess helper) and the authenticated HTTP case.
Eight real worker terminations, actual SQLiteFull, quota rollback, corruption,
stale/current event combinations, historical replay and raw pre46 checks pass.
Explicit native jobs separately pass FUSE/WinFsp Unicode reads/writes, sparse
chunk I/O, remount, callback-stack, TLS, coordinator and paired startup/recovery.
The ordinary ignored entries include separately selected native cases, fixture
subprocesses and the diagnostic retained-history study, not waived failures.

Both Windows observation ZIP digests were checked against GitHub's artifact
metadata. Every raw record in the full log equals its artifact record, and all
21 replacement plus 14 disposition records match committed fixtures with exact
field sets, JSON types and values. Source commit, Windows Server 2025, pinned
WinFsp version and all four upstream legacy selections were checked. Full
decoded job logs were retained and reviewed. All slow-test notices resolved.
Reviewed warnings are the existing WinFsp no-import delay-load linker warning,
GitHub action/runtime deprecations and three guarded disposable-VM profile-lock
deferrals. Dummy credentials were removed; other cleanup errors throw before
PASS. Intentional TLS/authorization/conflict failures remain visible in logs.

Actual CLI compatibility used a separately built schema45 executable from
`378c569b0b23b518db6f4e0404572261905dd702` and the unchanged46 candidate.
Eight invocations prove fresh45/fresh46 headers, old45 refusal of fresh46 and
upgraded46 without changing database/sidecar bytes, and 46 reopening both.
A genuine45-created database with a Unicode legacy cache row upgraded with an
identical complete schema/table-data dump. Executable SHA256 values:

- 45: `df752e691c184d6ae0e785a063229a4c0e2227604e9daee7bb84fd379e1c2f0f`
- 46: `e8d886001b1b3fafdfb97e3283db8502077d4c661b464e48d0f402bbe286a7d7`

The cloud workstation's full `bash scripts/gate.sh` did **not** pass: its
workspace is overlayfs and its temporary storage is tmpfs. The gate stopped in
authority fixtures after schema/fmt/Clippy passed. A no-fail-fast continuation
retained 205 passes/716 failures/15 ignored across 37 cohorts; an actual accepted45
control reproduced the authority initialization failure on the same host.
Local build, links, book build/test and metadata-only executable compatibility
passed. Tmpfs compatibility fixtures do not prove durable staging or power-loss
behavior. No failure was erased or individually presumed harmless.

The user explicitly requested standard GitHub Actions when the workstation is
unsuitable. Every command in `scripts/gate.sh` passes on those suitable hosts,
with the separate native gates above. This is the accepted equivalent full
gate, not a claim that the workstation gate passed. Local logs, controls and
fixtures are retained; the local evidence archive remains unpublished as
requested. No real Wasabi/provider write, user-computer change, user startup install
or new sparse SDK/native lifecycle claim is included in this acceptance.
