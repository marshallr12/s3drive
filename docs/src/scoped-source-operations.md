# Epoch-scoped source operations

W5b2a contract,2026-10-19. **Proposal; not accepted.**
Bounded retention must not forget a random operation ID and later admit it as a
new write. W5b1 already changes a file's base binding only in an acknowledged
clean transition. Bind each new source operation to that binding's canonical
SHA-256 epoch plus a caller nonce. Its raw ID is a separate domain-separated hash
of epoch and nonce. The caller must persist the complete token before an uncertain
operation; it must never replace the token after an unknown result.

Explicitly enabling scoped mode permanently disables fresh raw-ID source mutations
and acknowledgments on that private owner. Retained legacy operations may still
replay their exact results. New scoped initial views, writes, resizes and local
acknowledgments require a token for the actual current binding. A known token
returns the same immutable result without changing the head, even if a later base
has been acknowledged. A forgotten old token with a closed epoch cannot be
admitted against a new head. Reusing a nonce in another epoch creates a distinct
operation only when the caller explicitly supplies that new epoch.

This uses the existing clean-base boundary rather than an independent monotonic
producer allocator. It does not promise constant history within an active epoch;
large online writes must publish/acknowledge bounded windows, and offline quota
exhaustion remains explicit with data preserved. Pins, independently validated
view/epoch seals, physical/preparation/root retirement and actual local byte
reclamation are W5b2b/W5b3. No S3 object deletion follows from local retirement.

Add one STRICT scoped-mode table in schema36, with an integrity-bound singleton;
old source/operation/root records stay byte-for-byte unchanged. No token field is
needed in their serialized format: the supplied token hashes to the existing raw
operation ID, and retained view bindings/acknowledgment observed-head bindings
supply its epoch. Historical acknowledgments use the head observed in their
transaction; a same-head transition uses that old head's epoch. Both remain local.

Validate the marker even before the first source file exists, isolate it from
legacy publication/authority roles, and allow authority snapshots to see only an
empty marker table. Existing-only metadata recovery may read valid schema36
journals but cannot enable the mode or admit any new operation. Missing/corrupt
current-schema mode tables fail closed; deliberate pre36 fixtures remain readable.

Acceptance criteria: sync and same-owner async APIs enforce initial/write/resize/ack tokens,
known replay precedes head/quota admission, closed/wrong epochs and raw fresh
calls fail without changed rows, SDK publication/ack cycles continue correctly,
old literal rows survive migration, and wrong-role/corrupt marker/trigger/full-disk/
process-kill cases preserve exact state. Full local and own platform/source/native
regressions precede promotion. This slice alone does not enable data reclamation,
external-base observation, new-file origins or mounted chunk writes.

Implementation checkpoint,2026-10-19: sourcea3028bf/schema36/all329 is
isolated on codex/scoped-source-operations. Fourteen focused state cases passed
across runs3/5, including14 real kills (three enable/eleven acknowledgment),
mode migration/collision, exact replay, wrong epoch, role isolation and actual
SQLITE_FULL with exact retry after capacity restoration. The scoped SDK cycle
passed1/1 in69.97s with13 loopback PUTs, Unicode rename/data, old reads and an
empty manifest. These tests do not use Wasabi.

The complete local gate1 stopped101 with all329 inputs unchanged: formatting
and all-target Clippy passed; SDK publication22passed/2failed in254.48s. Existing
mixed dependency upload and mixed root crash recovery cases reported internal
upload Timeouts; the new scoped SDK case passed. Investigation and full gate
remain required before acceptance. Exact failed map/source archive/log are
retained under `/tmp/s3drive-scoped-operations-candidate3-*` and
`/tmp/s3drive-scoped-operations-gate1-*`. Earlier fixture/import/test-capacity
setup failures were retained and corrected without suppressions.

[Own hosted cohort](https://github.com/marshallr12/s3drive/actions/runs/39267986023)
for exacta3028bf remains in progress. Its source stays frozen. A separate
codex/scoped-source-operations-repair worktree holds investigation; no repaired
runtime is accepted. Main remains accepted W5b1/schema35. This phase still cannot
reclaim bytes, adopt external bases, create new-file origins or enable mounted
chunk writes.
