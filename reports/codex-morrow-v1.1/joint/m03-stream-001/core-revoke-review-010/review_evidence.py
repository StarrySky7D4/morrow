"""Read-only review of the single failed A run; never imports a runtime harness."""
from pathlib import Path
import ast, hashlib, json, sqlite3, struct

W = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
P = W.parent.parent.parent / 'morrow-codex'
R = W / 'reports/codex-morrow-v1.1/host/m03-stream-001/revoke-010-core-revoke-20260929T010056786082Z'
G = P / 'out/m03-host-revoke-010/20260929T010056786082Z'
J = Path(__file__).resolve().parent
H = R.parent / 'native-host-candidate-004'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def lines(p): return [json.loads(x) for x in p.read_text(encoding='utf-8').splitlines() if x]
def save(name, value):
    with (J/name).open('x', encoding='utf-8') as f:
        json.dump(value, f, indent=2); f.write('\n')
pins=[]
def pin(p,h):
    actual=sha(p); assert actual==h, str(p)
    pins.append(dict(path=str(p),sha256=actual))
pin(R/'result.json','b39c0907d5df6d3c0964abfec454eb5568d2069c11534221bb05b23fab23cf7e')
pin(R/'evidence-manifest.json','1aee8877b1c10743ae086295a6c25407be058d98945298d0388d111dde918417')
m=read(R/'evidence-manifest.json')
for rel,h in m['files'].items(): pin(R/rel,h)
for p,h in m['plugin_files'].items(): pin(Path(p),h)
locked=read(R/'locked-expectations.json')
pin(W/'tool/m03_revoke_barriers_010.py',locked['script_sha256'])
pin(H/'manifest.json',locked['host'][1]); hm=read(H/'manifest.json')
pin(Path(hm['executable']),locked['host'][2])
for rel,h in hm['source_files'].items(): pin(H/'source'/rel,h)
pm_path=P/'receipts/m03-fixture-001/native-candidate-fixture-001.json'
pin(pm_path,locked['plugin_manifest_sha256']); pm=read(pm_path)
pin(Path(pm['executable']),locked['plugin_exe_sha256'])
for rel,h in pm['input_sha256'].items(): pin(P/rel,h)

result=read(R/'result.json'); guest=read(G/'result.json')
rows=read(R/'host-rows.json'); finals=[r['value'] for r in rows if r['value'].get('event')=='final']
assert len(finals)==1
final=finals[0]; s=final['snapshot']; p=s['http']['progress']
server=read(R/'server-events.json'); posts=[x for x in server if x['kind']=='post']
assert len(posts)==result['post_count']==1 and result['status']=='failed' and result['automatic_retry'] is False
approval=read(R/'independent-approval.json'); q=approval['proposal']; ident=approval['identity']
body=bytes.fromhex(q['body_hex']); assert body==bytes.fromhex(posts[0]['body_hex']) and len(body)==22025
assert hashlib.sha256(body).hexdigest()==q['body_sha256']
u32=lambda n:struct.pack('<I',n); u64=lambda n:struct.pack('<Q',n); field=lambda b:u32(len(b))+b
pre=b'Morrow/native-http-proposal/v1\0'+u64(ident['session'])+u64(ident['epoch'])+field(approval['operation'].encode())+u64(1)
pre+=field(q['method'].encode())+field(q['absolute_target'].encode())+u32(len(q['headers']))
for h in q['headers']: pre+=field(h['name'].encode())+field(bytes.fromhex(h['value_hex']))
pre+=u32(q['response_limit'])+field(body)
assert hashlib.sha256(pre).hexdigest()==q['request_sha256']==approval['recomputed_request_sha256']
assert q['method']=='POST' and posts[0]['path']=='/v1/responses' and q['response_limit']==65536
assert not any(h['name'].lower()=='authorization' for h in q['headers'])
commands=read(R/'operator-commands.json'); actions=[x['value']['action'] for x in commands]
assert all(actions.count(a)==1 for a in ('approve','claim','approve_http','revoke'))
ack=read(R/'revoke-ack.json'); upper=(ack['row']['line_received_ns']-ack['command']['before_write_ns'])/1e6
assert upper==15.2387 and upper<=500 and ack['row'] in rows and ack['command'] in commands
assert ack['ack']==dict(application_pending=False,first_revocation_reason=19,first_revocation_source=1,persisted=True,runtime_applied=True)
applied=[e for e in s['events'] if e['event']=='http_cancel_applied']; assert len(applied)==1
assert applied[0]['detail']['source']==1 and applied[0]['detail']['code']==19
assert not any(e['event']=='authority_revocation_persistence_unconfirmed' for e in s['events'])

co=guest['close_observation']; ca=co['matching_ack']
assert all(co[k] for k in ('close_written','close_acked','control_eof','control_stream_ended','control_protocol_clean'))
assert co['sticky_control_failure'] is None and guest['control_end_result']=={'Ok':None}
assert ca['session']==s['session'] and ca['epoch']==s['epoch'] and ca['child_pid']==s['pid']
assert ca['attempt']==1 and ca['generation']==s['generation']==2 and ca['code']==0 and ca['sequence']==7
acks=[x for x in s['events'] if x['event']=='close_ack_written']; assert len(acks)==1 and acks[0]['detail']=={'bytes':420,'sequence':7}
assert all(next(e for e in s['events'] if e['event']==k)['ordinal']>acks[0]['ordinal'] for k in ('stdout_eof','stderr_eof','exit'))
assert all(s[k] for k in ('exit_observed','stdout_eof','stderr_eof')) and s['exit_code']==2 and result['host_exit']==0
assert final['state']['owner']['phase']=='Released' and final['state']['owner']['owner_retained'] is False
assert result['host_reader_joined'] and not result['reader_errors'] and all(x['joined'] for x in result['fixture_threads'])
assert guest['control_threads_joined'] and all(x['joined'] for x in guest['control_threads']) and guest['fixture_writer_joined']
assert guest['task']['core_terminal']=='Cancelled' and guest['task']['audit']['first_cancel_reason']=='NativeFailure'
error={'Protocol':'unexpected data EOF/error'}
assert co['aggregate_error']==error and guest['close_result']==guest['final_control_result']=={'Err':error}
assert guest['task']['cleanup']=={'Err':'CleanupUnconfirmed'} and guest['local_data_thread_joined'] is False
assert guest['io'][-1]==dict(action='os_completion',bytes=0,error=109,id=9,kind='Read',pending=False)
assert guest['io'][-2]==dict(action='os_issue',bytes=4,error=None,id=9,kind='Read',pending=True)
frames=guest['frames']; cancel_i=next(i for i,e in enumerate(frames) if e['lane']=='guest_control_written' and e['kind']=='HttpCancel')
terminal_i=next(i for i,e in enumerate(frames) if e['lane']=='host_control' and e['kind']=='HttpTerminal')
assert frames[cancel_i]['sequence']==5 and cancel_i<terminal_i
assert all(p[k]==197 for k in ('received_offset','reserved_offset','issued_offset','os_completed_offset','peer_consumed_offset','parser_yielded_bytes'))
assert p['intent']=='Unknown' and p['error_code']==19 and not p['http_eof'] and not p['response_material_stored']
assert all(p[k] for k in ('request_closed','data_closed','connect_reaped','read_reaped','write_reaped','worker_started','worker_joined'))
excluded={'owner','owner_released','child_exited','stdout_eof','stderr_eof'}
assert all(p[k]==v for k,v in co['final_progress'].items() if k not in excluded)
assert not any((s['event_overflow'],guest['frames_overflow'],guest['io_overflow'],guest['task']['audit']['overflow']))
markers=lines(G/'fixture-events.jsonl'); business=lines(G/'core-events.jsonl')
assert len(markers)==7 and [x['ordinal'] for x in markers]==list(range(1,8))
mi={x['kind']:x for x in markers}; assert len(mi)==7
assert [x['kind'] for x in markers]==['core_event_queued','core_event_reserved','core_event_held','core_reserved_suppressed','host_revoke_received','gate_closed','core_event_suppressed']
for x in markers:
    assert x['mode']==locked['mode'] and x['nonce']==locked['spec']['nonce'] and x['spec_sha256']==locked['spec_sha256']
    assert all(guest['identity'][k]==v for k,v in x['identity'].items())
ae=mi['core_event_queued']['detail']['event']; be=mi['core_event_reserved']['detail']['event']
assert ae==mi['core_event_held']['detail']['event']==mi['core_event_suppressed']['detail']['event']
assert be==mi['core_reserved_suppressed']['detail']['event']
assert ae['sha256']==hashlib.sha256(b'barrier-A').hexdigest() and be['sha256']==hashlib.sha256(b'barrier-B').hexdigest()
assert ae['delta_ordinal']==1 and be['delta_ordinal']==2 and ae['stream_ordinal']==3 and be['stream_ordinal']==4
assert mi['core_reserved_suppressed']['detail']['gate_closed'] and mi['core_reserved_suppressed']['detail']['permit_released']
assert mi['core_event_suppressed']['detail']['gate_closed'] and mi['core_event_suppressed']['ordinal']>mi['gate_closed']['ordinal']
assert len(business)==2 and all(x['event']['kind']=='actual_core_other_event' for x in business)
assert guest['fixture_observation']['evidence_complete'] and guest['fixture_observation']['failure'] is None

# Pure bounded decoder only, never execute mutation helpers or restore authority.
codec=W/'reports/codex-morrow-v1.1/joint/m02-admission-owner-002/ledger_codec.py'
tree=ast.parse(codec.read_text(encoding='utf-8'))
defs=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('take_varint','raw_fields','decompress')]
ns={}; exec(compile(ast.Module(body=defs,type_ignores=[]),str(codec),'exec'),ns)
def unpack(blob):
    assert 50<=len(blob)<=8352 and blob[:10]==b'MRNADM04\x01\x00'
    n,size=struct.unpack('<II',blob[10:18]); assert n<=8192 and size==len(blob)-50
    raw=ns['decompress'](blob[50:],n); assert hashlib.sha256(raw).digest()==blob[18:50]
    return {k:v for k,(_,v) in ns['raw_fields'](raw).items()}
pin(codec,sha(codec)); db=R/'profile/native-admissions.sqlite'; before=sha(db)
assert not db.with_name(db.name+'-wal').exists()
con=sqlite3.connect(db.as_uri()+'?mode=ro&immutable=1',uri=True)
try:
    assert con.execute('pragma user_version').fetchone()[0]==4
    owners=con.execute('select payload from owner').fetchall(); aps=con.execute('select payload from approvals').fetchall(); hps=con.execute('select payload from http_approvals').fetchall()
    assert len(owners)==len(aps)==len(hps)==1
    owner=unpack(owners[0][0]); ap=unpack(aps[0][0]); hp=unpack(hps[0][0])
    assert owner[5]==b'Released' and owner[6]==s['pid'] and owner[7]==s['session'] and owner[8]==s['epoch']
    assert all(owner[k]==1 for k in (9,10,11)) and owner[2]==ap[2]==hp[3]
    assert ap[8].decode()==approval['operation'] and ap[9].hex()==locked['plugin_exe_sha256'] and ap[10].hex()==guest['identity']['host_execution_config_sha256']
    assert ap[14]==10000 and ap[16]==3 and ap[17]==1 and ap[18]==19
    assert hp[6].hex()==q['request_sha256'] and hp[7].hex()==q['body_sha256'] and hp[10]==65536 and hp[11]==3 and hp[12]==1
finally: con.close()
assert sha(db)==before
# Recheck all original files after reading the database.
for rel,h in m['files'].items(): assert sha(R/rel)==h
for path,h in m['plugin_files'].items(): assert sha(Path(path))==h
save('inputs.json',dict(pins=pins,host_batch_files=15,guest_batch_files=5,fixture_inputs=55))
save('durable-native-ledger.json',dict(db_sha256=before,immutable_read_only=True,database_unchanged=True,version=4,owner_phase='Released',pid=owner[6],session=owner[7],epoch=owner[8],exit_and_dual_eof=True,parent_state=ap[16],revocation_source=ap[17],revocation_reason=ap[18],original_ttl_ms=ap[14],http_state=hp[11],send_budget=hp[12],response_limit=hp[10],request_sha256=hp[6].hex(),body_sha256=hp[7].hex(),scope='native owner and approval records only; Core coordination payload not decoded'))
save('result.json',dict(status='original_A_failure_independently_confirmed',original_result_sha256=sha(R/'result.json'),post_count=1,request_body_bytes=22025,revoke_ack_upper_bound_ms=upper,first_cancel_reason='NativeFailure',aggregate_error=error,guest_cleanup='CleanupUnconfirmed',guest_data_thread_explicit_join_confirmed=False,independent_control_clean=True,host_exit=0,guest_exit=2,host_owner_durably_released=True,target_events_suppressed=True,clean_host_control_cancellation_qualified=False,marker_order=[dict(ordinal=x['ordinal'],kind=x['kind']) for x in markers],independent_runtime_runs=0,B_run=False,retry=False,product_gate_upgrade=False))
print(json.dumps(dict(status='review_complete_original_failure_preserved',verified_pin_entries=len(pins),ledger_sha256=sha(J/'durable-native-ledger.json'),result_sha256=sha(J/'result.json'))))
