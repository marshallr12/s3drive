#!/usr/bin/env python3
"""Independent S3DROOT1 bytes and v2 key vectors; no Rust codec imported."""
import json
from pathlib import Path
import struct

DESTINATION = Path(__file__).resolve().parents[1] / 'crates/s3drive-core/src/manifest/objects/fixtures'
ROOT_DIGEST = bytes.fromhex('f5ce76e1863fc41050e598bca75748323f0f17ee1cc9fe19a16dac007e6fdc10')


def main():
    volume, node, store, generation = (bytes([i]) * 16 for i in (1, 2, 3, 6))
    ordinal, length = 11, 1048576 + 16
    raw = struct.pack('<8s16s16sQ16s16sQ32s', b'S3DROOT1', volume, node, length,
                      store, generation, ordinal, ROOT_DIGEST)
    prefix = 'managed/v2/' + '/'.join(x.hex() for x in (volume, node, store, generation))
    keys = {role: f'{prefix}/{role}/{ordinal:016x}' for role in ('data', 'page')}
    DESTINATION.mkdir(parents=True, exist_ok=True)
    (DESTINATION / 'root-descriptor.bin').write_bytes(raw)
    (DESTINATION / 'keys.json').write_text(json.dumps(keys, indent=2) + '\n')
    print('root-descriptor:', len(raw), 'bytes; two fixed object roles')


if __name__ == '__main__':
    main()
