"""Seal existing plugin/host pairing evidence; no subprocess or authority mutation."""
from pathlib import Path
import hashlib, json
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return json.loads(p.read_text(encoding='utf-8'))
candidate=HERE/'candidate-001.json'; review=HERE/'host-pair-readonly-001.json'
assert sha(candidate)=='8851b4b05d3f1fe5ffec21bcfe8eeaf67f35b55573e9e95a12b45a25f8e396b5'
assert sha(review)=='ae5f819194c7e544c613e0dfb0b7763af6fbec573eaa0b62f7d878202f97573d'
c=read(candidate); r=read(review); all_inputs=dict(r['input_sha256'])
for path,digest in all_inputs.items(): assert sha(Path(path))==digest,path
assert sha(HERE/'review_host_pair001.py')==r['reviewer_sha256']
for p in [Path(__file__),HERE/'delivery.md',HERE/'review_host_pair001.py',review,candidate]: all_inputs[str(p)]=sha(p)
plugin_inputs={};external_inputs={}
for path,digest in sorted(all_inputs.items()):
    p=Path(path)
    if p.is_relative_to(ROOT): plugin_inputs[p.relative_to(ROOT).as_posix()]=digest
    else: external_inputs[str(p)]=digest
result={
    'status':'ready_for_joint_review_admission_owner_plugin_slice',
    'candidate':str(candidate),'candidate_sha256':sha(candidate),
    'peer_executable':c['exe'],'peer_executable_sha256':c['exe_sha256'],
    'ordinary_client_reused_without_rebuild':True,
    'ordinary_client_manifest':c['ordinary_client_manifest'],
    'ordinary_client_manifest_sha256':c['ordinary_client_manifest_sha256'],
    'ordinary_client_executable_sha256':c['ordinary_client_exe_sha256'],
    'wire':c['wire'],'schema_sha256':c['schema_sha256'],
    'host_ready':r['host_ready'],'host_ready_sha256':r['host_ready_sha256'],
    'host_manifest':r['host_manifest'],'host_manifest_sha256':r['host_manifest_sha256'],
    'producer_pair':r['producer_pair'],'producer_pair_sha256':r['producer_pair_sha256'],
    'readonly_review':str(review),'readonly_review_sha256':sha(review),
    'real_producer_peer_cases':4,'cases':r['cases'],
    'historical_hello_and_query_byte_identical':True,
    'captured_peer_bytes_match_host_channel_capture':True,
    'local_unit_cases':3,'local_offline_cli_cases':4,
    'independent_joint_runtime':'pending at handoff; not inferred from producer or read-only review',
    'input_sha256':plugin_inputs,'external_input_sha256':external_inputs,
    'M-02':'partial','G0':'blocked','P-02':'blocked','J-00':'blocked','G1':'not_passed',
    'product_graphs':'0/2','original_product_cases_not_run':84,'product_pass_credit':0,
    'limits':r['limits']+['Peer holder PID is a locator, not independent OS evidence','Same executable with changed argv is not strong artifact isolation','No A/B role interchange coverage claimed','Raw frames stay in restricted test materials; no raw bytes printed in handoff','Old source/candidates/receipts frozen; no commit/push/release or next implementation batch']}
output=HERE/'handoff.json'
with output.open('x',encoding='utf-8') as stream: stream.write(json.dumps(result,indent=2)+'\n')
print(json.dumps({'handoff':str(output),'sha256':sha(output),'plugin_inputs':len(plugin_inputs),'external_inputs':len(external_inputs),'producer_peer_cases':4,'independent_joint_runtime':result['independent_joint_runtime']}))
