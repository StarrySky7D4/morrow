"""Read-only semantic review of frozen actual runs; no new product execution."""
import hashlib,json,os
from pathlib import Path
from check_review import HERE,PLUGIN,read,write,sha,verify_pins
from runtime_decode import decode_frame

SCHEMA='fa419bc52779e160985bf55da8ef05b7ed5f1603ebf2350d412473775ffa6450'
def main():
    out=HERE/'runs/semantic-review-001';out.mkdir(exist_ok=False)
    batch=HERE/'runs/independent-sessions-001';summary=read(batch/'result.json')
    checks=[];cases=[]
    def check(name,ok):checks.append({'check':name,'passed':bool(ok)})
    pins=[{'path':x['path'],'sha256':x['expected']} for x in read(batch/'identities-before.json')]
    identities,issues=verify_pins(pins);write(out/'identities-current.json',identities)
    check('1812_original_runtime_inputs_still_frozen',len(pins)==1812 and not issues and read(batch/'identities-before.json')==read(batch/'identities-after.json'))
    hand=PLUGIN/'receipts/m02-native-session-001/handoff.json'
    check('unified_plugin_handoff_identity',sha(hand)=='2ec516c150da95bf592b07353fe6c8b75917884f39e92c99675880cd10a9d682')
    h=read(hand);hp={str(PLUGIN/p):s for p,s in h['input_sha256'].items()};hp.update(h['external_input_sha256']);hp.update({str(PLUGIN/p):s for p,s in h['old_frozen_handoffs'].items()});hp[str(hand)]=sha(hand)
    hi,errors=verify_pins([{'path':p,'sha256':s} for p,s in hp.items()]);write(out/'unified-handoff-identities-before.json',hi)
    check('unified_handoff_all_current_hashes',not errors)
    for name,digest in read(batch/'reviewer-inputs.json').items():check('reviewer_frozen:'+name,sha(HERE/name)==digest)
    for item in summary['cases']:
        name=item['case'];r=read(item['observations']);snap=r['final_host_snapshot'];ev=snap['events'];frames=r['frames'];c=frames[0]['decoded']
        def ck(label,ok):check(name+':'+label,ok)
        ck('frozen_observation',sha(item['observations'])==item['observation_sha256'])
        ck('expected_exit',item['exit_expectations_match'] and r['host_exit_code']==item['expected_host_exit'])
        ck('host_OS_exit',r['host_OS_observations'][-1]['process_handle_signaled'] and r['host_OS_observations'][-1]['exit_code']==r['host_exit_code'])
        ck('no_forced_cleanup_or_unexplained_observer_failure',all(x.startswith('child_OS_observation_unavailable:') for x in r['failures']))
        ck('controller_stdin_did_not_hold_host_open',r['controller_stdin_open_at_host_exit'])
        ck('host_output_drained_without_gap',all(x['eof'] and x['dropped_bytes']==0 and x['queue_drops']==0 for x in r['host_output_observations'].values()))
        ck('final_release_three_facts',snap['phase']=='Released' and not snap['owner_retained'] and snap['exit_observed'] and snap['stdout_eof'] and snap['stderr_eof'] and snap['event_overflow']==0)
        release=next(x['at_us'] for x in ev if x['event']=='phase' and x['detail']=='Released')
        times={k:next(x['at_us'] for x in ev if x['event']==k) for k in ['exit','stdout_eof','stderr_eof','revoked']}
        ck('release_after_exit_and_two_EOF',release>=max(times[k] for k in ['exit','stdout_eof','stderr_eof']))
        raw_events=[x for x in ev if x['event'] in ['frame_sent','frame_received']]
        ck('all_raw_frames_accounted',len(raw_events)==len(frames) and all(x['detail']['raw_hex']==f['raw_hex'] and x['event']==f['direction'] and x['at_us']==f['host_at_us'] for x,f in zip(raw_events,frames)))
        ck('fresh_Capnp_decode_matches_saved',all(decode_frame(bytes.fromhex(f['raw_hex']))[0]==f['decoded'] for f in frames))
        ck('challenge_actual_binding',c['kind']=='challenge' and c['sequence']==0 and c['childPid']==r['child_pid'] and c['artifactSha256']==r['client_sha256'] and c['schemaSha256']==SCHEMA and c['session']==snap['session'] and c['instanceEpoch']==snap['epoch'] and c['revocationGeneration']==1 and c['capabilities']==1 and c['major']==2 and c['revision']==1 and c['reserved']==0)
        args=r['argv'];opts={};clientargs=[]
        for key,val in zip(args[1::2],args[2::2]):
            if key=='--client-arg':clientargs.append(val)
            else:opts[key]=val
        spec={'slot':'native-control','executable':opts['--client'],'artifact_sha256':list(bytes.fromhex(r['client_sha256'])),'cwd':opts['--work-dir'],'args':clientargs,'ttl_ms':int(opts['--ttl-ms']),'handshake_ms':int(opts['--handshake-ms']),'frame_ms':int(opts['--frame-ms']),'close_ms':int(opts['--close-ms']),'request_budget':int(opts['--budget'])}
        config={'launch':spec,'platform':'windows','arch':'x86_64','wire':SCHEMA,'environment':{k:os.environ[k] for k in ['SystemRoot','WINDIR','COMSPEC'] if k in os.environ}}
        digest=hashlib.sha256(json.dumps(config,sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
        ck('independent_config_digest_reconstruction',digest==c['executionConfigSha256'])
        immutable=['major','revision','session','instanceEpoch','childPid','nonce','schemaSha256','artifactSha256','executionConfigSha256','capabilities','reserved']
        seq=0;ttl=c['remainingMs'];budget=c['requestBudget'];gen=1;states=[]
        for i,f in enumerate(frames):
            d=f['decoded'];bad='artifactSha256' if name=='wrong-artifact' else 'instanceEpoch' if name=='modified-epoch' else None
            allowed=bad if f['direction']=='frame_received' else None
            ck(f'frame{i}_immutable',all(d[k]==c[k] for k in immutable if k!=allowed))
            if allowed:ck(f'frame{i}_exact_mutation',d[allowed]!=c[allowed])
            if f['direction']=='frame_received':
                seq+=1;ck(f'frame{i}_strict_request',d['sequence']==seq and d['code']==0 and all(d[k]==c[k] for k in ['remainingMs','requestBudget','revocationGeneration']))
            else:
                ck(f'frame{i}_monotonic_response',d['remainingMs']<=ttl and d['requestBudget']<=budget and gen<=d['revocationGeneration']<=2)
                ttl,budget,gen=d['remainingMs'],d['requestBudget'],d['revocationGeneration']
                ck(f'frame{i}_response_sequence',d['sequence']==(0 if d['kind'] in ['challenge','stop'] else seq))
                if d['kind']=='state':states.append(f['host_at_us'])
        oschild=r['child_OS_observations']
        if oschild:ck('child_OS_actual_identity_exit',oschild[0]['parent_pid']==r['host_pid'] and oschild[0]['image_path_sha256']==r['client_sha256'] and oschild[-1]['process_handle_signaled'] and oschild[-1]['exit_code']==snap['exit_code'])
        if name=='normal':ck('two_real_queries',len(states)==2)
        if name=='revoke':
            ack=next(x for x in ev if x['event']=='control_ack' and x['detail']['action']=='revoke')
            denied=[f for f in frames if f['decoded']['kind']=='denied']
            ck('post_ack_query_denied_no_state',len(denied)==1 and denied[0]['decoded']['code']==19 and denied[0]['decoded']['revocationGeneration']==2 and denied[0]['host_at_us']>ack['at_us'] and not any(t>ack['at_us'] for t in states) and not any(x['event']=='state_read' and x['at_us']>ack['at_us'] for x in ev))
        reason=next(x['detail']['reason'] for x in ev if x['event']=='revoked')
        expected={'normal':25,'revoke':19,'stop':25,'expiry':20,'wrong-artifact':17,'modified-epoch':17,'no-hello':23,'partial-frame':16,'ignore-stop':25,'hold-output':25}
        ck('correct_revocation_reason',reason==expected[name])
        if name=='expiry':ck('queries_do_not_renew_700ms_TTL',len(states)>=5 and 700000<=times['revoked']<1000000 and frames[-1]['decoded']['remainingMs']==0)
        if name=='hold-output':
            unconfirmed=next(x['at_us'] for x in ev if x['event']=='phase' and x['detail']=='ClosingUnconfirmed')
            holders=[x for x in r['descendant_OS_observations']['processes'] if x['initial']['parent_pid']==r['child_pid']]
            ck('real_holder_and_unconfirmed_until_EOF',len(holders)==1 and holders[0]['final']['process_handle_signaled'] and times['exit']<unconfirmed<min(times['stdout_eof'],times['stderr_eof']) and any(x['event']=='owner_retained' and x['detail'] is True for x in ev))
        cases.append({'case':name,'host_pid':r['host_pid'],'child_pid':r['child_pid'],'host_exit':r['host_exit_code'],'child_exit':snap['exit_code'],'independent_child_OS':bool(oschild),'frames':len(frames),'states':len(states),'times_us':times|{'released':release},'watcher_exclusion_records':len(r['descendant_OS_observations']['errors']),'observation_sha256':sha(item['observations'])})
    hi2,errors=verify_pins([{'path':p,'sha256':s} for p,s in hp.items()]);write(out/'unified-handoff-identities-after.json',hi2)
    check('unified_handoff_unchanged_during_review',not errors and hi==hi2)
    result={'status':'limited_semantic_checks_passed' if all(x['passed'] for x in checks) else 'review_failed','checks':checks,'check_count':len(checks),'failed':[x for x in checks if not x['passed']],'cases':cases,'frame_count':sum(x['frames'] for x in cases),'independent_child_OS_cases':sum(x['independent_child_OS'] for x in cases),'unified_handoff_sha256':sha(hand),'unified_plugin_inputs':len(h['input_sha256']),'unified_external_inputs':len(h['external_input_sha256']),'late_manifest_scope':'unified handoff checked before and after semantic review; original 1812 identities checked before/after actual execution; newly enumerated inputs have no retroactive execution precheck','config_environment_scope':'digest reconstructed using current unchanged allowlisted OS values; values not retained','watcher_limit':'excluded candidates lack PID/details; not a complete descendant inventory or containment proof','product_pass_credit':0,'M02_complete':False,'G0_passed':False,'G1_passed':False,'validator_sha256':sha(__file__)}
    write(out/'result.json',result);print(json.dumps({k:result[k] for k in ['status','check_count','failed','frame_count','independent_child_OS_cases']},indent=2));return 0 if not result['failed'] else 1
if __name__=='__main__':raise SystemExit(main())
