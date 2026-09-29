"""Offline compiler/codec vectors for the new sole v3 contract; no native/HTTP run."""
import datetime,hashlib,json,os,pathlib,subprocess,sys,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
TC=pathlib.Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
CRATE=ROOT/'contracts/experimental/agent_host_v3_http_stream'
BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001';TARGET=ROOT/'target/m03-stream-001-wire'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(roots):return {str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for root in roots for p in sorted(root.rglob('*')) if p.is_file() and not any(x in p.relative_to(root).parts for x in ['target','.git'])}
def main():
    run=BASE/('wire-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));run.mkdir()
    prior=json.loads((BASE/'transport-20260928T210834849150Z/result.json').read_text())
    frozen={**prior['old_before'],**json.loads((BASE/'transport-kit-001/manifest.json').read_text())['inputs']}
    for p,h in frozen.items():assert sha(ROOT/p)==h,p
    before=snapshot([CRATE]);result={'status':'failed','before':before,'commands':[],'frozen_count':len(frozen)}
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATH','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','SYSTEMDRIVE'] if k in os.environ}
    tmp=run/'tmp';tmp.mkdir();profile=run/'profile';profile.mkdir()
    env.update({'USERPROFILE':str(profile),'APPDATA':str(profile/'AppData/Roaming'),'LOCALAPPDATA':str(profile/'AppData/Local'),'TEMP':str(tmp),'TMP':str(tmp),'CARGO_HOME':r'C:\Users\Administrator\.cargo','CARGO_NET_OFFLINE':'true','RUSTC':str(TC/'rustc.exe'),'RUSTDOC':str(TC/'rustdoc.exe'),'RUSTUP_AUTO_INSTALL':'0'})
    vcroot=pathlib.Path(r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC');version=(vcroot/'Auxiliary/Build/Microsoft.VCToolsVersion.default.txt').read_text().strip();vc=vcroot/'Tools/MSVC'/version
    sdk=pathlib.Path(r'C:\Program Files (x86)\Windows Kits\10');sdkversion=sorted((sdk/'Lib').iterdir(),key=lambda p:p.name)[-1].name
    env['PATH']=str(TC)+os.pathsep+str(vc/'bin/Hostx64/x64')+os.pathsep+r'C:\Users\Administrator\capnp-bin'+os.pathsep+env.get('PATH','')
    env['LIB']=';'.join(str(p) for p in [vc/'lib/x64',sdk/'Lib'/sdkversion/'ucrt/x64',sdk/'Lib'/sdkversion/'um/x64'])
    env['INCLUDE']=';'.join(str(p) for p in [vc/'include',sdk/'Include'/sdkversion/'ucrt',sdk/'Include'/sdkversion/'um',sdk/'Include'/sdkversion/'shared'])
    result['generator']=subprocess.check_output([r'C:\Users\Administrator\capnp-bin\capnp.exe','--version'],env=env,text=True).strip()
    def execute(name,args,program=TC/'cargo.exe'):
        started=time.monotonic()
        with (run/(name+'.stdout')).open('wb') as out,(run/(name+'.stderr')).open('wb') as err:
            p=subprocess.run([str(program),*[str(a) for a in args]],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err,timeout=180)
        result['commands'].append({'name':name,'args':[str(a) for a in args],'exit_code':p.returncode,'seconds':time.monotonic()-started,'stdout_sha256':sha(run/(name+'.stdout')),'stderr_sha256':sha(run/(name+'.stderr'))})
        if p.returncode:raise RuntimeError(name+' failed')
    try:
        flags=['--locked','--offline','--manifest-path',CRATE/'Cargo.toml','--target-dir',TARGET]
        execute('fmt',['fmt','--manifest-path',CRATE/'Cargo.toml','--','--check'])
        execute('tests',['test',*flags,'--','--nocapture'])
        execute('build',['build',*flags,'--examples','--message-format=json-render-diagnostics'])
        exe=TARGET/'debug/examples/vectors.exe';execute('vectors',[run/'vectors'],exe)
        bindings=list(TARGET.glob('debug/build/morrow-native-http-stream-wire-*/out/native_http_capnp.rs'));assert len(bindings)==1
        result['binding']={'path':str(bindings[0].relative_to(ROOT)).replace('\\','/'),'sha256':sha(bindings[0])};result['vectors_exe_sha256']=sha(exe)
        result['after']=snapshot([CRATE]);assert before==result['after']
        for p,h in frozen.items():assert sha(ROOT/p)==h,p
        result['frozen_unchanged']=True;result['status']='codec_tests_and_vectors_passed_runtime_not_run'
    except Exception as e:result['error']=repr(e)
    (run/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'status':result['status'],'path':str(run/'result.json'),'error':result.get('error')}))
    return 0 if result['status'].startswith('codec_') else 1
if __name__=='__main__':sys.exit(main())
