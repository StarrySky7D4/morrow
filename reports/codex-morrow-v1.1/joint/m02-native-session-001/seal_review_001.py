"""Seal this bounded joint report, never overwrite previous evidence."""
import json
from pathlib import Path
from check_review import HERE,read,write,sha,verify_pins

def main():
    if (HERE/'ready-handoff.json').exists():raise ValueError('already sealed')
    run=HERE/'runs/final-seal-001';run.mkdir(exist_ok=False)
    b=HERE/'runs/independent-sessions-001';s=read(HERE/'runs/semantic-review-001/result.json')
    if s['failed'] or s['check_count']!=322:raise ValueError('semantic review incomplete')
    raw=[]
    for item in read(b/'result.json')['cases']:
        p=Path(item['observations']);r=read(p);records=read(p.parent/'host-events.json')
        events=[json.loads(line) for line in (p.parent/'host.stdout').read_bytes().splitlines()]
        observations=[x['observation'] for x in events if x['event']=='host_observation']
        ok=events==[x['event'] for x in records] and observations==r['final_host_snapshot']['events'] and [x['snapshot'] for x in events if x['event']=='final']==[r['final_host_snapshot']]
        raw.append({'case':item['case'],'raw_stdout_sha256':sha(p.parent/'host.stdout'),'raw_stderr_sha256':sha(p.parent/'host.stderr'),'events_sha256':sha(p.parent/'host-events.json'),'raw_lines':len(events),'matches_event_stream_and_final':ok})
    if not all(x['matches_event_stream_and_final'] for x in raw):raise ValueError('raw evidence differs')
    rows=[
      ('M02D-01','partial_verified','10 configured launches and digest bindings; 8 independent child OS handles','production approval, trusted install, hash/load race, two fast negative OS witnesses'),
      ('M02D-02','verified_limited_control_slice','real Challenge/Hello/Welcome/Query/State and 64 decoded frames','read-own-session only; no business authority or external pipe interception'),
      ('M02D-03','partial_verified','wrong-artifact Hello rejected, source authority reviewed','other forged identity/platform/capability fields not fully run; no approved field'),
      ('M02D-04','not_run_historical_replay','current epoch alteration rejected','no prior-session/pipe/permit replay; alteration is not historical replay'),
      ('M02D-05','partial_verified','new query after revoke ack denied; no later State','unsent and partial-write in-flight windows unexecuted'),
      ('M02D-06','partial_verified','700 ms TTL not renewed by seven queries','authorize-to-launch delay/retry not independently executed'),
      ('M02D-07','not_run_saturation','ordinary control stop bounded; source priority reviewed','no independent saturated output/control queue or OS backpressure proof'),
      ('M02D-08','partial_verified','real partial frame, no Hello, ignored Stop; independent codec negatives','codec threshold depth/traversal, write backpressure and all malformed runtime variants unexecuted'),
      ('M02D-09','partial_verified','8 OS child exit witnesses, host-reported child EOF, independent host output EOF','two rapid rejects lack independent child handles; child EOF is host evidence'),
      ('M02D-10','partial_verified','real holder PID, ClosingUnconfirmed, retained owner then release','same-slot re-admission producer only; no tree containment proof'),
      ('M02D-11','not_run_business_unknown','client disconnect after revoke observed, no retry in this control exchange','no business writer, pending operation ledger or crash reconciliation'),
      ('M02D-12','partial_source_and_build_verified','shared Core HostPolicy and reusable native_session actual dependency path','private per-host control policy; no production approval integration or release qualification'),
      ('M02D-13','verified_integrity_only','1485 old paths unchanged','no old UI or guest runtime compatibility regression'),
      ('M02D-14','partial_verified','frozen source/build/exe identities, fresh runs, codec independent build, negative exits','no independent host/client rebuild or all production preflight fault cases'),
    ]
    matrix=read(HERE/'matrix.json');byid={x['id']:x for x in matrix['diagnostics']}
    diag={'scope':'append-only progress overlay; original matrix and product ledger frozen','matrix_sha256':sha(HERE/'matrix.json'),'diagnostics':[{'id':i,'title':byid[i]['title'],'related_original_ids':byid[i]['related_original_ids'],'status':status,'evidence':evidence,'remaining':limit,'product_pass_credit':0} for i,status,evidence,limit in rows],'M-02':'partial','G0':'blocked','G1':'not_passed','P-02':'blocked','J-00':'blocked','product_graphs':'0/2','original_acceptance_not_run':84}
    write(HERE/'diagnostic-results.json',diag)
    pins=[{'path':x['path'],'sha256':x['expected']} for x in read(b/'identities-before.json')]
    verified,issues=verify_pins(pins);write(run/'execution-inputs-current.json',verified)
    hp=[{'path':x['path'],'sha256':x['expected']} for x in read(HERE/'runs/semantic-review-001/unified-handoff-identities-before.json')]
    hv,hi=verify_pins(hp);write(run/'unified-inputs-current.json',hv)
    if issues or hi:raise ValueError('input drift')
    gate=HERE.parent/'runs/m02-native-session-001-final-gate/result.json'
    if read(gate)['exit_code']!=2 or read(gate)['product_scenarios_verified']!=0:raise ValueError('gate unexpectedly changed')
    result={'status':'sealed_limited_control_slice','raw_log_checks':raw,'raw_log_check_count':len(raw),'raw_json_lines':sum(x['raw_lines'] for x in raw),'execution_input_count':len(verified),'input_issues':issues+hi,'semantic_result_sha256':sha(HERE/'runs/semantic-review-001/result.json'),'gate_result_sha256':sha(gate),'gate_exit_code':2,'watcher_exclusion_records':sum(x['watcher_exclusion_records'] for x in s['cases']),'product_pass_credit':0,'M02_complete':False,'G0_passed':False,'sealer_sha256':sha(__file__)}
    write(run/'result.json',result)
    # Include human-readable reports, reviewer sources, root-level run receipts,
    # all ten raw cases, exact consumer source/exe. Avoid build caches/profiles.
    files=set(p for p in HERE.iterdir() if p.is_file() and p.name not in ['ready-handoff.json','evidence-manifest.json'])
    for r in (HERE/'runs').iterdir():
        if r.is_dir():files.update(p for p in r.iterdir() if p.is_file())
    for item in read(b/'result.json')['cases']:files.update(p for p in Path(item['observations']).parent.iterdir() if p.is_file())
    for rel in ['Cargo.toml','Cargo.lock','src/main.rs']:files.add(HERE/'wire-consumer-001'/rel)
    files.add(HERE/'t-wire-002/debug/joint-m02-wire-consumer.exe');files.add(gate)
    manifest={'scope':'selected evidence and reviewer tools; caches excluded; build input manifests retained','files':[{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size} for p in sorted(files,key=str)]}
    write(HERE/'evidence-manifest.json',manifest)
    handoff={'status':'ready_limited_independently_executed_review','report':str(HERE/'review-2026-09-29.md'),'report_sha256':sha(HERE/'review-2026-09-29.md'),'manifest':str(HERE/'evidence-manifest.json'),'manifest_sha256':sha(HERE/'evidence-manifest.json'),'semantic_result':str(HERE/'runs/semantic-review-001/result.json'),'semantic_result_sha256':sha(HERE/'runs/semantic-review-001/result.json'),'final_seal_result':str(run/'result.json'),'final_seal_result_sha256':sha(run/'result.json'),'diagnostic_result_sha256':sha(HERE/'diagnostic-results.json'),'plugin_unified_handoff_sha256':s['unified_handoff_sha256'],'independent_real_cases':10,'decoded_complete_frames':64,'semantic_checks':322,'independent_child_OS_cases':8,'independent_host_client_rebuild':False,'independent_codec_consumer_build':True,'execution_inputs_frozen_before_after':1812,'unified_handoff_plugin_inputs':151,'unified_handoff_external_inputs':263,'unified_handoff_timing':'checked during post-run review; no retroactive pre-run claim','limits':['two fast rejects lack independent child OS handles','child EOF and pipe byte capture are host observations; independent decoder/OS witness provenance separated','261 unclassified watcher exclusions; not complete process-tree inventory','modified epoch is not historical replay','in-flight partial writes, saturation, formal CLI auth and production owner/approval/crash integration remain unqualified','same-slot rejection and delayed launch producer/API evidence only','no business execution/network/store success or product credit'],'M-02':'partial','P-02':'blocked','J-00':'blocked','G0':'blocked','G1':'not_passed','product_graphs':'0/2','original_acceptance_not_run':84,'product_pass_credit':0,'next_implementation_started':False}
    write(HERE/'ready-handoff.json',handoff)
    # Validate the just-created seal once. No outputs are changed after these hashes.
    _,fail=verify_pins(manifest['files'])
    if fail:raise ValueError(fail)
    print(json.dumps({'status':handoff['status'],'handoff_sha256':sha(HERE/'ready-handoff.json'),'report_sha256':handoff['report_sha256'],'manifest_sha256':handoff['manifest_sha256'],'sealed_files':len(files),'raw_json_lines':result['raw_json_lines']},indent=2))
if __name__=='__main__':main()
