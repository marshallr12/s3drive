#!/usr/bin/env python3
"""Actual old/new executables, unchanged refusal bytes and complete legacy rows."""
import hashlib
from contextlib import closing
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys

old, new, root = (Path(p).resolve() for p in sys.argv[1:])
root.mkdir(parents=True, exist_ok=False)
report = {"format": 1, "kind": "owned-sparse-schema-compatibility", "refusal_cases": [], "old_schema": 46, "new_schema": 47, "invocations": [], "success": False,
          "old_source_commit": "5343eb41b6132ba3a8a5cfc2f74b4d6ff01d0b28",
          "new_source_commit": os.environ.get("GITHUB_SHA", "local-uncommitted")}
report_path = root / "report.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def invoke(binary, directory, version=None):
    result = subprocess.run([str(binary), "init-state", "--directory", str(directory)],
                            text=True, capture_output=True, timeout=60)
    item = {"binary": binary.name, "directory": directory.name,
            "returncode": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
    report["invocations"].append(item)
    if version is None:
        assert result.returncode != 0, item
    else:
        assert result.returncode == 0 and f"schema={version}, journal=wal" in result.stdout, item


def bytes_on_disk(directory):
    return {p.name: digest(p) for p in directory.iterdir() if p.name.startswith("metadata.sqlite3")}


def contents(directory):
    with closing(sqlite3.connect(directory / "metadata.sqlite3")) as db:
        return list(db.iterdump())


try:
    report["old_binary_sha256"] = digest(old)
    report["new_binary_sha256"] = digest(new)
    assert report["old_binary_sha256"] != report["new_binary_sha256"]
    legacy = root / "legacy café 🦀"
    fresh = root / "fresh 日本語"
    invoke(old, legacy, 46)
    with sqlite3.connect(legacy / "metadata.sqlite3") as db:
        db.execute("INSERT INTO legacy_cached_objects_v1 VALUES(?,?,?,?,?)", ("café", "日本語 🦀", '"legacy"', None, 17))
    db.close()
    before = contents(legacy)
    invoke(new, legacy, 47)
    after = contents(legacy)
    assert after == before, "legacy schema or rows were rewritten"
    report["legacy_dump_statements"] = len(before)
    report["legacy_dump_sha256"] = hashlib.sha256(json.dumps(before, ensure_ascii=False).encode()).hexdigest()
    report["legacy_complete_dump_unchanged"] = True
    for directory in [legacy, fresh]:
        if directory == fresh:
            invoke(new, directory, 47)
        before_bytes = bytes_on_disk(directory)
        invoke(old, directory)
        after_bytes = bytes_on_disk(directory)
        report["refusal_cases"].append({"directory": directory.name, "before": before_bytes, "after": after_bytes})
        assert after_bytes == before_bytes, "old reader mutated current database/sidecars"
        invoke(new, directory, 47)
    report["refusal_bytes_unchanged"] = True
    report["success"] = True
finally:
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(report_path.read_text())
