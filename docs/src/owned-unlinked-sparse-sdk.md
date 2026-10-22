# Owned sparse SDK integration

Accepted SDK slice, 2026-10-22. Schema47 remains unchanged.
The [accepted state contract](owned-unlinked-sparse-state.md) is the prerequisite.

## SDK contract

`EditedFile::begin_owned_preparation` takes the live chunk capability plus the
original open ID and exact acknowledged removal publication claim. A private
worker identity check rejects cross-worker composition even when persisted
identifiers match. The existing state transaction independently captures and
validates original-open and current-event provenance before staging. One reader
permit/deadline covers admission and fresh source checks. Exact existing-plan
retries retain operation/generation/view/ownership and precede fresh source
probes; changed intent is rejected by the state layer.

The existing mixed uploader retains consumed PUT identity, observes claimed
objects without resending, and requires current-session materialization for
unclaimed objects. Publication now requires event-fenced-manifest capability
for owned work and reads the coherent RemovalEpoch node/claim. Fresh work must
match the frozen claim; the saved Reserve/Publish retains its exact authority
fence even if ownership changes after that observation. Historical metadata
replay precedes fresh credentials, source, policy and feature checks.

Explicit `Preparation::settle` delegates exact NoEffect/Rejected settlement to
the state journal. It performs no network probes, deletes no bytes and never
reports a clean-source acknowledgment. NoEffect requires no staged objects or
root request; Rejected requires a saved terminal Conflict. An unknown reply
cannot be relabeled rejection. Source acknowledgment and successor lineage use
the existing state APIs and preserve newer dirty views. Metadata-only recovery
can recover saved root outcomes with missing credentials/policy and damaged
source bytes, but cannot create new preparation, PUT, publication request or
source acknowledgment. Terminal preparation settlement still requires the normal
validated local owner, not this restricted metadata-only capability.

Publisher operations share the existing publication semaphore. Its separate
recovery semaphore is unchanged; exact state transactions and immutable request
identities arbitrate overlapping recovery. Reader operations retain their
existing bounded permits/deadlines. This phase introduces no wider concurrency
or reclamation guarantee.

## Accepted tests and evidence

Real authenticated HTTP and AWS SDK loopback tests cover pending and committed
ownership, >8MiB sparse Unicode bytes, retained original open/current event,
lost reserve/publish replies, same-epoch successor roots and newer dirty views,
feature/policy refusal, explicit no-effect/rejected settlement, precommit event
races, obsolete unclaimed restart refusal and killed metadata recovery after
source/staging corruption with credentials/policy removed.

Runtime/test/CI source `1645861186421c3c5c10a9b4c3d3244389665e7f` is accepted in
[run39822691603](https://github.com/marshallr12/s3drive/actions/runs/39822691603).
All three jobs are terminal success. Full workspace logs show Linux955 and
Windows951 passed across37 cohorts each (15/31 ignored native/platform cases,
with applicable native cases run separately). All six new SDK cases pass both
in the focused gate and full suite, including consumed physical PUT observation
without a resend after restart under expired/narrowed authority. All16 owned
state cases pass on both platforms. Fmt, workspace/all-target Clippy with warnings
denied, workspace build, mdBook and separate FUSE/WinFsp native regression gates
pass. Existing Windows delay-load linker and Actions Node20 deprecation notices
are retained; no assertions, durability guards or native prerequisites were relaxed.

All759 tracked files match the exact-source archive. Full Windows logs and both
reference artifacts match all35 typed records, field types and values, including
raw output. Linux real46/47 compatibility uses distinct binaries from separate
source/build directories: seven invocations, two byte-preserving old-version
refusals, and complete unchanged75-statement legacy dump. Artifact ZIP SHA256:

- Replacement12445827138: `2f717bc59e4bf2d68add5fe94d0b13c220af70633df9254298f1d9022cc740ab`.
- Disposition12447955608: `c80c539faa92c7be7eb5c1ce71db26f3dfa8ebbd913970a3e9201a0a8864165e`.
- Compatibility12446046979: `64d00c8a875ba4743e386bf36f475350db8f27c0b693fde3ea29e429c72db6ee`.

The first candidate run39822301108 failed on both OSes because its fixture tried
to register a delete-on-close open before acknowledging removal ownership. The
fixture was corrected to register a normal controlled open, then acknowledge its
removal event. The state rejection was preserved, not weakened. Additional
negative, historical-read and physical-lost-PUT coverage was added before this
successful exact-source run. Both cohorts remain in the private validation archive.

Local workspace build/lint/fmt/core tests and documentation links pass. The local
full gate correctly fails on unsupported overlayfs/tmpfs durable storage; local
mdBook is unavailable. Standard hosted runners supplied those gates without
relaxing guards. Local validation archives remain unpublished. Before integration
compare fresh main and branch heads; after integration verify main's full CI.

No new FUSE/WinFsp lifecycle flag is enabled. Actual owned-unlinked sparse native
unlink-while-open, save/replace, dirty-handle read, final-close and cold/remount
recovery gates are a subsequent implementation phase, not proved by these SDK
or existing native regression tests. Physical MX/Windows sign-in/reboot, real
providers, independent hosts/deployment, retention and GC remain separate.
