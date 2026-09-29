"""Read-only consumer review of host-produced real-pair evidence; not a replay."""
from pathlib import Path
import hashlib,json
ROOT=Path(__file__).resolve().parents[2];HERE=Path(__file__).resolve().parent
HOST=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
PAIR=HOST/'reports/codex-morrow-v1.1/host/m02-native-session-001/pair-20260928T192451567771Z/result.json'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
assert sha(PAIR)=='45138cacbf8732382a7c5553ee80fdbd1fb6f782ba6afb6fa35d04208c30bba0'
pair=read(PAIR);client=HERE/'candidate-001.json'
assert sha(client)==pair['client_manifest_sha256']=='9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee'
assert pair['client_inputs_before']==pair['client_inputs_after']==read(client)['input_sha256']
for name,want in pair['client_inputs_after'].items():assert sha(ROOT/name)==want
checks=[]
for case in pair['cases']:
    snap=case['snapshot'];events=snap['events']
    expected=19 if case['name']=='revoke' else 0
    assert case['host_exit']==0 and snap['exit_code']==expected
    assert case['stdin_held_open_until_exit'] and snap['exit_observed'] and snap['stdout_eof'] and snap['stderr_eof'] and not snap['owner_retained'] and snap['phase']=='Released'
    for name in ['spawn','frame_sent','frame_received','state_read','exit','stdout_eof','stderr_eof']:assert any(e['event']==name for e in events),name
    assert [e['ordinal'] for e in events]==list(range(len(events)))
    assert [e['at_us'] for e in events]==sorted(e['at_us'] for e in events)
    acks=[e for e in events if e['event']=='control_ack']
    if case['name']=='revoke':
        revoke=[e for e in acks if e['detail']['action']=='revoke'];assert len(revoke)==1
        assert not any(e['event']=='state_read' and e['ordinal']>revoke[0]['ordinal'] for e in events)
    checks.append({'name':case['name'],'host_pid':case['host_pid'],'child_pid':snap['pid'],'host_exit':case['host_exit'],'child_exit':snap['exit_code'],'phase':snap['phase'],'generation':snap['generation'],'exit_observed':snap['exit_observed'],'stdout_eof':snap['stdout_eof'],'stderr_eof':snap['stderr_eof'],'owner_retained':snap['owner_retained'],'state_reads':sum(e['event']=='state_read' for e in events),'control_stdin_open_until_host_exit':True,'events':len(events)})
result={'status':'host_producer_pair_reviewed_not_independent_replay','source':str(PAIR),'source_sha256':sha(PAIR),'client_manifest_sha256':sha(client),'client_inputs_unchanged':56,'cases':checks,'limits':['Real host-produced observations reviewed; no independent process launch by this checker','Joint separately observes OS process handles and decodes raw frames','Exit0 not business success; no M02/product/OS isolation completion']}
p=HERE/'host-pair-readonly-001.json';assert not p.exists();p.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result))
