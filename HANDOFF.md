# S3Drive handoff

Updated 2026-10-19. Goal ACTIVE. Assistant: Mira. No voice-context handoff capability
is available. Read this handoff and [implementation plan](docs/src/implementation-plan.md)
before implementation. Keep research, plans and acceptance evidence in docs/src.

## Current decisions and boundaries

- marshallr12/s3drive is intentionally PUBLIC. AGENTS.md supersedes the original
  private goal and historical notes. Never change visibility without a new direct
  request; checks are GET-only.
- Managed immutable chunks/manifests, normal coordinated filenames, a shared Rust
  authority and private client SQLite. Both hosting options remain open. Turso is
  researched, not implemented or provisioned here.
- Support MX Linux without systemd, Windows, and same-account SSH/tmux access after
  terminal disconnect. Physical sign-in/reboot is a separate acceptance gate.
- Run bash scripts/gate.sh. Keep native/provider/recovery gates separate. Preserve
  dirty data, unknown outcomes and unsupported paths; no overwrite fallback.
- No new Wasabi writes, provider probes, credentials, root/startup installation or
  hosting changes are authorized by the local fixture work. Never dump config.

## Accepted runtime and publication

Accepted runtime is66d78db94beabb666eb9cfeab2b99503de1af58e, schema41/all362 runtime
inputs, based on accepted SDK950479c/schema40/all357. Exactly27 runtime deltas add
explicit same-owner client composition. The complete local gate and own hosted
CI39335223526 attempt1 all3 passed, including exact source/API ZIP, strict typed
21+14/project/all35 raw record field/type review. [Contract and evidence](docs/src/native-chunk-integration.md).

Canonical publication is recorded at
/tmp/s3drive-canonical-chunk-composition-publication.json once this publication
completes. Verify its commit, remote identity and all362 digests; do not copy an
older main SHA. Main before this phase was947d0f1889f0be8b125e2e0dc0a87e3cde0a4b82,
clean and matching all357 accepted SDK bytes at pre-promotion inspection.

Native FUSE9/WinFsp7, manifest remount and no-systemd/Windows startup regressions
passed on this source. Those are existing whole-generation mounted operations.
Mounted writes still have the old8MiB limit. Native chunk writes, retention,
real managed Wasabi large files and physical MX/Windows10/11 reboot/sign-in remain
unfinished. Authority snapshots support create/inspect, not writable restore.

## Explicit composition implementation

Worktree /home/marshall/src/worktrees/s3drive-chunk-client-composition,
branch codex/chunk-client-composition. Source66d78db is frozen and accepted;
later documentation is checked separately without changing runtime bytes.

Storage::chunk_mount(Input) atomically binds volume, permanent chunk identity and
up to64 retained source/store controls. It returns same-owner namespace/open
capabilities. Legacy constructors remain excluded, including before any sparse
row. The independent identity checksum prevents a missing/downgraded role marker
from becoming an older writer. Compatible sparse/creation intents must match
scope; controls append without changing store routes or dropping dependencies.
Fresh namespace changes require clean sparse data and no unknown root outcome;
acknowledgments preserve exact bindings/heads and newer dirty edits.

Canonical files use the accepted creation SDK. Chunk namespace calls refuse
metadata-empty file creation. Directories retain the existing mutation protocol.
Open/session history retains exact ownership and receipt checks. No native
callbacks or S3 permissions are activated by this role.

Recovery boundary: saved outcomes can be inspected on an already open owner
without source-body reads. Namespace/open clients still use normal Storage/State
opening; cold metadata-only namespace/open recovery is not implemented. The
creation SDK has a distinct existing-only recovery store. Do not conflate them.
There is no separate new SDK fixture for that store under a damaged composed
sparse tree; namespace corruption tests and existing creation recovery evidence
have their own bounded scope.

## Verification and archives

Local gate1 runner1704986/child1704997 CLOSED0 at22:42:13Z, all681 inputs unchanged,
797.32s. Log SHA7665b3f0257399d756e0b439382f9185ef9b9f1edb3f2f3c7b78119d91a8a8aa.
State538+1diagnostic ignored/272.48s, publication31/164.46s, reader26+1native-only
ignored/70.48s, core39/S362, format/Clippy/build/93links/book89 passed. New follow-on
proposal documentation has its own94-link/book90 checks. Focused13 state cases
include seven killed owners; unknown-root namespace barrier and SDK composition
with a Unicode rename and >8MiB sparse edit also passed.

Own CI39335223526: Linux118342911242, Windows118342910955, book118342911228 all
success. Observer2389 CLOSED0. All681 remote source files matched. Artifacts
replacement12226851699/disposition12226874753 passed API ZIP digests, independent
strict21/14, project oracles and all35 raw record fields/JSON types. All17 slow
notices and15 interleaved reference endings completed. Exactly3 known disposable
Windows profile locks: dummy credentials removed, deletion deferred to VM disposal;
following pass lines reviewed. Tooling and unused delay-import warnings retained.
State538/533 plus1ignored per OS; FUSE9/80.03s, WinFsp7/93.61s; manifest remount
1.74s/2.44s; coordinator startup24.46s no-systemd/81.55s paired Windows. Images:
Ubuntu24.04/20261012.334.1; WindowsServer2025/windows-2025-vs2026/20261007.264.1.

Indexed archives in docs/src/evidence, all members verified:
- chunk-client-composition-local-20261019.tar.gz:382 members/989868 bytes,
  SHA690cf0e1fb7d84c4d187070b9e6fa6d38821b530b87de7d46e2e3e4ef1d527fc.
- chunk-client-composition-hosted-20261019.tar.gz:27 members/140050 bytes,
  SHA7375d1a36588a12b6813620f181b3191d07b0e56d05732b6f02aefff84a0e006.
Both one-use archivers DONE; never rerun on later docs. Frozen runtime map
/tmp/s3drive-chunk-composition-candidate1-frozen.json. Full checkpoint
/tmp/s3drive-chunk-composition-checkpoint.json. Initial SDK1 failed a missing
expected-PUT harness match; SDK2 passed. Clippy1 helper-placement error was fixed
without suppressing the lint. Pre-final state4 success is not final acceptance.

## Next implementation toward actual chunk drives

Use the accepted creator and explicit combined role, not constructor exclusions
removed wholesale or populated old identities adopted. Finish cold namespace/open
metadata recovery as needed, then declare/test the native shared sparse engine:
canonical create, >8MiB write, resize, read-your-writes, exact flush/local source
acknowledgment, reopen and remount. Keep unsupported flags explicit.

[Deferred canonical creation review](docs/src/deferred-manifest-integration.md)
is a proposal, not a decided wire/API contract. Metadata-empty deferred creation
cannot silently become a manifest origin. Mixed root reservations still hardcode
linked guards; owned-unlinked publication needs exact removal ownership/fence/event
proof. Complete save/replace, rename/delete and open-unlinked close in actual
FUSE/WinFsp acceptance. Close all retry forests before retention/GC;16/128 creation
and4096 authority history limits are scaffold limits, not finished usability.

Still required: legacy/empty origin handling, real Wasabi large-file acceptance,
independent-host partitions/deployment/fencing/restore, physical MX/Windows10/11
sign-in/reboot/SSH, usable policy renewal and packaging/licensing. No hosting or
Turso account has been provisioned.

## Provider scope and history

Ignored config /home/marshall/src/s3drive/s3drive.local.toml. Wasabi aliaswasabi,
s3drivetest/us-east-2, profilewasabi-s3drive. No keys in chat or Git. Earlier3-object
Unicode and6-key conformance scopes were completed/cleaned. The latter consumed
all14 PUT attempts on2026-10-16 at23:36:18Z; approvals are exhausted. Any new live
mutations require a concrete bounded plan and new explicit authorization. Never
extend an old expiry or replay an old probe. The optional plan pointer under/tmp
was never approved/executed.

The accepted SDK950479c and its independently verified physical-proof audit are
recorded in [SDK acceptance](docs/src/empty-manifest-publisher.md), including the
rejected706 source and failed stack cohorts. Keep those archives/evidence.
Previous handoffs remain verbatim in [SDK history](docs/archive/HANDOFF-before-creation-sdk-refresh-20261019.md)
and [pre-composition refresh](docs/archive/HANDOFF-before-chunk-composition-refresh-20261019.md).
Pre-composition refresh SHA61af6caab77d919518c78185c1c8c45a2cb3856a0cefff711075db4cfd824c8a. Historical private requirements are superseded.
