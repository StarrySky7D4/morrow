"""Seal all new evidence including failed attempts; no old artifact writes."""
import json,sqlite3
from pathlib import Path
from check_review import HERE,read,write,sha,verify
from ledger_codec import unpack
SCHEMA='fa419bc52779e160985bf55da8ef05b7ed5f1603ebf2350d412473775ffa6450'
def main():
    if (HERE/'ready-handoff.json').exists():raise ValueError('already sealed')
    out=HERE/'runs/final-seal-001'
    if not (out/'acl-receipt.json').is_file():raise ValueError('private final seal required')
    semantic=HERE/'runs/semantic-001/result.json';s=read(semantic)
    if s['failed']or s['check_count']!=501 or s['accepted_case_groups']!=11:raise ValueError('semantic review not complete')
    summaries={x['host']:x for x in s['hosts']};binding=[]
    run_for={c['case']:HERE/'runs'/r/c['case'] for r in ['independent-001','independent-002','independent-003']for c in read(HERE/'runs'/r/'result.json')['cases']}
    for item in read(HERE/'runs/semantic-001/decoded-frames.json'):
        if not item['frames']:continue
        key=item['host'];case,name=key.split('/');folder=run_for[case]/name;r=read(folder/'result.json');events=read(folder/'events.json')
        sent=[x['request']['grant_id']for x in r['commands']if x['action']=='claim']
        results=[x['event']for x in events if x['event'].get('event')=='operator_result'and x['event'].get('action')=='claim']
        if len(sent)!=len(results):raise ValueError('claim command/result cardinality')
        successes=[(g,x)for g,x in zip(sent,results)if x['ok']]
        if len(successes)!=1:raise ValueError('expected one successful claim per accepted serve')
        grant,answer=successes[0];argv=r['argv'];profile=Path(argv[argv.index('--profile')+1])
        with sqlite3.connect('file:'+(profile/'native-admissions.sqlite').as_posix()+'?mode=ro',uri=True)as db:
            fields=unpack(db.execute('SELECT payload FROM approvals WHERE id=?',(grant,)).fetchone()[0])
        first=item['frames'][0]['decoded'];match=first['instanceEpoch']==fields[12][1]==answer['result']['epoch']and first['executionConfigSha256']==fields[10][1].hex()and first['artifactSha256']==fields[9][1].hex()and first['childPid']==answer['result']['pid']==summaries[key]['child_pid']and first['session']==answer['result']['session']and all(f['decoded']['schemaSha256']==SCHEMA for f in item['frames'])
        binding.append({'host':key,'successful_claim_grant':grant,'generation':fields[12][1],'actual_child_pid':first['childPid'],'matches_claim_record_and_initial_frame':match,'all_frames_authoritative_schema':all(f['decoded']['schemaSha256']==SCHEMA for f in item['frames'])})
    if len(binding)!=10 or not all(x['matches_claim_record_and_initial_frame']for x in binding):raise ValueError('successful claim/wire binding failure')
    current=verify([{'path':x['path'],'sha256':x['expected']}for x in read(HERE/'runs/independent-001/identities-before.json')]);write(out/'runtime-inputs-current.json',current)
    late=verify([{'path':x['path'],'sha256':x['expected']}for x in read(HERE/'runs/semantic-001/unified-identities-before.json')]);write(out/'unified-inputs-current.json',late)
    if not all(x['match']for x in current+late):raise ValueError('frozen input drift')
    gate=read(HERE/'runs/final-gate-001/result.json')
    if gate['exit_code']!=2 or gate['acceptance_not_run']!=84 or gate['product_scenarios_verified']!=0:raise ValueError('product gate changed')
    states=[
      ('M02A-01','source_and_control_boundary_verified','HostAuthority sole public entry, typed native ledger, approved injection rejected','production approval UI/CLI and installed identity authentication absent'),
      ('M02A-02','partial_verified','20 configs reconstructed; three record identity mutations and executable drift rejected','other field mutations producer/API only; role labels not authenticated product identities'),
      ('M02A-03','verified_limited_profile','actual duplicate claims, consumed ledger and one initial child','not all transaction failure/interleaving windows dynamically injected'),
      ('M02A-04','verified_limited_profile','two real concurrent serve hosts, one initial winner, no loser spawn before release','one fixed trusted profile/slot only; no global process inventory'),
      ('M02A-05','verified_limited_binding','real prior Hello and Query byte-identical; new session rejects17','multiple fields naturally differ; not isolated field or production CLI auth proof'),
      ('M02A-06','verified_limited_control','real approval then revoke prevents claim','not all simultaneous pre-spawn transaction schedules covered independently'),
      ('M02A-07','partial_verified','second host persists revoke; owner runtime ACK, no later State','pending not instantaneous revoke; in-flight partial response/saturation unqualified'),
      ('M02A-08','verified_limited_control','250ms original approval, delayed and repeated claims reject','no wall-clock/restart revival; cross-issuer persisted data separately rejected'),
      ('M02A-09','verified_limited_profile','root OS exited, known holder alive, persistent Unconfirmed blocks host2','no OS process-tree containment or complete inherited-handle proof'),
      ('M02A-10','verified_fail_closed_only','host OS exit/old child alive witnessed; refusal persists after natural child exit','no full reconciliation or automatic recovery; last durable phase not live OS proof'),
      ('M02A-11','verified_limited_profile','new approvals launch only after normal complete release in two groups','cannot reuse old issuer authority; crash Unknown remains blocked'),
      ('M02A-12','partial_verified','real identity lock, fixed root/profile metadata; profile copy rejected','initialization race producer only; arbitrary backup/alias/hostile replacement excluded'),
      ('M02A-13','partial_verified','transaction/spawn/register order source reviewed; running-host crash actual','not all commit-uncertain or pre-registration fault points dynamically forced'),
      ('M02A-14','partial_verified','2233 before/after pins, ready builds associated, preserved failed attempt, independent runs','no independent rebuild this batch; no package/install/OS/platform product qualification'),
    ]
    matrix=read(HERE/'matrix.json');lookup={x['id']:x for x in matrix['diagnostics']}
    diag={'scope':'append-only diagnostic overlay, zero original product credit','matrix_sha256':sha(HERE/'matrix.json'),'diagnostics':[{'id':i,'title':lookup[i]['title'],'original_ids':lookup[i]['original_ids'],'status':status,'evidence':evidence,'remaining':limit,'product_pass_credit':0}for i,status,evidence,limit in states],'M-02':'partial','G0':'blocked','G1':'not_passed','P-02':'blocked','J-00':'blocked','graphs':'0/2','acceptance_not_run':84}
    write(HERE/'diagnostic-results.json',diag)
    write(out/'result.json',{'status':'final_identity_and_claim_binding_verified','successful_claim_bindings':binding,'runtime_input_count':len(current),'runtime_inputs_unchanged':True,'unified_inputs_unchanged':True,'gate_exit_code':2,'semantic_result_sha256':sha(semantic),'sealer_sha256':sha(__file__),'product_pass_credit':0})
    files=[p for p in HERE.rglob('*')if p.is_file()and p.name not in ['ready-handoff.json','evidence-manifest.json']]
    manifest={'scope':'all files in this new joint batch, including temporary test data and failed attempts; no previous batch mutation','files':[{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size}for p in sorted(files,key=str)]}
    write(HERE/'evidence-manifest.json',manifest)
    handoff={'status':'ready_limited_independently_executed_admission_owner_review','report':str(HERE/'review-2026-09-29.md'),'report_sha256':sha(HERE/'review-2026-09-29.md'),'manifest':str(HERE/'evidence-manifest.json'),'manifest_sha256':sha(HERE/'evidence-manifest.json'),'semantic_result':str(semantic),'semantic_result_sha256':sha(semantic),'seal_result':str(out/'result.json'),'seal_result_sha256':sha(out/'result.json'),'diagnostic_results_sha256':sha(HERE/'diagnostic-results.json'),'host_ready_sha256':'ee8986c3ab614b77d9f321a6d4961e605374c66e1e52bbf91b48f5070e6bc52c','host_manifest_sha256':'ee257b1cc15dcb72a4f92343406431fbc4a35153915e23e5f73e9265917e373e','plugin_unified_handoff_sha256':s['unified_handoff_sha256'],'independent_case_groups':11,'accepted_serve_host_instances':17,'independently_witnessed_children':10,'independently_witnessed_holders':1,'complete_decoded_frames':59,'semantic_checks':501,'successful_claim_binding_checks':10,'reconstructed_approval_configs':20,'execution_inputs_frozen_before_after':2233,'old_baseline_paths_unchanged':2033,'failed_attempt_retained':'independent-001/crash-child-alive, observer combined OS exit and diagnostic EOF; excluded from pass count; corrected continuation only','late_manifest_scope':'533 plugin and402 external inputs checked during post-run review; no retroactive execution precheck','independent_build':False,'limits':['trusted same-user fixed profile/slot only; typed native authority not production approval UI or authenticated CLI','known PID held handles only; no independent PPID/full-tree inventory or image attestation','child IPC/EOF from host; OS process facts and host output EOF separately observed','actual historical frames prove combined identity rejection, not isolated field coverage','host diagnostic EOF lagged OS exit; inherited-handle containment unqualified','running-host crash refusal is not recovery; no automatic owner clearing even after child natural exit','not all transaction failures/interleavings, in-flight partial writes, saturated control/output paths, installer/TOCTOU or other platforms qualified','read-own-session only; no business network/store/command success'],'M-02':'partial','G0':'blocked','P-02':'blocked','J-00':'blocked','G1':'not_passed','product_graphs':'0/2','original_acceptance_not_run':84,'product_pass_credit':0,'next_implementation_performed':False}
    write(HERE/'ready-handoff.json',handoff)
    if not all(x['match']for x in verify(manifest['files'])):raise ValueError('seal verification failed')
    print(json.dumps({'status':handoff['status'],'handoff_sha256':sha(HERE/'ready-handoff.json'),'report_sha256':handoff['report_sha256'],'manifest_sha256':handoff['manifest_sha256'],'sealed_files':len(files),'semantic_sha256':sha(semantic)},indent=2))
if __name__=='__main__':main()
