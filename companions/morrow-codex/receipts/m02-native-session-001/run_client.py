"""Isolated local stages; no provider, personal configuration, or version selection."""
from pathlib import Path
import argparse,datetime,hashlib,json,os,subprocess,time,uuid

ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
SOURCE=ROOT/'native/m02-native-session-001'
OUT=ROOT/'out/m02-native-session-001'
RUST=Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
def snapshot():
    paths=[Path(__file__),OUT/'cargo-home/config.toml',*SOURCE.rglob('*')]
    return {p.relative_to(ROOT).as_posix():sha(p) for p in paths if p.is_file()}
def frozen():
    paths=['receipts/handoff.json','receipts/p02-native-probe-001/handoff.json','receipts/p02-exec-store-002/handoff.json','receipts/p02-core-network-003/handoff.json','receipts/p02-integration-004/handoff.json']
    expected=['995cfa721bae3b94a494a0ee88e9e76f9e29afeec71ee6481c9ac756446d3ca4','1fc5833e632bb3afab827a9ca3519418ab7afdf34c6e50e6cafa59ecce707020','d3c19e26c5c04461eed188ff1d84518add5d8af711c3dfc78e6e49ad8312cd76','97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb','513b1cfbf4b19b32c247b06810430ae29b1e1b779078b9761100a86c7a8d1751']
    result={}
    for name,digest in zip(paths,expected):
        p=ROOT/name;assert sha(p)==digest,name
        for item,want in read(p)['input_sha256'].items():assert sha(ROOT/item)==want,item
        result[name]=digest
    return result
def environment(run):
    env={key:os.environ[key] for key in ['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS'] if key in os.environ}
    profile=OUT/'isolated-profile';temp=run/'tmp'
    for p in [profile,profile/'AppData/Local',profile/'AppData/Roaming',temp]:p.mkdir(parents=True,exist_ok=True)
    env.update(HOME=str(profile),USERPROFILE=str(profile),APPDATA=str(profile/'AppData/Roaming'),LOCALAPPDATA=str(profile/'AppData/Local'),TEMP=str(temp),TMP=str(temp),CARGO_HOME=str(OUT/'cargo-home'),CARGO_TARGET_DIR=str(OUT/'target'),CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',CARGO_TERM_COLOR='never',RUSTC=str(RUST/'rustc.exe'),RUSTDOC=str(RUST/'rustdoc.exe'),RUSTUP_AUTO_INSTALL='0',GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_SYSTEM=os.devnull,GIT_CONFIG_GLOBAL=os.devnull,GIT_TERMINAL_PROMPT='0')
    windows=Path(env['SYSTEMROOT'])
    env['PATH']=os.pathsep.join(map(str,[RUST,Path(r'C:\Users\Administrator\capnp-bin'),windows/'System32',windows]))
    return env
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('stage',choices=['lock','build','test','offline-cli']);args=parser.parse_args()
    name=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    run=HERE/'runs'/(args.stage+'-'+name);run.mkdir(parents=True,exist_ok=False)
    result={'stage':args.stage,'status':'failed','commands':[],'qualification':'bootstrap local only, not IPC evidence'}
    before=None
    try:
        for p in [Path('C:/.cargo/config'),Path('C:/.cargo/config.toml')]:assert not p.exists(),'unexpected root Cargo configuration'
        result['frozen_before']=frozen();before=snapshot();env=environment(OUT/'runs'/name)
        result['environment_keys']=sorted(env)
        manifest=['--manifest-path',str(SOURCE/'Cargo.toml')]
        cargo=[str(RUST/'cargo.exe')];target=['--target','x86_64-pc-windows-msvc','--target-dir',str(OUT/'target')]
        exe=OUT/'target/x86_64-pc-windows-msvc/debug/morrow-session-client.exe'
        if args.stage=='lock':commands=[(cargo+['generate-lockfile','--offline']+manifest,0)]
        elif args.stage=='build':commands=[(cargo+['build','--locked','--offline','--message-format=json-render-diagnostics']+manifest+target,0)]
        elif args.stage=='test':commands=[(cargo+['test','--locked','--offline','--lib']+manifest+target,0)]
        else:
            result['artifact_sha256']=sha(exe)
            commands=[([str(exe),'--help'],0),([str(exe),'--version'],0),([str(exe),'--check'],0),([str(exe)],2),([str(exe),'--approved'],2),([str(exe),'run','cmd.exe'],2)]
        for index,(argv,expected) in enumerate(commands):
            started=time.monotonic()
            process=subprocess.Popen(argv,cwd='C:/',env=env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:stdout,stderr=process.communicate(timeout=300)
            except subprocess.TimeoutExpired:
                subprocess.run([str(Path(env['SYSTEMROOT'])/'System32/taskkill.exe'),'/PID',str(process.pid),'/T','/F'],capture_output=True,check=False,timeout=30)
                raise RuntimeError('owned process timed out; no replay')
            (run/f'{index}.stdout').write_bytes(stdout);(run/f'{index}.stderr').write_bytes(stderr)
            record={'argv':argv,'pid':process.pid,'exit_code':process.returncode,'expected_exit_code':expected,'elapsed_seconds':time.monotonic()-started,'stdout_eof':True,'stderr_eof':True,'stdout_sha256':hashlib.sha256(stdout).hexdigest(),'stderr_sha256':hashlib.sha256(stderr).hexdigest()}
            result['commands'].append(record)
            assert process.returncode==expected,'unexpected process exit; see retained output'
        if args.stage=='build':result['artifact']={'path':str(exe),'sha256':sha(exe)}
        result['status']='passed_bootstrap_'+args.stage
    except Exception as error:result['error']=str(error)
    finally:
        try:
            result['frozen_after']=frozen();after=snapshot();result['inputs_before']=before;result['inputs_after']=after
            changed={p for p in set(before or {})|set(after) if (before or {}).get(p)!=after.get(p)}
            allowed={'native/m02-native-session-001/Cargo.lock'} if args.stage=='lock' else set()
            assert not changed-allowed,'input mutation during stage'
        except Exception as error:result['status']='failed';result['verification_error']=str(error)
        (run/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':result['status'],'receipt':str(run/'result.json'),'error':result.get('error')}))
    return 0 if result['status'].startswith('passed_') else 2
if __name__=='__main__':raise SystemExit(main())
