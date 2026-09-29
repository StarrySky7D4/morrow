"""Continue only unfinished cases. OS process exit and pipe EOF are separate."""
import argparse,json,time,traceback
from pathlib import Path
from check_review import HERE,read,write,sha,verify
from run_independent_owner_001 import Suite

def crash(suite):
    c,p,e=suite.setup('crash-child-alive');a=suite.host(c,p,e,'owner',suite.oldpeer,suite.ops,['--scenario','ignore-stop']);b=suite.host(c,p,e,'contender')
    ga=suite.approve(a);gb=suite.approve(b);suite.claim(a,ga);suite.state_sent(a)
    a.crash_own_host()
    deadline=time.monotonic()+2
    while time.monotonic()<deadline and a.proc.poll() is None:a.pump(.01)
    assert a.proc.poll() is not None and a.host_os.snapshot()['handle_signaled']
    a.snapshot_os('host_OS_exit_before_pipe_EOF')
    alive=a.child.snapshot();assert not alive['handle_signaled']
    drains_at_exit={k:dict(v)for k,v in a.drains.items()}
    r=suite.claim(b,gb,False);assert 'unconfirmed' in r['error'];a.snapshot_os('after_crash_takeover_rejected')
    assert not a.child.snapshot()['handle_signaled']
    first=b.command('inspect',ga);assert first['result']['owner']['owner_retained']
    deadline=time.monotonic()+10
    while not a.child.snapshot()['handle_signaled'] and time.monotonic()<deadline:a.pump(.02)
    assert a.child.snapshot()['handle_signaled'];a.snapshot_os('child_natural_exit_after_crash')
    a.wait_exit(2);a.save()
    again=suite.claim(b,gb,False);assert 'unconfirmed' in again['error'];last=b.command('inspect',ga);assert last['result']['owner']['owner_retained']
    b.command('quit');suite.done(b,2)
    suite.case_result(c.name,{'crashed_host_pid':a.proc.pid,'old_child_pid':alive['pid'],'child_alive_after_host_OS_exit':alive,'diagnostic_drains_at_host_OS_exit':drains_at_exit,'rejected_while_alive':r,'rejected_after_child_exit':again,'inspection_after_child_exit':last,'automatic_recovery_claimed':False,'forced_child_cleanup':False})

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--run-id',required=True);args=ap.parse_args();run=(HERE/'runs'/args.run_id).resolve()
    if not run.is_relative_to(HERE/'runs') or not (run/'acl-receipt.json').is_file():raise ValueError('fresh private run required')
    old=HERE/'runs/independent-001';prior=read(old/'result.json')
    if len(prior['cases'])!=7 or prior['failure']['type']!='TimeoutError':raise ValueError('unexpected prior state')
    pins=[{'path':x['path'],'sha256':x['expected']}for x in read(old/'identities-before.json')];before=verify(pins)
    if not all(x['match']for x in before):raise ValueError('pre-run identity drift')
    write(run/'identities-before.json',before)
    names=['run_remaining_owner_002.py','run_independent_owner_001.py','observe_owner.py','known_process.py','ledger_codec.py','decode_frames.py','prepare_private_run.ps1','check_review.py']
    write(run/'reviewer-inputs.json',{n:sha(HERE/n)for n in names})
    suite=Suite(run,read(HERE/'runs/runtime-ready-001/result.json'));failure=None
    try:
        print('Independent continuation: host OS exit before EOF, then refusal while child alive',flush=True);crash(suite)
        suite.artifact_drift();suite.profile_copy()
    except Exception as exc:failure={'type':type(exc).__name__,'message':str(exc),'traceback':traceback.format_exc()}
    finally:
        for h in suite.hosts:
            if not h.finished:
                try:h.cleanup()
                except Exception as exc:
                    if failure is None:failure={'type':'cleanup','message':str(exc)}
        after=verify(pins);write(run/'identities-after.json',after)
        result={'status':'executed_pending_semantic_review'if failure is None else 'execution_review_failed','prior_run':str(old),'prior_result_sha256':sha(old/'result.json'),'prior_failure_preserved':True,'cases':suite.results,'failure':failure,'input_count':len(pins),'inputs_unchanged':before==after and all(x['match']for x in after),'host_processes_observed':len(suite.hosts),'product_pass_credit':0,'M02_complete':False,'G0_passed':False}
        write(run/'result.json',result);print(json.dumps({k:v for k,v in result.items()if k!='cases'},indent=2))
    return 0 if failure is None and result['inputs_unchanged']else 1
if __name__=='__main__':raise SystemExit(main())
