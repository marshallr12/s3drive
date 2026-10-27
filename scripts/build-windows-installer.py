#!/usr/bin/env python3
"""Build a private x64 MSI from a clean checkout and locked dependency sources."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib
import uuid
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
NS = 'http://wixtoolset.org/schemas/v4/wxs'
NAMESPACE = uuid.UUID('28ae71f8-3d39-4af2-88c5-76e28e2a83ef')
WIX_VERSION = '4.0.6'
NOTICE = re.compile(r'^(licen[cs]e|copying|copyright|notice|authors|patents)(?:$|[._-])', re.I)
FILES = ('LICENSE', 'THIRD-PARTY-NOTICES.md', 'README.md',
         'docs/src/windows-installer.md', 'docs/src/managed-bootstrap.md',
         'scripts/windows-session.ps1')


def run(*args):
    return subprocess.check_output(list(map(str, args)), cwd=ROOT, text=True, timeout=180).strip()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def regular(path, root):
    path = Path(path)
    if ('..' in path.parts or not path.is_relative_to(root)
            or not path.resolve().is_relative_to(root.resolve())
            or any(p.is_symlink() for p in (path, *path.parents))):
        raise ValueError('notice source escapes package or is a symlink')
    if not path.is_file() or path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError('missing or oversized notice source')
    return path


def notice_fallback(package):
    index = ROOT / 'packaging/notices/provenance.json'
    if not index.exists():
        return None
    records = json.loads(index.read_text(encoding='utf-8'))
    key = package['name'] + '-' + package['version']
    record = records.get(key)
    if record is None:
        return None
    if record['license'] != package.get('license'):
        raise ValueError('fallback license declaration mismatch')
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())['package']
    pinned = [p for p in lock if p['name'] == package['name'] and p['version'] == package['version']]
    if len(pinned) != 1 or pinned[0].get('checksum') != record['crate_sha256']:
        raise ValueError('fallback crate identity mismatch')
    vcs = Path(package['manifest_path']).parent / '.cargo_vcs_info.json'
    if json.loads(vcs.read_text(encoding='utf-8'))['git']['sha1'] != record['upstream_commit']:
        raise ValueError('fallback upstream source identity mismatch')
    base = ROOT / 'packaging/notices' / key
    paths = []
    for file in record['files']:
        path = regular(base / file['name'], base)
        if sha(path) != file['sha256']:
            raise ValueError('fallback notice content mismatch')
        paths.append(path)
    return base, paths, record


def collect_notices(metadata, output):
    resolved = {node['id'] for node in metadata['resolve']['nodes']} if metadata.get('resolve') else None
    packages = [p for p in metadata['packages'] if resolved is None or p['id'] in resolved]
    inventory = []
    total = 0
    for package in sorted(packages, key=lambda p: (p['name'], p['version'], p['id'])):
        name, version = package['name'], package['version']
        if not re.fullmatch(r'[A-Za-z0-9_.+-]+', name + version):
            raise ValueError('unsafe dependency name/version')
        root = Path(package['manifest_path']).parent
        source = package.get('source')
        if source is None:
            if not root.resolve().is_relative_to(ROOT):
                raise ValueError('unrecognized local dependency')
            paths = [ROOT / 'LICENSE']
            base = ROOT
        else:
            if not source.startswith('registry+'):
                raise ValueError('unreviewed non-registry dependency')
            base = root
            paths = [p for p in root.rglob('*') if NOTICE.match(p.name) and p.is_file()]
            if package.get('license_file'):
                paths.append(root / package['license_file'])
        if not package.get('license') and not package.get('license_file'):
            raise ValueError(f'missing license declaration: {name} {version}')
        provenance = None
        if not paths:
            fallback = notice_fallback(package)
            if fallback is None:
                raise ValueError(f'missing license/notice files: {name} {version}')
            base, paths, provenance = fallback
        records = []
        for path in sorted(set(paths)):
            regular(path, base)
            relative = path.relative_to(base)
            destination = output / (name + '-' + version) / relative
            if (not destination.resolve().is_relative_to(output.resolve())
                    or any(p.is_symlink() for p in (destination, *destination.parents))):
                raise ValueError('notice destination escapes packet')
            if destination.exists():
                raise ValueError('duplicate dependency notice destination')
            total += path.stat().st_size
            if total > 64 * 1024 * 1024:
                raise ValueError('notice packet size limit')
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
            records.append({'path': destination.relative_to(output).as_posix(), 'sha256': sha(destination)})
        inventory.append({'name': name, 'version': version, 'source': source,
                          'license': package.get('license'), 'notices': records, 'fallback_provenance': provenance})
    (output / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n', encoding='utf-8')
    return inventory


def identifier(prefix, name):
    return prefix + hashlib.sha256(name.encode()).hexdigest()[:32]


def payload_xml(payload, destination):
    ET.register_namespace('', NS)
    wix = ET.Element('{' + NS + '}Wix')
    fragment = ET.SubElement(wix, 'Fragment')
    group = ET.SubElement(fragment, 'ComponentGroup', Id='Payload')
    directories = {'': 'INSTALLFOLDER'}
    files = sorted(p for p in payload.rglob('*') if p.is_file())
    for path in files:
        relative = path.relative_to(payload)
        if path.is_symlink():
            raise ValueError('symlink payload')
        parent = relative.parent.as_posix()
        if parent == '.':
            parent = ''
        current = ''
        for part in relative.parts[:-1]:
            child = current + '/' + part if current else part
            if child not in directories:
                ref = ET.SubElement(fragment, 'DirectoryRef', Id=directories[current])
                directories[child] = identifier('D', child)
                ET.SubElement(ref, 'Directory', Id=directories[child], Name=part)
            current = child
        name = relative.as_posix()
        component = ET.SubElement(group, 'Component', Id=identifier('C', name),
                                  Guid=str(uuid.uuid5(NAMESPACE, name)), Directory=directories[parent], Bitness='always64')
        ET.SubElement(component, 'File', Id=identifier('F', name), Source=str(path), Name=path.name, KeyPath='yes')
    ET.ElementTree(wix).write(destination, encoding='utf-8', xml_declaration=True)


def version_tuple(version):
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise ValueError('MSI requires major.minor.patch numeric version')
    result = tuple(map(int, version.split('.')))
    if result[0] > 255 or result[1] > 255 or result[2] > 65535:
        raise ValueError('version exceeds MSI bounds')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--dependencies', type=Path, required=True)
    parser.add_argument('--output-root', type=Path, required=True)
    parser.add_argument('--wix', type=Path, required=True)
    parser.add_argument('--test-rollback', action='store_true', help='hosted-only deferred failure fixture')
    parser.add_argument('--test-older-version', help='disposable CI upgrade fixture only; binary unchanged')
    args = parser.parse_args()
    if os.name != 'nt':
        raise ValueError('MSI compiler requires Windows')
    output = args.output_root.resolve()
    if output.is_relative_to(ROOT):
        raise ValueError('private installer output must be outside checkout')
    if run('git', 'status', '--porcelain', '--untracked-files=no'):
        raise ValueError('clean tracked source required')
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']
    source_version = version
    version_tuple(version)
    if args.test_rollback and os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('rollback fixture allowed only on hosted CI')
    if args.test_older_version:
        if os.environ.get('GITHUB_ACTIONS') != 'true' or version_tuple(args.test_older_version) >= version_tuple(version):
            raise ValueError('older version override allowed only for hosted downgrade/upgrade fixture')
        version = args.test_older_version
    if run(args.binary.resolve(strict=True), '--version') != 's3drive-cli ' + source_version:
        raise ValueError('unexpected binary version')
    if not run(args.wix, '--version').startswith(WIX_VERSION):
        raise ValueError('unexpected WiX tool version')
    output.mkdir(parents=True, exist_ok=False)
    payload = output / 'payload'
    payload.mkdir()
    shutil.copyfile(args.binary, payload / 's3drive.exe')
    for name in FILES:
        source = regular(ROOT / name, ROOT)
        target = payload / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
    notices = payload / 'third-party'
    notices.mkdir()
    metadata = json.loads(args.dependencies.read_text(encoding='utf-8-sig'))
    cli = [p for p in metadata['packages'] if p['name'] == 's3drive-cli']
    if (Path(metadata['workspace_root']).resolve() != ROOT
            or len(cli) != 1 or cli[0]['version'] != source_version
            or cli[0].get('source') is not None
            or Path(cli[0]['manifest_path']).resolve() != ROOT / 'crates/s3drive-cli/Cargo.toml'):
        raise ValueError('dependency metadata does not identify this source workspace')
    collect_notices(metadata, notices)
    manifest = {'source_commit': run('git', 'rev-parse', 'HEAD'), 'source_tree': run('git', 'rev-parse', 'HEAD^{tree}'),
                'msi_version': version, 'binary_version': source_version, 'test_fixture': bool(args.test_older_version or args.test_rollback),
                'target': 'x86_64-pc-windows-msvc', 'cargo_lock_sha256': sha(ROOT / 'Cargo.lock'),
                'files': {p.relative_to(payload).as_posix(): sha(p) for p in sorted(payload.rglob('*')) if p.is_file()}}
    (payload / 'build-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    license_rtf = output / 'license.rtf'
    license_intro = ('Private experimental installer. S3Drive-owned source is MIT licensed.\n'
                     'Dependencies retain their own terms, including GPL-3.0 Windows Rust bindings.\n'
                     'See THIRD-PARTY-NOTICES.md and the installed third-party notice packet.\n'
                     'This is not a declaration that the combined executable is MIT-only.\n\n')
    license_text = (license_intro + (ROOT / 'LICENSE').read_text()).replace('\\', '\\\\').replace('{', '\\{').replace('}', '\\}')
    license_rtf.write_text('{\\rtf1\\ansi\\deff0 {\\fonttbl {\\f0 Segoe UI;}}\\f0\\fs18\n' + license_text.replace('\n', '\\par\n') + '}', encoding='ascii')
    payload_xml(payload, output / 'payload.wxs')
    product = str(uuid.uuid5(NAMESPACE, 'product:' + version))
    msi = output / ('s3drive-' + version + '-x64.msi')
    product_source = ROOT / 'packaging/windows/product.wxs'
    extra = []
    if args.test_rollback:
        # Official WiX fault injection exists ONLY in this separate hosted fixture.
        text = product_source.read_text().replace('<Package ', '<Package xmlns:util="http://wixtoolset.org/schemas/v4/wxs/util" ', 1)
        text = text.replace('</Package>', '<util:FailWhenDeferred /></Package>')
        product_source = output / 'rollback-product.wxs'
        product_source.write_text(text, encoding='utf-8')
        extra = ['-ext', 'WixToolset.Util.wixext/' + WIX_VERSION]
    run(args.wix, 'build', product_source, output / 'payload.wxs',
        '-arch', 'x64', '-ext', 'WixToolset.UI.wixext/' + WIX_VERSION,
        '-d', 'Version=' + version, '-d', 'ProductCode=' + product,
        '-d', 'LicenseRtf=' + str(license_rtf), '-o', msi, *extra)
    result = {'source': manifest['source_commit'], 'msi': msi.name, 'sha256': sha(msi), 'product_code': '{' + product.upper() + '}', 'version': version,
              'binary_sha256': sha(payload / 's3drive.exe')}
    (output / 'installer.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print('PRIVATE_MSI_BUILT ' + json.dumps(result))


if __name__ == '__main__':
    main()
