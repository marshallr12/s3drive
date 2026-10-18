#!/usr/bin/env python3
"""Linux-only disposable CLI cancellation check; not a restore/deployment gate.

Starts one private loopback fixture authority, stops it, then grows SQLite's
freelist using disposable zero bytes. No live configuration, credentials or S3.
Requires an explicitly supplied, source-verified snapshot-capable executable.
Allocates about 128 MiB plus a partial copy; cleans only its own temp directory.
"""
import hashlib
import json
import os
import pathlib
import secrets
import select
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time

if not sys.platform.startswith("linux") or len(sys.argv) != 2:
    raise SystemExit("Usage (Linux only): authority-snapshot-sigint.py VERIFIED_S3DRIVE_BINARY")
binary = pathlib.Path(sys.argv[1]).resolve(strict=True)
os.umask(0o077)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


processes = []
with tempfile.TemporaryDirectory(prefix="s3drive-snapshot-sigint-") as scratch:
    root = pathlib.Path(scratch)
    environment = dict(os.environ, AWS_EC2_METADATA_DISABLED="true",
                       AWS_CONFIG_FILE=str(root / "absent-config"),
                       AWS_SHARED_CREDENTIALS_FILE=str(root / "absent-credentials"))
    secret = root / "token"
    secret.write_text(secrets.token_hex(32))
    secret.chmod(0o600)
    source = root / "source 資料 café 🦀"
    output = root / "partial snapshot 資料 café 🦀"
    policy = "portable-collision-v1"
    try:
        authority = subprocess.Popen([
            str(binary), "authority-serve", "--directory", str(source),
            "--token-file", str(secret), "--name-policy", policy,
            "--listen", "127.0.0.1:0", "--stop-on-stdin-close",
        ], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment)
        processes.append(authority)
        if not select.select([authority.stdout], [], [], 20)[0]:
            raise RuntimeError("fixture readiness deadline")
        ready = json.loads(authority.stdout.readline())
        volume = ready["volume"]
        authority.stdin.close()
        assert authority.wait(timeout=20) == 0, "fixture did not stop"
        database = source / "metadata.sqlite3"
        # No new namespace/history records. Dropping this scratch table leaves
        # a valid original namespace with free pages, making copying observable.
        db = sqlite3.connect(database)
        assert db.execute("PRAGMA auto_vacuum").fetchone() == (0,)
        assert db.execute("PRAGMA journal_mode=DELETE").fetchone() == ("delete",)
        db.execute("CREATE TABLE sigint_fixture_padding(data BLOB)")
        db.execute("INSERT INTO sigint_fixture_padding VALUES(zeroblob(?))", (128 * 1024 * 1024,))
        db.commit()
        db.execute("DROP TABLE sigint_fixture_padding")
        db.commit()
        free = db.execute("PRAGMA freelist_count").fetchone()[0]
        assert free > 32
        assert db.execute("PRAGMA integrity_check").fetchone() == ("ok",)
        db.close()
        original = digest(database)
        args = [str(binary), "authority-snapshot", "create", "--directory", str(source),
                "--output", str(output), "--expect-volume", volume, "--name-policy", policy]
        child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, env=environment)
        processes.append(child)
        deadline = time.monotonic() + 20
        while not output.is_dir():
            assert child.poll() is None, "snapshot completed/refused before observed boundary"
            if time.monotonic() >= deadline:
                raise RuntimeError("snapshot creation boundary deadline")
            time.sleep(0.001)
        # send_signal acts on this owned, unreaped Popen, never a PID-file hint.
        started = time.monotonic()
        child.send_signal(signal.SIGINT)
        stdout, stderr = child.communicate(timeout=20)
        elapsed = time.monotonic() - started
        assert child.returncode != 0 and b"Cancelled" in stderr, "cooperative cancellation not observed"
        assert not stdout, "cancelled CLI reported success"
        assert not (output / "snapshot.json").exists(), "cancellation reached completion publication instead"
        assert digest(database) == original, "source bytes changed"
        inspect = subprocess.run([
            str(binary), "authority-snapshot", "inspect", "--directory", str(output),
            "--expect-volume", volume, "--name-policy", policy,
        ], stdin=subprocess.DEVNULL, capture_output=True, timeout=20, env=environment)
        assert inspect.returncode != 0 and not inspect.stdout, "partial was inspectable as complete"
        retry = subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True,
                               timeout=20, env=environment)
        assert retry.returncode != 0 and not retry.stdout and b"Destination" in retry.stderr
        assert digest(database) == original
        print(json.dumps({
            "scope": "Linux CLI SIGINT; disposable private SQLite and loopback fixtures; no S3",
            "binary_sha256": digest(binary), "source_database_bytes": database.stat().st_size,
            "freelist_pages": free, "signal": "SIGINT", "cooperative_cancel_observed": True,
            "cancel_to_exit_seconds": round(elapsed, 6), "success_stdout": False,
            "completion_manifest_absent": True, "partial_inspection_refused": True,
            "same_destination_retry_refused": True, "source_sha256_preserved": True,
            "restore_authorized": False,
        }))
    finally:
        for child in processes:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=20)
