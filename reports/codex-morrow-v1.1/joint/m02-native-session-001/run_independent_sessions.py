"""Selected independent real-process cases against fixed reviewed artifacts."""
import json,sys
from pathlib import Path
sys.dont_write_bytecode=True
from check_review import HERE,read,write,sha,verify_pins
from observe_session import observe

def main():
    batch=HERE/'runs/independent-sessions-001';batch.mkdir(exist_ok=False)
    review=read(HERE/'runs/runtime-ready-review-001/result.json')
    if review['status']!='verified_read_only':raise ValueError('source review not ready')
    entries=read(HERE/'runs/runtime-ready-review-001/identities.json')
    pins=[{'path':e['path'],'sha256':e['expected']} for e in entries]
    pins+=read(HERE/'baseline.json')['files']
    # Runtime and old inputs overlap; verify one consistent identity per path.
    combined={}
    for p in pins:
        if p['path'] in combined and combined[p['path']]!=p['sha256']:raise ValueError('conflicting source identities')
        combined[p['path']]=p['sha256']
    pins=[{'path':p,'sha256':s} for p,s in combined.items()]
    before,issues=verify_pins(pins)
    if issues:raise ValueError(issues)
    write(batch/'identities-before.json',before)
    write(batch/'reviewer-inputs.json',{n:sha(HERE/n) for n in ['run_independent_sessions.py','observe_session.py','process_witness.py','watch_owned_processes.py','runtime_decode.py','review_capnp_kit.py']})
    host=Path(review['host_executable']);hs=review['host_sha256'];client=Path(review['client_executable']);cs=review['client_sha256']
    peer=Path(review['peer_executable']);ps=review['peer_sha256']
    specs=[
      ('normal',client,cs,['--queries','2','--interval-ms','200'],{},0,0),
      ('revoke',client,cs,['--queries','16','--interval-ms','200'],{'control':'revoke'},0,19),
      ('stop',client,cs,['--queries','16','--interval-ms','200'],{'control':'stop'},0,0),
      ('expiry',client,cs,['--queries','64','--interval-ms','100'],{'ttl_ms':700},2,20),
      ('wrong-artifact',peer,ps,['--scenario','wrong-artifact'],{},2,0),
      ('modified-epoch',peer,ps,['--scenario','modified-epoch'],{},2,0),
      ('no-hello',peer,ps,['--scenario','no-hello'],{'handshake_ms':250},2,None),
      ('partial-frame',peer,ps,['--scenario','partial-frame'],{'frame_ms':150},2,None),
      ('ignore-stop',peer,ps,['--scenario','ignore-stop'],{'control':'stop'},0,None),
      ('hold-output',peer,ps,['--scenario','hold-output'],{},0,0),
    ]
    summaries=[]
    for name,exe,digest,args,opts,host_exit,child_exit in specs:
        print('Independent real-process case: '+name,flush=True)
        r=observe(batch/name,host,hs,exe,digest,args,**opts)
        facts={'case':name,'host_exit':r['host_exit_code'],'child_exit':(r['final_host_snapshot'] or {}).get('exit_code'),
               'expected_host_exit':host_exit,'expected_child_exit':child_exit,'observations':str(batch/name/'result.json'),
               'observation_sha256':sha(batch/name/'result.json'),'observer_failures':r['failures']}
        facts['exit_expectations_match']=facts['host_exit']==host_exit and (child_exit is None or facts['child_exit']==child_exit)
        summaries.append(facts)
        write(batch/'progress.json',summaries)
        if any('deadline_exceeded' in s or 'forced_host' in s or 'observer_error' in s or 'child_exit_independently_unconfirmed' in s for s in r['failures']):
            break
    after,issues=verify_pins(pins);write(batch/'identities-after.json',after)
    result={'status':'observed_pending_semantic_review','cases':summaries,'input_count':len(pins),
      'inputs_unchanged':not issues and before==after,'input_issues':issues,'independent_execution':True,
      'independent_host_or_client_build':False,'product_pass_credit':0,'M02_complete':False,'G0_passed':False}
    write(batch/'result.json',result);print(json.dumps(result,ensure_ascii=False,indent=2))
    return 0 if len(summaries)==len(specs) and not issues and all(c['exit_expectations_match'] for c in summaries) else 1

if __name__=='__main__':raise SystemExit(main())
