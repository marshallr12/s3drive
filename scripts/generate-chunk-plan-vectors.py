#!/usr/bin/env python3
"""Independent expected full-snapshot builder pages and frozen-input digest."""
import hashlib
import json
from pathlib import Path
import struct
DESTINATION = Path(__file__).resolve().parents[1] / 'crates/s3drive-state/src/chunked/fixtures'
CHUNK = 1048576
DOMAIN = b's3drive-manifest-page-v1\0'

def reference(ordinal, digest):
    return struct.pack('<16s16sQ32s', bytes([3])*16, bytes([4])*16, ordinal, digest)

def page(level, start, length, entries):
    return struct.pack('<8sBBH16s16sQQI', b'S3DMAN01', level, 0, len(entries),bytes([1])*16,bytes([2])*16,start,length,CHUNK)+b''.join(entries)

def main():
    tail = 'hello\0café 🦀'.encode()
    first = hashlib.sha256(b'a'*CHUNK).digest()
    last = hashlib.sha256(tail).digest()
    leaf0 = page(0,0,128*CHUNK,[struct.pack('<BQ',1,CHUNK)+reference(0,first),struct.pack('<BQ',0,127*CHUNK)])
    leaf1 = page(0,128*CHUNK,len(tail),[struct.pack('<BQ',1,len(tail))+reference(128,last)])
    root = page(1,0,128*CHUNK+len(tail),[struct.pack('<BQ',2,128*CHUNK)+reference(1<<56,hashlib.sha256(DOMAIN+leaf0).digest()),struct.pack('<BQ',2,len(tail))+reference((1<<56)|1,hashlib.sha256(DOMAIN+leaf1).digest())])
    header = dict(id=[7]*32,volume=[1]*16,node=[2]*16,store=[3]*16,generation=[4]*16,root='managed/',source=[8]*32,fingerprint='a'*64,parent=None,size=128*CHUNK+len(tail),expected_content=[9]*32)
    DESTINATION.mkdir(parents=True,exist_ok=True)
    for name,body in [('leaf0',leaf0),('leaf1',leaf1),('root',root)]:
        (DESTINATION/f'{name}.bin').write_bytes(body)
    digest=hashlib.sha256(b's3drive-chunk-input-v1\0'+json.dumps(header,separators=(',',':')).encode()).hexdigest()
    (DESTINATION/'input.sha256').write_text(digest+'\n')
    print('3 independently encoded pages; frozen input digest',digest)
if __name__=='__main__':main()
