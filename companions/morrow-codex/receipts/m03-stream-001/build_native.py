"""Build the new native driver only. No host/HTTP/IPC execution in this runner."""
from pathlib import Path
import argparse, datetime as dt, hashlib, importlib.util, json, os, shutil, subprocess, uuid
ROOT=Path(__file__).resolve().parents[2]
HERE=ROOT/'receipts/m03-stream-001'
SOURCE=ROOT/'native/m03-stream-001'
OUT=ROOT/'out/m03-native-001'
# Mutable Cargo dependency cache only. Sealed pre-wire artifacts are independent
# copies under candidates/pre-wire-001, checked before/after every new build.
TARGET=ROOT/'out/m03-stream-001/target'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):return json.loads(p.read_text(encoding='utf-8'))
def frozen():
    result={}
    for filename,expected in [('pre-wire-candidate-001.json','1c0f9a109bd90df4bd7f909387f55b6d7ce87902ba02a68a75e695824a354d57'),
        ('wire-intake-001.json','0699caf2bb4ee290402e1738b9dbabf02fadc73e90db6fd45592d4e5e23b9137'),
        ('pipe-intake-001.json','8b9d4a800616b709afaad897e0f7b38626ae28531eb771825e4342cf80ea6019')]:
        path=HERE/filename
        assert sha(path)==expected,filename
        for name,wanted in load(path)['input_sha256'].items():
            assert sha(ROOT/name)==wanted,name
            result[name]=wanted
    return result
def inputs():
    paths=[*SOURCE.rglob('*'),Path(__file__),OUT/'cargo-home/config.toml']
    return {p.relative_to(ROOT).as_posix():sha(p) for p in paths if p.is_file()}
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('stage',choices=['lock','build','test']);args=parser.parse_args()
    tag=dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    receipt=HERE/'native-runs'/(args.stage+'-'+tag);receipt.mkdir(parents=True,exist_ok=False)
    run=OUT/'runs'/tag;run.mkdir(parents=True,exist_ok=False)
    result={'stage':args.stage,'status':'blocked','native_runtime_run':False,'host_or_http_launched':False,'target_cache':str(TARGET)}
    before=protected=None
    try:
        protected=frozen()
        spec=importlib.util.spec_from_file_location('frozen_pre_wire_build',HERE/'build_candidate.py')
        helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper);helper.OUT=OUT
        home=OUT/'cargo-home';home.mkdir(parents=True,exist_ok=True)
        config=(ROOT/'out/p02-integration-004/cargo-home/config.toml').read_bytes()
        if (home/'config.toml').exists():assert (home/'config.toml').read_bytes()==config
        else:(home/'config.toml').write_bytes(config)
        if args.stage=='lock' and not (SOURCE/'Cargo.lock').exists():shutil.copyfile(ROOT/'qualification/m03-stream-001/Cargo.lock',SOURCE/'Cargo.lock')
        before=inputs();env,commands=helper.environment(run);env['CARGO_TARGET_DIR']=str(TARGET)
        result['tools']=commands
        cargo=[str(helper.TOOLCHAIN/'bin/cargo.exe')]
        manifest=['--manifest-path',str(SOURCE/'Cargo.toml')]
        if args.stage=='lock':argv=cargo+['metadata','--offline','--format-version','1']+manifest
        else:argv=cargo+[args.stage,'--offline','--locked','--target','x86_64-pc-windows-msvc']+manifest+(['--message-format=json-render-diagnostics'] if args.stage=='build' else ['--lib','--','--test-threads=1'])
        result.update(argv=argv,environment_keys=sorted(env))
        print(json.dumps({'stage':args.stage,'receipt':str(receipt),'state':'starting'}),flush=True)
        with (receipt/'stdout.txt').open('wb') as out,(receipt/'stderr.txt').open('wb') as err:
            process=subprocess.Popen(argv,cwd='C:/',env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err)
            result['pid']=process.pid
            try:code=process.wait(timeout=3600)
            except subprocess.TimeoutExpired:
                subprocess.run([r'C:\Windows\System32\taskkill.exe','/PID',str(process.pid),'/T','/F'],capture_output=True,timeout=30,check=False)
                raise RuntimeError('owned native build exceeded bounded timeout')
        result['exit_code']=code
        if code:raise RuntimeError('native stage failed; logs retained')
        result['status']={'lock':'native_lock_prepared','build':'native_compiled_not_runtime_proof','test':'native_local_unit_passed'}[args.stage]
    except Exception as e:result['error']=str(e)
    finally:
        try:
            result['protected_unchanged']=frozen()==protected
            assert result['protected_unchanged']
            result['inputs_before'],result['inputs_after']=before,inputs()
            changed={p for p in set(before or{})|set(result['inputs_after']) if (before or{}).get(p)!=result['inputs_after'].get(p)}
            result['changed_inputs']=sorted(changed)
            assert not changed-({'native/m03-stream-001/Cargo.lock'} if args.stage=='lock' else set()),'changed input during native stage'
        except Exception as e:result['status']='blocked';result['verification_error']=str(e)
        result['log_sha256']={p.name:sha(p) for p in receipt.glob('*.txt')}
        (receipt/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
        print(json.dumps({'status':result['status'],'receipt':str(receipt/'result.json'),'error':result.get('error'),'verification_error':result.get('verification_error')}),flush=True)
    return 2 if result['status']=='blocked' else 0
if __name__=='__main__':raise SystemExit(main())
