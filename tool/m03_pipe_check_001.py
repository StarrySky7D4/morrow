"""Compile and run only new owned-I/O platform tests with recorded inputs."""
import datetime,hashlib,json,os,pathlib,subprocess,sys,time
ROOT=pathlib.Path(__file__).resolve().parents[1];CRATE=ROOT/'native_pipe_win_001';BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001';TARGET=ROOT/'target/m03-stream-001-pipe'
TC=pathlib.Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot():return {str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted(CRATE.rglob('*')) if p.is_file()}
def main():
    run=BASE/('pipe-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));run.mkdir();before=snapshot()
    old=json.loads((BASE/'transport-20260928T210834849150Z/result.json').read_text())['old_before']
    for path,prefix in [('transport-kit-001','network_node_stream_001'),('wire-kit-001','contracts/experimental/agent_host_v3_http_stream')]:
        m=json.loads((BASE/path/'manifest.json').read_text());old.update({prefix+'/'+p:h for p,h in m['source_files'].items()})
    for p,h in old.items():assert sha(ROOT/p)==h,p
    result={'status':'failed','before':before,'frozen_count':len(old),'commands':[]}
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATH','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','SYSTEMDRIVE'] if k in os.environ}
    tmp=run/'tmp';tmp.mkdir();profile=run/'profile';profile.mkdir()
    env.update({'USERPROFILE':str(profile),'APPDATA':str(profile/'AppData/Roaming'),'LOCALAPPDATA':str(profile/'AppData/Local'),'TEMP':str(tmp),'TMP':str(tmp),'CARGO_HOME':r'C:\Users\Administrator\.cargo','CARGO_NET_OFFLINE':'true','RUSTC':str(TC/'rustc.exe'),'RUSTDOC':str(TC/'rustdoc.exe'),'RUSTUP_AUTO_INSTALL':'0'})
    vcroot=pathlib.Path(r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC');version=(vcroot/'Auxiliary/Build/Microsoft.VCToolsVersion.default.txt').read_text().strip();vc=vcroot/'Tools/MSVC'/version
    sdk=pathlib.Path(r'C:\Program Files (x86)\Windows Kits\10');sdkversion=sorted((sdk/'Lib').iterdir(),key=lambda p:p.name)[-1].name
    env['PATH']=str(TC)+os.pathsep+str(vc/'bin/Hostx64/x64')+os.pathsep+env.get('PATH','')
    env['LIB']=';'.join(str(p) for p in [vc/'lib/x64',sdk/'Lib'/sdkversion/'ucrt/x64',sdk/'Lib'/sdkversion/'um/x64'])
    env['INCLUDE']=';'.join(str(p) for p in [vc/'include',sdk/'Include'/sdkversion/'ucrt',sdk/'Include'/sdkversion/'um',sdk/'Include'/sdkversion/'shared'])
    result['environment_keys']=sorted(env)
    def execute(name,args):
        start=time.monotonic()
        with (run/(name+'.stdout')).open('wb') as out,(run/(name+'.stderr')).open('wb') as err:
            p=subprocess.run([str(TC/'cargo.exe'),*[str(a) for a in args]],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err,timeout=120)
        result['commands'].append({'name':name,'args':[str(a) for a in args],'exit_code':p.returncode,'seconds':time.monotonic()-start,'stdout_sha256':sha(run/(name+'.stdout')),'stderr_sha256':sha(run/(name+'.stderr'))})
        if p.returncode:raise RuntimeError(name+' failed')
    try:
        flags=['--locked','--offline','--manifest-path',CRATE/'Cargo.toml','--target-dir',TARGET]
        execute('fmt',['fmt','--manifest-path',CRATE/'Cargo.toml','--','--check'])
        execute('compile',['test',*flags,'--no-run','--message-format=json-render-diagnostics'])
        execute('tests',['test',*flags,'--','--nocapture','--test-threads=1'])
        result['after']=snapshot();assert before==result['after']
        for p,h in old.items():assert sha(ROOT/p)==h,p
        result['frozen_unchanged']=True;result['artifacts']={str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in (TARGET/'debug/deps').glob('*.exe')};result['status']='platform_producer_tests_passed_native_runtime_not_run'
    except Exception as e:result['error']=repr(e)
    (run/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'status':result['status'],'path':str(run/'result.json'),'error':result.get('error')}))
    return 0 if result['status'].startswith('platform_') else 1
if __name__=='__main__':sys.exit(main())
