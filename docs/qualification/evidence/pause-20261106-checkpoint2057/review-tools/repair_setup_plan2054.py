"""Separate qualification metadata from the original strict executable plan.

Preserve the failed bytes and every guard. No VM or credential action occurs.
"""
from pathlib import Path
import ast,copy,difflib,hashlib,json,os,sys
B=Path(__file__).parent;O=B/'setup2050-plan-repair-2054'
load=lambda p:json.loads(p.read_bytes())
def pin(p):
 raw=p.read_bytes();return dict(path=str(p),bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
def save(p,v):
 with p.open('x',encoding='utf8') as f:json.dump(v,f,indent=2)
def once(text,a,z):
 assert text.count(a)==1,a;return text.replace(a,z)
def safe(p):
 assert p.is_absolute() and str(p).startswith(str(B)+'\\')
 for q in [p,*p.parents]:
  if q.exists():assert not q.is_symlink() and not q.is_junction(),str(q)
 if p.exists() and p.is_file():assert p.stat().st_nlink==1
prepared=load(B/'case2050-preparation.json');selection=load(B/'vm-stage-selection-2050.json');S=Path(selection['root'])
assert load(B/'setup2050-preparation-entry-2052/result.json')==dict(exit_code=1,original_process_returned=True,case_dispatch=False)
prior=load(B/'setup2050-preparation-pipeline-2050/result.json')
assert prior['failed_phase']=='review-current-stage' and [r['exit_code'] for r in prior['completed']]==[0,0,0,0,1]
assert load(B/'public-preflight-diagnostic-2053/result.json')['exit_code']==78
assert b'PLAN_KEYS_NO_APPROVAL_BOOLEAN' in (B/'public-preflight-diagnostic-2053/stderr').read_bytes()
assert not O.exists()
for leaf in ['stage-invocation.reserved','bind-invocation.reserved','case-invocation.reserved','public-only-result.json']:
 assert not (S/leaf).exists(),leaf
for phase in ['stage','bind','case']:
 assert not (B/('c28-root-vm-transaction2769-'+selection['tag']+'-'+phase)).exists()
 assert not (B/('c28-root-vm-transaction2769-'+selection['tag']+'-'+phase+'.reservation')).exists()
for row in [selection['script'],selection['plan']]:assert pin(Path(row['path']))==row
plan=load(Path(selection['plan']['path']))
keys=('qualification_profile','p2_wire_bytes','original47_qualification')
metadata={k:plan[k] for k in keys}
assert metadata==dict(qualification_profile='P2_SETUP_AFTER_ACCOUNTS_QUALIFICATION',p2_wire_bytes=3523,original47_qualification=False)
fixed={k:v for k,v in plan.items() if k not in keys}
expected={'schema','phase','tag','vm_id','firmware_uuid','sid','source_inputs','artifacts','binding','stage_receipt','binding_receipt'}
assert set(fixed)==expected and {**fixed,**metadata}==plan
common=(S/'Common1737.ps1').read_bytes();invoke=(S/'Invoke-Transaction1737.ps1').read_bytes()
assert b"'binding','stage_receipt','binding_receipt')" in common
assert b"'PLAN_KEYS_NO_APPROVAL_BOOLEAN'" in common
assert b'qualification_profile' not in common and b'qualification_profile' not in (S/'Guest-Transaction1737.ps1').read_bytes()
immutable=[pin(S/n) for n in ['Common1737.ps1','Invoke-Transaction1737.ps1','Guest-Transaction1737.ps1','public-only-invocation.reserved','public-only.stdout','public-only.stderr']]
immutable+=plan['artifacts']+plan['source_inputs']
drafts=[]
def derive(oldname,newname,change):
 p=B/oldname;before=p.read_text('utf8');after=change(before);ast.parse(after)
 assert not (B/newname).exists();drafts.append((p,B/newname,before,after))
derive('prepare_vm_stage_2050.py','prepare_vm_stage_2054.py',lambda t:once(t,
 "new_plan=dict(plan,qualification_profile='P2_SETUP_AFTER_ACCOUNTS_QUALIFICATION',p2_wire_bytes=3523,original47_qualification=False,tag=TAG,vm_id=NEW_VM,firmware_uuid=NEW_FIRMWARE,sid=NEW_SID,artifacts=artifacts,source_inputs=new_inputs)",
 "save(ROOT/'qualification-stage2054.json',dict(tag=TAG,vm_id=NEW_VM,qualification_profile='P2_SETUP_AFTER_ACCOUNTS_QUALIFICATION',p2_wire_bytes=3523,original47_qualification=False))\nnew_plan=dict(plan,tag=TAG,vm_id=NEW_VM,firmware_uuid=NEW_FIRMWARE,sid=NEW_SID,artifacts=artifacts,source_inputs=new_inputs)"))
derive('verify_roles070_stage2050.py','verify_roles070_stage2054.py',lambda t:once(t,"OUT=B/'roles070-stage2050-binding-review'","OUT=B/'roles070-stage2054-binding-review'"))
def fix_public(t):
 for a,z in [('public-only-invocation.reserved','public-only-2054-invocation.reserved'),('public-only.stdout','public-only-2054.stdout'),('public-only.stderr','public-only-2054.stderr')]:t=t.replace(a,z)
 return once(t,"assert done.returncode==0 and done.stdout.strip()", "with (root/'public-only-2054-process-result.json').open('x') as f:json.dump(dict(exit_code=done.returncode,original_process_returned=True,vm_calls=0,private_reads=0),f)\nassert done.returncode==0 and done.stdout.strip()")
derive('review_stage_2050.py','review_stage_2054.py',fix_public)
profile_assert="assert plan['qualification_profile']=='P2_SETUP_AFTER_ACCOUNTS_QUALIFICATION' and plan['p2_wire_bytes']==3523 and not plan['original47_qualification']"
def fix_case_prepare(t):
 t=once(t,profile_assert,"stage_profile=json.loads((root/'qualification-stage2054.json').read_bytes())\nassert stage_profile['tag']==tag and stage_profile['vm_id']==plan['vm_id']\nassert stage_profile['qualification_profile']=='P2_SETUP_AFTER_ACCOUNTS_QUALIFICATION' and stage_profile['p2_wire_bytes']==3523 and not stage_profile['original47_qualification']")
 return once(t,"source_roles=pin(base/'roles-070/result.json')","p2_wire_bytes=3523,stage_profile=pin(root/'qualification-stage2054.json'),source_roles=pin(base/'roles-070/result.json')")
derive('prepare_isolated_case_2050.py','prepare_isolated_case_2054.py',fix_case_prepare)
def fix_case_run(t):
 t=once(t,profile_assert,"stage_profile=json.loads((root/'qualification-stage2054.json').read_bytes())\nassert stage_profile['tag']==tag and stage_profile['vm_id']==plan['vm_id']\nassert stage_profile['qualification_profile']=='P2_SETUP_AFTER_ACCOUNTS_QUALIFICATION' and stage_profile['p2_wire_bytes']==3523 and not stage_profile['original47_qualification']")
 t=once(t,"pins.extend([selected['qualification'],qualification['source_roles'],qualification['qualification_fault_derivation']])","assert qualification['p2_wire_bytes']==3523 and qualification['stage_profile']['path']==str(root/'qualification-stage2054.json')\npins.extend([selected['qualification'],qualification['stage_profile'],qualification['source_roles'],qualification['qualification_fault_derivation']])")
 return t
derive('run_isolated_case_2050.py','run_isolated_case_2054.py',fix_case_run)
def fix_cold(t):
 t=once(t,"B/'roles070-stage2050-binding-review/result.json'","B/'roles070-stage2054-binding-review/result.json'")
 t=once(t,"'run_isolated_case_2050.py'","'run_isolated_case_2054.py'")
 t=once(t,"matches=[r['derived'] for r in prepared['rows'] if Path(r['derived']['path'])==driver]","repair=load(B/'setup2050-plan-repair-2054/result.json');matches=[r['derived'] for r in (prepared['rows']+repair['rows']) if Path(r['derived']['path'])==driver]")
 return t
derive('run_cold_guarded_2050.py','run_cold_guarded_2054.py',fix_cold)
O.mkdir();rows=[]
for p,new,before,after in drafts:
 safe(new)
 with new.open('x',encoding='utf8') as f:f.write(after)
 diff=O/(new.name+'.diff')
 with diff.open('x',encoding='utf8') as f:f.writelines(difflib.unified_diff(before.splitlines(True),after.splitlines(True)))
 rows.append(dict(original=pin(p),derived=pin(new),diff=pin(diff)))
# Both original files and original closed-process failure records stay available.
for p in [S/'stage-plan.json',B/'vm-stage-selection-2050.json']:
 safe(p);archive=O/(p.name+'.original')
 with archive.open('xb') as f:f.write(p.read_bytes())
 assert archive.read_bytes()==p.read_bytes()
save(S/'qualification-stage2054.json',dict(tag=plan['tag'],vm_id=plan['vm_id'],**metadata))
for p,value in [(S/'stage-plan.json',fixed)]:
 safe(p)
 with p.open('w',encoding='utf8',newline='\n') as f:json.dump(value,f,indent=2)
reselected=copy.deepcopy(selection);reselected['plan']=pin(S/'stage-plan.json')
reselected['metadata_repair']=dict(record=str(O/'result.json'),failed_original=pin(O/'stage-plan.json.original'),qualification=pin(S/'qualification-stage2054.json'))
with (B/'vm-stage-selection-2050.json').open('w',encoding='utf8',newline='\n') as f:json.dump(reselected,f,indent=2)
for row in immutable:assert all(pin(Path(row['path']))[k]==row[k] for k in ('path','bytes','sha256'))
save(O/'result.json',dict(status='STRICT_PLAN_METADATA_SEPARATED_ALL_RUNTIME_GUARDS_UNCHANGED',tag=plan['tag'],source_digest=prepared['source_digest'],
 original_plan=pin(O/'stage-plan.json.original'),original_selection=pin(O/'vm-stage-selection-2050.json.original'),corrected_plan=pin(S/'stage-plan.json'),corrected_selection=pin(B/'vm-stage-selection-2050.json'),qualification=pin(S/'qualification-stage2054.json'),
 removed_keys=list(keys),execution_plan_keys_unchanged=True,all_other_plan_values_identical=True,immutable_files=immutable,rows=rows,
 prior_pipeline_failure_preserved=True,original_failed_exit_not_inferred=True,vm_calls=0,credential_reads=0,policy_changes=0,canonical_source_edits=0,release_eligible=False))
print('STRICT_PLAN_FIXED_RUNTIME_GUARDS_AND_FAILED_EVIDENCE_PRESERVED',flush=True)
