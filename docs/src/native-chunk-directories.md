# Chunk-backed directory creation

Declared2026-10-21 from accepted main6417ee7, schema41/runtime366.
Accepted runtime7c980745 after own complete local and hosted verification.
The candidate and rejected-run history below is retained as dated evidence.
This phase adds ordinary folder
creation to explicit managed version8 on Linux FUSE and Windows WinFsp.

The existing chunk-mount capability exposes the same owner's namespace journal.
CreateEntry/Directory admits no file entries and applies no sparse file effect;
its parent/name guards and retained exact outcome already exist. File barriers
apply to affected file entries, not unrelated dirty heads. Authority content
and entry guards tolerate unrelated global sequence advancement. These are
source findings in namespace_client::{begin,changes}, sparse_views::composition
and namespace::apply; the integration must demonstrate them through actual APIs.

Use a durable directory intent before RPC and exact saved-request replay after
unknown outcomes. Namespace acknowledgments must retain dirty sparse bytes and
unresolved root publications. Directory creation must not flush unrelated files,
PUT S3 objects, create metadata-empty file origins, or relax role exclusions.
Fresh directory work needs the selected default-store policy and explicit
namespace-create capability. Known saved outcomes recover before fresh policy
or feature admission, within the existing normal-owner recovery boundary.

Done criteria:

- Shared API and real FUSE/WinFsp create/list/open nested Unicode folders and
  canonical chunk files beneath them, preserving them across restart/remount.
- Directory-only operations perform zero S3 PUTs and preserve unrelated dirty
  heads, including a publication committed remotely but not acknowledged locally.
- Lost Apply replies replay the exact retained operation after restart, including
  narrowed/expired fresh write admission. No new operation ID substitutes for an
  unknown outcome. Wrong scope, collision and parent-revision races fail explicitly.
- Keep the128KiB callback control, bounded owned-task cancellation and all existing
  file/native/startup/reference gates. No larger callback stack or weaker timeout.
- Complete own unchanged-source local and Linux/Windows hosted acceptance before
  promoting code. Provider and physical workstation acceptance remain separate.

Expected footprint: chunk writer namespace helper and owned dispatcher, shared
filesystem directory dispatch, owned SDK/native fixtures and CI, book/handoff.
No schema, dependency, provider authorization or installation change is planned.
Rename/delete, deferred canonical creation, owned-unlinked publication, retention/
GC and cold namespace/open recovery remain separate requirements. This phase is
part of enabling usable chunk drives, not completion of the overall goal.


Candidate review2026-10-21: the thin shared directory entry point selects chunk
versus legacy futures before construction. The chunk dispatcher acquires one of
four permits before spawning owned work; its120-second bound and abort-on-drop
remain unchanged. A same-owner namespace intent precedes RPC. Fresh mkdir recovers
only pending directory intents, so it does not resume unrelated S3 publication.
Constructor recovery also visits that journal. No schema/dependency change.

Focused SDK5 passed3 directory cases: zero S3 PUTs with nested Unicode, collision,
wrong authority/name policy and a real parent-move race; exact lost-reply replay
under narrowed admission/missing namespace feature and separately expired policy;
and folder creation while a committed file-root response is held, preserving
newer dirty bytes. The tests inspect exact requests, authority file metadata and
PUT totals. SDK1 had a private-field compilation mistake; SDK3/4 failed fixture
setup (a sibling creation did not change parent revision, then an injected wrong
volume remained enabled). SDK5 corrected those fixtures without changing runtime
or weakening conflict expectations. Logs remain retained. Earlier SDK2 passed
three basic cases before the additional controls. The128KiB callback fixture and
real native fixture now also create Unicode folders; those changes await the
full gate and hosted FUSE/WinFsp runs. No local native success is claimed.


Candidate3004479 full gate1 stopped at Clippy after38.76s, all701 inputs
unchanged: the new proxy fixture needed collapsed conditionals. Its hosted Linux
job also rejected that lint. No native acceptance is borrowed from that cohort.
A corrected fixture will receive a new commit and independent complete gates.


Corrected6471af9 gate2 was deliberately stopped after229.29s (exit-15,
all701 inputs unchanged) to add a missing native acceptance assertion: directory
enumeration and directory metadata for each Unicode path before and after
remount. Clippy and the focused SDK controls had passed; the stopped complete
gate does not count as acceptance. Cohort2 hosted outcome remains separate.


## Accepted native directory integration —2026-10-21

Runtime `7c980745c6d2a2bd55805a3536b15d874d6b6b22` is schema41/all368, exactly7 runtime paths changed
from accepted main6417ee7. No dependency/schema change. Full local gate3 passed
712.93s, all701 inputs unchanged; logSHA `14d5fa41bf5ea5fc6ea2ee4fc35622a164bbf59dceaf40c014b7f438662d58b6`.
Own [CI39568987445](https://github.com/marshallr12/s3drive/actions/runs/39568987445)
attempt1 Linux/Windows/book all succeeded. Exact GitHub source ZIP/all701 digests,
API artifact ZIP digests, strict typed21+14/project oracles and every raw-record
field and JSON type passed. All warnings, slow notices and interleaved reference
completions were reviewed in the indexed hosted evidence.
The native chunk fixture took30.73s on Linux and54.58s on Windows; Windows
128KiB control5.18s. All10 slow notices completed and15 interleaved reference
completions were verified. Three disposable Windows VM profiles remained locked;
dummy credentials were removed and profile deletion deferred to VM disposal.
Tooling deprecations and unused delay-import warnings remain in the raw logs.

The real FUSE and WinFsp fixture creates nested Unicode folders, enumerates
all path components and validates directory metadata, creates and edits an11MiB
sparse binary/Unicode file, flushes, shrinks/regrows and remounts. The six exact
immutable PUTs remain file-content work; the SDK directory-only case makes zero
S3 PUTs. Both callback-stack controls retain128KiB. Existing native and startup
regressions pass on the same source. The directory concurrency case keeps a
committed root response pending while creating a folder and preserving newer
dirty bytes. Narrowed/expired admission never replaces a saved unknown request.

- [Local indexed archive](evidence/chunk-directories-local-20261021.tar.gz): 401 members, 1126835 bytes; SHA256 `d24d53df92f9d415ad9c9d18081513d76a16aef83addb13ed5cc981ec1c92303`. Every member verified.
- [Hosted indexed archive](evidence/chunk-directories-hosted-20261021.tar.gz): 47 members, 259473 bytes; SHA256 `82f78398cd075ea19f04c2188fce0d5cf449fd621b44581cbafab7bb36a6aa0f`. Every member verified.

These are owned loopback S3/authority fixtures on hosted Ubuntu and Windows
Server2025, not Wasabi writes or physical MX/Windows10/11 reboot acceptance.
Version8 startup supervision, independent-host deployment/restore, cold
namespace/open recovery, rename/delete/save-replace, deferred/unlinked publication
and retention/GC remain required. Prototype quotas are unchanged. No user mount,
credentials, writable provider policy, hosting or Turso backend was installed.
