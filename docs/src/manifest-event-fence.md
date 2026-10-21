# Event-bound manifest authority candidate

Research and design date: 2026-10-21. Candidate based on accepted main378c569;
not accepted, installed or enabled in native callbacks.

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

Schema46 is proposed as a marker-only change, with no new DDL. A raw pre46 check
scans fixed bounded JSON metadata columns for field presence, including null,
before a temporary marker change. It never interprets user file bytes as metadata.
Genuine legacy records must upgrade byte-for-byte; every failed check rolls back.
An actual retained schema45 binary must refuse46 without changing the database.

Required acceptance: both modes, Unicode names, stale events paired with current node
revisions, cancellation and successor epochs, competing clients, lost replies,
immutable history/corruption, real worker termination, actual SQLite full, quota
rollback, old-shape bytes, pre46 marker laundering and genuine upgrade. Complete
local gate and exact-source Linux/Windows/native regression evidence remain
separate requirements. No new SDK sparse-publisher call site or native lifecycle
flag is enabled by this authority phase. No S3 writes or provider claim occurs.

Authority implementation and verification are in progress. The next
phases must bind sparse local preparation, physical claims and source settlement
to this event and preserve node-wide unresolved-effect barriers before final
removal. Existing whole-generation limits are not a substitute for that work.

## Workstation transfer checkpoint

2026-10-21: all 15 focused state tests and the authenticated loopback HTTP race,
lost-reply and restart test pass. State coverage includes eight real worker kills,
SQLiteFull rollback/retry, marker43/44/45 laundering refusal, unchanged legacy
bytes and genuine45 upgrade. Workspace/all-target Clippy with warnings denied
passes. Earlier compiler and test-assumption failures are retained in the
[portable development archive](evidence/manifest-event-checkpoint-20261021.tar.gz),
alongside successful runs and a per-file runtime digest map. Tests used owned
local fixtures; this does not establish S3, mounted-drive or physical Windows
behavior. Full local gate, actual old45/new46 binary compatibility, exact-source
hosted/native regressions and phase acceptance remain pending. Continue on
`codex/manifest-event-fence`; accepted main378c569 remains unchanged.

The [cleanup audit](evidence/workstation-cleanup-20261021.tar.gz) records the
user-requested retirement of obsolete worktrees and branches. Main and this
active candidate remain; a dirty historical experiment and its references are
preserved. Deleted committed history also has a verified local recovery bundle,
whose path and digest are recorded in HANDOFF.md. The bundle is not in Git.
