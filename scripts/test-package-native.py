#!/usr/bin/env python3
"""Hosted-only real-binary repeat packaging and relocated offline state startup.

Bundles stay private on the disposable runner. This proves deterministic packing
of one built binary, NOT reproducible compiler output across machines/toolchains.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import zipfile

spec = importlib.util.spec_from_file_location("package_test", Path(__file__).with_name("package-test.py"))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


def run(*args):
    return subprocess.run(list(map(str, args)), check=True, capture_output=True, text=True, timeout=30)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--dependencies", required=True, type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output-root", required=True, type=Path)
    args = parser.parse_args()
    root = args.output_root.resolve()
    root.mkdir(exist_ok=False)
    a, b = root / "first.zip", root / "second.zip"
    bundle.build(args.binary, a, args.target, args.dependencies)
    bundle.build(args.binary, b, args.target, args.dependencies)
    assert a.read_bytes() == b.read_bytes(), "real-binary package bytes are not deterministic"
    manifest = bundle.verify_bundle(a)
    assert manifest == bundle.verify_bundle(b)
    relocated = root / "relocated application 日本語"
    relocated.mkdir()
    binary = relocated / args.binary.name
    with zipfile.ZipFile(a) as z:
        binary.write_bytes(z.read("bin/" + args.binary.name))
    binary.chmod(0o700)
    manager_name = "windows-session.ps1" if os.name == "nt" else "linux-session.py"
    manager = relocated / manager_name
    with zipfile.ZipFile(a) as z:
        manager.write_bytes(z.read("scripts/" + manager_name))
    assert hashlib.sha256(manager.read_bytes()).hexdigest() == manifest["files"]["scripts/" + manager_name]["sha256"]
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == manifest["files"]["bin/" + args.binary.name]["sha256"]
    # Shipped CLI parser and durable SQLite initialization, no provider access.
    config = relocated / "offline.toml"
    config.write_text("version=1\nmode='read-only'\n[[sources]]\nalias='docs'\nbucket='fixture'\nregion='us-east-1'\nendpoint='http://127.0.0.1:1'\nallow_http_loopback=true\n", encoding="utf-8")
    run(binary, "config-check", config)
    state = relocated / "private state"
    run(binary, "init-state", "--directory", state)
    database = state / "metadata.sqlite3"
    def snapshot():
        with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as db:
            assert db.execute("PRAGMA integrity_check").fetchall() == [("ok",)]
            assert db.execute("PRAGMA foreign_key_check").fetchall() == []
            return db.execute("PRAGMA user_version").fetchone(), db.execute("SELECT type,name,sql FROM sqlite_schema ORDER BY type,name").fetchall()
    first = snapshot()
    assert first[0][0] == 50
    run(binary, "init-state", "--directory", state)
    assert snapshot() == first, "idempotent startup changed schema"
    # A syntactically invalid configuration must fail before any network/mount.
    invalid = relocated / "invalid.toml"
    invalid.write_text("version='not-an-integer'\n", encoding="utf-8")
    bad = subprocess.run([str(binary), "config-check", str(invalid)], capture_output=True, timeout=30)
    assert bad.returncode != 0
    assert snapshot() == first
    print("HOSTED_PACKAGE " + json.dumps({"source": manifest["source_commit"], "tree": manifest["source_tree"], "target": args.target, "zip_sha256": hashlib.sha256(a.read_bytes()).hexdigest(), "binary_sha256": manifest["files"]["bin/" + args.binary.name]["sha256"], "repeat_archive_bytes": "identical", "relocated_schema": 50, "integrity": "ok"}, sort_keys=True))
    envfile = os.environ.get("GITHUB_ENV")
    if envfile:
        assert "\n" not in str(binary) and "\r" not in str(binary)
        with open(envfile, "a", encoding="utf-8") as stream:
            stream.write("S3DRIVE_TEST_PACKAGED_CLI=" + str(binary) + "\n")
            stream.write("S3DRIVE_TEST_PACKAGED_SESSION=" + str(manager) + "\n")


if __name__ == "__main__":
    main()
