# Clone-sharing source reader guards

Current status,2026-10-19: this component is included in the accepted combined
schema37 runtime. See [own full/platform acceptance](managed-reader-guards.md).
The declarations and component-only evidence below retain their historical scope.


Declared 2026-10-19. Proposed component phase from the isolated, component-tested
read-pin registry. Its schema37 parent is not accepted for promotion. This phase
owns the state worker, a new guard module, the sparse client constructor, focused
lifetime/cancellation tests and this contract. It does not activate mounted writes,
reclamation, or managed-reader integration.

`Client::guard(view)` returns an opaque clone-sharing `ReadGuard`. All clones
retain one exact local view and one registry pin until the last clone is dropped.
The guard exposes its immutable view and pinned local reads, not its manual token
or a method that closes other clones. The worker creates the complete guard before
sending the reply. A canceled or lost reply therefore drops a real guard and
schedules cleanup even when acquisition committed. No SQL transaction lasts across
the reply or any network request; this capability authorizes local retention only.

Last-drop cleanup must not rely on a best-effort command alone: the existing
worker queue holds eight commands. Maintain a same-owner bounded pending-release
map keyed by unique pin IDs, with at most256 actual active pins. Drop records the
release before attempting a nonblocking wake. The worker drains pending releases
before/after commands and before closing; a full command queue therefore cannot
lose a release. The pending-map lock is never held during SQL. Failed release
retains the entry, does not delete source data and does not spin or spawn tasks;
subsequent work retries it. Owner replacement retires old transient sessions using
the existing exclusive ownership rule, never a wall-clock timeout. No guard
exposes a manual token that could prematurely release a shared pin.

Done criteria: clone survival and exact Unicode reads; last-drop release while
idle and under all eight queued commands; canceled acquisition both before work
and after commit; repeated lifetime opens without tombstone growth; full256-pin
admission and recovery after dropping; cleanup failure preserving the registry and
retry after the fault is removed; close/replacement and stale-owner isolation.
Focused checks precede the full gate after parent integration. Do not promote any
source here using another branch's platform artifacts.

Managed `EditedFile` integration remains a separate required step. This component
alone does not make existing raw reads safe for garbage collection.

## Focused verification (2026-10-19)

Six lifetime/queue/cancellation/fault cases passed in38.18s at normal four test
threads. They cover exact Unicode pieces, a surviving clone, idle last-drop
cleanup without another command, all eight queue slots full, cancellation before
execution and after an unread committed reply, a trigger preventing cleanup,
retry after removing that trigger,256 simultaneous pins and556 lifetime opens,
and a late drop after close/replacement. A full queue remains an explicit
admission error for ordinary commands; only the test's read-only observation
retries QueueFull, never an unknown mutation.

Earlier attempts retained their exact test sources/logs: a SQL count used a
non-supported usize result type; the fixtures passed the temporary parent rather
than a private managed child directory; and the quota observation did not allow
for the expected full queue of cleanup wakes. Fixes preserved database/private
mode checks and all lifetime assertions. Clippy then required expect_err in one
test; no diagnostic suppression was added. Broad regression and platform evidence
remain pending. The registry and worker perform no byte reclamation.

## Component regression evidence

Checkpoint eef4da2, schema37, passed480 state tests with one diagnostic ignored
in266.78s. All336 frozen working and committed runtime inputs matched. State
all-target Clippy passed15.35s with warnings denied. This is component evidence,
not the complete workspace gate or native/platform acceptance.

The [indexed component archive](evidence/source-read-guards-component-20261019.tar.gz)
contains358 verified members and927175bytes: exact336-input source, logs, runner,
result, source maps and retained failed test variants. SHA-256:
`627cf14ad2f0bf8397a783e41a7a7c709aa8e1c01e949745e83c18755be1ba76`.
[Dated evidence](evidence/source-read-guards-20261019.json) records the scope.
The one-use archiver is complete and must not be rerun. The parent is unaccepted;
managed EditedFile integration and full workspace/platform/source acceptance are
still required. No reclamation or native chunk writes are enabled.
