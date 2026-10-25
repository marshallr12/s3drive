#!/usr/bin/env python3
"""Portable archive safety tests; not a real-binary/driver acceptance test."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("package_test", Path(__file__).with_name("package-test.py"))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


class BundleTests(unittest.TestCase):
    def test_round_trip_reproducible_no_overwrite_and_binary_tamper(self):
        with tempfile.TemporaryDirectory() as temp:
            a, b = Path(temp) / "a.zip", Path(temp) / "b.zip"
            entries = {"bin/s3drive": b"test fixture, not executable", "README.md": b"private"}
            bundle.write_bundle(a, entries, {"source_commit": "a" * 40})
            bundle.write_bundle(b, entries, {"source_commit": "a" * 40})
            self.assertEqual(a.read_bytes(), b.read_bytes())
            self.assertEqual(set(bundle.verify_bundle(a)["files"]), set(entries))
            with self.assertRaises(FileExistsError):
                bundle.write_bundle(a, entries, {})
            data = {}
            with zipfile.ZipFile(a) as archive:
                for name in archive.namelist():
                    data[name] = archive.read(name)
            data["bin/s3drive"] = b"changed"
            a.unlink()
            with zipfile.ZipFile(a, "w") as archive:
                for name, content in data.items():
                    info = zipfile.ZipInfo(name)
                    info.external_attr = 0o100644 << 16
                    archive.writestr(info, content)
            with self.assertRaisesRegex(ValueError, "content mismatch"):
                bundle.verify_bundle(a)

    def test_unsafe_names_refused(self):
        for name in ("../secret", "/absolute", "a/../b", "a\\b", "C:secret", "a//b", ""):
            with self.subTest(name=name), self.assertRaises(ValueError):
                bundle.safe_name(name)

    def test_missing_extra_and_symlink_entries_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            for mode in ("extra", "missing", "symlink", "duplicate"):
                path = Path(temp) / (mode + ".zip")
                bundle.write_bundle(path, {"bin/s3drive": b"data"}, {})
                with zipfile.ZipFile(path) as archive:
                    manifest = archive.read("manifest.json")
                path.unlink()
                with zipfile.ZipFile(path, "w") as archive:
                    items = [("manifest.json", manifest)]
                    if mode != "missing":
                        items.append(("bin/s3drive", b"data"))
                    if mode in ("extra", "duplicate"):
                        items.append(("unexpected" if mode == "extra" else "bin/s3drive", b"data"))
                    for name, data in items:
                        info = zipfile.ZipInfo(name)
                        info.external_attr = (0o120777 if mode == "symlink" and name.startswith("bin/") else 0o100644) << 16
                        archive.writestr(info, data)
                with self.subTest(mode=mode), self.assertRaises(ValueError):
                    bundle.verify_bundle(path)


if __name__ == "__main__":
    unittest.main()
