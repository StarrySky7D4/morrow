from pathlib import Path
import hashlib,json,shutil,sys
sys.dont_write_bytecode=True
from run_peer import ROOT,HERE,SOURCE,OUT,frozen
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
client=ROOT/'receipts/m02-native-session-001/candidate-001.json'
assert sha(client)=='9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee'
for name,want in read(client)['input_sha256'].items():assert sha(ROOT/name)==want
build=HERE/'runs/build-20260928T192552Z-9f76771c/result.json';record=read(build)
assert record['status']=='passed_bootstrap_build' and record['inputs_before']==record['inputs_after']
for name,want in record['inputs_after'].items():assert sha(ROOT/name)==want
original=Path(record['artifact']['path']);assert sha(original)==record['artifact']['sha256']
candidate=OUT/'candidates/peer-001';candidate.mkdir(parents=True,exist_ok=False)
exe=candidate/'morrow-native-session-adversary.exe';shutil.copyfile(original,exe)
inputs={p.relative_to(ROOT).as_posix():sha(p) for p in SOURCE.rglob('*') if p.is_file()}
for p in [Path(__file__),HERE/'run_peer.py',build,client,exe,OUT/'cargo-home/config.toml']:inputs[p.relative_to(ROOT).as_posix()]=sha(p)
result={'status':'test_peer_compiled_not_host_runtime_verified','exe':str(exe),'exe_sha256':sha(exe),'normal_client_manifest_sha256':sha(client),'arguments':['--morrow-native-session-v2','--scenario','SCENARIO'],'scenarios':['wrong-pid','modified-epoch','wrong-artifact','unsupported-kind','no-hello','partial-frame','flood-invalid','ignore-stop','hold-output'],'input_sha256':inputs,'frozen_old_handoffs':frozen(),'limits':['Separate adversarial test peer, never a production client/host authority','No scenario run yet','Modified epoch is synthetic bad identity, not actual historical-session replay','Hold-output creates only this same fixed executable with inherited output handles for1.5s','Do not duplicate already sufficient host cases; independent reviewer selects necessary gaps','Parent watchdog10s; no arbitrary commands/accounts/user data']}
path=HERE/'candidate-001.json';path.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'manifest':str(path),'manifest_sha256':sha(path),'exe':str(exe),'exe_sha256':sha(exe),'inputs':len(inputs)}))
