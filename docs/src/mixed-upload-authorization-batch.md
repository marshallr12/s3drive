# One authorization scope for a mixed upload dependency batch

Declared implementation contract,2026-10-19. **Prototype and focused SDK passed; full/platform gates pending.**
Independent from the unaccepted scoped-operation/schema36 repair; baseline main
has accepted W5b1/schema35. Original scoped gate and two repair gates retained
upload Timeouts; both failing cases passed alone (88.68s/89.91s). Limiting separate
SDK fixture families to two left one failure. Do not claim admission alone fixed it.

Inspection found that one page upload calls the public dependency API separately
for each direct child. Each call repeats complete SQL inspection and fresh
historical metadata/root HEAD authorization. A fanout128 page can therefore repeat
that same authorization more than128 times inside one30-second upload deadline.
This is an implementation observation, not proof of a sole timeout cause.

Keep the public one-dependency API unchanged. For fresh mixed uploads only, add a
private, nonserializable authorization scope bound to the exact Preparation and
its pinned EditedFile. It owns one reader-family permit. Validate the plan and
parent/dependency positions before minting it; authorize the exact historical
base/root before child semantic checks. Check every direct child sequentially,
including complete original-byte rules for changed partial chunks, inherited
proof context and false-hole rejection. Only one child's bytes may be materialized
at a time. Use the existing composition/classification logic; no caller-created
proof, arbitrary bytes, stale credential substitution or SQL transaction across
network I/O is permitted.

Revalidate local plan scope and reauthorize the pinned base again after semantic
checks, before materialization/send. Thus authorization cost is bounded by two
base checks per new physical object rather than its child count. The existing
outer upload deadline covers the whole batch and both authorizations; do not
increase or reset it. Cancellation drops the family permit. Existing physical
receipt/dependency checks and write-policy admission remain unchanged.

Already claimed/unknown physical attempts retain their existing metadata-only
recovery path before any fresh source authorization; no reader permit or new
source check may obstruct known-result recovery. Public dependency construction,
root CAS, local acknowledgments and metadata recovery are otherwise unchanged.
No schema/dependency/platform fixture change, provider capability, native chunk
write or GC activation follows from this optimization.

Ownership: managed composition planning/preparation/uploading and the existing
SDK root case/harness. Keep all other runtime inputs at the accepted324 baseline.
Evidence must include exact request counts for the130-chunk sparse page case:
three new physical objects and six base HEAD/history authorizations within a
handshaked window, while preserving the case's lost Publish reply, killed owner,
credential-free recovery, Unicode/tail data and newer dirty head checks. All old
forged/missing/corrupt/unread-tail/policy/concurrency cases and the complete gate
must pass, followed by this source's own Linux/Windows/native/artifact cohort.

Initial implementation check passed20.76s; focused SDK1 passed78.26s with exact
six authority/six base HEAD checks, three new objects and all inherited root
recovery assertions. Candidate1 source/map retained. A further acceptance probe
now permits opening authorization and denies the final authorization, asserting
two observed checks and no staging generation before failure. Fresh verification
and full gate remain required; no pass is claimed for that new probe yet.

SDK2 passed79.54s with the closing-authorization denial probe: exactly two
historical checks, failure before any staging generation, then successful normal
upload with exactly six history/six base HEAD checks for three new objects. All
existing lost-reply/killed-owner/credential-free recovery and Unicode/tail/newer
head assertions passed. Candidate2 is324 frozen inputs, only the declared five
runtime files changed from accepted W5b1. No SDK admission semaphore is present
on this branch: its upcoming full gate uses the normal four test threads.
