#!/usr/bin/env python3
"""Private developer bundle only; no installation, signing or release license grant."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
ALLOWLIST = (
    "README.md", "docs/src/managed-bootstrap.md", "docs/src/beta-completion.md",
    "scripts/linux-session.py", "scripts/windows-session.ps1",
    "scripts/package-test.py",
)
MAX_BYTES = 512 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True, timeout=30).strip()


def safe_name(name):
    p = PurePosixPath(name)
    if not name or p.is_absolute() or ".." in p.parts or "\\" in name or ":" in name:
        raise ValueError("unsafe bundle path")
    if str(p) != name:
        raise ValueError("noncanonical bundle path")
    return name


def write_bundle(path, entries, metadata):
    if "manifest.json" in entries or sum(map(len, entries.values())) > MAX_BYTES:
        raise ValueError("invalid bundle size or reserved entry")
    records = {}
    for name, data in sorted(entries.items()):
        safe_name(name)
        records[name] = {"bytes": len(data), "sha256": digest(data)}
    manifest = {"format": 1, "purpose": "private-developer-test-only", **metadata, "files": records}
    content = {**entries, "manifest.json": (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode()}
    # Exclusive creation: never overwrite another test's result or a user file.
    with path.open("xb") as output, zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(content.items()):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            mode = 0o755 if name.startswith("bin/") else 0o644
            info.external_attr = (stat.S_IFREG | mode) << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    verify_bundle(path)


def verify_bundle(path):
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = [safe_name(i.filename) for i in entries]
        if len(names) != len(set(names)) or sum(i.file_size for i in entries) > MAX_BYTES:
            raise ValueError("duplicate entries or bundle size limit")
        if any(stat.S_IFMT(i.external_attr >> 16) != stat.S_IFREG for i in entries):
            raise ValueError("bundle contains nonregular entry")
        info = archive.getinfo("manifest.json")
        if info.file_size > 1024 * 1024:
            raise ValueError("manifest too large")
        manifest = json.loads(archive.read(info))
        if manifest.get("format") != 1 or manifest.get("purpose") != "private-developer-test-only":
            raise ValueError("unknown bundle contract")
        files = manifest["files"]
        if set(names) != {"manifest.json", *files}:
            raise ValueError("manifest entry set mismatch")
        for name, expected in files.items():
            data = archive.read(name)
            if len(data) != expected["bytes"] or digest(data) != expected["sha256"]:
                raise ValueError("bundle content mismatch")
        return manifest


def build(binary, output, target, dependencies):
    binary = binary.resolve(strict=True)
    output = output.resolve()
    if output.is_relative_to(ROOT):
        raise ValueError("private bundle output must stay outside the repository")
    if command("git", "status", "--porcelain", "--untracked-files=no"):
        raise ValueError("bundle requires a clean tracked source tree")
    if not binary.is_file() or binary.stat().st_size > MAX_BYTES:
        raise ValueError("invalid test binary")
    version = command(str(binary), "--version")
    if not version.startswith("s3drive "):
        raise ValueError("unexpected binary version output")
    entries = {"bin/" + binary.name: binary.read_bytes()}
    for name in ALLOWLIST:
        path = ROOT / name
        if path.is_symlink() or not path.is_file():
            raise ValueError("allowlisted source must be a regular file")
        entries[name] = path.read_bytes()
    packages = json.loads(dependencies.read_text(encoding="utf-8"))["packages"]
    inventory = [{"name": p["name"], "version": p["version"], "license": p.get("license")} for p in packages]
    entries["dependency-license-inventory.json"] = (json.dumps(sorted(inventory, key=lambda p: (p["name"], p["version"])), indent=2) + "\n").encode()
    entries["TEST-BUNDLE-NOTICE.txt"] = (
        "Private developer test bundle, not a licensed/signed public distribution.\n"
        "No WinFsp/FUSE driver, credentials, configuration, database or user data included.\n"
        "No automatic installation, startup registration, elevation or reboot.\n"
        "The dependency inventory is not legal clearance or a complete license packet.\n"
        "Build provenance identifies the source checkout and binary digest; it is not code signing.\n"
    ).encode()
    write_bundle(output, entries, {
        "source_commit": command("git", "rev-parse", "HEAD"),
        "source_tree": command("git", "rev-parse", "HEAD^{tree}"),
        "target": target, "compiler": command("rustc", "--version", "--verbose"),
        "version": version, "cargo_lock_sha256": digest((ROOT / "Cargo.lock").read_bytes()),
    })
    # Execute only the verified bundled CLI, in an isolated disposable directory.
    # No mount/configuration/startup commands and no driver installation.
    with tempfile.TemporaryDirectory(prefix="s3drive-package-smoke-") as temp:
        packaged = Path(temp) / binary.name
        with zipfile.ZipFile(output) as archive:
            packaged.write_bytes(archive.read("bin/" + binary.name))
        packaged.chmod(0o700)
        if command(str(packaged), "--version") != version:
            raise ValueError("packaged CLI version mismatch")
        if "Usage:" not in command(str(packaged), "--help"):
            raise ValueError("packaged CLI help smoke failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", type=Path)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--target")
    parser.add_argument("--dependencies", type=Path, help="cargo metadata --locked --format-version 1 output")
    args = parser.parse_args()
    if args.verify:
        if any((args.binary, args.output, args.target, args.dependencies)):
            parser.error("verify does not accept build options")
        verify_bundle(args.verify)
        print("TEST_BUNDLE_VERIFIED")
    else:
        if not all((args.binary, args.output, args.target, args.dependencies)):
            parser.error("build requires binary, output, target and dependencies")
        build(args.binary, args.output, args.target, args.dependencies)
        print("TEST_BUNDLE_CREATED_AND_VERIFIED")


if __name__ == "__main__":
    main()
