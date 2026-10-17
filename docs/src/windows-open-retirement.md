# Completed-open retirement

**Implementation contract, 2026-10-17.** The executable controller is accepted
at `28c527e`, [CI39055610947](https://github.com/marshallr12/s3drive/actions/runs/39055610947).
Its history currently consumes live admission after close. Retirement addresses
that limit without destroying recovery evidence or enabling native deletion.

[Mount-session recovery](windows-session-recovery.md) adds a separate proof path
for obsolete registered sessions with missing callbacks. It preserves those
missing facts rather than manufacturing normal cleanup/close completion.

Schema 19 adds a sealed retirement index in the existing private SQLite database.
Retirement requires a controlled, activated file object, completed cleanup and
close, completed replies/effects for every recorded callback, and no unknown
linked effect. It removes only live admission: original input, anchor, events,
namespace proofs and dirty data remain. Exact callbacks can replay historical
results; new events and reusing a retired ID are refused. Legacy evidence-only
records cannot be reinterpreted as completed controller policy.

The 256-open/4096-event budgets apply to live records. Retirement does not require
every other reader of that node to close: its own completed file object no longer
participates in callback policy, while full ownership proofs remain accessible.
The controller retires after Close and retries completed-but-unretired closes
during recovery. SQL failures retain the complete close and live admission for
retry. No S3 DELETE, proof pruning, namespace-history reclamation or unbounded
payload cache is introduced. SQLite disk limits and the separate namespace
admission cap still apply; this is lossless retention, not storage garbage
collection. Retirement seals/indexes are checked during live enumeration;
archived event chains and linked proofs are checked in full on historical lookup.

Unclosed records, incomplete callbacks and uncertain admissions remain live.
A different session ID alone neither retires them nor proves a kernel cleanup.
Abandoned mount-session recovery and native response acknowledgment uncertainty
remain prerequisites for Windows integration, alongside deferred destination
replacement proof and native lifecycle acceptance. Native callbacks/config 6 and
real Wasabi/workstation state remain unchanged.

Acceptance uses more than 256 file-object lifetimes and 4,096 retained events with
a long-lived reader; exact replay/identity refusal; missing or changed retained
proofs; ignored SQL and actual SQLite-full rollback; schema18 migration/refusal;
and killed real SDK/HTTP clients between close completion and retirement. Require
the local repository gate and terminal exact-tip Linux/Windows/book CI, including
all existing native/reference/startup gates, before private publication.

Local gate passed on 2026-10-17: 228 Linux state tests, nine authority HTTP,
fifteen SDK cases, formatting, Clippy, builds and book checks. Eight retirement
tests include 300 lifetimes with more than 4,096 retained events and a live reader,
full-disk/ignored-write rollback, replay/identity refusal, seal/proof corruption
and schema18 migration/refusal. The controller SDK case now uses six process
stages: recovery retires a completed close after an injected failed retirement
and killed owner, preserves all six immutable objects, and performs no extra PUT.
This evidence does not enable native deletion or establish mount-session recovery.
