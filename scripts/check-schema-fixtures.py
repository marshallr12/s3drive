#!/usr/bin/env python3
"""Reject native fixture expectations that disagree with the Rust state schema."""
from pathlib import Path
import re
import sys


root = Path(__file__).resolve().parent.parent
versions = re.findall(
    r"^pub const SCHEMA_VERSION: i32 = (\d+);$",
    (root / "crates/s3drive-state/src/lib.rs").read_text(),
    re.MULTILINE,
)
if len(versions) != 1:
    sys.exit("Cannot identify the single Rust state schema version")

checks = (
    ("scripts/test-windows-session.ps1", r"-notmatch 'schema=(\d+), journal=wal'"),
    ("scripts/test-windows-snapshot-worker.ps1", r"\$value\.manifest\.schema_version\s+-ne\s+(\d+)"),
)
for filename, pattern in checks:
    actual = re.findall(pattern, (root / filename).read_text())
    if actual != versions:
        sys.exit(f"{filename}: expected exactly one assertion for state schema {versions[0]}")

print(f"Native fixture schema expectations OK ({versions[0]}; {len(checks)} fixtures).")
