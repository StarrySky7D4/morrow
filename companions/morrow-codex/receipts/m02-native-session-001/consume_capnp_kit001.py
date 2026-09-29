from pathlib import Path
import hashlib,json,shutil
ROOT=Path(__file__).resolve().parents[2]
HOST=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
KIT=HOST/'reports/codex-morrow-v1.1/host/m02-native-session-001/capnp-kit-001'
DEST=ROOT/'native/m02-native-session-001/vendor/capnp-kit-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
manifest=KIT/'manifest.json';assert sha(manifest)=='2a0146a8f18b658b906c6a5f2d0334272a794edf43b0b6ac5757d9ba45cb66d9'
data=json.loads(manifest.read_text(encoding='utf-8'));assert not DEST.exists()
for name,expected in data['authority_files'].items():assert sha(Path(data['authority'])/name)==expected
for name,expected in data['files'].items():
    source=KIT/name;assert sha(source)==expected
    target=DEST/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,target);assert sha(target)==expected
shutil.copyfile(manifest,DEST/'manifest.json')
print(json.dumps({'status':'capnp_kit001_verified','manifest_sha256':sha(manifest),'files':len(data['files']),'authority_files':len(data['authority_files']),'destination':str(DEST)}))
