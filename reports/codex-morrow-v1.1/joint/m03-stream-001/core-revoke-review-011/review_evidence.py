"""Read-only A011 evidence, stored-frame decode and immutable ledger review."""
from pathlib import Path
import ast,hashlib,json,sqlite3,struct,subprocess
W=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
P=W.parent.parent.parent/'morrow-codex';J=Path(__file__).resolve().parent
R=W/'reports/codex-morrow-v1.1/host/m03-stream-001/revoke-011-core-revoke-20260929T015028059792Z'
G=P/'out/m03-host-revoke-011/20260929T015028059792Z'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
def lines(p):return [json.loads(x) for x in p.read_text(encoding='utf-8').splitlines() if x]
def save(n,v):
    with (J/n).open('x',encoding='utf-8') as f:json.dump(v,f,indent=2);f.write('\n')
pins=[]
def pin(p,h):
    assert sha(p)==h,str(p)
    pins.append(dict(path=str(p),sha256=h))
pin(R/'result.json','84636aa83df189a8fa6cacaf076c2473a0a679e95c02d0b8bf183d0d7111ae99')
pin(R/'evidence-manifest.json','888ab7bec35e60459251743171e154fad4b6aa8525a1a43a7cbe2d67ee84ad6a')
m=read(R/'evidence-manifest.json')
for rel,h in m['files'].items():pin(R/rel,h)
for path,h in m['plugin_files'].items():pin(Path(path),h)
locked=read(R/'locked-expectations.json');pin(W/'tool/m03_revoke_barriers_011.py',locked['script_sha256'])
pm=P/'receipts/m03-fixture-002/native-candidate-fixture-002.json';pin(pm,locked['plugin_manifest_sha256'])
pin(Path(read(pm)['executable']),locked['plugin_exe_sha256'])
hm=R.parent/'native-host-candidate-004/manifest.json';pin(hm,locked['host'][1]);pin(Path(read(hm)['executable']),locked['host'][2])
pin(J.parent/'native-fixture-review-002/result.json','877381bab95263f2cdd27cce9c8b463e5b13fba5fc48822f568ef97c614806e9')
# Fixed79 inputs were reviewed immediately beforehand; do not rescan them here.
r=read(R/'result.json');g=read(G/'result.json');rows=read(R/'host-rows.json')
finals=[x['value'] for x in rows if x['value'].get('event')=='final'];assert len(finals)==1
final=finals[0];s=final['snapshot'];p=s['http']['progress'];ev=s['events']
assert r['status']=='passed' and not r['automatic_retry'] and r['guest_result']==g
server=read(R/'server-events.json');posts=[x for x in server if x['kind']=='post'];assert len(posts)==r['post_count']==1
a=read(R/'independent-approval.json');q=a['proposal'];body=bytes.fromhex(q['body_hex'])
assert body==bytes.fromhex(posts[0]['body_hex']) and len(body)==22025 and hashlib.sha256(body).hexdigest()==q['body_sha256']
u32=lambda x:struct.pack('<I',x);u64=lambda x:struct.pack('<Q',x);field=lambda b:u32(len(b))+b
pre=b'Morrow/native-http-proposal/v1\0'+u64(a['identity']['session'])+u64(a['identity']['epoch'])+field(a['operation'].encode())+u64(1)
pre+=field(q['method'].encode())+field(q['absolute_target'].encode())+u32(len(q['headers']))
for h in q['headers']:pre+=field(h['name'].encode())+field(bytes.fromhex(h['value_hex']))
pre+=u32(q['response_limit'])+field(body);assert hashlib.sha256(pre).hexdigest()==q['request_sha256']==a['recomputed_request_sha256']
assert q['method']=='POST' and posts[0]['path']=='/v1/responses' and q['response_limit']==65536
assert len([x for x in server if x['kind']=='body_prefix_sent' and x['bytes']==197])==1
commands=read(R/'operator-commands.json');acts=[x['value']['action'] for x in commands]
assert all(acts.count(k)==1 for k in ('approve','claim','approve_http','revoke'))
ack=read(R/'revoke-ack.json');assert ack['row'] in rows and ack['command'] in commands
rtt=(ack['row']['line_received_ns']-ack['command']['before_write_ns'])/1e6;assert rtt==14.6303 and rtt<=500
assert ack['ack']==dict(application_pending=False,first_revocation_reason=19,first_revocation_source=1,persisted=True,runtime_applied=True)
applied=[x for x in ev if x['event']=='http_cancel_applied'];assert len(applied)==1
assert applied[0]['detail']['source']==1 and applied[0]['detail']['code']==19
assert not any(x['event']=='revoke_persistence_unconfirmed' for x in ev)
marks=lines(G/'fixture-events.jsonl');mi={x['kind']:x for x in marks};business=lines(G/'core-events.jsonl')
assert len(marks)==len(mi)==7 and [x['ordinal'] for x in marks]==list(range(1,8))
for x in marks:
    assert x['mode']=='core-revoke' and x['nonce']==locked['spec']['nonce'] and x['spec_sha256']==locked['spec_sha256']
    assert all(g['identity'][k]==v for k,v in x['identity'].items())
qa,qb=mi['core_event_queued']['detail']['event'],mi['core_event_reserved']['detail']['event']
assert qa==mi['core_event_held']['detail']['event']==mi['core_event_suppressed']['detail']['event']
assert qb==mi['core_reserved_suppressed']['detail']['event']
for e,t,n,stream in ((qa,b'barrier-A',1,3),(qb,b'barrier-B',2,4)):
    assert e==dict(bytes=9,delta_ordinal=n,kind='OutputTextDelta',sha256=hashlib.sha256(t).hexdigest(),stream_ordinal=stream)
assert mi['core_event_queued']['ordinal']<mi['core_event_held']['ordinal']<mi['gate_closed']['ordinal']<mi['core_event_suppressed']['ordinal']
assert mi['core_event_reserved']['ordinal']<mi['core_reserved_suppressed']['ordinal']
assert all(mi[k]['detail']['gate_closed'] for k in ('core_reserved_suppressed','core_event_suppressed'))
assert mi['core_reserved_suppressed']['detail']['permit_released']
assert len(business)==2 and all(x['event']['kind']=='actual_core_other_event' for x in business)
transition=mi['host_revoke_received']['detail']['cancel_transition']
assert transition==dict(cancel_gate_before=False,cancel_gate_after=True,delivery_paused_before=True,first_cancel_reason_before=None,first_cancel_reason_after='HostCancelled',host_control_closed_gate=True)
assert g['task']['audit']['first_cancel_reason']=='HostCancelled' and g['task']['core_terminal']=='Cancelled' and g['task']['transport_drain'] is None
release=read(G/'release-consumer.json');rp=read(R/'release-preconditions.json');qualified=read(R/'qualified-before-revoke.json')
assert release['event']==qa and release['identity']==mi['host_revoke_received']['identity'] and release['stage']=='release-consumer'
assert release['nonce']==locked['spec']['nonce'] and release['spec_sha256']==locked['spec_sha256'] and (G/'release-consumer.json').stat().st_size<=2048
assert rp['guest_revoke']==mi['host_revoke_received'] and rp['guest_gate']==mi['gate_closed'] and applied[0] in rp['host_applied']
assert qualified==dict(queued=mi['core_event_queued'],held=mi['core_event_held'],reserved=mi['core_event_reserved'])
d=g['data_end_observation'];frames=g['frames'];io=g['io']
assert d==dict(authority_deadline_renewed=False,control_explained=True,delivery_resumed=False,fixed_wait_budget_ns=500000000,frames_index_at_pause=26,io_index_at_pause=18,pause_observed=True,wait_cap_ms=500)
assert io[17]==dict(action='os_completion',bytes=0,error=109,id=9,kind='Read',pending=False)
assert not any(x['action']=='os_issue' for x in io[18:])
for f in frames[26:]:
    if f['lane']=='guest_control_write_admitted':
        assert f['delivery_paused'] and f['kind']!='HttpCommit'
        assert f['kind']!='HttpCredit' or f['credit_window_bytes']==0
find=lambda lane,kind,seq:next(i for i,f in enumerate(frames) if f['lane']==lane and f['kind']==kind and f['sequence']==seq)
assert find('host_control','HttpTerminal',0)<find('guest_control_written','HttpCancel',5)
assert find('guest_control_written','HttpCredit',6)<find('host_control','CreditState',6)<find('guest_control_write_admitted','Close',7)<find('guest_control_written','Close',7)
assert frames[find('guest_control_written','HttpCredit',6)]['credit_window_bytes']==0
cleanup=g['task']['cleanup']['Ok'];assert all(cleanup[k]==197 for k in ('received_offset','delivered_offset','acknowledged_offset'))
assert all(cleanup[k] for k in ('data_channel_closed','data_connect_reaped','data_read_reaped','data_write_reaped','network_worker_started','network_worker_exited','request_closed'))
assert not cleanup['network_eof'] and not cleanup['durable_observed'] and cleanup['session_release'] is None
assert all(p[k]==197 for k in ('received_offset','reserved_offset','issued_offset','os_completed_offset','peer_consumed_offset','parser_yielded_bytes'))
assert p['intent']=='Unknown' and p['error_code']==19 and not p['http_eof'] and not p['response_material_stored']
assert sum(p[k] for k in ('parser_yielded_bytes','drain_discarded_bytes','error_consumed_bytes','cancel_discarded_bytes'))==197
co=g['close_observation'];assert co['aggregate_error']=='Unknown' and g['close_result']==g['final_control_result']=={'Err':'Unknown'}
assert all(co[k] for k in ('close_written','close_acked','control_eof','control_stream_ended','control_protocol_clean'))
assert co['sticky_control_failure'] is None and g['control_end_result']=={'Ok':None}
assert co['matching_ack']==dict(attempt=1,child_pid=s['pid'],code=0,epoch=s['epoch'],generation=2,sequence=7,session=s['session'])
excluded={'owner','owner_released','child_exited','stdout_eof','stderr_eof'}
assert all(p[k]==v for k,v in co['final_progress'].items() if k not in excluded)
assert g['local_data_thread_joined'] and g['control_threads_joined'] and g['fixture_writer_joined']
assert len(g['control_threads'])==3 and all(t['joined'] and t['finished'] and not t['handle_retained'] for t in g['control_threads'])
assert r['host_reader_joined'] and not r['reader_errors'] and all(t['joined'] for t in r['fixture_threads'])
assert r['host_exit']==0 and s['exit_code']==2 and all(s[k] for k in ('exit_observed','stdout_eof','stderr_eof'))
assert final['state']['owner']['phase']=='Released' and not final['state']['owner']['owner_retained']
assert not any((g['frames_overflow'],g['io_overflow'],g['task']['audit']['overflow'],s['event_overflow']))
fo=g['fixture_observation'];assert fo['failure'] is None and fo['evidence_complete'] and fo['writer_joined'] and fo['written_records']==fo['enqueued_records']==7

# Decode stored wire bytes offline using the pinned schema, not a native client.
schema=W/'contracts/experimental/agent_host_v3_http_stream/native_http.capnp'
pin(schema,'8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864')
decoded=[]
for e in ev:
    if e['event'] not in ('control_frame_sent','control_frame_received'):continue
    b=bytes.fromhex(e['detail']['raw_hex']);assert struct.unpack('<I',b[:4])[0]==len(b)-4
    out=subprocess.run([r'C:\Users\Administrator\capnp-bin\capnp.exe','convert','binary:json',str(schema),'Frame'],input=b[4:],capture_output=True,check=True)
    f=json.loads(out.stdout);decoded.append(dict(ordinal=e['ordinal'],host_supervisor_at_us=e['at_us'],direction=e['event'],frame=f))
save('decoded-control.json',decoded)

codec=W/'reports/codex-morrow-v1.1/joint/m02-admission-owner-002/ledger_codec.py'
tree=ast.parse(codec.read_text(encoding='utf-8'));defs=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('take_varint','raw_fields','decompress')]
ns={};exec(compile(ast.Module(body=defs,type_ignores=[]),str(codec),'exec'),ns)
def unpack(b):
    assert 50<=len(b)<=8352 and b[:10]==b'MRNADM04\x01\x00'
    n,size=struct.unpack('<II',b[10:18]);assert n<=8192 and size==len(b)-50
    raw=ns['decompress'](b[50:],n);assert hashlib.sha256(raw).digest()==b[18:50]
    return {k:v for k,(_,v) in ns['raw_fields'](raw).items()}
db=R/'profile/native-admissions.sqlite';before=sha(db);assert not db.with_name(db.name+'-wal').exists()
con=sqlite3.connect(db.as_uri()+'?mode=ro&immutable=1',uri=True)
try:
    assert con.execute('pragma user_version').fetchone()[0]==4
    tables=[con.execute('select payload from '+t).fetchall() for t in ('owner','approvals','http_approvals')];assert all(len(t)==1 for t in tables)
    owner,ap,hp=[unpack(t[0][0]) for t in tables]
    assert owner[5]==b'Released' and owner[6]==s['pid'] and owner[7]==s['session'] and owner[8]==s['epoch'] and all(owner[k]==1 for k in (9,10,11))
    assert owner[2]==ap[2]==hp[3] and ap[8].decode()==a['operation'] and ap[9].hex()==locked['plugin_exe_sha256'] and ap[10].hex()==g['identity']['host_execution_config_sha256']
    assert (ap[14],ap[16],ap[17],ap[18])==(10000,3,1,19)
    assert hp[6].hex()==q['request_sha256'] and hp[7].hex()==q['body_sha256'] and (hp[10],hp[11],hp[12])==(65536,3,1)
finally:con.close()
assert sha(db)==before
for rel,h in m['files'].items():assert sha(R/rel)==h
for path,h in m['plugin_files'].items():assert sha(Path(path))==h
save('durable-native-ledger.json',dict(db_sha256=before,immutable_read_only=True,database_unchanged=True,owner='Released',pid=owner[6],session=owner[7],epoch=owner[8],exit_and_dual_eof=True,parent_state=3,source=1,reason=19,ttl_ms=10000,http_state=3,send_budget=1,response_limit=65536,request_sha256=q['request_sha256'],body_sha256=q['body_sha256'],core_coordination_payload_decoded=False))
save('inputs.json',dict(pins=pins,original_batch_files=20,fixed79_rescanned=False))
save('result.json',dict(status='single_A011_producer_run_independently_reviewed_pass',post_count=1,request_bytes=22025,response_bytes=197,host_ack_upper_bound_ms=rtt,first_reason='HostCancelled',data_first_pause=True,original_targets_suppressed=True,zero_credit_ack_before_close=True,cleanup_acknowledged_offset=197,guest_data_joined=True,control_clean=True,aggregate='Unknown',host_exit=0,guest_exit=2,durable_owner_released=True,independent_http_runs=0,B_run=False,old_A010_still_failed=True,product_gate_upgrade=False))
print(json.dumps(dict(status='read_only_review_complete',pins=len(pins),decoded_control_frames=len(decoded),result_sha256=sha(J/'result.json'))))
