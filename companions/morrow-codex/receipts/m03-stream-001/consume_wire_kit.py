"""Copy only the authoritative frozen host codec kit; never edit/regenerate it."""
from pathlib import Path
import hashlib
import json

ROOT = Path(__file__).resolve().parents[2]
HOST = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor\reports\codex-morrow-v1.1\host\m03-stream-001')
OUT = ROOT/'upstream/m03-stream-001/wire-kit-001'

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    handoff = HOST/'wire-handoff-001.json'
    manifest = HOST/'wire-kit-001/manifest.json'
    assert sha(handoff) == '3cb2e70480638d2094b82b2cff0a62739ebee2546fa1236650e81daa99e6286b'
    assert sha(manifest) == '9d43385067d09a016ef950edf5fe4d489803e02f7bee8c419014d6b6fc2af5ae'
    spec = json.loads(manifest.read_text(encoding='utf-8'))
    for name, expected in spec['files'].items():
        path = HOST/'wire-kit-001'/name
        assert path.resolve().is_relative_to((HOST/'wire-kit-001').resolve())
        assert sha(path) == expected, name
    OUT.mkdir(parents=True, exist_ok=False)
    copied = {}
    for name in [*(('source/'+p) for p in spec['source_files']), 'generated/native_http_capnp.rs']:
        target = OUT/name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((HOST/'wire-kit-001'/name).read_bytes())
        assert sha(target) == spec['files'][name]
        copied[target.relative_to(ROOT).as_posix()] = sha(target)
    for path in [handoff, manifest]:
        target = OUT/path.name
        target.write_bytes(path.read_bytes())
        copied[target.relative_to(ROOT).as_posix()] = sha(target)
    receipt = {'status':'authoritative_codec_inputs_copied_no_build_no_runtime',
        'host_handoff_sha256':sha(handoff),'host_manifest_sha256':sha(manifest),
        'host_kit_files_verified':len(spec['files']),'input_sha256':copied,
        'own_schema_created':False,'native_http_runtime_verified':False}
    path = ROOT/'receipts/m03-stream-001/wire-intake-001.json'
    assert not path.exists()
    path.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':receipt['status'],'files_verified':len(spec['files']),'receipt_sha256':sha(path)}))

if __name__ == '__main__': main()
