#!/usr/bin/env python3
"""Linux-only disposable CLI/SQLite source-lock experiment; no S3/user state.

Supply a verified snapshot-capable binary. The temporary authority is stopped
before an independent raw SQLite writer holds EXCLUSIVE in DELETE journal mode.
This deliberately uncooperative writer tests SQL locking beyond the app lock.
"""
import hashlib
import json
import os
import pathlib
import secrets
import select
import sqlite3
import subprocess
import sys
import tempfile
import time

if not sys.platform.startswith("linux") or len(sys.argv) != 2:
    raise SystemExit("Usage (Linux only): authority-snapshot-busy.py VERIFIED_S3DRIVE_BINARY")
binary = pathlib.Path(sys.argv[1]).resolve(strict=True)
os.umask(0o077)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


with tempfile.TemporaryDirectory(prefix="s3drive-snapshot-busy-") as scratch:
    root = pathlib.Path(scratch)
    source, output = root / "source 資料 café 🦀", root / "snapshot"
    env = dict(os.environ, AWS_EC2_METADATA_DISABLED="true",
               AWS_CONFIG_FILE=str(root / "absent-config"),
               AWS_SHARED_CREDENTIALS_FILE=str(root / "absent-credentials"))
    secret = root / "token"
    secret.write_text(secrets.token_hex(32))
    secret.chmod(0o600)
    policy = "portable-collision-v1"
    child = subprocess.Popen([
        str(binary), "authority-serve", "--directory", str(source),
        "--token-file", str(secret), "--name-policy", policy,
        "--listen", "127.0.0.1:0", "--stop-on-stdin-close",
    ], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    try:
        assert select.select([child.stdout], [], [], 20)[0], "fixture readiness deadline"
        volume = json.loads(child.stdout.readline())["volume"]
        child.stdin.close()
        assert child.wait(timeout=20) == 0
        database = source / "metadata.sqlite3"
        db = sqlite3.connect(database)
        assert db.execute("PRAGMA journal_mode=DELETE").fetchone() == ("delete",)
        db.close()
        original = digest(database)
        args = [str(binary), "authority-snapshot", "create", "--directory", str(source),
                "--output", str(output), "--expect-volume", volume,
                "--name-policy", policy, "--timeout-seconds", "2"]
        locker = sqlite3.connect(database)
        locker.execute("BEGIN EXCLUSIVE")
        try:
            started = time.monotonic()
            refused = subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True,
                                     timeout=10, env=env)
            elapsed = time.monotonic() - started
            assert refused.returncode != 0 and not refused.stdout
            assert elapsed < 2, "source locking exceeded the configured cooperative budget"
            assert not output.exists(), "locked source created destination"
        finally:
            locker.rollback()
            locker.close()
        # Never raw-open/close a DB file in a process with a live SQLite
        # connection to it: doing so can release that process's POSIX locks.
        assert digest(database) == original
        completed = subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True,
                                   timeout=10, env=env)
        assert completed.returncode == 0, "released valid source still refused"
        created = json.loads(completed.stdout)
        inspected = subprocess.run([
            str(binary), "authority-snapshot", "inspect", "--directory", str(output),
            "--expect-volume", volume, "--name-policy", policy,
        ], stdin=subprocess.DEVNULL, capture_output=True, timeout=10, env=env)
        assert inspected.returncode == 0 and json.loads(inspected.stdout) == created
        assert created["restore_authorized"] is False
        assert created["history_continuity_proven"] is False
        assert digest(database) == original
        print(json.dumps({
            "scope": "Linux CLI with an independent SQLite EXCLUSIVE source writer; private disposable fixtures; no S3",
            "binary_sha256": digest(binary), "cooperative_budget_seconds": 2,
            "source_lock_refused_seconds": round(elapsed, 6),
            "locked_source_refused": True, "no_destination_on_lock_refusal": True,
            "create_after_release_succeeded": True, "inspection_matches_creation": True,
            "source_sha256_preserved": True, "restore_authorized": False,
            "history_continuity_proven": False,
        }))
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=20)
