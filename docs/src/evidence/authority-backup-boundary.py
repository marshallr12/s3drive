#!/usr/bin/env python3
"""Historical Linux-only research reproducer for source 7962058.

Not a gate: deliberately measures the absence of restore-freshness protection.
Uses only disposable SQLite/loopback fixtures, never S3 or real user state.
Usage: python3 docs/src/evidence/authority-backup-boundary.py [s3drive executable]
Caller must verify the executable was built from the source being investigated.
The printed baseline identifies this research, not arbitrary executable provenance.
An implementation that adds restore fencing may change this dated observation.
"""
import json, os, pathlib, secrets, select, shutil, sqlite3, subprocess, sys, tempfile, urllib.request

os.umask(0o077)
if not sys.platform.startswith('linux'):
    raise SystemExit('This dated pipe/permission reproducer is Linux-only')
binary = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parents[3] / 'target/debug/s3drive'
processes = []
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
with tempfile.TemporaryDirectory(prefix='s3drive-backup-boundary-') as scratch:
    root = pathlib.Path(scratch)
    token = secrets.token_hex(32)
    secret = root / 'token'
    secret.write_text(token)
    secret.chmod(0o600)
    def start(directory, volume=None):
        args = [str(binary), 'authority-serve', '--directory', str(directory), '--token-file', str(secret), '--name-policy', 'portable-collision-v1', '--listen', '127.0.0.1:0', '--stop-on-stdin-close']
        if volume:
            args += ['--expect-volume', volume]
        child = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        processes.append(child)
        if not select.select([child.stdout], [], [], 20)[0]:
            raise RuntimeError('fixture readiness deadline')
        line = child.stdout.readline()
        if not line:
            raise RuntimeError('fixture refused startup')
        return child, json.loads(line)
    def stop(child):
        child.stdin.close()
        if child.wait(timeout=20) != 0:
            raise RuntimeError('fixture shutdown failed')
    def rpc(endpoint, body):
        request = urllib.request.Request(endpoint, data=json.dumps(body).encode(), headers={'Authorization':'Bearer '+token, 'Content-Type':'application/json'})
        with opener.open(request, timeout=10) as response:
            return json.load(response)
    try:
        original = root / 'original'
        snapshot = root / 'snapshot'
        child, ready = start(original)
        volume = ready['volume']
        stop(child)
        snapshot.mkdir(mode=0o700)
        source = sqlite3.connect('file:'+str(original / 'metadata.sqlite3')+'?mode=ro', uri=True)
        destination = sqlite3.connect(snapshot / 'metadata.sqlite3')
        source.backup(destination)
        assert destination.execute('PRAGMA integrity_check').fetchone() == ('ok',)
        destination.close()
        source.close()
        shutil.copy2(original / 'volume.lock', snapshot / 'volume.lock')
        child, ready = start(original, volume)
        operation = 'c'*32
        reply = rpc(ready['endpoint'], {'action':'apply', 'request': {'volume':volume, 'operation':operation, 'expected_sequence':0, 'mutation': {'kind':'create-entry', 'parent':'0'*32, 'parent_revision':0, 'store':None, 'name':'after snapshot', 'node_kind':'directory'}}})
        assert reply['result'] == 'applied'
        acknowledged_sequence = reply['value']['sequence']
        stop(child)
        child, restored = start(snapshot, volume)
        info = rpc(restored['endpoint'], {'action':'info'})['value']
        receipt = rpc(restored['endpoint'], {'action':'operation','volume':volume,'operation':operation})
        stop(child)
        evidence = {'research_baseline':'7962058784da99c67b07273275c7b6c007f68ca5', 'scope':'isolated Linux CLI/SQLite fixture; no S3 or live-user configuration', 'snapshot_integrity_ok':True, 'pinned_start_accepted_same_volume':info['id'] == volume, 'acknowledged_sequence_before_restore':acknowledged_sequence, 'sequence_after_restore':info['sequence'], 'acknowledged_operation_missing_after_restore':receipt == {'result':'operation','value':None}}
        assert evidence['pinned_start_accepted_same_volume'] and evidence['acknowledged_sequence_before_restore'] == 1 and evidence['sequence_after_restore'] == 0 and evidence['acknowledged_operation_missing_after_restore']
        print(json.dumps(evidence))
    finally:
        for child in processes:
            if child.poll() is None:
                child.terminate()
                child.wait(timeout=20)
