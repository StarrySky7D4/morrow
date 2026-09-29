"""Read-only intake of the shared, host-owned Windows platform kit."""
from pathlib import Path
import hashlib, json
ROOT = Path(__file__).resolve().parents[2]
HOST = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor\reports\codex-morrow-v1.1\host\m03-stream-001')
OUT = ROOT/'upstream/m03-stream-001/pipe-kit-001'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    handoff, manifest = HOST/'pipe-handoff-001.json', HOST/'pipe-kit-001/manifest.json'
    assert sha(handoff) == 'a7949c64c87319a77376a75d2535d7bf1b4bc8ad9fc9ca63c6b73b0304063600'
    assert sha(manifest) == 'd6eee6bdb2f933d59ebeddb10a550359204cb024fe09e58f98182739bbd1787b'
    data = json.loads(manifest.read_text(encoding='utf-8'))
    OUT.mkdir(parents=True,exist_ok=False)
    copied = {}
    for name, expected in data['files'].items():
        source = HOST/'pipe-kit-001'/name
        assert source.resolve().is_relative_to((HOST/'pipe-kit-001').resolve())
        assert sha(source) == expected, name
        if name.startswith('source/'):
            target = OUT/name
            target.parent.mkdir(parents=True,exist_ok=True)
            target.write_bytes(source.read_bytes())
            assert sha(target) == expected
            copied[target.relative_to(ROOT).as_posix()] = expected
    for source in [handoff,manifest]:
        target = OUT/source.name
        target.write_bytes(source.read_bytes())
        copied[target.relative_to(ROOT).as_posix()] = sha(target)
    receipt = {'status':'shared_platform_copied_no_runtime','verified_kit_files':len(data['files']),
        'input_sha256':copied,'unsafe_ffi_added_in_plugin':False}
    target = ROOT/'receipts/m03-stream-001/pipe-intake-001.json'
    assert not target.exists()
    target.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':receipt['status'],'verified_kit_files':len(data['files']),'receipt_sha256':sha(target)}))
if __name__ == '__main__': main()
