# Windows disposition controller

**Implementation contract, 2026-10-17.** The callback journal is accepted at
`b3267a4`, [CI 39051731184](https://github.com/marshallr12/s3drive/actions/runs/39051731184).
This phase adds executable node-wide policy using the real Publisher, authority
and retained local file state. Native callbacks remain gated until integration
and the complete platform acceptance pass.

SetDelete True arms pending or promotes the same deferred fence. False demotes
to deferred while file objects remain, preserving the fence for cancellation
recovery and later delete-on-close cleanup. The final ordinary close releases a
clear deferred fence. It must never cancel then re-arm with fresh guards after
an uncertain response. Persist callback acceptance/rejection separately from
namespace acknowledgment. An observed transport failure plans a rejected kernel
response; recovery replays that request and restores the same fence's prior mode
before processing later callbacks. An abandoned callback with no recorded reply
is conservatively rejected, not silently accepted because an RPC later succeeds.

Cleanup reasserts an uncleaned delete-on-close obligation. FspCleanupDelete
requires the final outstanding cleanup in the selected legacy mode, and unlinks
by current node/parent IDs after retained publications settle. Additional I/O
may still arrive until Close; use retained snapshots and detached publication
rather than delaying all namespace deletion until close. These are versioned
WinFsp contract facts and controller requirements, checked 2026-10-17:
[API](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/inc/winfsp/winfsp.h),
[cleanup state](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/src/sys/file.c).
A nonempty deferred directory must retain its children when promotion is refused.
Raw callback flags and NTFS reference cases do not themselves prove native parity.

Schema 18 distinguishes controlled opens from existing evidence-only records and
adds durable response/completion actions without rewriting old hashes. Controlled
admission protects the full callback work budget: activation, cleanup, close,
exact-result acknowledgment, rejection compensation and completion. Preserve old
schema-17 evidence and conservative recovery; never reinterpret it as accepted
native policy. Explicit limits/retention and abandoned mount-session handling
must be resolved before native enablement.

Acceptance includes accepted/rejected repeated True/False, multiple file objects,
DoC cleanup reassertion, same-fence compensation after lost responses, late I/O,
nonempty directories, current-name cleanup after moves, source/owner refusal,
SQL atomicity/full disk/quota/migration and killed controller owners. Run real
SDK/HTTP scenarios with binary/empty/Unicode data, no PUT replay or S3 DELETE,
and all existing hosted native/reference/startup gates. Deferred target
replacement still requires explicit reference/native proof before integration.
No new real Wasabi mutation, workstation mount bypass or configuration enablement.

Local gate passed on 2026-10-17: 220 Linux state tests, nine authority HTTP
tests and fifteen SDK cases (including the child helper), plus lint, build and
book checks. Eight new state cases cover response proofs, same-fence restoration,
successor refusal, ignored writes, actual SQLite-full rollback, protected quota,
schema-17 migration and cleanup/close prerequisites. Five killed SDK stages
exercise six immutable PUTs with binary, empty and Unicode fixtures. They cover
lost and absent replies, cleanup deletion followed by late detached writes,
replacement-path preservation and accepted callbacks with pending bookkeeping.
Exact active open-ID replay remains bookkeeping; closed IDs cannot reopen files.

This is controller acceptance, not native Windows deletion acceptance. Hosted
Linux/Windows native, startup and reference gates must pass at the exact candidate
commit before publication. Retention/pruning, abandoned mount sessions, native
response acknowledgment uncertainty and deferred target replacement remain work.
