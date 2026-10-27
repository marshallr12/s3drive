# Third-party notices

S3Drive's own source is licensed under GNU GPL version 3 only (`GPL-3.0-only`)
in `LICENSE`, selected by its owner on 2026-10-27, superseding the earlier MIT
choice for this revision. `COPYRIGHT` records the project's copyright notice.
The license text was copied verbatim from https://www.gnu.org/licenses/gpl-3.0.txt
on 2026-10-27; the project grants version 3 only, not an or-later option. This does not relicense dependencies or previously
included third-party material. Their original licenses and notices apply.

The Windows packaging build generates `third-party/inventory.json` from locked
Cargo metadata (Windows-resolved graph) and includes the available LICENSE, COPYING, COPYRIGHT, NOTICE,
AUTHORS and PATENTS files from resolved registry sources, including nested
vendored sources. Each entry records its SPDX declaration, source identity and
notice hashes. Crates that omit workspace license texts use checked-in verbatim upstream notices
from the crate-recorded Git commit, bound to the locked crate checksum and notice
hashes in `packaging/notices/provenance.json`. No build-time web fallback occurs.
The collector refuses missing declarations, unresolved notice omissions,
symlinks, unsafe paths and oversized inputs instead of inventing a license.
This conservative inventory includes build and target dependencies and may be
broader than the set linked into the executable. Non-Windows-only resolved nodes
are excluded from the Windows packet; the full lockfile remains unchanged. It is not a legal opinion or
proof that every dependency-specific distribution obligation is satisfied.

The locked Rust bindings `winfsp 0.13.1+winfsp-2.1` and
`winfsp-sys 0.12.1+winfsp-2.1` declare **GPL-3.0**. This is separate from
the WinFsp driver license. The project license selection does not remove
GPL source/distribution obligations. Public binary distribution remains held
for compliance/source-offer and complete dependency-notice review; no legal
clearance is claimed by this private installer gate. See the pinned upstream
license sources in the provenance file.

In particular, bundled SQLite and AWS-LC/ring crypto sources retain their own
terms, copyright and patent notices. Review the generated packet for the exact
release before public binary distribution. The separately installed WinFsp
runtime is NOT bundled or relicensed by S3Drive's project license; obtain it from the
official WinFsp project under its own license. WiX is a build-time tool, not an
S3Drive runtime license grant; retain any applicable WiX UI notices in a public
installer distribution. Public binary publication is not part of the current
private hosted acceptance gate.
