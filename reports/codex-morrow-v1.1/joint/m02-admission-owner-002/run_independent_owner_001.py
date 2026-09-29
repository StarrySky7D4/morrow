"""Actual ready-kit review; consumes only after runtime-ready-001 verifies pins."""
import argparse,json,shutil,sqlite3,time,traceback
from pathlib import Path
from check_review import HERE,HOST,sha,read,write,verify
from observe_owner import Host,environment,short_command
from ledger_codec import unpack,pack_literal
from decode_frames import decode

PLUGIN=HOST.parents[2]/'morrow-codex'
class Suite:
    def __init__(self,run,ready):
        self.run=run;self.ready=ready;self.exe=Path(ready['host_executable']);self.hs=ready['host_sha256'];self.hosts=[];self.results=[]
        client=read(PLUGIN/'receipts/m02-native-session-001/candidate-001.json')
        self.client=Path(client['exe']);self.cs=client['exe_sha256']
        peer=read(HERE/'runs/peer-ready-001/result.json');self.peer=Path(peer['peer_executable']);self.ps=peer['peer_sha256']
        old=read(PLUGIN/'receipts/m02-native-session-adversary-002/candidate-002.json');self.oldpeer=Path(old['exe']);self.ops=old['exe_sha256']
    def setup(self,name):
        case=self.run/name;case.mkdir();env=environment(case);profile=case/'authority';profile.mkdir()
        init=short_command(case/'init',self.exe,self.hs,['init','--profile',profile,'--slot','joint-slot'],env)
        assert init['exit_code']==0
        return case,profile,env
    def host(self,case,profile,env,name,client=None,digest=None,args=None,ttl=10000,handshake=2500):
        client=client or self.client;digest=digest or self.cs;work=case/(name+'-work');work.mkdir()
        argv=['--profile',profile,'--client',client,'--sha256',digest,'--work-dir',work,'--plugin-id','joint-plugin','--role','native-client','--operation',case.name+'-'+name,'--ttl-ms',ttl,'--handshake-ms',handshake,'--frame-ms',500,'--close-ms',200,'--budget',64]
        for arg in (args or ['--queries','64','--interval-ms','200']):argv.extend(['--client-arg',arg])
        h=Host(case/name,self.exe,self.hs,client,digest,argv,env);self.hosts.append(h);h.wait(lambda e:e.get('event')=='proposal')
        return h
    def approve(self,h):
        r=h.command('approve');assert r['ok'],r
        return r['result']['grant_id']
    def claim(self,h,g,ok=True):
        r=h.command('claim',g);assert r['ok']==ok,r;return r
    def event(self,h,name,timeout=5):return h.wait(lambda e:e.get('event')=='host_observation' and e['observation']['event']==name,timeout)
    def phase(self,h,name,timeout=5):return h.wait(lambda e:e.get('event')=='host_observation' and e['observation']['event']=='phase' and e['observation']['detail']==name,timeout)
    def state_sent(self,h):return h.wait(lambda e:e.get('event')=='host_observation' and e['observation']['event']=='frame_sent' and decode(bytes.fromhex(e['observation']['detail']['raw_hex']))['kind']=='state')
    def done(self,h,exitcode):
        assert h.wait_exit()==exitcode
        finals=[x['event']for x in h.records if x['event'].get('event')=='final']
        if finals:
            s=finals[-1]['snapshot'];assert s['exit_observed'] and s['stdout_eof'] and s['stderr_eof'] and s['phase']=='Released'
        h.save();return finals[-1]if finals else None
    def case_result(self,name,facts):
        self.results.append({'case':name,'status':'executed_pending_semantic_review','facts':facts});write(self.run/name/'case.json',self.results[-1]);print('Completed independent case: '+name,flush=True)
    def authority_boundary(self):
        c,p,e=self.setup('authority-boundary');h=self.host(c,p,e,'host')
        assert not h.command('approve',extra={'approved':True})['ok']
        self.claim(h,'0'*64,False);g=self.approve(h)
        ins=h.command('inspect',g);assert ins['ok'] and ins['result']['owner'] is None and ins['result']['grant']['state']==1
        assert h.command('revoke',g)['ok'];self.claim(h,g,False)
        assert h.child is None;h.command('quit');self.done(h,2)
        self.case_result(c.name,{'grant_id':g,'guest_style_approved_rejected':True,'no_known_spawn':True,'revoked_before_claim':True})
    def first_deadline(self):
        c,p,e=self.setup('first-deadline');h=self.host(c,p,e,'host',ttl=250)
        g=self.approve(h);start=time.monotonic_ns();time.sleep(.5)
        r=self.claim(h,g,False);assert 'expired' in r['error'];self.claim(h,g,False)
        ins=h.command('inspect',g);assert ins['result']['grant']['state']==4 and ins['result']['owner'] is None
        h.command('quit');self.done(h,2)
        self.case_result(c.name,{'grant_id':g,'delay_ns':time.monotonic_ns()-start,'expired_no_spawn':True})
    def fixed_context(self):
        c,p,e=self.setup('fixed-context');h=self.host(c,p,e,'host');records=[]
        for field,label in [(6,'plugin_id'),(7,'role'),(8,'operation')]:
            g=self.approve(h)
            with sqlite3.connect(p/'native-admissions.sqlite',timeout=2) as db:
                before=db.execute('SELECT payload FROM approvals WHERE id=?',(g,)).fetchone()[0];f=unpack(before);config=f[10]
                f[field]=(2,('changed-'+label).encode());after=pack_literal(f);assert unpack(after)[10]==config
                db.execute('UPDATE approvals SET payload=? WHERE id=?',(after,g));db.commit()
            before_path=c/(label+'-before.record');after_path=c/(label+'-after.record');before_path.write_bytes(before);after_path.write_bytes(after)
            r=self.claim(h,g,False);assert 'binding' in r['error'];assert h.child is None
            records.append({'field':label,'grant_id':g,'before_sha256':sha(before_path),'after_sha256':sha(after_path),'config_unchanged':True,'claim_rejected':True,'mutation_scope':'test-created record only, not authority restoration'})
        h.command('quit');self.done(h,2);self.case_result(c.name,{'mutations':records,'no_known_spawn':True})
    def two_hosts(self):
        c,p,e=self.setup('two-hosts');a=self.host(c,p,e,'a');b=self.host(c,p,e,'b');ga=self.approve(a);gb=self.approve(b)
        ca=a.send('claim',ga);cb=b.send('claim',gb)
        ra=a.wait(lambda v:v.get('event')=='operator_result'and v.get('action')=='claim',start=ca)['event'];rb=b.wait(lambda v:v.get('event')=='operator_result'and v.get('action')=='claim',start=cb)['event']
        assert int(ra['ok'])+int(rb['ok'])==1
        winner,loser,wg,lg=(a,b,ga,gb)if ra['ok']else(b,a,gb,ga)
        self.event(winner,'state_read');self.claim(winner,wg,False);self.claim(loser,lg,False)
        assert winner.child and not loser.child
        winning_pid=winner.child.pid;winner.command('stop');first=self.done(winner,2)
        old=loser.command('inspect',wg);assert old['result']['grant']['state']==2 and old['result']['owner']['phase']=='Released'
        fresh=self.approve(loser);assert fresh not in [ga,gb];self.claim(loser,fresh);self.event(loser,'state_read');next_pid=loser.child.pid;loser.command('stop');self.done(loser,2)
        self.case_result(c.name,{'claim_results':[ra,rb],'winner_pid':winning_pid,'duplicate_claim_rejected':True,'pre_release_known_child_count':1,'fresh_grant':fresh,'post_release_new_pid':next_pid,'first_release_snapshot':first['snapshot']})
    def external_revoke(self):
        c,p,e=self.setup('external-revoke');a=self.host(c,p,e,'owner');b=self.host(c,p,e,'operator');g=self.approve(a);self.claim(a,g);self.event(a,'state_read')
        r=b.command('revoke',g);assert r['ok'] and r['result']['persisted'] and not r['result']['runtime_applied']
        a.wait(lambda v:v.get('event')=='authority_observation'and v['observation']['event']=='external_revocation_applied')
        final=self.done(a,0);assert final['snapshot']['exit_code']==19
        ins=b.command('inspect',g);assert ins['result']['grant']['state']==3 and ins['result']['owner']['phase']=='Released'
        b.command('quit');self.done(b,0);self.case_result(c.name,{'grant_id':g,'external_revoke_result':r,'persisted_and_runtime_ack_separate':True,'child_exit':19})
    def history(self):
        c,p,e=self.setup('historical-replay');facts=[];capture=c/'capture-material';capture.mkdir()
        for mode in ['capture','replay-hello','replay-query']:
            material=capture if mode=='capture' else c/(mode+'-material')
            if mode!='capture':material.mkdir()
            args=['--scenario',mode,'--evidence-dir',str(material)]
            if mode!='capture':args.extend(['--capture-dir',str(capture)])
            h=self.host(c,p,e,mode,self.peer,self.ps,args);g=self.approve(h);self.claim(h,g)
            final=self.done(h,0 if mode=='capture' else 2)
            assert h.child and read(material/'result.json')['process_exit_proven'] is False
            facts.append({'mode':mode,'grant_id':g,'host_pid':h.proc.pid,'child_pid':final['snapshot']['pid'],'material':str(material),'release_observed_before_next_host':True})
        self.case_result(c.name,{'sessions':facts,'raw_replay_comparison_pending':True})
    def unconfirmed(self):
        c,p,e=self.setup('unconfirmed');material=c/'holder-material';material.mkdir()
        a=self.host(c,p,e,'owner',self.peer,self.ps,['--scenario','hold-output','--evidence-dir',str(material)]);b=self.host(c,p,e,'contender');ga=self.approve(a);gb=self.approve(b);self.claim(a,ga)
        deadline=time.monotonic()+3
        while not (material/'holder.json').exists() and time.monotonic()<deadline:a.pump(.01)
        a.attach_holder(material/'holder.json');self.phase(a,'ClosingUnconfirmed');a.snapshot_os('unconfirmed')
        assert a.child.snapshot()['handle_signaled'] and not a.holder.snapshot()['handle_signaled']
        refused=self.claim(b,gb,False);ins=b.command('inspect',ga)
        assert ins['result']['owner']['phase']=='ClosingUnconfirmed' and ins['result']['owner']['owner_retained']
        holder=a.holder.pid;self.done(a,0);fresh=self.approve(b);self.claim(b,fresh);self.event(b,'state_read');b.command('stop');self.done(b,2)
        self.case_result(c.name,{'holder_pid':holder,'second_host_rejected':refused,'fresh_after_full_release':fresh})
    def crash(self):
        c,p,e=self.setup('crash-child-alive');a=self.host(c,p,e,'owner',self.oldpeer,self.ops,['--scenario','ignore-stop']);b=self.host(c,p,e,'contender');ga=self.approve(a);gb=self.approve(b);self.claim(a,ga);self.state_sent(a)
        a.crash_own_host();a.wait_exit(3);alive=a.child.snapshot();assert not alive['handle_signaled']
        r=self.claim(b,gb,False);assert 'unconfirmed' in r['error'];a.snapshot_os('after_crash_takeover_rejected')
        first=b.command('inspect',ga);assert first['result']['owner']['owner_retained']
        deadline=time.monotonic()+10
        while not a.child.snapshot()['handle_signaled'] and time.monotonic()<deadline:time.sleep(.02)
        assert a.child.snapshot()['handle_signaled'];a.snapshot_os('child_natural_exit_after_crash');a.save()
        again=self.claim(b,gb,False);assert 'unconfirmed' in again['error'];last=b.command('inspect',ga);assert last['result']['owner']['owner_retained']
        b.command('quit');self.done(b,2)
        self.case_result(c.name,{'crashed_host_pid':a.proc.pid,'old_child_pid':alive['pid'],'child_alive_after_host_exit':alive,'rejected_while_alive':r,'rejected_after_child_exit':again,'inspection_after_child_exit':last,'automatic_recovery_claimed':False,'forced_child_cleanup':False})
    def artifact_drift(self):
        c,p,e=self.setup('artifact-drift');copy=c/'client-copy.exe';shutil.copyfile(self.client,copy)
        h=self.host(c,p,e,'host',copy,self.cs);g=self.approve(h)
        with copy.open('ab')as f:f.write(b'joint-controlled-post-approval-drift')
        changed=sha(copy);assert changed!=self.cs
        r=self.claim(h,g,False);assert 'artifact changed' in r['error'] and not h.child
        ins=h.command('inspect',g);assert ins['result']['owner']['phase']=='LaunchPending'
        h.command('quit');self.done(h,2);self.case_result(c.name,{'grant_id':g,'original_sha256':self.cs,'changed_fixture_sha256':changed,'claim_error':r,'no_known_spawn':True,'delivered_client_unchanged':sha(self.client)==self.cs})
    def profile_copy(self):
        c,p,e=self.setup('profile-copy');clone=c/'copied-authority';shutil.copytree(p,clone)
        r=short_command(c/'inspect-copy',self.exe,self.hs,['inspect','--profile',clone],e);assert r['exit_code']==2
        self.case_result(c.name,{'copied_profile_rejected':True,'no_child_requested':True})

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--run-id',required=True);args=ap.parse_args()
    run=(HERE/'runs'/args.run_id).resolve()
    if not run.is_relative_to(HERE/'runs') or not (run/'acl-receipt.json').is_file():raise ValueError('fresh private run required')
    ready=read(HERE/'runs/runtime-ready-001/result.json')
    if ready['status']!='verified_read_only_ready_host':raise ValueError('host not ready reviewed')
    pins=read(HERE/'baseline.json')['files']+[{'path':x['path'],'sha256':x['expected']}for x in read(HERE/'runs/peer-ready-001/identities.json')]+[{'path':x['path'],'sha256':x['expected']}for x in read(ready['identities'])]
    merged={}
    for x in pins:
        if x['path']in merged and merged[x['path']]!=x['sha256']:raise ValueError('conflicting pins')
        merged[x['path']]=x['sha256']
    pins=[{'path':p,'sha256':s}for p,s in merged.items()];before=verify(pins)
    if not all(x['match']for x in before):raise ValueError('pre-run input drift')
    write(run/'identities-before.json',before)
    write(run/'reviewer-inputs.json',{n:sha(HERE/n)for n in ['run_independent_owner_001.py','observe_owner.py','known_process.py','ledger_codec.py','decode_frames.py','prepare_private_run.ps1','check_review.py']})
    suite=Suite(run,ready);failure=None
    try:
        for name in ['authority_boundary','first_deadline','fixed_context','two_hosts','external_revoke','history','unconfirmed','crash','artifact_drift','profile_copy']:
            print('Starting independent owner case: '+name,flush=True);getattr(suite,name)()
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
    return 0 if failure is None and result['inputs_unchanged'] else 1
if __name__=='__main__':raise SystemExit(main())
