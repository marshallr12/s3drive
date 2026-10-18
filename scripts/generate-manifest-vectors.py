#!/usr/bin/env python3
"""Independent fixed vectors for the offline S3DMAN01 page specification.

No Rust codec is imported. Re-running overwrites only these named test vectors.
"""
import hashlib
from pathlib import Path
import struct

DESTINATION = Path(__file__).resolve().parents[1] / "crates/s3drive-core/src/manifest/fixtures"
CHUNK = 1024 * 1024
DOMAIN = b"s3drive-manifest-page-v1\0"


def page(level, length, entries):
    return struct.pack("<8sBBH16s16sQQI", b"S3DMAN01", level, 0, len(entries),
                       bytes([1]) * 16, bytes([2]) * 16, 0, length, CHUNK) + b"".join(entries)


def reference(generation, ordinal, digest):
    return struct.pack("<16s16sQ32s", bytes([3]) * 16, bytes([generation]) * 16, ordinal, digest)


def main():
    data = "hello\0café 🦀".encode()
    digest = hashlib.sha256(data).digest()
    empty = page(0, 0, [])
    leaf = page(0, CHUNK + len(data), [struct.pack("<BQ", 0, CHUNK),
                struct.pack("<BQ", 1, len(data)) + reference(4, 7, digest)])
    root = page(1, CHUNK + len(data), [struct.pack("<BQ", 2, CHUNK + len(data))
                + reference(5, 9, hashlib.sha256(DOMAIN + leaf).digest())])
    DESTINATION.mkdir(parents=True, exist_ok=True)
    for name, raw in [("empty", empty), ("leaf", leaf), ("root", root), ("data", data)]:
        checksum = hashlib.sha256((b"" if name == "data" else DOMAIN) + raw).hexdigest()
        (DESTINATION / f"{name}.bin").write_bytes(raw)
        (DESTINATION / f"{name}.sha256").write_text(checksum + "\n")
        print(name, len(raw), checksum)


if __name__ == "__main__":
    main()
