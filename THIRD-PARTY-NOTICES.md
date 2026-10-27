# Third-party notices

S3Drive's own source is licensed under the MIT license in `LICENSE`, selected
by its owner on 2026-10-27. This does not relicense dependencies or previously
included third-party material. Their original licenses and notices apply.

The Windows packaging build generates `third-party/inventory.json` from locked
Cargo metadata and includes the available LICENSE, COPYING, COPYRIGHT, NOTICE,
AUTHORS and PATENTS files from resolved registry sources, including nested
vendored sources. Each entry records its SPDX declaration, source identity and
notice hashes. The collector refuses missing declarations, missing notice files,
symlinks, unsafe paths and oversized inputs instead of inventing a license.
This conservative inventory includes build and target dependencies and may be
broader than the set linked into the executable. It is not a legal opinion or
proof that every dependency-specific distribution obligation is satisfied.

In particular, bundled SQLite and AWS-LC/ring crypto sources retain their own
terms, copyright and patent notices. Review the generated packet for the exact
release before public binary distribution. The separately installed WinFsp
runtime is NOT bundled or covered by S3Drive's MIT license; obtain it from the
official WinFsp project under its own license. WiX is a build-time tool, not an
S3Drive runtime license grant; retain any applicable WiX UI notices in a public
installer distribution. Public binary publication is not part of the current
private hosted acceptance gate.
