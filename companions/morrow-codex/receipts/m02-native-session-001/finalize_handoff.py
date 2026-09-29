from pathlib import Path
import hashlib,json,sys
sys.dont_write_bytecode=True
from run_client import ROOT,HERE,frozen
HOST=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
BASE=HOST/'reports/codex-morrow-v1.1/host/m02-native-session-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
old=frozen();candidates=[(HERE/'candidate-001.json','9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee'),(ROOT/'receipts/m02-native-session-adversary-001/candidate-001.json','654fb30808208895cb3da208660292cb352e65595a63c12275b1a239cd931e4d'),(ROOT/'receipts/m02-native-session-adversary-002/candidate-002.json','afc3f57c1c430d61f88d22ecbe71c170254f6dc47477d67054aff4fde8e1ef42')]
inputs={}
for p,want in candidates:
    assert sha(p)==want
    inputs[p.relative_to(ROOT).as_posix()]=want
    for name,digest in read(p)['input_sha256'].items():assert sha(ROOT/name)==digest;inputs[name]=digest
manifest=BASE/'runtime-kit-001/manifest.json';handoff=BASE/'runtime-handoff-001.json'
assert sha(manifest)=='178044ee85d0f1c15e0168e8fcf6b728b5105ac5b2c63b8d2134b16b49a23be9'
assert sha(handoff)=='09ec3b47a4425cff6bd7347314e2844862ed2412bf416adc3ba521c7360ce51d'
m=read(manifest);external={str(manifest):sha(manifest),str(handoff):sha(handoff)}
for field,base in [('files',manifest.parent),('build_inputs_sha256',HOST),('evidence_sha256',HOST)]:
    for name,want in m[field].items():p=base/name;assert sha(p)==want,name;external[str(p)]=want
assert m['plugin_manifest_sha256']==candidates[0][1] and m['plugin_sha256']==read(candidates[0][0])['exe_sha256']
pair=read(Path(m['pair_result']));assert pair['client_sha256']==m['plugin_sha256'] and pair['host_sha256']==m['host_sha256']
review=read(HERE/'host-pair-readonly-002.json');assert review['source_sha256']==sha(Path(m['pair_result']))
for directory in [HERE,ROOT/'receipts/m02-native-session-adversary-001',ROOT/'receipts/m02-native-session-adversary-002']:
    for p in directory.rglob('*'):
        if p.is_file() and p.name!='handoff.json' and '__pycache__' not in p.parts:inputs[p.relative_to(ROOT).as_posix()]=sha(p)
assert frozen()==old
result={'status':'ready_for_joint_review_real_native_client_slice','ordinary_client':str(candidates[0][0]),'ordinary_client_sha256':read(candidates[0][0])['exe_sha256'],'host_manifest':str(manifest),'host_manifest_sha256':sha(manifest),'host_exe_sha256':m['host_sha256'],'latest_producer_pair':m['pair_result'],'latest_producer_pair_sha256':sha(Path(m['pair_result'])),'real_pair_cases':3,'producer_case_names':['normal','revoke','stop'],'local_unit_cases':9,'offline_cli_cases':6,'independent_joint_runtime':'pending at handoff; do not infer from producer results','input_sha256':inputs,'external_input_sha256':external,'old_frozen_handoffs':old,'M-02':'partial_not_complete','G0':'blocked','P-02':'not_complete','J-00':'not_complete','product_graphs':'0/2','product_84_cases':'not_run','limits':['No local authority or fake host; real host-producer pair, independent review pending','Test peers compiled separately; no peer runtime success claimed here','No full trusted installation/image attestation/OS sandbox/descendant containment/host-crash recovery','No OS output backpressure/partial-write-cancellation qualification','Only read status/control; M03/M04/M06/M08 unsupported; no Core graph rebuild','All candidate inputs and old kits/handoffs frozen; no commit/push/release']}
p=HERE/'handoff.json';assert not p.exists();p.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'handoff':str(p),'sha256':sha(p),'plugin_inputs':len(inputs),'external_host_inputs':len(external),'real_producer_cases':3}))
