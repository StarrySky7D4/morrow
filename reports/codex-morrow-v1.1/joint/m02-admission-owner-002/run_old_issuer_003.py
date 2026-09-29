"""Additional required old-grant check, separate from historical wire replay."""
import json,traceback
from check_review import HERE,read,write,sha,verify
from run_independent_owner_001 import Suite
def main():
    run=HERE/'runs/independent-003'
    if not (run/'acl-receipt.json').is_file():raise ValueError('private run required')
    pins=[{'path':x['path'],'sha256':x['expected']}for x in read(HERE/'runs/independent-001/identities-before.json')];before=verify(pins)
    if not all(x['match']for x in before):raise ValueError('input drift')
    write(run/'identities-before.json',before)
    write(run/'reviewer-inputs.json',{n:sha(HERE/n)for n in ['run_old_issuer_003.py','run_independent_owner_001.py','observe_owner.py','known_process.py','ledger_codec.py','decode_frames.py','prepare_private_run.ps1','check_review.py']})
    suite=Suite(run,read(HERE/'runs/runtime-ready-001/result.json'));failure=None
    try:
        c,p,e=suite.setup('old-issuer');a=suite.host(c,p,e,'original');g=suite.approve(a)
        before_quit=a.command('inspect',g);assert before_quit['result']['grant']['state']==1 and before_quit['result']['grant']['live_in_this_issuer']
        a.command('quit');suite.done(a,0)
        b=suite.host(c,p,e,'replacement');loaded=b.command('inspect',g);assert loaded['result']['grant']['state']==1 and not loaded['result']['grant']['live_in_this_issuer']
        rejected=suite.claim(b,g,False);assert 'no live approval' in rejected['error'] and not b.child
        fresh=suite.approve(b);suite.claim(b,fresh);suite.event(b,'state_read');b.command('stop');suite.done(b,2)
        suite.case_result(c.name,{'old_grant':g,'fresh_grant':fresh,'original_issuer_exit_before_replacement':True,'persisted_approved_did_not_restore_live_authority':True,'rejected':rejected,'fresh_approval_launched_once':True})
    except Exception as exc:failure={'type':type(exc).__name__,'message':str(exc),'traceback':traceback.format_exc()}
    finally:
        for h in suite.hosts:
            if not h.finished:
                try:h.cleanup()
                except Exception as exc:
                    if failure is None:failure={'type':'cleanup','message':str(exc)}
        after=verify(pins);write(run/'identities-after.json',after)
        result={'status':'executed_pending_semantic_review'if failure is None else 'execution_review_failed','cases':suite.results,'failure':failure,'input_count':len(pins),'inputs_unchanged':before==after and all(x['match']for x in after),'host_processes_observed':len(suite.hosts),'product_pass_credit':0,'M02_complete':False,'G0_passed':False}
        write(run/'result.json',result);print(json.dumps({k:v for k,v in result.items()if k!='cases'},indent=2))
    return 0 if failure is None and result['inputs_unchanged']else 1
if __name__=='__main__':raise SystemExit(main())
