#!/usr/bin/env python3
"""Portable packaging refusal tests, not native MSI acceptance."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET

spec = importlib.util.spec_from_file_location('installer', Path(__file__).with_name('build-windows-installer.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class InstallerTests(unittest.TestCase):
    def test_version_bounds(self):
        self.assertEqual(m.version_tuple('0.1.0'), (0, 1, 0))
        for bad in ('1.2', '1.2.3.4', '1.2.3-beta', '256.0.0', '0.256.0', '0.0.65536', '../a'):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                m.version_tuple(bad)

    def test_payload_has_stable_components_and_no_user_data_actions(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            payload = root / 'payload'
            (payload / 'docs').mkdir(parents=True)
            (payload / 's3drive.exe').write_bytes(b'not executable')
            (payload / 'docs' / 'license & notice.txt').write_text('test')
            a, b = root / 'a.wxs', root / 'b.wxs'
            m.payload_xml(payload, a)
            m.payload_xml(payload, b)
            self.assertEqual(a.read_bytes(), b.read_bytes())
            tree = ET.parse(a)
            self.assertEqual(len(tree.findall('.//{*}File')), 2)
            self.assertEqual(len(tree.findall('.//{*}Component')), 2)
            self.assertEqual(tree.findall('.//{*}RemoveFile'), [])
        product = (m.ROOT / 'packaging/windows/product.wxs').read_text()
        for prohibited in ('<CustomAction', '<RemoveFile', '<RemoveFolder', '<ServiceInstall', '<Environment'):
            self.assertNotIn(prohibited, product)
        self.assertIn('Installed OR WINFSP_RUNTIME', product)
        self.assertIn('Schedule="afterInstallInitialize"', product)
        self.assertIn('MSIRESTARTMANAGERCONTROL', product)

    def test_notice_collection_and_missing_notice_refusal(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            source = root / 'crate'
            source.mkdir()
            (source / 'Cargo.toml').touch()
            (source / 'LICENSE-MIT').write_text('upstream license')
            (source / 'vendor').mkdir()
            (source / 'vendor' / 'NOTICE').write_text('vendored notice')
            metadata = {'packages': [{'name': 'example', 'version': '1.0.0', 'id': 'example', 'source': 'registry+https://github.com/rust-lang/crates.io-index', 'manifest_path': str(source / 'Cargo.toml'), 'license': 'MIT'}]}
            out = root / 'out'
            out.mkdir()
            inventory = m.collect_notices(metadata, out)
            self.assertEqual(len(inventory[0]['notices']), 2)
            self.assertEqual(json.loads((out / 'inventory.json').read_text()), inventory)
            (source / 'LICENSE-MIT').unlink()
            (source / 'vendor' / 'NOTICE').unlink()
            with self.assertRaisesRegex(ValueError, 'missing license/notice'):
                m.collect_notices(metadata, root / 'missing')
            metadata['packages'][0]['name'] = '../escape'
            with self.assertRaisesRegex(ValueError, 'unsafe dependency'):
                m.collect_notices(metadata, root / 'escape')

    def test_notice_source_escape_and_symlink_refused(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            outside = root / 'outside'
            outside.write_text('not a dependency')
            package = root / 'package'
            package.mkdir()
            with self.assertRaises(ValueError):
                m.regular(outside, package)
            link = package / 'LICENSE'
            try:
                link.symlink_to(outside)
            except OSError:
                self.skipTest('symlinks unavailable')
            with self.assertRaises(ValueError):
                m.regular(link, package)


if __name__ == '__main__':
    unittest.main()
