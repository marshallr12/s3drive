#!/usr/bin/env python3
"""Independent frozen legacy wire bytes. stdlib only, no Rust codec imports."""
import hashlib
import json
from pathlib import Path


def generate(out):
    out.mkdir(parents=True, exist_ok=True)
    store = dict(id='03' * 16, route='a' * 64, root='managed/')
    reservation = dict(id='04' * 16, volume='01' * 16, node='02' * 16,
                       expected_revision=2, store=store,
                       key='managed/v1/' + '01' * 16 + '/' + '02' * 16 + '/' + '04' * 16,
                       size=17, sha256='b' * 64)
    for guarded in (False, True):
        r = dict(reservation)
        if guarded:
            r['guard'] = dict(expected='c' * 64)
        p = dict(reservation=r, upload_operation='d' * 64, etag='"opaque"', version=None)
        raw = json.dumps(p, separators=(',', ':'), ensure_ascii=False).encode()
        name = 'guarded' if guarded else 'unguarded'
        (out / (name + '-payload.json')).write_bytes(raw)
        (out / (name + '-reference.sha256')).write_text(
            hashlib.sha256(b's3drive-content-reference-v1\0' + raw).hexdigest() + '\n')
    import struct
    page = (Path(__file__).resolve().parents[1] /
            'crates/s3drive-state/src/chunked/fixtures/root.bin').read_bytes()
    merkle = hashlib.sha256(b's3drive-manifest-page-v1\0' + page).digest()
    descriptor = (b'S3DROOT1' + bytes([1]) * 16 + bytes([2]) * 16 +
                  struct.pack('<Q', 128 * 1048576 + 16) + bytes([3]) * 16 +
                  bytes([4]) * 16 + struct.pack('<Q', 2 << 56) + merkle)
    r = dict(reservation)
    r['key'] = ('managed/v2/' + '01' * 16 + '/' + '02' * 16 + '/' +
                '03' * 16 + '/' + '04' * 16 + '/page/0200000000000000')
    r['size'] = len(page)
    r['sha256'] = hashlib.sha256(page).hexdigest()
    r['guard'] = dict(expected=None)
    r['manifest'] = dict(format='s3dman01', descriptor=descriptor.hex())
    p = dict(reservation=r, upload_operation='d' * 64, etag='"opaque"', version=None)
    raw = json.dumps(p, separators=(',', ':')).encode()
    (out / 'manifest-payload.json').write_bytes(raw)
    (out / 'manifest-reference.sha256').write_text(
        hashlib.sha256(b's3drive-manifest-content-reference-v1\0' + raw).hexdigest() + '\n')
    apply = dict(volume='01' * 16, operation='04' * 16, expected_sequence=2,
                 mutation=dict(kind='reserve-content', node='02' * 16,
                               expected_revision=2, store='03' * 16,
                               size=17, sha256='b' * 64, expected_payload='c' * 64))
    (out / 'legacy-apply.json').write_bytes(json.dumps(apply, separators=(',', ':')).encode())


if __name__ == '__main__':
    import sys
    generate(Path(sys.argv[1]) if len(sys.argv) == 2 else Path(__file__).resolve().parents[1] /
             'crates/s3drive-state/src/namespace/publication/fixtures')
