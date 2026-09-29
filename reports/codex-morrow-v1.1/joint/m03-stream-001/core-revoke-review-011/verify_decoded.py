from pathlib import Path
import json
J=Path(__file__).resolve().parent
rows=json.loads((J/'decoded-control.json').read_text(encoding='utf-8'))
def one(kind,seq):
    matches=[r for r in rows if r['frame']['kind']==kind and int(r['frame']['sequence'])==seq]
    assert len(matches)==1
    return matches[0]
terminal=one('httpTerminal',0);closed=one('requestClosed',0);credit=one('httpCredit',6);ack=one('creditState',6);close=one('close',7);close_ack=one('state',7)
reference=terminal['frame']
for r in rows:
    f=r['frame']
    for k in ('major','revision','session','instanceEpoch','childPid','nonce','schemaSha256','artifactSha256','executionConfigSha256','requestBudget','capabilities','operationId','attempt'):assert f[k]==reference[k],k
assert reference['childPid']==12212 and int(reference['session'])==11601825647018375088
assert terminal['ordinal']<closed['ordinal']<credit['ordinal']<ack['ordinal']<close['ordinal']<close_ack['ordinal']
c=credit['frame']['payload']['credit'];p=ack['frame']['payload']['progress'];early=closed['frame']['payload']['progress']
assert c['windowBytes']==0 and int(c['consumedOffset'])==int(c['parserYieldedBytes'])==197
assert int(early['peerConsumedOffset'])==int(early['parserYieldedBytes'])==0 and early['requestClosed']
assert int(p['peerConsumedOffset'])==int(p['parserYieldedBytes'])==197 and p['requestClosed']
for k in ('drainDiscardedBytes','errorConsumedBytes','cancelDiscardedBytes'):assert int(c[k])==int(p[k])==0
assert ack['frame']['code']==close_ack['frame']['code']==0 and int(ack['frame']['revocationGeneration'])==int(close_ack['frame']['revocationGeneration'])==2
facts=[dict(ordinal=r['ordinal'],kind=r['frame']['kind'],sequence=int(r['frame']['sequence']),generation=int(r['frame']['revocationGeneration']),host_supervisor_at_us=r['host_supervisor_at_us'],progress=r['frame']['payload'].get('progress'),credit=r['frame']['payload'].get('credit')) for r in (terminal,closed,credit,ack,close,close_ack)]
with (J/'wire-settlement.json').open('x',encoding='utf-8') as f:json.dump(dict(status='raw_control_settlement_verified',same_admission=True,old_closed_peer=0,final_credit_peer=197,zero_window=True,ack_before_close=True,records=facts),f,indent=2);f.write('\n')
print(json.dumps(dict(status='verified',ordinals=[r['ordinal'] for r in (terminal,closed,credit,ack,close,close_ack)],early_peer=0,confirmed_peer=197)))
