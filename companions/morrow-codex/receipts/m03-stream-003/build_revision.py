"""Offline native003 checks. Reuses frozen Core002; never launches host/HTTP."""
from pathlib import Path
import argparse,datetime as dt,hashlib,importlib.util,json,subprocess,uuid
ROOT=Path(__file__).resolve().parents[2]
HERE=ROOT/'receipts/m03-stream-003'
OLD=ROOT/'receipts/m03-stream-001'
NATIVE=ROOT/'native/m03-stream-003'
CORE=ROOT/'qualification/m03-stream-002'
OUT=ROOT/'out/m03-native-003'
TARGET=ROOT/'out/m03-stream-001/target'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):return json.loads(p.read_text(encoding='utf-8'))
def frozen():
    result={}
    for name,digest in [('native-candidate-001.json','7d2dc5319e4adeda829c8c80579fc09f56b84ba68eb6d746fbc9f6700b5514e1'),
        ('pre-wire-candidate-001.json','1c0f9a109bd90df4bd7f909387f55b6d7ce87902ba02a68a75e695824a354d57'),
        ('wire-intake-001.json','0699caf2bb4ee290402e1738b9dbabf02fadc73e90db6fd45592d4e5e23b9137'),
        ('pipe-intake-001.json','8b9d4a800616b709afaad897e0f7b38626ae28531eb771825e4342cf80ea6019')]:
        receipt=OLD/name;assert sha(receipt)==digest,name
        for p,h in load(receipt)['input_sha256'].items():assert sha(ROOT/p)==h,p;result[p]=h
    receipt=ROOT/'receipts/m03-stream-002/native-candidate-002.json'
    assert sha(receipt)=='20135ba780b86a344eeb396fd8d8f0ba85ee3600f79308140380c3fc94fbf561'
    for p,h in load(receipt)['input_sha256'].items():assert sha(ROOT/p)==h,p;result[p]=h
    return result
def inputs():
    paths=[*NATIVE.rglob('*'),*CORE.rglob('*'),Path(__file__),OUT/'cargo-home/config.toml']
    return {p.relative_to(ROOT).as_posix():sha(p) for p in paths if p.is_file()}
def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('stage',choices=['lock','build','test']);args=ap.parse_args()
    tag=dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    receipt=HERE/'runs'/(args.stage+'-'+tag);receipt.mkdir(parents=True,exist_ok=False)
    run=OUT/'runs'/tag;run.mkdir(parents=True,exist_ok=False)
    result={'stage':args.stage,'status':'blocked','host_runtime_launched':False,'actual_core_http_invoked':False}
    protected=before=None
    source=NATIVE
    action=args.stage.split('-')[0]
    try:
        protected=frozen()
        home=OUT/'cargo-home';home.mkdir(parents=True,exist_ok=True)
        config=(ROOT/'out/p02-integration-004/cargo-home/config.toml').read_bytes()
        if (home/'config.toml').exists():assert (home/'config.toml').read_bytes()==config
        else:(home/'config.toml').write_bytes(config)
        before=inputs()
        spec=importlib.util.spec_from_file_location('frozen_build_helper',OLD/'build_candidate.py');helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper);helper.OUT=OUT
        env,commands=helper.environment(run);env['CARGO_TARGET_DIR']=str(TARGET)
        argv=[str(helper.TOOLCHAIN/'bin/cargo.exe')]
        if action=='lock':argv+=['metadata','--offline','--format-version','1']
        else:argv+=[action,'--offline','--locked','--target','x86_64-pc-windows-msvc']
        argv+=['--manifest-path',str(source/'Cargo.toml')]
        if action=='build':argv+=['--message-format=json-render-diagnostics']
        if action=='test':argv+=['--lib','--','--test-threads=1']
        result.update(argv=argv,tools=commands,environment_keys=sorted(env))
        print(json.dumps({'stage':args.stage,'receipt':str(receipt),'state':'starting'}),flush=True)
        with (receipt/'stdout.txt').open('wb') as out,(receipt/'stderr.txt').open('wb') as err:
            process=subprocess.Popen(argv,cwd='C:/',env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err);result['pid']=process.pid
            try:code=process.wait(timeout=3600)
            except subprocess.TimeoutExpired:
                subprocess.run([r'C:\Windows\System32\taskkill.exe','/PID',str(process.pid),'/T','/F'],capture_output=True,timeout=30,check=False)
                raise RuntimeError('owned build timeout')
        result['exit_code']=code
        if code:raise RuntimeError('revision stage failed; preserved logs')
        result['status']={'lock':'revision_lock_prepared','build':'revision_native_core_compiled_not_runtime_proof','test':'revision_local_tests_passed'}[action]
    except Exception as error:result['error']=str(error)
    finally:
        try:
            result['old_001_002_and_kits_unchanged']=protected==frozen();assert result['old_001_002_and_kits_unchanged']
            after=inputs();result['inputs_before'],result['inputs_after']=before,after
            changed={p for p in set(before or {})|set(after) if (before or {}).get(p)!=after.get(p)}
            allowed={(source/'Cargo.lock').relative_to(ROOT).as_posix()} if action=='lock' else set()
            assert not changed-allowed,'input changed during stage';result['changed_inputs']=sorted(changed)
        except Exception as error:result['status']='blocked';result['verification_error']=str(error)
        result['log_sha256']={p.name:sha(p) for p in receipt.glob('*.txt')}
        (receipt/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'status':result['status'],'receipt':str(receipt/'result.json'),'error':result.get('error'),'verification_error':result.get('verification_error')}),flush=True)
    return 2 if result['status']=='blocked' else 0
if __name__=='__main__':raise SystemExit(main())
