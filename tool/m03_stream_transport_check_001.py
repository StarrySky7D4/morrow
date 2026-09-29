"""Offline build and loopback-only tests for the new transport slice, with pinned inputs."""
import datetime,hashlib,json,os,pathlib,subprocess,sys,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
TC=pathlib.Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
CRATE=ROOT/'network_node_stream_001'
BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
TARGET=ROOT/'target/m03-stream-001-transport'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot(roots):
    return {str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for root in roots for p in sorted(root.rglob('*')) if p.is_file() and not any(x in p.relative_to(root).parts for x in ['target','.git'])}
def main():
    run=BASE/('transport-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));run.mkdir()
    frozen_roots=[ROOT/p for p in ['network_node','core','native_session','native_session_owner_002','contracts/experimental/agent_host_v1','contracts/experimental/agent_host_v2_capnp','reports/codex-morrow-v1.1/host/host-kit-003','reports/codex-morrow-v1.1/host/m02-native-session-001','reports/codex-morrow-v1.1/host/m02-admission-owner-002','reports/codex-morrow-v1.1/joint/m02-native-session-001','reports/codex-morrow-v1.1/joint/m02-admission-owner-002']]
    before=snapshot([CRATE]);old=snapshot(frozen_roots)
    result={'status':'failed','before':before,'old_before':old,'commands':[],'scope':'producer transport only; no native IPC/Core success claim'}
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATH','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','SYSTEMDRIVE'] if k in os.environ}
    tmp=run/'tmp';tmp.mkdir();profile=run/'profile';profile.mkdir()
    env.update({'USERPROFILE':str(profile),'APPDATA':str(profile/'AppData/Roaming'),'LOCALAPPDATA':str(profile/'AppData/Local'),'TEMP':str(tmp),'TMP':str(tmp),'CARGO_HOME':r'C:\Users\Administrator\.cargo','CARGO_NET_OFFLINE':'true','RUSTC':str(TC/'rustc.exe'),'RUSTDOC':str(TC/'rustdoc.exe'),'RUSTUP_AUTO_INSTALL':'0'})
    vcroot=pathlib.Path(r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC');version=(vcroot/'Auxiliary/Build/Microsoft.VCToolsVersion.default.txt').read_text().strip();vc=vcroot/'Tools/MSVC'/version
    sdk=pathlib.Path(r'C:\Program Files (x86)\Windows Kits\10');sdkversion=sorted((sdk/'Lib').iterdir(),key=lambda p:p.name)[-1].name
    env['PATH']=str(TC)+os.pathsep+str(vc/'bin/Hostx64/x64')+os.pathsep+env.get('PATH','')
    env['LIB']=';'.join(str(p) for p in [vc/'lib/x64',sdk/'Lib'/sdkversion/'ucrt/x64',sdk/'Lib'/sdkversion/'um/x64'])
    env['INCLUDE']=';'.join(str(p) for p in [vc/'include',sdk/'Include'/sdkversion/'ucrt',sdk/'Include'/sdkversion/'um',sdk/'Include'/sdkversion/'shared'])
    result['toolchain']={'rustc':subprocess.check_output([str(TC/'rustc.exe'),'--version'],env=env,text=True).strip(),'msvc':version,'sdk':sdkversion,'environment_keys':sorted(env)}
    def execute(name,args):
        start=time.monotonic()
        with (run/(name+'.stdout')).open('wb') as out,(run/(name+'.stderr')).open('wb') as err:
            p=subprocess.run([str(TC/'cargo.exe'),*args],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err,timeout=600)
        result['commands'].append({'name':name,'args':args,'exit_code':p.returncode,'seconds':time.monotonic()-start,'stdout_sha256':sha(run/(name+'.stdout')),'stderr_sha256':sha(run/(name+'.stderr'))})
        if p.returncode: raise RuntimeError(name+' failed')
    try:
        execute('fmt',['fmt','--manifest-path',str(CRATE/'Cargo.toml'),'--','--check'])
        execute('tests',['test','--locked','--offline','--manifest-path',str(CRATE/'Cargo.toml'),'--target-dir',str(TARGET),'--','--nocapture'])
        result['after']=snapshot([CRATE]);result['old_after']=snapshot(frozen_roots)
        assert before==result['after'];assert old==result['old_after']
        result['test_artifacts']={str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in (TARGET/'debug/deps').glob('*.exe')}
        result['status']='transport_producer_tests_passed'
    except Exception as e:
        result['error']=repr(e)
        result['after']=snapshot([CRATE]);result['old_after']=snapshot(frozen_roots)
    (run/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':result['status'],'path':str(run/'result.json'),'error':result.get('error')}))
    return 0 if result['status']=='transport_producer_tests_passed' else 1
if __name__=='__main__':sys.exit(main())
