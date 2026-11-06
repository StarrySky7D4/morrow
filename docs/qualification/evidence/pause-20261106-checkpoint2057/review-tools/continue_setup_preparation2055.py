"""Continue only the four undispatched preparation phases after a public-plan repair."""
from pathlib import Path
import datetime,hashlib,json,os,subprocess,sys
B=Path(__file__).parent;C=Path('C:/ms1020');O=B/'setup2050-preparation-continuation-2055'
load=lambda p:json.loads(p.read_bytes())
def pin(p):
 raw=p.read_bytes();return dict(path=str(p),bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
def verify(row):assert pin(Path(row['path']))=={k:row[k] for k in ('path','bytes','sha256')},row['path']
def save(p,v):
 with p.open('x',encoding='utf8') as f:json.dump(v,f,indent=2)
p=load(B/'case2050-preparation.json');repair=load(B/'setup2050-plan-repair-2054/result.json')
assert repair['status']=='STRICT_PLAN_METADATA_SEPARATED_ALL_RUNTIME_GUARDS_UNCHANGED'
assert repair['source_digest']==p['source_digest'] and repair['tag']==p['tag']
for k in ('original_plan','original_selection','corrected_plan','corrected_selection','qualification'):verify(repair[k])
for row in repair['immutable_files']:verify(row)
for row in repair['rows']:
 for k in ('original','derived','diff'):verify(row[k])
prior=load(B/'setup2050-preparation-pipeline-2050/result.json')
assert prior['status']=='PIPELINE_STOPPED_ON_ORIGINAL_NONZERO_NO_RETRY' and prior['failed_phase']=='review-current-stage'
assert [r['exit_code'] for r in prior['completed']]==[0,0,0,0,1]
assert load(B/'setup2050-preparation-entry-2052/result.json')['original_process_returned']
sel=load(B/'vm-stage-selection-2050.json');S=Path(sel['root'])
assert load(S/'public-only-2054-process-result.json')==dict(exit_code=0,original_process_returned=True,vm_calls=0,private_reads=0)
assert load(S/'public-only-result.json')['status']=='PASS_PUBLIC_PINS_AND_FIXED_SECURITY_MODULE'
review=load(B/'roles070-stage2054-binding-review/result.json')
assert review['status']=='PASS_MECHANICAL_ROLE_AND_COMPILER_INPUT_BINDING' and review['plan']==sel['plan']
assert review['all_current_code_bytes_verified'] and review['all_four_cold_compiler_outputs_verified']
assert not O.exists()
for leaf in ('stage-invocation.reserved','bind-invocation.reserved','case-invocation.reserved'):assert not (S/leaf).exists()
stages=[('stage-once','run_cold_guarded_2054.py',['stage']),('prepare-reviewed-binding','prepare_bind_2050.py',[]),('bind-once','run_vm_bind_2050.py',[]),('prepare-setup-failure','prepare_isolated_case_2054.py',[])]
pins={r['derived']['path']:r['derived'] for r in p['rows']+repair['rows']}
for _,name,_ in stages:verify(pins[str(B/name)])
sys.path.insert(0,str(C/'tool'));import verify_structural_repairs as source
assert source.source_digest()==p['source_digest']
O.mkdir();save(O/'invocation.reserved.json',dict(tag=p['tag'],source_digest=p['source_digest'],old_failure=pin(B/'setup2050-preparation-pipeline-2050/result.json'),repair=pin(B/'setup2050-plan-repair-2054/result.json'),fresh_role_review=pin(B/'roles070-stage2054-binding-review/result.json'),phases=[s[0] for s in stages],old_vm_actions_repeated=False,case_dispatch=False,automatic_retry=False))
env={k:v for k,v in os.environ.items() if not k.startswith('MORROW_')}
env.update(PYTHONUTF8='1',PYTHONDONTWRITEBYTECODE='1',TEMP='D:/build_cache/tmp',TMP='D:/build_cache/tmp',PSModulePath=r'C:\Windows\System32\WindowsPowerShell\v1.0\Modules')
completed=prior['completed'][:4]+[dict(phase='review-current-stage',exit_code=0,stdout=pin(S/'public-only-2054.stdout'),stderr=pin(S/'public-only-2054.stderr'),process=pin(S/'public-only-2054-process-result.json'),repaired_public_only=True)]
for n,(phase,name,args) in enumerate(stages,6):
 path=B/name;verify(pins[str(path)])
 command=[sys.executable,'-B',str(path),*args];stem=f'{n:02d}-{phase}'
 save(O/(stem+'-reserved.json'),dict(phase=phase,script=pin(path),command=command,utc=datetime.datetime.now(datetime.timezone.utc).isoformat()))
 print(json.dumps(dict(phase=phase,state='RUNNING_ONCE')),flush=True)
 with (O/(stem+'.stdout')).open('xb') as out,(O/(stem+'.stderr')).open('xb') as err:
  process=subprocess.Popen(command,cwd=C,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
  save(O/(stem+'-process.json'),dict(pid=process.pid,phase=phase));code=process.wait()
 row=dict(phase=phase,exit_code=code,stdout=pin(O/(stem+'.stdout')),stderr=pin(O/(stem+'.stderr')))
 save(O/(stem+'-result.json'),row);completed.append(row)
 print(json.dumps(dict(phase=phase,state='RETURNED',exit_code=code)),flush=True)
 if code:
  save(O/'result.json',dict(status='CONTINUATION_STOPPED_ON_ORIGINAL_NONZERO_NO_RETRY',failed_phase=phase,phase_exit=code,completed=completed,tag=p['tag'],source_digest=p['source_digest'],case_dispatch=False,source_accepted=False,release_eligible=False))
  sys.exit(code)
assert source.source_digest()==p['source_digest']
save(O/'result.json',dict(status='CURRENT_SETUP2050_PREPARED_BOUND_NOT_DISPATCHED',tag=p['tag'],source_digest=p['source_digest'],completed=completed,all_selected_preparation_processes_returned=True,original_failed_preflight_preserved=pin(B/'setup2050-preparation-pipeline-2050/result.json'),actual_fault_injection=False,old_unknown_owner_contacts=0,source_accepted=False,release_eligible=False))
print('CURRENT_SETUP2050_PREPARED_BOUND_NOT_DISPATCHED',flush=True)
