"""Offline isolated build/tests for the separate replay peer; never a host fixture."""
from pathlib import Path
import argparse, datetime, hashlib, json, os, subprocess, time, uuid

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
SOURCE = ROOT / 'native/m02-admission-owner-002'
OUT = ROOT / 'out/m02-admission-owner-002'
RUST = Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def snapshot():
    paths = [*SOURCE.rglob('*'), *HERE.glob('*.py'), *HERE.glob('*.ps1'), OUT/'cargo-home/config.toml']
    return {p.relative_to(ROOT).as_posix():sha(p) for p in paths if p.is_file()}
def frozen():
    manifest = ROOT/'receipts/m02-native-session-001/handoff.json'
    assert sha(manifest) == '2ec516c150da95bf592b07353fe6c8b75917884f39e92c99675880cd10a9d682'
    inputs = {str(manifest): sha(manifest)}
    data = read(manifest)
    for name, digest in data['input_sha256'].items(): inputs[str(ROOT/name)] = digest
    inputs.update(data['external_input_sha256'])
    for name, digest in data['old_frozen_handoffs'].items():
        old = ROOT/name
        assert sha(old) == digest
        inputs[str(old)] = digest
        for item, wanted in read(old)['input_sha256'].items(): inputs[str(ROOT/item)] = wanted
    for name, digest in inputs.items(): assert sha(Path(name)) == digest, name
    return inputs
def environment(run):
    keys = ['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS']
    env = {k:os.environ[k] for k in keys if k in os.environ}
    profile = OUT/'isolated-profile'; temp = run/'tmp'
    for p in [profile, profile/'AppData/Local', profile/'AppData/Roaming', temp]: p.mkdir(parents=True, exist_ok=True)
    env.update(HOME=str(profile),USERPROFILE=str(profile),APPDATA=str(profile/'AppData/Roaming'),LOCALAPPDATA=str(profile/'AppData/Local'),TEMP=str(temp),TMP=str(temp),CARGO_HOME=str(OUT/'cargo-home'),CARGO_TARGET_DIR=str(OUT/'target'),CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',CARGO_TERM_COLOR='never',RUSTC=str(RUST/'rustc.exe'),RUSTDOC=str(RUST/'rustdoc.exe'),RUSTUP_AUTO_INSTALL='0',GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_SYSTEM=os.devnull,GIT_CONFIG_GLOBAL=os.devnull,GIT_TERMINAL_PROMPT='0')
    windows = Path(env['SYSTEMROOT'])
    env['PATH'] = os.pathsep.join(map(str,[RUST,Path(r'C:\Users\Administrator\capnp-bin'),windows/'System32',windows]))
    return env
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage',choices=['lock','build','test','offline-cli'])
    args=parser.parse_args()
    tag=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    run=HERE/'runs'/(args.stage+'-'+tag); run.mkdir(parents=True,exist_ok=False)
    result={'stage':args.stage,'status':'failed','commands':[],'qualification':'local peer checks, not host runtime or approval evidence'}
    before=None
    try:
        for p in [Path('C:/.cargo/config'),Path('C:/.cargo/config.toml')]: assert not p.exists(),'unexpected root Cargo config'
        result['frozen_before']=frozen(); before=snapshot(); env=environment(OUT/'runs'/tag)
        result['environment_keys']=sorted(env)
        manifest=['--manifest-path',str(SOURCE/'Cargo.toml')]
        cargo=[str(RUST/'cargo.exe')]; target=['--target','x86_64-pc-windows-msvc','--target-dir',str(OUT/'target')]
        exe=OUT/'target/x86_64-pc-windows-msvc/debug/morrow-session-replay-peer.exe'
        if args.stage=='lock': commands=[(cargo+['generate-lockfile','--offline']+manifest,0)]
        elif args.stage=='build': commands=[(cargo+['build','--locked','--offline','--message-format=json-render-diagnostics']+manifest+target,0)]
        elif args.stage=='test': commands=[(cargo+['test','--locked','--offline','--bin','morrow-session-replay-peer']+manifest+target,0)]
        else:
            result['artifact_sha256']=sha(exe)
            commands=[([str(exe),'--help'],0),([str(exe)],2),([str(exe),'--approved','true'],2),([str(exe),'--morrow-native-session-v2','--scenario','replay-hello','--evidence-dir','C:/missing'],2)]
        for index,(argv,expected) in enumerate(commands):
            started=time.monotonic()
            process=subprocess.Popen(argv,cwd='C:/',env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
            try: stdout,stderr=process.communicate(timeout=300)
            except subprocess.TimeoutExpired:
                # Compiler/test processes are owned here. No user process enumeration.
                process.kill(); stdout,stderr=process.communicate(timeout=30)
                (run/f'{index}.stdout').write_bytes(stdout); (run/f'{index}.stderr').write_bytes(stderr)
                raise RuntimeError('owned direct process timed out; no automatic replay')
            (run/f'{index}.stdout').write_bytes(stdout); (run/f'{index}.stderr').write_bytes(stderr)
            result['commands'].append({'argv':argv,'pid':process.pid,'exit_code':process.returncode,'expected_exit_code':expected,'elapsed_seconds':time.monotonic()-started,'stdout_eof':True,'stderr_eof':True,'stdout_sha256':hashlib.sha256(stdout).hexdigest(),'stderr_sha256':hashlib.sha256(stderr).hexdigest()})
            assert process.returncode==expected,'unexpected exit; retained logs'
        if args.stage=='build': result['artifact']={'path':str(exe),'sha256':sha(exe)}
        result['status']='passed_local_'+args.stage
    except Exception as error: result['error']=str(error)
    finally:
        try:
            result['frozen_after']=frozen(); after=snapshot(); result['inputs_before']=before; result['inputs_after']=after
            changed={p for p in set(before or {})|set(after) if (before or {}).get(p)!=after.get(p)}
            allowed={'native/m02-admission-owner-002/Cargo.lock'} if args.stage=='lock' else set()
            assert not changed-allowed,'input mutation during stage'
        except Exception as error: result['status']='failed'; result['verification_error']=str(error)
        (run/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':result['status'],'receipt':str(run/'result.json'),'error':result.get('error')}))
    return 0 if result['status'].startswith('passed_') else 2
if __name__=='__main__': raise SystemExit(main())
