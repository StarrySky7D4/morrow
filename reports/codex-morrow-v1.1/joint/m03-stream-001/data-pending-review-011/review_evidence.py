"""Read-only single B011 review; no harness execution or network requests."""
from pathlib import Path
import ast,hashlib,json,sqlite3,struct,subprocess
W=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor');P=W.parent.parent.parent/'morrow-codex';J=Path(__file__).resolve().parent
R=W/'reports/codex-morrow-v1.1/host/m03-stream-001/revoke-011-data-pending-20260929T015836101741Z';G=P/'out/m03-host-revoke-011/20260929T015836101741Z'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
def lines(p):return [json.loads(x) for x in p.read_text(encoding='utf-8').splitlines() if x]
def save(n,v):
    with (J/n).open('x',encoding='utf-8') as f:json.dump(v,f,indent=2);f.write('\n')
pins=[]
def pin(p,h):
    assert sha(p)==h,str(p)
    pins.append(dict(path=str(p),sha256=h))
pin(R/'result.json','d86ded6671fb9dbb1fb6d48a7956cba07d8d5c10bee51641ac9620fc7ffc603a');pin(R/'evidence-manifest.json','0559231548c887835c88202e04780fc933dd17b48be9b5756d85c9826b8619cf')
m=read(R/'evidence-manifest.json')
for rel,h in m['files'].items():pin(R/rel,h)
for path,h in m['plugin_files'].items():pin(Path(path),h)
locked=read(R/'locked-expectations.json');pin(W/'tool/m03_revoke_barriers_011.py',locked['script_sha256'])
pm=P/'receipts/m03-fixture-002/native-candidate-fixture-002.json';pin(pm,locked['plugin_manifest_sha256']);pin(Path(read(pm)['executable']),locked['plugin_exe_sha256'])
hm=R.parent/'native-host-candidate-005/manifest.json';pin(hm,locked['host'][1]);pin(Path(read(hm)['executable']),locked['host'][2])
r=read(R/'result.json');g=read(G/'result.json');rows=read(R/'host-rows.json');finals=[x['value'] for x in rows if x['value'].get('event')=='final'];assert len(finals)==1
final=finals[0];s=final['snapshot'];p=s['http']['progress'];events=s['events'];assert r['status']=='passed' and r['guest_result']==g and not r['automatic_retry']
server=read(R/'server-events.json');posts=[x for x in server if x['kind']=='post'];assert len(posts)==r['post_count']==1
a=read(R/'independent-approval.json');q=a['proposal'];body=bytes.fromhex(q['body_hex']);assert body==bytes.fromhex(posts[0]['body_hex']) and len(body)==22025 and hashlib.sha256(body).hexdigest()==q['body_sha256']
u32=lambda x:struct.pack('<I',x);u64=lambda x:struct.pack('<Q',x);field=lambda b:u32(len(b))+b
pre=b'Morrow/native-http-proposal/v1\0'+u64(a['identity']['session'])+u64(a['identity']['epoch'])+field(a['operation'].encode())+u64(1)
pre+=field(q['method'].encode())+field(q['absolute_target'].encode())+u32(len(q['headers']))
for h in q['headers']:pre+=field(h['name'].encode())+field(bytes.fromhex(h['value_hex']))
pre+=u32(q['response_limit'])+field(body);assert hashlib.sha256(pre).hexdigest()==q['request_sha256']==a['recomputed_request_sha256']
commands=read(R/'operator-commands.json');assert all(sum(x['value']['action']==k for x in commands)==1 for k in ('approve','claim','approve_http','revoke'))
ack=read(R/'revoke-ack.json');assert ack['row'] in rows and ack['command'] in commands
rtt=(ack['row']['line_received_ns']-ack['command']['before_write_ns'])/1e6;assert rtt==15.8194 and rtt<=500
assert ack['ack']==dict(application_pending=False,first_revocation_reason=19,first_revocation_source=1,persisted=True,runtime_applied=True)
applied=[e for e in events if e['event']=='http_cancel_applied'];assert len(applied)==1 and applied[0]['detail']['source']==1 and applied[0]['detail']['code']==19
assert not any(e['event']=='revoke_persistence_unconfirmed' for e in events)

# Same actual operation, original owner clock domain, no mixed observation clocks.
qualified=read(R/'qualified-before-revoke.json');obs=[e['detail'] for e in events if e['event']=='pipe_owner_observation' and e['detail']['operation']['id']==12 and e['detail']['operation']['issue_ordinal']==3]
assert len(obs)==8
op=dict(id=12,issue_ordinal=3,body_end=1024,frame_offset=0,requested_bytes=1364,initial_pending=True)
for o in obs:
    assert all(o['operation'][k]==v for k,v in op.items()) and o['clock_domain']==qualified['clock_domain']
    assert o['clock']=='std::time::Instant' and o['duration_unit']=='ns' and o['duration_units_per_second']==1000000000 and o['resolution']=='not_measured'
    assert o['before_ns']<=o['after_ns']
assert all(a['after_ns']<=b['before_ns'] for a,b in zip(obs,obs[1:]))
assert [o['stage'] for o in obs]==['write_issue','incomplete_sample','incomplete_sample','incomplete_sample','cancel_request_observed','cancel_probe','cancel_requested','write_reaped']
assert obs[0]['outcome']==dict(kind='io_pending',win32_error=997)
samples=obs[1:4];assert samples==qualified['samples'] and qualified['id']==12 and qualified['issue_ordinal']==3
assert [o['operation']['incomplete_samples'] for o in samples]==[1,2,3]
assert all(o['outcome']==dict(kind='incomplete',win32_error=996) for o in samples+[obs[5]])
gaps=[(b['before_ns']-a['after_ns'])/1e6 for a,b in zip(samples,samples[1:])];assert gaps==[25.4935,25.8365]
assert obs[6]['outcome']['completion_claimed'] is False
assert obs[7]['outcome']==dict(bytes=0,error=995,id=12,kind='completed')
reaps=[e for e in events if e['event']=='data_operation_reaped' and e['detail']['id']==12];assert len(reaps)==1 and reaps[0]['detail']==dict(bytes=0,error=995,id=12,kind='Write')

marks=lines(G/'fixture-events.jsonl');mi={x['kind']:x for x in marks};assert len(marks)==len(mi)==3 and [x['ordinal'] for x in marks]==[1,2,3]
for x in marks:
    assert x['mode']=='data-pending' and x['nonce']==locked['spec']['nonce'] and x['spec_sha256']==locked['spec_sha256']
    assert all(g['identity'][k]==v for k,v in x['identity'].items())
pause=mi['data_read_paused']['detail'];assert pause==dict(framer_bytes=0,framer_expected=4,last_read_id=3,read_issue_count=2,read_operation_present=False)
io=g['io'];reads=[x for x in io if x['action']=='os_issue' and x['kind']=='Read'];assert len(reads)==2 and reads[-1]['id']==3
assert len([x for x in io if x==dict(action='os_completion',bytes=464,error=None,id=3,kind='Read',pending=False)])==1
assert mi['gate_closed']['detail']['read_issue_count']==g['fixture_observation']['read_issue_count']==2
assert mi['host_revoke_received']['detail']['cancel_transition']==dict(cancel_gate_before=False,cancel_gate_after=True,delivery_paused_before=False,first_cancel_reason_before=None,first_cancel_reason_after='HostCancelled',host_control_closed_gate=True)
assert g['data_end_observation']==dict(authority_deadline_renewed=False,control_explained=False,delivery_resumed=False,fixed_wait_budget_ns=None,frames_index_at_pause=None,io_index_at_pause=None,pause_observed=False,wait_cap_ms=500)
assert not any(x['event']['kind']=='actual_core_output_text_delta' for x in lines(G/'core-events.jsonl'))
assert g['task']['core_terminal']=='Cancelled' and g['task']['audit']['first_cancel_reason']=='HostCancelled' and g['task']['transport_drain'] is None
assert g['task']['audit']['parser_yielded_bytes']==g['task']['audit']['received_bytes']==0
assert [p[k] for k in ('received_offset','reserved_offset','issued_offset','os_completed_offset','peer_consumed_offset')]==[8186,2048,1024,0,0]
assert all(p[k]==0 for k in ('parser_yielded_bytes','drain_discarded_bytes','error_consumed_bytes','cancel_discarded_bytes'))
assert p['intent']=='Unknown' and p['error_code']==19 and p['http_status']==200 and not p['http_eof'] and not p['response_material_stored']
cp=g['task']['cleanup']['Ok'];assert cp['received_offset']==8186 and cp['delivered_offset']==cp['acknowledged_offset']==0
assert all(cp[k] for k in ('data_channel_closed','data_connect_reaped','data_read_reaped','data_write_reaped','network_worker_started','network_worker_exited','request_closed'))
assert not cp['durable_observed'] and not cp['network_eof'] and cp['session_release'] is None
co=g['close_observation'];assert co['aggregate_error']=='Unknown' and g['close_result']==g['final_control_result']=={'Err':'Unknown'}
assert all(co[k] for k in ('close_written','close_acked','control_eof','control_stream_ended','control_protocol_clean')) and co['sticky_control_failure'] is None and g['control_end_result']=={'Ok':None}
assert co['matching_ack']==dict(attempt=1,child_pid=s['pid'],code=0,epoch=s['epoch'],generation=2,sequence=7,session=s['session'])
excluded={'owner','owner_released','child_exited','stdout_eof','stderr_eof'};assert all(p[k]==v for k,v in co['final_progress'].items() if k not in excluded)
assert g['local_data_thread_joined'] and g['control_threads_joined'] and g['fixture_writer_joined'] and len(g['control_threads'])==3
assert all(t['joined'] and t['finished'] and not t['handle_retained'] for t in g['control_threads'])
assert r['host_reader_joined'] and not r['reader_errors'] and all(t['joined'] for t in r['fixture_threads'])
assert r['host_exit']==0 and s['exit_code']==2 and all(s[k] for k in ('exit_observed','stdout_eof','stderr_eof'))
assert final['state']['owner']['phase']=='Released' and not final['state']['owner']['owner_retained']
assert not any((g['frames_overflow'],g['io_overflow'],g['task']['audit']['overflow'],s['event_overflow']))
fo=g['fixture_observation'];assert fo['failure'] is None and fo['evidence_complete'] and fo['writer_joined'] and fo['written_records']==fo['enqueued_records']==3

# Offline decode the actual terminal-control bytes; never execute a client/harness.
schema=W/'contracts/experimental/agent_host_v3_http_stream/native_http.capnp';pin(schema,'8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864')
decoded=[]
for e in events:
    if e['event'] not in ('control_frame_sent','control_frame_received'):continue
    b=bytes.fromhex(e['detail']['raw_hex']);assert struct.unpack('<I',b[:4])[0]==len(b)-4
    f=json.loads(subprocess.run([r'C:\Users\Administrator\capnp-bin\capnp.exe','convert','binary:json',str(schema),'Frame'],input=b[4:],capture_output=True,check=True).stdout)
    assert int(f['session'])==s['session'] and f['childPid']==s['pid'] and int(f['instanceEpoch'])==s['epoch'] and int(f['attempt'])==1
    if (f['kind'],int(f['sequence'])) in [('httpCredit',6),('creditState',6),('close',7),('state',7)]:decoded.append(dict(ordinal=e['ordinal'],frame=f))
assert [x['frame']['kind'] for x in decoded]==['httpCredit','creditState','close','state']
c=decoded[0]['frame']['payload']['credit'];aprog=decoded[1]['frame']['payload']['progress']
assert c['windowBytes']==0 and int(c['consumedOffset'])==int(aprog['peerConsumedOffset'])==0
assert all(x['frame']['code']==0 for x in decoded) and int(decoded[1]['frame']['revocationGeneration'])==int(decoded[3]['frame']['revocationGeneration'])==2
close_written=next(e for e in events if e['event']=='close_ack_written');assert close_written['detail']['sequence']==7
assert all(next(e for e in events if e['event']==k)['ordinal']>close_written['ordinal'] for k in ('stdout_eof','stderr_eof','exit'))

codec=W/'reports/codex-morrow-v1.1/joint/m02-admission-owner-002/ledger_codec.py';tree=ast.parse(codec.read_text(encoding='utf-8'));defs=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('take_varint','raw_fields','decompress')]
ns={};exec(compile(ast.Module(body=defs,type_ignores=[]),str(codec),'exec'),ns)
def unpack(b):
    assert 50<=len(b)<=8352 and b[:10]==b'MRNADM04\x01\x00'
    n,size=struct.unpack('<II',b[10:18]);assert n<=8192 and size==len(b)-50
    raw=ns['decompress'](b[50:],n);assert hashlib.sha256(raw).digest()==b[18:50]
    return {k:v for k,(_,v) in ns['raw_fields'](raw).items()}
db=R/'profile/native-admissions.sqlite';before=sha(db);assert not db.with_name(db.name+'-wal').exists();con=sqlite3.connect(db.as_uri()+'?mode=ro&immutable=1',uri=True)
try:
    assert con.execute('pragma user_version').fetchone()[0]==4
    ts=[con.execute('select payload from '+t).fetchall() for t in ('owner','approvals','http_approvals')];assert all(len(t)==1 for t in ts);owner,ap,hp=[unpack(t[0][0]) for t in ts]
    assert owner[5]==b'Released' and owner[6]==s['pid'] and owner[7]==s['session'] and owner[8]==s['epoch'] and all(owner[k]==1 for k in (9,10,11))
    assert owner[2]==ap[2]==hp[3] and ap[8].decode()==a['operation'] and ap[9].hex()==locked['plugin_exe_sha256'] and ap[10].hex()==g['identity']['host_execution_config_sha256']
    assert (ap[14],ap[16],ap[17],ap[18])==(10000,3,1,19) and hp[6].hex()==q['request_sha256'] and hp[7].hex()==q['body_sha256'] and (hp[10],hp[11],hp[12])==(65536,3,1)
finally:con.close()
assert sha(db)==before
for rel,h in m['files'].items():assert sha(R/rel)==h
for path,h in m['plugin_files'].items():assert sha(Path(path))==h
save('pending-chain.json',dict(operation=op,clock_domain=qualified['clock_domain'],sample_gaps_ms=gaps,records=obs,actual_write_reap=reaps[0]))
save('decoded-terminal-control.json',decoded)
save('durable-native-ledger.json',dict(db_sha256=before,immutable_read_only=True,database_unchanged=True,owner='Released',pid=owner[6],session=owner[7],epoch=owner[8],exit_and_dual_eof=True,parent_state=3,source=1,reason=19,ttl_ms=10000,http_state=3,send_budget=1,response_limit=65536,request_sha256=q['request_sha256'],body_sha256=q['body_sha256'],core_coordination_payload_decoded=False))
save('inputs.json',dict(pins=pins,original_batch_files=20,fixed79_rescanned=False))
save('result.json',dict(status='single_B011_producer_run_independently_reviewed_pass',post_count=1,request_bytes=22025,write_operation=op,three_incomplete_samples=True,sample_gaps_ms=gaps,cancel_probe_incomplete=True,actual_reap_error=995,actual_reap_bytes=0,read_issue_count=2,parser_and_peer_consumed=0,host_ack_upper_bound_ms=rtt,cleanup_ok=True,actual_data_join=True,control_and_other_joins=True,control_clean=True,host_exit=0,guest_exit=2,durable_owner_released=True,independent_http_runs=0,retries=0,new_fault_scenarios_run=False,product_gate_upgrade=False))
print(json.dumps(dict(status='B_read_only_review_complete',pins=len(pins),gaps_ms=gaps,ack_upper_ms=rtt,result_sha256=sha(J/'result.json'))))
