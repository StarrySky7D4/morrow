"""Read-only review of sealed matrix008; never imports or executes its runtime harness."""
from pathlib import Path
import ast
import hashlib
import json
import sqlite3
import struct

W = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
P = W.parent.parent.parent / 'morrow-codex'
R = W / 'reports/codex-morrow-v1.1/host/m03-stream-001/http-matrix-008-20260928T235143862332Z'
J = Path(__file__).resolve().parent
H = R.parent / 'native-host-candidate-004'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def save(name, value):
    with (J / name).open('x', encoding='utf-8') as f:
        json.dump(value, f, indent=2); f.write('\n')

# Reuse only pure decoding functions, never the older helper's mutation functions.
codec = W / 'reports/codex-morrow-v1.1/joint/m02-admission-owner-002/ledger_codec.py'
tree = ast.parse(codec.read_text(encoding='utf-8'))
defs = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in ('take_varint', 'raw_fields', 'decompress')]
ns = {}; exec(compile(ast.Module(body=defs, type_ignores=[]), str(codec), 'exec'), ns)
def unpack(blob):
    assert 50 <= len(blob) <= 8352 and blob[:10] == b'MRNADM04\x01\x00'
    n, size = struct.unpack('<II', blob[10:18]); assert n <= 8192 and size == len(blob)-50
    raw = ns['decompress'](blob[50:], n)
    assert hashlib.sha256(raw).digest() == blob[18:50]
    return {k: v for k, (_, v) in ns['raw_fields'](raw).items()}

pins = []
def pin(path, expected):
    actual = sha(path); assert actual == expected, str(path)
    pins.append({'path': str(path), 'sha256': actual})
pin(R/'result.json', 'be1d8addf1275ee32c49a17c6a891b5d7e7aeea75fe8c5d3062a314e9e30d548')
pin(R/'producer-handoff-008.json', '0828b7e3028276b997ac9150ef32e9f8dea6f67191167c721d14b870551c6a60')
pin(R/'batch-evidence-manifest.json', '270ed265aaf3755b79d2dabf5e5f4cfb463493e5771344405872163a866c4e2b')
batch = read(R/'batch-evidence-manifest.json')
for rel, h in batch['files'].items(): pin(R/rel, h)
for path, h in batch['plugin_evidence'].items(): pin(Path(path), h)
locked = read(R/'locked-expectations.json')
pin(W/'tool/m03_http_matrix_008.py', locked['script_sha256'])
hm = read(H/'manifest.json'); pm_path = P/'receipts/m03-stream-003/native-candidate-003.json'; pm = read(pm_path)
pin(H/'manifest.json', 'a1c46e448450f1066177e8f51f655f202757517f1cdb83451e7545ac68bbf6f2')
pin(pm_path, '54f4c6cd38e1968d1dbc54d26b8bf40460ee660498f8590c7f44c10fe04396c5')
pin(Path(hm['executable']), locked['host_sha256']); pin(Path(pm['executable']), locked['guest_sha256'])
for rel, h in pm['input_sha256'].items(): pin(P/rel, h)
for rel, h in hm['source_files'].items(): pin(H/'source'/rel, h)
pins.append({'path':str(codec), 'sha256':sha(codec)})

offsets = ('received_offset','reserved_offset','issued_offset','os_completed_offset','peer_consumed_offset')
facts = []; ledger = []; manifest_counts = {}
for case in ('s503','r307','exact','over'):
    C = R/case; cr = read(C/'result.json'); G = Path(cr['plugin_evidence']); guest = read(G/'result.json')
    final = read(C/'retained-final.json'); s = final['snapshot']; p = s['http']['progress']
    rows = read(C/'host-rows.json'); server = read(C/'server-events.json'); sink = read(C/'sink-events.json')
    assert [x['value'] for x in rows if x['value'].get('event')=='final'] == [final]
    cm = read(C/'evidence-manifest.json')
    for rel,h in cm['files'].items(): pin(C/rel,h)
    for path,h in cm['plugin_evidence'].items(): pin(Path(path),h)
    manifest_counts[case] = {k:len(v) for k,v in cm.items()}
    posts = [x for x in server if x['event']=='post_received']; assert len(posts)==1 and not sink
    assert not [x for x in server if x['event']=='server_error']
    approval = read(C/'independent-approval.json'); q = approval['proposal']; ident = approval['identity']
    body = bytes.fromhex(q['body_hex']); assert body == bytes.fromhex(posts[0]['body_hex'])
    assert hashlib.sha256(body).hexdigest()==q['body_sha256'] and q['response_limit']==65536
    u32=lambda n:struct.pack('<I',n); u64=lambda n:struct.pack('<Q',n); field=lambda b:u32(len(b))+b
    pre=b'Morrow/native-http-proposal/v1\0'+u64(ident['session'])+u64(ident['epoch'])+field(approval['operation'].encode())+u64(1)
    pre+=field(q['method'].encode())+field(q['absolute_target'].encode())+u32(len(q['headers']))
    for h in q['headers']: pre+=field(h['name'].encode())+field(bytes.fromhex(h['value_hex']))
    pre+=u32(q['response_limit'])+field(body)
    assert hashlib.sha256(pre).hexdigest()==q['request_sha256']==approval['recomputed_request_sha256']
    assert q['method']=='POST' and posts[0]['path']=='/v1/responses'
    assert not any(k.lower()=='authorization' for k,v in posts[0]['headers'])
    commands=read(C/'operator-commands.json'); acts=[x['value']['action'] for x in commands]
    assert acts.count('approve')==acts.count('claim')==acts.count('approve_http')==1
    assert set(acts) <= {'approve','claim','inspect_http','approve_http'}
    co=guest['close_observation']; ack=co['matching_ack']
    assert all(co[k] is True for k in ('close_written','close_acked','control_eof','control_stream_ended','control_protocol_clean'))
    assert co['sticky_control_failure'] is None and guest['control_end_result']=={'Ok':None}
    expected={'session':s['session'],'epoch':s['epoch'],'child_pid':s['pid'],'attempt':1,'generation':s['generation'],'code':0}
    assert all(ack[k]==v for k,v in expected.items())
    writes=[x for x in guest['frames'] if x['lane']=='guest_control_written' and x['kind']=='Close']
    acks=[x for x in s['events'] if x['event']=='close_ack_written']; assert len(writes)==len(acks)==1
    assert writes[0]['sequence']==ack['sequence']==acks[0]['detail']['sequence'] and acks[0]['detail']['bytes']==412
    host_ack=[x for x in guest['frames'] if x['lane']=='host_control' and x['kind']=='State' and x['sequence']==ack['sequence']]
    assert len(host_ack)==1 and host_ack[0]['generation']==ack['generation'] and host_ack[0]['code']==0
    spawn=next(x['detail'] for x in s['events'] if x['event']=='spawn')
    expected_ident=dict(expected); expected_ident.pop('generation'); expected_ident.pop('code')
    expected_ident.update(artifact_sha256=locked['guest_sha256'],schema_sha256='8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864',host_execution_config_sha256=spawn['config_sha256'],operation_id_sha256=hashlib.sha256(approval['operation'].encode()).hexdigest())
    assert all(guest['identity'][k]==v for k,v in expected_ident.items())
    excluded={'owner','owner_released','child_exited','stdout_eof','stderr_eof'}
    assert all(p[k]==v for k,v in co['final_progress'].items() if k not in excluded)
    cleanup=guest['task']['cleanup']['Ok']; assert guest['local_data_thread_joined'] is True
    assert cleanup['received_offset']==p['received_offset'] and cleanup['delivered_offset']==cleanup['acknowledged_offset']==p['peer_consumed_offset']
    assert all(cleanup[k] for k in ('request_closed','data_channel_closed','data_connect_reaped','data_read_reaped','data_write_reaped','network_worker_started','network_worker_exited'))
    assert all(p[k] for k in ('request_closed','data_closed','connect_reaped','read_reaped','write_reaped','worker_started','worker_joined'))
    assert cleanup['durable_observed']==p['response_material_stored'] and cleanup['network_eof']==p['http_eof']
    end_events={name:next(x for x in s['events'] if x['event']==name) for name in ('stdout_eof','stderr_eof','exit')}
    assert all(x['ordinal']>acks[0]['ordinal'] for x in end_events.values())
    assert s['exit_code']==end_events['exit']['detail']['code']==(0 if case=='exact' else 2)
    assert all(s[k] for k in ('exit_observed','stdout_eof','stderr_eof'))
    assert cr['host_lifetime']['status']=='os_exit_observed' and cr['host_lifetime']['exit']==0
    assert final['state']['owner']['phase']=='Released' and final['state']['owner']['owner_retained'] is False
    assert not any((s['event_overflow'],guest['frames_overflow'],guest['io_overflow'],guest['task']['audit']['overflow']))
    response=(C/'response-body.bin').read_bytes(); expected_bytes=59 if case in ('s503','r307') else 65536
    assert all(p[k]==expected_bytes for k in offsets)
    assert p['peer_consumed_offset']==sum(p[k] for k in ('parser_yielded_bytes','drain_discarded_bytes','error_consumed_bytes','cancel_discarded_bytes'))
    assert co['aggregate_error']==(None if case=='exact' else 'Unknown')
    expected_aggregate={'Ok':None} if case=='exact' else {'Err':'Unknown'}
    assert guest['close_result']==guest['final_control_result']==expected_aggregate
    events=[json.loads(x) for x in (G/'core-events.jsonl').read_text().splitlines() if x]
    if case in ('s503','r307'):
        status=503 if case=='s503' else 307
        assert p['http_status']==status and str(status) in guest['task']['core_terminal']['Failed']
        assert p['intent']=='Observed' and p['http_eof'] and p['response_material_stored'] and p['error_code']==19
        assert p['error_consumed_bytes']==59 and p['parser_yielded_bytes']==0 and not events and len(response)==59
    else:
        assert guest['task']['core_terminal']=={'Completed':{'end_turn':None}}
        assert len(events)==4 and events[-1]['event']['kind']=='actual_core_completed'
        delta=next(x['event'] for x in events if x['event']['kind']=='actual_core_output_text_delta')
        assert delta['sha256']==hashlib.sha256(b'bounded matrix').hexdigest() and delta['bytes']==14
        gate=read(C/'pre-eof-host-snapshot.json'); gp=gate['snapshot']['progress']
        assert all(gp[k]==65536 for k in offsets) and gp['intent']=='Unknown' and gp['network']=='Streaming'
        assert not any(gp[k] for k in ('http_eof','response_material_stored','request_closed'))
        assert gp['parser_yielded_bytes']==1024 and gp['drain_discarded_bytes']==64512
        inspections=read(C/'pre-eof-inspections.json'); assert gate in inspections
        replies=[x for x in rows if x['value'].get('event')=='operator_result' and x['value'].get('action')=='inspect_http' and x['value'].get('result')==gate['snapshot']]
        assert replies and min(x['at_ns'] for x in replies)<=gate['at_ns']
        tail=next(x for x in server if x['event']=='tail_gate_opened_after_host_snapshot')
        assert next(x['at_ns'] for x in server if x['event']=='cap_prefix_sent')<gate['at_ns']<tail['at_ns']
        if case=='exact':
            end=next(x for x in server if x['event']=='http_eof_sent')
            assert tail['at_ns']<=end['at_ns'] and p['intent']=='Observed' and p['http_eof'] and p['response_material_stored']
            assert p['error_code']==0 and guest['task']['transport_drain']=={'Ok':None} and len(response)==65536
        else:
            end=next(x for x in server if x['event']=='over_cap_byte_sent')
            assert tail['at_ns']<=end['at_ns'] and end['bytes']==1 and len(response)==65537 and response[-1:]==b'x'
            assert p['intent']=='Unknown' and not p['http_eof'] and not p['response_material_stored'] and p['error_code']==21
            assert guest['task']['transport_drain']=={'Err':'Cancelled'}
            assert any(x['event']=='network_error' and x['detail']=='network: Limit' for x in s['events'])
            assert any(x['event']=='http_cancel_applied' and x['detail']['code']==21 for x in s['events'])
        assert p['parser_yielded_bytes']==1024 and p['drain_discarded_bytes']==64512
    db=C/'profile/native-admissions.sqlite'; before=sha(db)
    assert not db.with_name(db.name+'-wal').exists()
    con=sqlite3.connect(db.as_uri()+'?mode=ro&immutable=1',uri=True)
    try:
        assert con.execute('pragma user_version').fetchone()[0]==4
        owner=unpack(con.execute('select payload from owner').fetchone()[0])
        approvals=con.execute('select payload from approvals').fetchall(); https=con.execute('select payload from http_approvals').fetchall()
        assert len(approvals)==len(https)==1
        ap=unpack(approvals[0][0]); hp=unpack(https[0][0])
        assert owner[5]==b'Released' and owner[6]==s['pid'] and owner[7]==s['session'] and owner[8]==s['epoch']
        assert all(owner[k]==1 for k in (9,10,11)) and owner[2]==ap[2]
        assert ap[8].decode()==approval['operation'] and ap[9].hex()==locked['guest_sha256'] and ap[10].hex()==spawn['config_sha256']
        assert ap[14]==10000 and ap[16]==3 and ap[17]==2 and ap[18]==(25 if case=='exact' else 21 if case=='over' else 19)
        assert hp[3]==ap[2] and hp[6].hex()==q['request_sha256'] and hp[7].hex()==q['body_sha256']
        assert hp[10]==65536 and hp[11]==3 and hp[12]==1
    finally: con.close()
    assert sha(db)==before
    ledger.append(dict(case=case,db_sha256=before,immutable_read_only=True,version=4,owner_phase=owner[5].decode(),pid=owner[6],session=owner[7],epoch=owner[8],exit_and_dual_eof=True,parent_state=ap[16],revocation_source=ap[17],revocation_reason=ap[18],original_ttl_ms=ap[14],http_state=hp[11],send_budget=hp[12],response_limit=hp[10],request_sha256=hp[6].hex(),body_sha256=hp[7].hex(),database_unchanged=True))
    fact=dict(case=case,post_count=1,sink_count=0,core_terminal=guest['task']['core_terminal'],drain=guest['task']['transport_drain'],http_status=p['http_status'],intent=p['intent'],http_eof=p['http_eof'],material=p['response_material_stored'],error_code=p['error_code'],offsets={k:p[k] for k in offsets},close_ack=ack,host_ack_bytes=412,control_clean=True,host_exit=0,guest_exit=s['exit_code'],request_body_bytes=len(body),request_sha256=q['request_sha256'],raw_owner_released=True)
    if case in ('exact','over'): fact.update(gate_at_ns=gate['at_ns'],tail_effect_at_ns=end['at_ns'],gate_to_tail_ms=(end['at_ns']-gate['at_ns'])/1e6)
    facts.append(fact)

old=read(R.parent/'http-matrix-007-20260928T232647952048Z/result.json')
assert old['results'][0]['status']=='failed_expectations'
save('inputs.json', {'pins':pins,'batch_local_files':len(batch['files']),'batch_external_plugin_files':len(batch['plugin_evidence']),'case_manifests':manifest_counts})
save('durable-native-ledger.json', {'cases':ledger,'scope':'native approval/owner records only; no Core coordination payload decode or live authority restoration'})
save('result.json', {'status':'four_producer_cases_independently_reviewed_no_blocker','cases':facts,'producer_checks':124,'independent_runtime_runs':0,'old007_unchanged_failed':True,'terminal_poll_directly_measured':False,'os_control_threads_joined_proven':False,'product_gate_upgraded':False})
print(json.dumps({'cases':facts,'ledger':ledger,'hash_files':len(pins)},indent=2))
