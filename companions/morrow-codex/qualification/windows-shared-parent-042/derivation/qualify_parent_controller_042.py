"""Compile fixed shared-parent controller and run pure gates only; no VM I/O."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess
from derive_parent_session_controller_042 import patch_controller,SYMBOL,pin

BASE=Path(__file__).parent
OUT=BASE/'parent-controller-042-qualification-002'
STAGE=Path(json.loads((BASE/'vm-stage-selection-041.json').read_bytes())['root'])
OUT.mkdir();SRC=OUT/'source';SRC.mkdir();(OUT/'tmp').mkdir();(OUT/'bin').mkdir()
def save(path,value):
    with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2)
for old in (STAGE/'controller-source').glob('*.cs'):
    shutil.copy2(old,SRC/old.name)
derivation=patch_controller(SRC,OUT)
framework=Path('C:/Windows/Microsoft.NET/Framework64/v4.0.30319')
cmd=[str(framework/'csc.exe'),'/noconfig','/nologo','/utf8output','/codepage:65001','/langversion:5','/platform:x64','/nostdlib+','/optimize+','/debug-']
cmd+=['/reference:'+str(framework/name) for name in ('mscorlib.dll','System.dll','System.Core.dll','System.Security.dll')]
env=dict(os.environ,TEMP=str(OUT/'tmp'),TMP=str(OUT/'tmp'),TMPDIR=str(OUT/'tmp'))
sources=sorted(SRC.glob('*.cs'))
bridge=STAGE/'controller/RootPromptAllQA2187.cs'
rows=[]
for name,entry,symbol,extra,arg,expected in [
    ('original85','RootPromptAllQA2187',None,[bridge],'--all-pure-prompt2187','PURE_PASSED=85'),
    ('single_refusal70','P2SourcePolicyPure039','P2_SOURCE_POLICY_REFUSAL_QUALIFICATION',[],'--p2-refusal-pure','P2_PURE_PASSED=70'),
    ('shared','ParentSessionPure042',SYMBOL,[],'--parent-session-pure','PARENT_SESSION_PURE_PASSED=17'),
    ('controller','Root47Controller1725',SYMBOL,[],None,None),
]:
    output=OUT/'bin'/(name+'.exe')
    argv=cmd+['/target:exe','/out:'+str(output),'/main:C28.RootP1682.'+entry]
    if symbol:argv+=['/define:'+symbol]
    argv+=list(map(str,sources+extra));save(OUT/(name+'-compile-invocation.json'),dict(argv=argv,sources=[pin(p) for p in sources+extra]))
    run=subprocess.run(argv,env=env,capture_output=True,timeout=60)
    (OUT/(name+'-compile.stdout')).write_bytes(run.stdout);(OUT/(name+'-compile.stderr')).write_bytes(run.stderr)
    if run.returncode:
        print(run.stdout.decode('utf-8-sig'));print(run.stderr.decode('utf-8-sig'));raise RuntimeError(name+' compiler failed')
    row=dict(profile=name,artifact=pin(output),compiler_exit=0,runtime='NOT_RUN')
    if arg:
        result=subprocess.run([str(output),arg],env=env,capture_output=True,timeout=60)
        (OUT/(name+'-run.stdout')).write_bytes(result.stdout);(OUT/(name+'-run.stderr')).write_bytes(result.stderr)
        lines=result.stdout.decode('utf-8-sig').splitlines()
        if result.returncode or result.stderr or not lines or lines[-1]!=expected:
            print(result.stdout.decode('utf-8-sig'));print(result.stderr.decode('utf-8-sig'));raise RuntimeError(name+' pure qualification failed')
        row.update(pure_methods=len([line for line in lines if line.startswith('PASS ')]),pure_exit=0)
    rows.append(row);print(json.dumps(row),flush=True)
save(OUT/'result.json',dict(status='PASS_PURE_SHARED_PARENT_CONTROLLER_NOT_RUNTIME',rows=rows,derivation=pin(OUT/'parent-session-controller-derivation.json'),VM_calls=0,product_executions=0,source_accepted=False,release_eligible=False))
