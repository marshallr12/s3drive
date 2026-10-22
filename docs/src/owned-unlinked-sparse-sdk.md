# Owned sparse SDK integration

Candidate, 2026-10-22. Not yet accepted. Schema47 remains unchanged.
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

## Required acceptance

Real authenticated HTTP and AWS SDK loopback tests cover pending and committed
ownership, >8MiB sparse Unicode bytes, retained original open/current event,
lost reserve/publish replies, same-epoch successor roots and newer dirty views,
feature/policy refusal, explicit no-effect/rejected settlement, precommit event
races, obsolete unclaimed restart refusal and killed metadata recovery after
source/staging corruption with credentials/policy removed.

Local build/lint results and exact-source hosted Linux/Windows full suites and
native regression gates must be reviewed before acceptance. This cloud
workstation's overlayfs/tmpfs cannot provide durable staging; guards must not
be relaxed. Failed cohorts and validation archives remain private.

No new FUSE/WinFsp lifecycle flag is enabled. Actual owned-unlinked sparse native
unlink-while-open, save/replace, dirty-handle read, final-close and cold/remount
recovery gates are a subsequent implementation phase, not proved by these SDK
or existing native regression tests. Physical MX/Windows sign-in/reboot, real
providers, independent hosts/deployment, retention and GC remain separate.
