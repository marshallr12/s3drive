# Set up a managed drive

Implementation discussion date: **2026-10-17**. Implemented on
`codex/managed-bootstrap-cli`. Native acceptance is per exact source commit
and uses the separate hosted gates described below.
The coordinator and filesystem already have a managed layout; these commands
make its initial store registration available without writing Rust API code.

## Start the coordinator

The coordinator is the Rust `s3drive authority-serve` process. Keep its private
SQLite directory, token, certificates, client journals and mounted folders in
separate local locations. See the [architecture image and HTTPS contract](remote-authority.md).
For a local experiment, using absolute paths whose parent already exists:

```text
s3drive authority-token --directory /home/me/.config/s3drive/authority-secret
s3drive authority-serve --directory /home/me/.local/state/s3drive-authority --token-file /home/me/.config/s3drive/authority-secret/authority.token --name-policy portable-collision-v1
```

The foreground process prints its endpoint and generated volume ID. Keep those
values. HTTP binds only to numeric loopback; shared hosting requires explicit
TLS certificate/key paths and an HTTPS URL. No service, host, account, firewall
rule or startup entry is provisioned by these commands. On Windows use absolute
Windows paths; protect secret files with the current account's private ACL.

## Register a bucket mapping

Keep the existing source TOML and local credential profile. A source records
the endpoint, bucket, region and optional bucket prefix. The following example
uses `sources.toml`, alias `docs`, and placeholders `URL`, `VOLUME`, `TOKEN`:

```text
s3drive managed info --authority URL --volume VOLUME --token-file TOKEN
s3drive managed store-plan sources.toml docs --authority URL --volume VOLUME --token-file TOKEN --root managed/.s3drive/ --directory /home/me/.local/state/s3drive-plans/docs
```

For a private CA, add `--ca-file /absolute/private/ca.pem` to **every** command.
The CA bytes used for TLS are hashed into the plan; system-trust and custom-trust
plans cannot silently switch. Tokens themselves are never written into plans.
The token path and local AWS profile name may differ between computers.

`store-plan` writes one owner-private `store-plan.toml` with create-new semantics
and flushes it before reporting success. Keep that file on persistent local
storage, outside the mount and source tree. It freezes one registration operation
ID (also the store ID), volume, sequence, physical route and root. Its JSON output
includes `plan`, `plan_sha256`, `store_id` and the resulting physical prefix.
Confirmation hashes the parsed plan's canonical representation; comments and
whitespace do not alter the request. It contains no access keys or bearer token.

`--root` is **relative to the source prefix**, not a bucket setting. For source
prefix `desktop/` and root `managed/.s3drive/`, immutable payload keys begin with
`desktop/managed/.s3drive/v1/VOLUME/`. Root components allow ASCII letters,
digits, dot, underscore and hyphen, require a trailing slash and exclude empty,
`.` and `..` components; the root is at most 128 bytes. The source prefix may
contain Unicode. Setup checks the eventual 1024-byte S3 key budget.
This creates an empty managed namespace; it does not import existing S3 keys.

Apply the exact reviewed plan using its returned hash:

```text
s3drive managed store-apply sources.toml docs /absolute/docs/store-plan.toml --token-file TOKEN --confirm PLAN_SHA256
s3drive managed store-status /absolute/docs/store-plan.toml --token-file TOKEN
```

These commands use only the metadata service. They do not resolve S3 credentials,
probe a bucket or upload/delete objects. A registration proves metadata binding,
not provider access. The physical route binds endpoint, region, bucket and source
prefix while excluding machine-local aliases, profiles and LIST/addressing options.
Registration is a sequence CAS: **plan and apply each store before planning the
next** if you want to avoid a stale sequence. Up to 16 stores are supported.

## Recover an uncertain registration

If a reply is lost or the client terminates, retain the original plan. Run
`store-status`, or rerun `store-apply` with the same file and confirmation.
An observed successful operation must match the complete store descriptor,
original receipt sequence and current registered descriptor. An already observed
success returns without another Apply. If no operation is recorded, Apply may
submit only the frozen original request; authority deduplication handles a race
with a previously accepted request. No automatic sequence rebase or fresh ID
fallback is permitted.

`not-recorded` is a snapshot, not proof that a prior request was never sent.
`rejected` reports the retained domain error. A stale sequence conflict remains
rejected on replay. Inspect the original operation before deliberately preparing
a new plan in a new directory. Invalid or partially written plans are refused;
neither plans nor configuration files are overwritten or silently repaired.

## Generate this computer's configuration

```text
s3drive managed configure sources.toml --plan /absolute/docs/store-plan.toml --token-file TOKEN --directory /home/me/.config/s3drive/managed-drive
```

This verifies the registered receipt and writes a new private `drive.toml`.
Default output is version 3/read-only. Input configuration stays intact; source
profiles remain local names. A second computer can use another alias/profile
with the same physical route:

```text
s3drive managed configure this-computer.toml --plan /absolute/copied/store-plan.toml --map STORE_ID=local-docs --token-file LOCAL_TOKEN --directory /absolute/private/managed-drive
```

Repeat `--plan` and `--map` for multiple stores. Plans must share one endpoint,
volume, naming policy and trust mode. Duplicate IDs, unknown mappings, route
changes and unregistered plans are refused. An omitted mapping uses the plan's
source hint exactly; the CLI does not search for a convenient substitute.

Explicit writes require an input configuration that already contains valid
[write policies and provider evidence](write-policy.md), with each writable
source mapped. For example:

```text
s3drive managed configure write-enabled-sources.toml --plan /absolute/docs/store-plan.toml --token-file TOKEN --mode read-write --default-store STORE_ID --state-directory /absolute/new-managed-state --directory /absolute/private/managed-drive
```

The state directory must not exist and cannot reuse the input's direct journal
or overlap the output directory. Setup validates all local policy/evidence
documents and the default store's future key scope, size and evidence age. It
does not grant a provider send permit or verify resolved credentials; mounts
still enforce the full runtime policy. The volume must use portable-collision-v1.
Normal writable output is version 6. Add explicit `--windows-disposition posix-v1`
for version 7 and the accepted WinFsp lifecycle profile; versions 1–6 retain their
existing unsupported native Windows deletion paths. This profile has documented
[WinFsp differences from NTFS](native-windows-lifecycle.md).

Mount the generated file with `s3drive mount /absolute/managed-drive/drive.toml
/absolute/mountpoint`. Setup does not mount or install startup automatically;
use the [per-account startup design](startup-and-sessions.md), including the
no-systemd Linux supervisor. Physical workstation reboot, real multi-host
coordination, desktop Windows 10/11 and hosted authority supervision still need
separate acceptance. A successful setup command establishes none of those.

## Verification contract

The CLI integration harness uses real subprocesses, TCP, SQLite and Rust TLS.
It checks two stores/alternate local profiles without provider credentials,
Unicode prefix composition, confirmation/route/unknown-field refusal before RPC,
concurrent stale-plan rejection, no automatic rebase, lost replies, a killed
client after commit, and exact custom-CA binding. A listener records that setup
never contacts the synthetic S3 endpoint. Existing shared-engine/native fixtures
now use CLI registration and generated read-only configurations; mutation and
Windows lifecycle cases use generated writable configurations. Native acceptance
requires terminal exact-source hosted gates, not the local portable test result.

The final local gate passed on 2026-10-17: 257 state tests, ten HTTP, five TLS,
17 SDK, six bootstrap CLI and six shared-engine cases; formatting, Clippy,
builds and 55-document link/mdBook checks passed. Existing state and default-root
policy refusal tests preserved dirty fixture bytes and issued no provider
mutations. This local result excludes the seven ignored native mount cases;
require exact-source hosted Linux/Windows acceptance before publication.
