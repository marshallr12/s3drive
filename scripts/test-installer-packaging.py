#!/usr/bin/env python3
"""Portable packaging refusal tests, not native MSI acceptance."""
import importlib.util
import json
from pathlib import Path
import tempfile
import shutil
from unittest.mock import patch
import unittest
import xml.etree.ElementTree as ET

spec = importlib.util.spec_from_file_location('installer', Path(__file__).with_name('build-windows-installer.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class InstallerTests(unittest.TestCase):
    def test_rollback_log_requires_executed_fault_and_ordered_rollback(self):
        spec = importlib.util.spec_from_file_location('rollback', Path(__file__).with_name('check-msi-rollback.py'))
        checker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(checker)
        lines = [
            'Action start 12:00:00: RemoveExistingProducts.',
            'MSI (s): Executing op: FileCopy(SourceName=s3drive.exe)',
            'MSI (s): Executing op: CustomActionSchedule(Action=Wix4FailWhenDeferred_X64,ActionType=3073)',
            'CustomAction Wix4FailWhenDeferred_X64 returned actual error code 1603',
            'MSI (s): Executing op: Header(Signature=1,ScriptType=2,ScriptMajorVersion=21)',
            'Action ended 12:00:01: InstallFinalize. Return value 3.',
        ]
        self.assertTrue(checker.verify('\n'.join(lines)))
        for index in range(len(lines)):
            with self.subTest(missing=index), self.assertRaises(ValueError):
                checker.verify('\n'.join(lines[:index] + lines[index+1:]))
        with self.assertRaises(ValueError):
            checker.verify('WIXFAILWHENDEFERRED=1 FailWhenDeferred error 1603')
        with self.assertRaises(ValueError):
            checker.verify('\n'.join(reversed(lines)))

    def test_project_license_is_gpl_version_three_only(self):
        manifest = m.tomllib.loads((m.ROOT / 'Cargo.toml').read_text())
        self.assertEqual(manifest['workspace']['package']['license'], 'GPL-3.0-only')
        self.assertIn('GNU GENERAL PUBLIC LICENSE', (m.ROOT / 'LICENSE').read_text())
        self.assertIn('Version 3, 29 June 2007', (m.ROOT / 'LICENSE').read_text())
        self.assertIn('GPL-3.0-only', (m.ROOT / 'COPYRIGHT').read_text())
        for path in (m.ROOT / 'crates').glob('*/Cargo.toml'):
            self.assertTrue(m.tomllib.loads(path.read_text())['package']['license']['workspace'])
        self.assertIn('COPYRIGHT', m.FILES)

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
        # Pinned WixUI_Minimal already defines this property; duplicating it fails linking.
        self.assertNotIn('<Property Id="ARPNOMODIFY"', product)

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

    def test_pinned_fallback_identity_and_tamper_refusal(self):
        key = 'asn1-rs-impl-0.2.0'
        record = json.loads((m.ROOT / 'packaging/notices/provenance.json').read_text())[key]
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            source = root / 'crate'
            source.mkdir()
            vcs = source / '.cargo_vcs_info.json'
            vcs.write_text(json.dumps({'git': {'sha1': record['upstream_commit']}}))
            package = {'name': 'asn1-rs-impl', 'version': '0.2.0', 'license': record['license'],
                       'manifest_path': str(source / 'Cargo.toml')}
            shutil.copyfile(m.ROOT / 'Cargo.lock', root / 'Cargo.lock')
            shutil.copytree(m.ROOT / 'packaging/notices', root / 'packaging/notices')
            with patch.object(m, 'ROOT', root):
                base, paths, result = m.notice_fallback(package)
                self.assertEqual(len(paths), 2)
                self.assertEqual(result, record)
                vcs.write_text(json.dumps({'git': {'sha1': '0' * 40}}))
                with self.assertRaisesRegex(ValueError, 'upstream source identity'):
                    m.notice_fallback(package)
                vcs.write_text(json.dumps({'git': {'sha1': record['upstream_commit']}}))
                paths[0].write_text('tampered notice')
                with self.assertRaisesRegex(ValueError, 'notice content mismatch'):
                    m.notice_fallback(package)

    def test_notice_source_escape_and_symlink_refused(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            outside = root / 'outside'
            outside.write_text('not a dependency')
            package = root / 'package'
            package.mkdir()
            with self.assertRaises(ValueError):
                m.regular(outside, package)
            with self.assertRaises(ValueError):
                m.regular(package / '..' / 'outside', package)
            metadata = {'packages': [{'name': 'example', 'version': '1.0.0', 'id': 'example',
                'source': 'registry+https://github.com/rust-lang/crates.io-index',
                'manifest_path': str(package / 'Cargo.toml'), 'license_file': '../outside'}]}
            with self.assertRaisesRegex(ValueError, 'escapes package'):
                m.collect_notices(metadata, root / 'escaped-output')
            self.assertFalse((root / 'escaped-output').exists())
            link = package / 'LICENSE'
            try:
                link.symlink_to(outside)
            except OSError:
                self.skipTest('symlinks unavailable')
            with self.assertRaises(ValueError):
                m.regular(link, package)


if __name__ == '__main__':
    unittest.main()
