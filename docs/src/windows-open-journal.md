# Durable per-open callback journal

**Implementation contract, 2026-10-17.** This phase adds the persistent input and
callback evidence needed by the Windows lifecycle controller. Native deletion
stays unsupported until that controller and full native acceptance pass.

The WinFsp 2.1 callback contract allows I/O after cleanup and makes cleanup void;
duplicate kernel handles share a file object. The journal therefore uses one
stable open ID per file object, keeps cleanup distinct from close, and retains
the mount-session ID. See the [versioned API](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/inc/winfsp/winfsp.h)
and [driver cleanup](https://raw.githubusercontent.com/winfsp/winfsp/v2.1/src/sys/cleanup.c),
checked 2026-10-17. These are requirements, not S3Drive native deletion evidence.

Schema 17 adds frozen per-open records, retained input anchors and an append-only
callback/effect hash chain in the existing private SQLite worker. No second
connection owner or database inside the mount is introduced. Configuration 6
and production callbacks stay unchanged. Old namespace and content journal
formats/hashes are preserved.

Prepare the exact open ID/session/node/source/store/fingerprint before RPC. A
new delete-on-close open may carry the complete frozen CreateDeferredEntry or
PrepareRemoval input; activation requires successful exact acknowledgment and
the same deferred origin. A shared deferred open may use the already
acknowledged owner. Old origins cannot adopt a later epoch. Normal open records
are bookkeeping and never authorize reads or replace online service checks.
Activation retains the exact checked deferred-event input/outcome proof and
revalidates it when reading history, even after later mode changes or closure.

Record SetDelete requests, raw cleanup flags and close as separate facts. These
facts do not OR per-open booleans into node-wide disposition and do not by
themselves claim a successful remote effect. Closure can be recorded while an
older RPC is unknown. An old-session open lacking close is reported for explicit
recovery; elapsed time never invents cleanup or releases ownership. If close
arrives without cleanup, preserve that missing evidence instead of fabricating
it. Duplicate kernel aliases must reuse the same file-object ID.
Startup must inspect all records for unknown admissions and linked effects,
including records already containing close. The old-session helper only reports
unclosed records from other sessions; a different session ID is not proof that
a process died and never triggers automatic cleanup.

Link a callback to its complete frozen namespace request before namespace
admission/RPC. A node has at most one unresolved linked effect. Acknowledgment
requires the exact existing namespace input and checked terminal outcome hash;
it cannot invent success from local flags. Reopen/replay use original IDs.
Remote publication, retained dirty data, actual node-wide policy and disposition
acceptance continue through the existing journals and upcoming controller.

The bounded prototype retains 256 open records and 4,096 events without automatic
pruning. Ordinary history must leave slots for activation, one cleanup and one
close per open, plus acknowledgment of already linked effects. This protects
callback evidence, not physical disk space or capacity for a new RPC effect.
The namespace journal separately protects remote closing slots. Full disks and
admission limits retain unresolved work; they are not verified deletion.
Controller integration must provide final closing-work admission and retention
policy before enabling native callbacks.

Acceptance: exact admissions and epochs, multiple file objects, raw flags,
cleanup/close distinction, missing/corrupt rows and identity anchors, ignored SQL,
actual SQLite-full, quota, schema-16 migration/refusal and killed-owner recovery.
Use the real SDK/HTTP and worker to recover linked promotion/final commit, retain
binary/empty/Unicode data and prove no duplicate PUTs. Require the complete local
gate and terminal exact-tip Linux/Windows/book native/reference/startup CI before
private publication. No real Wasabi mutation or workstation mount bypass.

## Local verification (2026-10-17)

The complete repository gate passed: 212 Linux state tests, nine authority HTTP
tests and fourteen managed SDK cases, plus formatting, strict Clippy, build,
local links and mdBook build/chapter tests. Fifteen new state cases cover frozen
admission and epochs, activation/result proof corruption, multiple file objects,
raw flags, missing cleanup, exact replay, binding/owner refusal, ignored SQL,
actual SQLite-full, acknowledgment/cleanup/close quota credits, schema-16
migration and non-destructive migration refusal. Legacy migration fixtures now
remove the new tables when constructing their actual historical schemas.

The SDK fixture uses five child-process stages and the shared worker. It kills
owners after uncertain admissions, promotion, content publication and final
commit. Recovery preserves the original IDs, raw cleanup/close facts and exact
namespace proofs, including closed records whose commit is still unknown.
Binary data, an empty file and Unicode names are exercised with exactly three
PUTs, all three immutable objects retained and no managed DELETE. No Wasabi
mutation is included. These synthetic callback facts are not native lifecycle
acceptance or proof of kernel callback ordering.

Before publication require terminal all-three hosted CI for the exact commit,
including seven existing native cases per OS, startup and the fourteen NTFS/four
upstream/nine malformed-fixture checks. Then implement node-wide/per-open policy,
callback response/uncertainty handling, final closing-work capacity and retention,
and the native WinFsp integration with deferred destination replacement proof.
Keep native deletion disabled until its complete acceptance gate passes.

The first hosted candidate passed 208 Windows state tests and the SDK/reference
checks, but startup acceptance still expected `schema=16`. Its exact version
assertion is now updated to 17; the failed run is not native/startup acceptance.
Require a new terminal exact-commit all-green run before publication.
