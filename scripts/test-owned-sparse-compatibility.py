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
old_schema = int(os.environ.get("S3DRIVE_COMPAT_OLD_SCHEMA", "46"))
new_schema = int(os.environ.get("S3DRIVE_COMPAT_NEW_SCHEMA", "47"))
assert (old_schema, new_schema) in [(46, 47), (47, 48), (48, 49), (49, 50)]
report = {"format": 1,
          "kind": {46: "owned-sparse-schema-compatibility", 47: "native-sparse-close-schema-compatibility", 48: "retry-checkpoint-schema-compatibility", 49: "local-retention-schema-compatibility"}[old_schema],
          "refusal_cases": [], "old_schema": old_schema, "new_schema": new_schema,
          "invocations": [], "success": False,
          "old_source_commit": os.environ.get("S3DRIVE_COMPAT_OLD_SHA", "5343eb41b6132ba3a8a5cfc2f74b4d6ff01d0b28"),
          "new_source_commit": os.environ.get("S3DRIVE_COMPAT_NEW_SHA", os.environ.get("GITHUB_SHA", "local-uncommitted"))}
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
    invoke(old, legacy, old_schema)
    with sqlite3.connect(legacy / "metadata.sqlite3") as db:
        db.execute("INSERT INTO legacy_cached_objects_v1 VALUES(?,?,?,?,?)", ("café", "日本語 🦀", '"legacy"', None, 17))
    db.close()
    before = contents(legacy)
    invoke(new, legacy, new_schema)
    after = contents(legacy)
    if old_schema == 48:
        additions = [row for row in after if row.startswith("CREATE TABLE sparse_retry_mode (")]
        assert len(additions) == 1, "expected exactly one new retry-mode table"
        expected_ddl = (Path(__file__).resolve().parent.parent / "crates/s3drive-state/migrations/049.sql").read_text().split("CREATE TABLE", 1)[1].strip()
        assert additions[0] == "CREATE TABLE " + expected_ddl, "retry-mode DDL differs"
        assert [row for row in after if row not in additions] == before, "legacy schema or rows were rewritten"
        with closing(sqlite3.connect(legacy / "metadata.sqlite3")) as db:
            assert db.execute("SELECT count(*) FROM sparse_retry_mode").fetchone() == (0,)
        report["new_empty_retry_table_verified"] = True
        report["legacy_existing_dump_unchanged"] = True
    elif old_schema == 49:
        names = ["sparse_retention_role", "sparse_retention_anchors"]
        additions = [row for row in after if any(row.startswith(f"CREATE TABLE {name} (") for name in names)]
        assert len(additions) == 2, "expected exactly two additive retention tables"
        source = (Path(__file__).resolve().parent.parent / "crates/s3drive-state/migrations/050.sql").read_text()
        expected = ["CREATE TABLE" + part.split(";", 1)[0] + ";" for part in source.split("CREATE TABLE")[1:]]
        assert sorted(additions) == sorted(expected), "retention DDL differs"
        assert [row for row in after if row not in additions] == before, "legacy schema or rows were rewritten"
        with closing(sqlite3.connect(legacy / "metadata.sqlite3")) as db:
            for name in names:
                assert db.execute(f"SELECT count(*) FROM {name}").fetchone() == (0,)
        report["new_empty_retention_tables_verified"] = True
        report["legacy_existing_dump_unchanged"] = True
    else:
        assert after == before, "legacy schema or rows were rewritten"
    report["legacy_dump_statements"] = len(before)
    report["legacy_dump_sha256"] = hashlib.sha256(json.dumps(before, ensure_ascii=False).encode()).hexdigest()
    report["legacy_complete_dump_unchanged"] = old_schema not in (48, 49)
    for directory in [legacy, fresh]:
        if directory == fresh:
            invoke(new, directory, new_schema)
        before_bytes = bytes_on_disk(directory)
        invoke(old, directory)
        after_bytes = bytes_on_disk(directory)
        report["refusal_cases"].append({"directory": directory.name, "before": before_bytes, "after": after_bytes})
        assert after_bytes == before_bytes, "old reader mutated current database/sidecars"
        invoke(new, directory, new_schema)
    report["refusal_bytes_unchanged"] = True
    report["success"] = True
finally:
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(report_path.read_text())
