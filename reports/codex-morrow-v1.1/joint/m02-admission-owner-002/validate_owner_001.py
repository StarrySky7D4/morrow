"""Independent decoding, identity reconstruction and bounded semantic checks."""
import hashlib,json,os,sqlite3
from pathlib import Path
from check_review import HERE,HOST,sha,read,write,verify
from decode_frames import decode
from ledger_codec import unpack,encode_fields
PLUGIN=HOST.parents[2]/'morrow-codex'
def main():
    out=HERE/'runs/semantic-001'
    if not (out/'acl-receipt.json').is_file():raise ValueError('private semantic output required')
    checks=[];hosts={};cases={};decoded=[];bindings=[]
    def ck(name,ok):checks.append({'check':name,'passed':bool(ok)})
    def value(fields,n,default=None):return fields[n][1]if n in fields else default
    hand=PLUGIN/'receipts/m02-admission-owner-002/handoff.json'
    ck('late_unified_handoff_identity',sha(hand)=='948948b388ee0ce1c05e3e0742851cfc19f7aba917117a09e014abccfc55265b')
    unified=read(hand);hp={str(PLUGIN/p):s for p,s in unified['input_sha256'].items()};hp.update(unified['external_input_sha256']);hp[str(hand)]=sha(hand)
    upins=[{'path':p,'sha256':s}for p,s in hp.items()];ubefore=verify(upins);write(out/'unified-identities-before.json',ubefore);ck('late_handoff_current_inputs',all(x['match']for x in ubefore))
    for runname in ['independent-001','independent-002','independent-003']:
        run=HERE/'runs'/runname;s=read(run/'result.json');before=read(run/'identities-before.json');after=read(run/'identities-after.json')
        ck(runname+':2233_inputs_unchanged',len(before)==2233 and before==after and all(x['match']for x in before))
        for n,digest in read(run/'reviewer-inputs.json').items():ck(runname+':frozen_reviewer:'+n,sha(HERE/n)==digest)
        if runname=='independent-001':ck('first_failed_crash_attempt_preserved',len(s['cases'])==7 and s['failure']['type']=='TimeoutError')
        else:ck(runname+':completed_without_failure',s['failure'] is None)
        for case in s['cases']:
            name=case['case'];folder=run/name;cases[name]={'folder':str(folder),'case':case}
            ck(name+':case_receipt_matches',read(folder/'case.json')==case)
            for rp in folder.glob('*/result.json'):
                r=read(rp)
                if 'host_pid' not in r:continue
                key=name+'/'+rp.parent.name;events=read(rp.parent/'events.json');rows=[x['event']for x in events];raw=[json.loads(x)for x in (rp.parent/'host.stdout').read_bytes().splitlines()]
                def hc(label,ok):ck(key+':'+label,ok)
                hc('raw_stdout_matches_observer_records',raw==rows)
                hc('observer_no_errors_or_drops',not r['failures'] and all(x['eof']and not x['dropped_bytes']and not x['queue_drops']for x in r['drains'].values()))
                hc('controller_input_open_at_exit',r['controller_stdin_open_at_host_exit'])
                hc('no_owner_error',not any(x.get('event')=='owner_error'for x in rows))
                for role in ['host','child','holder']:
                    samples=[x['snapshot']for x in r['OS_observations']if x['role']==role]
                    if not samples:continue
                    wanted=r['host_sha256']if role=='host'else r['client_sha256']
                    hc(role+'_held_handle_identity',not samples[0]['handle_signaled']and samples[-1]['handle_signaled']and all(x['pid']==samples[0]['pid']and x['creation_filetime_100ns']==samples[0]['creation_filetime_100ns']and x['image_file_sha256']==wanted for x in samples))
                    if role=='host':hc('host_exit_matches_OS',samples[-1]['exit_code']==r['host_exit_code'])
                    else:hc(role+'_created_after_host',samples[0]['creation_filetime_100ns']>=r['OS_observations'][0]['snapshot']['creation_filetime_100ns'])
                native=[x['observation']for x in rows if x.get('event')=='host_observation'];authority=[x['observation']for x in rows if x.get('event')=='authority_observation']
                hc('native_time_order',all(a['at_us']<=b['at_us']for a,b in zip(native,native[1:])))
                hc('authority_time_order_separate_origin',all(a['at_us']<=b['at_us']for a,b in zip(authority,authority[1:])))
                finals=[x for x in rows if x.get('event')=='final'];frames=[]
                for n in native:
                    if n['event']in ['frame_sent','frame_received']:
                        b=bytes.fromhex(n['detail']['raw_hex']);d=decode(b);frames.append({'direction':n['event'],'at_us':n['at_us'],'raw_hex':b.hex(),'sha256':hashlib.sha256(b).hexdigest(),'decoded':d})
                if r['known_child_count']and not r['reviewer_crash_injected']:
                    hc('final_has_complete_native_evidence',len(finals)==1 and finals[0]['snapshot']['events']==native)
                    snap=finals[0]['snapshot'];owner=finals[0]['state']['owner']
                    hc('exit_and_double_EOF_before_persistent_release',snap['phase']=='Released'and snap['exit_observed']and snap['stdout_eof']and snap['stderr_eof']and not snap['owner_retained']and owner['phase']=='Released'and not owner['owner_retained'])
                    times={label:next(x['at_us']for x in native if x['event']==label)for label in ['exit','stdout_eof','stderr_eof']};release=next(x['at_us']for x in native if x['event']=='phase'and x['detail']=='Released')
                    hc('native_release_after_three_facts',release>=max(times.values()))
                    child=[x['snapshot']for x in r['OS_observations']if x['role']=='child'][-1];hc('child_exit_matches_OS',child['exit_code']==snap['exit_code'])
                if not r['known_child_count']:hc('no_child_frames_or_spawn',not frames and not any(x['event']=='spawn'for x in native))
                if frames:
                    initial=frames[0]['decoded'];child=[x['snapshot']for x in r['OS_observations']if x['role']=='child'][0]
                    hc('challenge_actual_PID_artifact_capability',initial['kind']=='challenge'and initial['sequence']==0 and initial['childPid']==child['pid']and initial['artifactSha256']==r['client_sha256']and initial['capabilities']==1 and initial['revocationGeneration']==1)
                    ttl=initial['remainingMs'];budget=initial['requestBudget'];gen=1;seq=0
                    immutable=['major','revision','session','instanceEpoch','childPid','nonce','schemaSha256','artifactSha256','executionConfigSha256','capabilities','reserved']
                    for number,f in enumerate(frames):
                        d=f['decoded'];historical=name=='historical-replay'and f['direction']=='frame_received'and ((rp.parent.name=='replay-hello'and d['kind']=='hello')or(rp.parent.name=='replay-query'and d['kind']=='query'))
                        if not historical:hc(f'frame{number}_immutable_binding',all(d[k]==initial[k]for k in immutable))
                        if f['direction']=='frame_received':
                            seq+=1;hc(f'frame{number}_request_sequence_and_code',d['sequence']==seq and d['code']==0)
                            if not historical:hc(f'frame{number}_initial_request_limits',all(d[k]==initial[k]for k in ['remainingMs','requestBudget','revocationGeneration']))
                        else:
                            hc(f'frame{number}_reply_sequence',d['sequence']==(0 if d['kind']in ['challenge','stop']else seq))
                            hc(f'frame{number}_nonrenewing_reply',d['remainingMs']<=ttl and d['requestBudget']<=budget and gen<=d['revocationGeneration']<=2)
                            ttl,budget,gen=d['remainingMs'],d['requestBudget'],d['revocationGeneration']
                # Reconstruct each actual approval config from original CLI+Protobuf.
                opts={};clientargs=[]
                for k,v in zip(r['argv'][2::2],r['argv'][3::2]):
                    if k=='--client-arg':clientargs.append(v)
                    else:opts[k]=v
                dbpath=Path(opts['--profile'])/'native-admissions.sqlite'
                with sqlite3.connect('file:'+dbpath.as_posix()+'?mode=ro',uri=True)as db:
                    profile=unpack(db.execute('SELECT payload FROM profile WHERE singleton=1').fetchone()[0])
                    approvals={i:(seq,blob)for seq,i,blob in db.execute('SELECT seq,id,payload FROM approvals')}
                for answer in [x for x in rows if x.get('event')=='operator_result'and x.get('action')=='approve'and x.get('ok')]:
                    gid=answer['result']['grant_id'];sequence,blob=approvals[gid];g=unpack(blob)
                    if name=='fixed-context':
                        mutation=next(x for x in case['facts']['mutations']if x['grant_id']==gid)
                        original=folder/(mutation['field']+'-before.record');changed=folder/(mutation['field']+'-after.record')
                        hc('controlled_mutation:'+mutation['field'],sha(original)==mutation['before_sha256']and sha(changed)==mutation['after_sha256']and changed.read_bytes()==blob)
                        g=unpack(original.read_bytes())
                    hc('grant_original_binding:'+str(sequence),value(g,2).decode()==gid and value(g,12)==sequence and value(g,4)==value(profile,2)and value(g,5)==value(profile,3)and value(g,6).decode()==opts['--plugin-id']and value(g,7).decode()==opts['--role']and value(g,8).decode()==opts['--operation']and value(g,9).hex()==r['client_sha256']and value(g,13)==1)
                    spec={'slot':value(profile,3).decode(),'executable':opts['--client'],'artifact_sha256':list(bytes.fromhex(r['client_sha256'])),'cwd':opts['--work-dir'],'args':clientargs,'ttl_ms':int(opts['--ttl-ms']),'handshake_ms':int(opts['--handshake-ms']),'frame_ms':int(opts['--frame-ms']),'close_ms':int(opts['--close-ms']),'request_budget':int(opts['--budget'])}
                    base={'launch':spec,'platform':'windows','arch':'x86_64','wire':value(g,11).hex(),'environment':{k:os.environ[k]for k in ['SystemRoot','WINDIR','COMSPEC']if k in os.environ}}
                    base_digest=hashlib.sha256(json.dumps(base,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).digest();context=dict(g);context.pop(10);context[16]=(0,1)
                    digest=hashlib.sha256(b'Morrow/native-admission/v1\0'+base_digest+encode_fields(context)).digest()
                    hc('independent_config_reconstruction:'+str(sequence),digest==value(g,10))
                    accepted=[x for x in r['commands']if x['action']=='claim'and x['request'].get('grant_id')==gid]
                    if frames and frames[0]['decoded']['instanceEpoch']==sequence:hc('wire_config_and_generation_from_approval',digest.hex()==frames[0]['decoded']['executionConfigSha256'])
                    bindings.append({'host':key,'grant_id':gid,'generation':sequence,'config_sha256':digest.hex(),'matched':digest==value(g,10)})
                hosts[key]={'folder':str(rp.parent),'result':r,'events':events,'frames':frames,'native':native,'authority':authority}
                decoded.append({'host':key,'frames':frames})
    # Concrete new-scenario semantics, using each clock domain separately.
    two=cases['two-hosts']['case']['facts'];a=hosts['two-hosts/a'];b=hosts['two-hosts/b']
    claim_sends=[next(x['sent_at_ns']for x in h['result']['commands']if x['action']=='claim')for h in [a,b]]
    first_results=[next(x for x in h['events']if x['event'].get('event')=='operator_result'and x['event'].get('action')=='claim')for h in [a,b]]
    ck('two_hosts_claims_both_outstanding',max(claim_sends)<min(x['observed_at_ns']for x in first_results))
    ck('two_hosts_one_initial_claim_success',sum(x['event']['ok']for x in first_results)==1)
    winner=next(h for h in [a,b]if h['result']['host_pid']==next(x['snapshot']['pid']for x in (a if first_results[0]['event']['ok']else b)['result']['OS_observations']if x['role']=='host'))
    loser=b if winner is a else a
    fresh=two['fresh_grant'];fresh_send=next(x['sent_at_ns']for x in loser['result']['commands']if x['action']=='claim'and x['request'].get('grant_id')==fresh)
    ck('fresh_launch_after_previous_host_exit_release',fresh_send>winner['result']['finished_monotonic_ns'])
    ext=hosts['external-revoke/owner'];ack=next(x['at_us']for x in ext['native']if x['event']=='control_ack'and x['detail']['action']=='revoke')
    ck('external_revoke_runtime_ack_and_no_later_success',any(x['event']=='external_revocation_applied'for x in ext['authority'])and any(f['decoded']['kind']=='denied'and f['decoded']['code']==19 and f['at_us']>ack for f in ext['frames'])and not any(f['decoded']['kind']=='state'and f['at_us']>ack for f in ext['frames']))
    ck('Revoked_persisted_before_final_release',any(x['event']=='owner_phase'and x['detail']['phase']=='Revoked'for x in ext['authority']))
    capture=hosts['historical-replay/capture'];material=Path(cases['historical-replay']['folder'])/'capture-material'
    names=['01-challenge.frame','02-hello.frame','03-welcome.frame','04-query.frame','05-state.frame','06-close.frame','07-stop.frame']
    ck('real_capture_seven_peer_frames_equal_host_capture',len(capture['frames'])==7 and all((material/name).read_bytes().hex()==f['raw_hex']for name,f in zip(names,capture['frames'])))
    ck('capture_was_actually_accepted',any(f['decoded']['kind']=='welcome'for f in capture['frames'])and any(f['decoded']['kind']=='state'for f in capture['frames']))
    history_diff=[];previous=capture
    for mode,kind,filename in [('replay-hello','hello','02-hello.frame'),('replay-query','query','04-query.frame')]:
        h=hosts['historical-replay/'+mode];f=next(f for f in h['frames']if f['direction']=='frame_received'and f['decoded']['kind']==kind)
        ck(mode+':real_prior_bytes_exact',bytes.fromhex(f['raw_hex'])==(material/filename).read_bytes())
        ck(mode+':rejection17_no_state',any(x['event']=='request_denied'and x['detail']['code']==17 for x in h['native'])and not any(x['decoded']['kind']=='state'for x in h['frames']))
        ck(mode+':previous_host_and_child_fully_closed',h['result']['started_monotonic_ns']>previous['result']['finished_monotonic_ns'])
        if mode=='replay-query':ck('old_query_after_real_new_welcome',any(x['decoded']['kind']=='welcome'for x in h['frames'])and h['frames'][1]['raw_hex']!=capture['frames'][1]['raw_hex'])
        initial=h['frames'][0]['decoded'];history_diff.append({'mode':mode,'different_fields':[k for k,v in f['decoded'].items()if k not in ['kind','sequence','code']and v!=initial[k]]});previous=h
    hold=hosts['unconfirmed/owner'];n=hold['native'];root_exit=next(x['at_us']for x in n if x['event']=='exit');unconfirmed=next(x['at_us']for x in n if x['event']=='phase'and x['detail']=='ClosingUnconfirmed');eofs=[x['at_us']for x in n if x['event']in ['stdout_eof','stderr_eof']]
    os_unconfirmed={x['role']:x['snapshot']for x in hold['result']['OS_observations']if x['stage']=='unconfirmed'}
    ck('unconfirmed_real_root_exit_live_holder_then_EOF',root_exit<unconfirmed<min(eofs)and os_unconfirmed['child']['handle_signaled']and not os_unconfirmed['holder']['handle_signaled'])
    crash=hosts['crash-child-alive/owner'];after={x['role']:x['snapshot']for x in crash['result']['OS_observations']if x['stage']=='host_OS_exit_before_pipe_EOF'}
    ck('crash_host_OS_exited_while_child_alive',after['host']['handle_signaled']and not after['child']['handle_signaled'])
    refused=[x['event']for x in hosts['crash-child-alive/contender']['events']if x['event'].get('event')=='operator_result'and x['event'].get('action')=='claim']
    ck('crash_rejects_before_and_after_child_natural_exit',len(refused)==2 and all(not x['ok']and 'unconfirmed'in x['error']for x in refused))
    ck('crash_does_not_invent_exit_EOF_or_recovery',not any(x['event']in ['exit','stdout_eof','stderr_eof']for x in crash['native'])and not any(x['event']=='phase'and x['detail']=='Released'for x in crash['native']))
    expired=hosts['first-deadline/host'];approval=next(x for x in expired['events']if x['event'].get('event')=='operator_result'and x['event'].get('action')=='approve'and x['event'].get('ok'));claim=next(x for x in expired['result']['commands']if x['action']=='claim')
    ck('claim_after_first_250ms_deadline_no_spawn',claim['sent_at_ns']-approval['observed_at_ns']>=500000000 and not expired['frames'])
    old=cases['old-issuer']['case']['facts'];ck('persisted_approved_old_issuer_not_restored',old['old_grant']!=old['fresh_grant']and old['persisted_approved_did_not_restore_live_authority'])
    before=read(HERE/'runs/independent-001/identities-before.json');current=verify([{'path':x['path'],'sha256':x['expected']}for x in before]);write(out/'runtime-identities-current.json',current);ck('2233_runtime_inputs_still_frozen',all(x['match']for x in current))
    uafter=verify(upins);write(out/'unified-identities-after.json',uafter);ck('late_handoff_inputs_frozen_during_review',ubefore==uafter)
    write(out/'decoded-frames.json',decoded);write(out/'approval-bindings.json',bindings)
    host_summary=[{'host':key,'host_pid':h['result']['host_pid'],'host_exit':h['result']['host_exit_code'],'child_pid':next((x['snapshot']['pid']for x in h['result']['OS_observations']if x['role']=='child'),None),'child_exit':next((x['snapshot']['exit_code']for x in reversed(h['result']['OS_observations'])if x['role']=='child'),None),'frames':len(h['frames']),'holder_count':h['result']['known_holder_count'],'crash_injected':h['result']['reviewer_crash_injected']}for key,h in hosts.items()]
    result={'status':'limited_semantic_checks_passed'if all(x['passed']for x in checks)else 'semantic_review_failed','checks':checks,'check_count':len(checks),'failed':[x for x in checks if not x['passed']],'accepted_case_groups':len(cases),'accepted_serve_hosts':len(hosts),'independently_witnessed_children':sum(h['result']['known_child_count']for h in hosts.values()),'independently_witnessed_holders':sum(h['result']['known_holder_count']for h in hosts.values()),'decoded_complete_frames':sum(len(h['frames'])for h in hosts.values()),'reconstructed_approval_configs':len(bindings),'hosts':host_summary,'historical_replay_natural_binding_differences':history_diff,'failed_attempt_excluded_from_pass_count':'independent-001/crash-child-alive; driver awaited diagnostic EOF with OS exit; preserved','unified_handoff_sha256':sha(hand),'late_handoff_plugin_inputs':len(unified['input_sha256']),'late_handoff_external_inputs':len(unified['external_input_sha256']),'late_handoff_scope':'review-time current hashes, not retroactive execution precheck','independent_compile':False,'whole_process_tree_observation':False,'product_pass_credit':0,'M02_complete':False,'G0_passed':False,'G1_passed':False,'validator_sha256':sha(__file__)}
    write(out/'result.json',result);print(json.dumps({k:v for k,v in result.items()if k not in ['checks','hosts']},indent=2));return 0 if not result['failed']else 1
if __name__=='__main__':raise SystemExit(main())
