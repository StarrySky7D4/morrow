"""Verify raw control envelopes and settlement ordering offline."""
from pathlib import Path
import hashlib, json, struct, subprocess

W=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
J=Path(__file__).resolve().parent
C=W/'reports/codex-morrow-v1.1/host/m03-lifecycle-004/candidate-002'
schema=W/'contracts/experimental/agent_host_v3_http_stream/native_http.capnp'
assert hashlib.sha256(schema.read_bytes()).hexdigest()=='8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864'
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def save(name,value):
    with (J/name).open('x',encoding='utf-8') as f:
        json.dump(value,f,indent=2);f.write('\n')
facts={}
for mode,stamp,peer in [('core-revoke','20260929T213241184115Z',197),('data-pending','20260929T213307996034Z',0)]:
    R=C/'runs'/('revoke-011-'+mode+'-'+stamp)
    r=read(R/'result.json');s=r['final']['snapshot'];g=r['guest_result'];events=s['events']
    decoded=[]
    for e in events:
        if e['event'] not in ('control_frame_sent','control_frame_received'):continue
        raw=bytes.fromhex(e['detail']['raw_hex'])
        assert struct.unpack('<I',raw[:4])[0]==len(raw)-4
        decoded.append({'ordinal':e['ordinal'],'host_supervisor_at_us':e['at_us'],'direction':e['event'],
            'frame':json.loads(subprocess.run([r'C:\Users\Administrator\capnp-bin\capnp.exe','convert','binary:json',str(schema),'Frame'],
                input=raw[4:],capture_output=True,check=True).stdout)})
    def one(kind,seq):
        matches=[x for x in decoded if x['frame']['kind']==kind and int(x['frame']['sequence'])==seq]
        assert len(matches)==1,(kind,seq)
        return matches[0]
    terminal=one('httpTerminal',0);closed=one('requestClosed',0)
    credit=one('httpCredit',6);ack=one('creditState',6);close=one('close',7);close_ack=one('state',7)
    ref=terminal['frame']
    for x in decoded:
        f=x['frame']
        for k in ('major','revision','session','instanceEpoch','childPid','nonce','schemaSha256','artifactSha256',
                  'executionConfigSha256','requestBudget','capabilities','operationId','attempt'):
            assert f[k]==ref[k],k
    assert int(ref['session'])==s['session'] and ref['childPid']==s['pid'] and int(ref['instanceEpoch'])==s['epoch']
    assert int(ref['attempt'])==1
    assert bytes(ref['schemaSha256']).hex()==g['identity']['schema_sha256']
    assert bytes(ref['artifactSha256']).hex()==g['identity']['artifact_sha256']
    assert bytes(ref['executionConfigSha256']).hex()==g['identity']['host_execution_config_sha256']
    assert hashlib.sha256(bytes(ref['operationId'])).hexdigest()==g['identity']['operation_id_sha256']
    assert terminal['ordinal']<credit['ordinal']<ack['ordinal']<close['ordinal']<close_ack['ordinal']
    assert terminal['ordinal']<closed['ordinal']<close['ordinal']
    cp=credit['frame']['payload']['credit'];ap=ack['frame']['payload']['progress'];old=closed['frame']['payload']['progress']
    assert cp['windowBytes']==0 and int(cp['consumedOffset'])==int(cp['parserYieldedBytes'])==peer
    assert int(ap['peerConsumedOffset'])==int(ap['parserYieldedBytes'])==peer
    assert old['requestClosed'] and int(old['peerConsumedOffset'])<=peer
    for k in ('drainDiscardedBytes','errorConsumedBytes','cancelDiscardedBytes'):
        assert int(cp[k])==int(ap[k])==0
    assert ack['frame']['code']==close_ack['frame']['code']==0
    assert int(terminal['frame']['revocationGeneration'])==int(ack['frame']['revocationGeneration'])==int(close_ack['frame']['revocationGeneration'])==2
    assert terminal['frame']['payload']['progress']['errorCode']==19
    written=next(e for e in events if e['event']=='close_ack_written')
    assert written['detail']['sequence']==7 and written['ordinal']>close['ordinal']
    assert all(next(e for e in events if e['event']==k)['ordinal']>written['ordinal'] for k in ('stdout_eof','stderr_eof','exit'))
    q=read(R/'independent-approval.json')['proposal']
    assert q['absolute_target'].startswith('http://127.0.0.1:') and q['absolute_target'].endswith('/v1/responses')
    assert all(h['name'].lower()!='authorization' for h in q['headers'])
    facts[mode]={'decoded_frame_count':len(decoded),'same_admission_tuple':True,'zero_credit_peer_confirmed':peer,
       'request_closed_peer':int(old['peerConsumedOffset']),'credit_ack_before_close':True,
       'request_closed_before_close':True,'close_ack_before_exit_and_dual_eof':True,
       'close_ack_written':written,'selected_control':[x for x in decoded if x in (terminal,closed,credit,ack,close,close_ack)]}
    if mode=='data-pending':
        frame=[e for e in events if e['event']=='pipe_owner_observation' and e['detail']['stage']=='write_frame_state'
               and (e['detail'].get('operation') or {}).get('id')==12]
        assert len(frame)==1 and frame[0]['detail']['outcome']=={'cancelling':True,'confirmed_frame_prefix':0,'result':'Ok(Cancelled)'}
        facts[mode]['actual_frame_state']=frame[0]
    save(mode+'/all-decoded-control.json',decoded)
save('wire-settlement.json',facts)
print(json.dumps({'status':'wire_settlement_verified','cases':{k:{'frames':v['decoded_frame_count'],
                 'request_closed_peer':v['request_closed_peer'],'confirmed_peer':v['zero_credit_peer_confirmed']} for k,v in facts.items()}}))
