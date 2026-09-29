"""Copy only the host-authored, digest-bound wire kit; never author a second schema."""
from pathlib import Path
import hashlib,json,shutil
ROOT=Path(__file__).resolve().parents[2]
HOST=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
HANDOFF=HOST/'reports/codex-morrow-v1.1/host/m02-native-session-001/wire-handoff.json'
DEST=ROOT/'native/m02-native-session-001/vendor/host-wire-v2-r1'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
assert sha(HANDOFF)=='62dc56b74170096d74b51125c87a617bea98effd33686338598d19b2b78a1e8a'
receipt=json.loads(HANDOFF.read_text(encoding='utf-8'))
assert receipt['schema_sha256']=='40d87f6a5dc07c134faef67a2767429531a292f4e85f22f3c53eb6296b63001a'
assert not DEST.exists()
base=Path('contracts/experimental/agent_host_v2')
for name,expected in receipt['input_sha256'].items():
    source=HOST/name;relative=Path(name).relative_to(base);assert sha(source)==expected
    target=DEST/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,target);assert sha(target)==expected
target=ROOT/'receipts/m02-native-session-001/host-wire-handoff-r1.json';assert not target.exists();shutil.copyfile(HANDOFF,target)
print(json.dumps({'status':'host_wire_source_verified_and_copied','source':str(HANDOFF),'handoff_sha256':sha(HANDOFF),'copy':str(DEST),'files':len(receipt['input_sha256'])}))
