"""Read-only verification of sealed host-produced pairing, not another execution."""
from pathlib import Path
import hashlib, json

ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
HOST=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
BASE=HOST/'reports/codex-morrow-v1.1/host/m02-admission-owner-002'
KIT=BASE/'runtime-kit-002'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return json.loads(p.read_text(encoding='utf-8'))
inputs={}
def verify(p,wanted):
    assert sha(p)==wanted,str(p)
    inputs[str(p)]=wanted

ready=BASE/'ready-handoff-001.json'
verify(ready,'ee8986c3ab614b77d9f321a6d4961e605374c66e1e52bbf91b48f5070e6bc52c')
manifest=KIT/'manifest.json'
verify(manifest,'ee257b1cc15dcb72a4f92343406431fbc4a35153915e23e5f73e9265917e373e')
handoff=read(ready); kit=read(manifest)
verify(Path(handoff['delivery']),handoff['delivery_sha256'])
verify(Path(handoff['host_executable']),'59b1427cb8c50ebbc50184411bb9520f8114253e8c114d83c170c8e6015b04b5')
for field,base in [('files',KIT),('build_inputs_sha256',HOST),('evidence_sha256',HOST)]:
    for name,wanted in kit[field].items(): verify(base/name,wanted)
candidate=HERE/'candidate-001.json'
verify(candidate,'8851b4b05d3f1fe5ffec21bcfe8eeaf67f35b55573e9e95a12b45a25f8e396b5')
peer=read(candidate)
for name,wanted in peer['input_sha256'].items(): verify(ROOT/name,wanted)
for name,wanted in peer['frozen_previous_inputs'].items(): verify(Path(name),wanted)
pair_path=Path(kit['receipts']['real_historical_peer'])
verify(pair_path,'0a032c8ce8a43f8839925f8bc07c4f0bcc2c360d872d3b41eea1e8465c600320')
pair=read(pair_path)
assert pair['status']=='passed_real_historical_peer_cases'
assert pair['host_sha256']==handoff['host_sha256']
assert pair['peer_sha256']==peer['exe_sha256']
assert pair['peer_manifest_sha256']==sha(candidate)
assert pair['plugin_inputs_before']==pair['plugin_inputs_after']==peer['input_sha256']
assert len(pair['cases'])==4 and all(case['passed'] for case in pair['cases'])
material={name:Path(path) for name,path in pair['materials']['paths'].items()}
summaries=[]; frames={}; host_pids=[]
for name in ['capture','replay-hello','replay-query','peer-held-output']:
    rows=[json.loads(line) for line in (pair_path.parent/(name+'.stdout.jsonl')).read_text(encoding='utf-8').splitlines()]
    proposal=next(row for row in rows if row['event']=='proposal')
    host_pids.append(proposal['host_pid'])
    final=rows[-1]; assert final['event']=='final'
    snapshot=final['snapshot']; events=snapshot['events']
    assert snapshot['phase']=='Released' and snapshot['exit_observed'] and snapshot['stdout_eof'] and snapshot['stderr_eof'] and not snapshot['owner_retained']
    assert snapshot['exit_code']==0
    assert final['state']['owner']['phase']=='Released' and not final['state']['owner']['owner_retained']
    emitted=[row['observation'] for row in rows if row['event']=='host_observation']
    assert emitted==events
    assert all(a['ordinal']<b['ordinal'] and a['at_us']<=b['at_us'] for a,b in zip(events,events[1:]))
    released=next(i for i,event in enumerate(events) if event['event']=='phase' and event['detail']=='Released')
    for event_name in ['exit','stdout_eof','stderr_eof']:
        assert next(i for i,event in enumerate(events) if event['event']==event_name)<released
    received=[bytes.fromhex(event['detail']['raw_hex']) for event in events if event['event']=='frame_received']
    sent=[bytes.fromhex(event['detail']['raw_hex']) for event in events if event['event']=='frame_sent']
    frames[name]=(received,sent)
    denial=[event['detail']['code'] for event in events if event['event']=='request_denied']
    if name.startswith('replay-'): assert denial==[17]
    summaries.append({'case':name,'host_pid_from_proposal':proposal['host_pid'],'child_pid_from_host':snapshot['pid'],'child_exit_from_host':snapshot['exit_code'],'exit_and_double_eof_before_release':True,'received_frames':len(received),'sent_frames':len(sent),'denial_codes':denial})
assert len(set(host_pids))==4
old_received,old_sent=frames['capture']
assert len(old_received)==3 and len(old_sent)==4
capture=material['capture']
for filename,raw in zip(['02-hello.frame','04-query.frame','06-close.frame'],old_received): assert (capture/filename).read_bytes()==raw
for filename,raw in zip(['01-challenge.frame','03-welcome.frame','05-state.frame','07-stop.frame'],old_sent): assert (capture/filename).read_bytes()==raw
assert (capture/'complete.txt').is_file() and read(capture/'result.json')['accepted_welcome']
assert frames['replay-hello'][0][0]==old_received[0]==(material['replay-hello']/'02-hello.frame').read_bytes()
assert frames['replay-query'][0][1]==old_received[1]==(material['replay-query']/'04-query.frame').read_bytes()
assert frames['replay-query'][0][0]!=old_received[0]
for name in ['replay-hello','replay-query']:
    assert frames[name][1][0]!=old_sent[0]
    assert read(material[name]/'result.json')['identity_rejection_observed']
holder=read(material['hold-output']/'holder.json')
assert holder['root_pid']==summaries[-1]['child_pid_from_host']
assert holder['source']=='peer_owned_child_handle' and holder['create_no_window'] and holder['holder_lifetime_ms']==2000
holder_rows=[json.loads(line) for line in (pair_path.parent/'peer-held-output.stdout.jsonl').read_text(encoding='utf-8').splitlines()]
assert any(event['event']=='phase' and event['detail']=='ClosingUnconfirmed' for event in holder_rows[-1]['snapshot']['events'])
result={'status':'passed_readonly_host_producer_pair_review','host_ready':str(ready),'host_ready_sha256':sha(ready),'host_manifest':str(manifest),'host_manifest_sha256':sha(manifest),'plugin_candidate':str(candidate),'plugin_candidate_sha256':sha(candidate),'producer_pair':str(pair_path),'producer_pair_sha256':sha(pair_path),'cases':summaries,'historical_hello_and_query_byte_identical':True,'capture_peer_bytes_match_host_channel_capture':True,'holder_pid_source':'peer owned Child handle; not independent OS evidence','verified_unique_inputs':len(inputs),'input_sha256':inputs,'reviewer_sha256':sha(Path(__file__)),'independent_runtime':'pending joint review; no subprocess launched by this verifier','limits':['Host exit codes and first-host wait ordering asserted by sealed producer harness; no separate plugin process observation','Raw frame capture and child wait/EOF are host evidence, correlated with peer files','This checker does not independently decode Capnp or prove strong artifact isolation','No production approval/CLI auth, global owner, crash recovery, saturation or business backend qualification','M02 partial, G0/P02/J00 blocked, G1 not passed, product0/2 and84not_run remain unchanged']}
output=HERE/'host-pair-readonly-001.json'
with output.open('x',encoding='utf-8') as stream: stream.write(json.dumps(result,indent=2)+'\n')
print(json.dumps({'receipt':str(output),'sha256':sha(output),'cases':len(summaries),'verified_inputs':len(inputs),'historical_hello_and_query_byte_identical':True}))
