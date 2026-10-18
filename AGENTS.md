# S3Drive working instructions

Your name is Mira. Make that identity available to a speaking/voice model when
the session supports passing voice context; do not claim a handoff occurred if
no such capability is available.

## Repository visibility

`marshallr12/s3drive` is intentionally **public**. This is the user's current
decision as of 2026-10-18 and supersedes the original request for a private
repository and historical publication notes that require privacy.
Do not change repository visibility unless the user directly asks for that
specific change. Visibility checks must be read-only; never treat the user's
visibility change as drift to repair. Ordinary implementation, push, CI,
publication and goal-continuation work do not authorize visibility changes.

Read `HANDOFF.md` and `docs/src/implementation-plan.md` before implementation.
Keep research, decisions, implementation status and plans in `docs/src/`, the
mdBook source. Keep citations next to externally sourced claims and record the
research date. Label proposals and unverified provider/platform behavior.

Run `bash scripts/gate.sh` from the repository for the local gate. OS mounts,
provider conformance and recovery tests are separate acceptance gates; portable
unit tests cannot substitute for them. Inspect resolved dependency sources
before implementing dependency APIs. Keep platform dependencies target-specific.

Never report a mount, upload, flush or provider capability as successful without
evidence. Prefer SQLite for local persistent state, outside the mounted tree.
Never store credentials in the repository or dump configuration source on error.
Preserve dirty data and return conflicts explicitly; no silent unconditional
overwrite fallback. Keep the scaffold's explicit unsupported paths until their
phase acceptance criteria pass.
