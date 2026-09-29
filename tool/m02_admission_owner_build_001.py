"""Local M02 control-slice build/run evidence. Only new outputs; no network or accounts."""
import datetime, hashlib, json, os, pathlib, subprocess, sys, time
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m02-admission-owner-002'
TC=pathlib.Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
TARGET=ROOT/'target/m02-admission-owner-002'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot():
    files=[]
    for root in [ROOT/'native_session_owner_002',ROOT/'contracts/experimental/agent_host_v2_capnp',ROOT/'core']:
        files += [p for p in root.rglob('*') if p.is_file() and not any(x in p.relative_to(root).parts for x in ['target','.git'])]
    files.append(pathlib.Path(__file__))
    return {str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in sorted(files)}
def frozen():
    kit=ROOT/'reports/codex-morrow-v1.1/host/host-kit-003';m=json.loads((kit/'manifest.json').read_text())
    assert sha(kit/'manifest.json')=='5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01'
    for base,rows in [(kit,m['files']),(ROOT/'contracts/experimental/agent_host_v1',m['source_files'])]:
        for r in rows: assert sha(base/r['path'])==r['sha256'],r['path']
    return {'kit_files':len(m['files']),'canonical_files':len(m['source_files']),'matches':True}
def main():
    run=BASE/('run-'+datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'));run.mkdir()
    before=snapshot();result={'status':'failed','before':before,'frozen_before':frozen(),'commands':[]}
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATH','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','SYSTEMDRIVE','LIB','LIBPATH','INCLUDE','VCTOOLSINSTALLDIR','VCTOOLSVERSION','VSINSTALLDIR','VCINSTALLDIR','WINDOWSSDKDIR','WINDOWSSDKVERSION','UNIVERSALCRTSDKDIR','UCRTVERSION'] if k in os.environ}
    profile=run/'profile';profile.mkdir();tmp=run/'tmp';tmp.mkdir()
    env.update({'HOME':str(profile),'USERPROFILE':str(profile),'APPDATA':str(profile/'AppData/Roaming'),'LOCALAPPDATA':str(profile/'AppData/Local'),'TEMP':str(tmp),'TMP':str(tmp),'CARGO_HOME':r'C:\Users\Administrator\.cargo','CARGO_NET_OFFLINE':'true','RUSTC':str(TC/'rustc.exe'),'RUSTDOC':str(TC/'rustdoc.exe'),'RUSTUP_AUTO_INSTALL':'0','GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':os.devnull,'GIT_CONFIG_SYSTEM':os.devnull})
    env['PATH']=str(TC)+os.pathsep+r'C:\Users\Administrator\capnp-bin'+os.pathsep+env.get('PATH','')
    vcroot=pathlib.Path(r'C:\Program Files\Microsoft Visual Studio\2022\Community\VC')
    version=(vcroot/'Auxiliary/Build/Microsoft.VCToolsVersion.default.txt').read_text().strip()
    vc=vcroot/'Tools/MSVC'/version
    sdk=pathlib.Path(r'C:\Program Files (x86)\Windows Kits\10')
    sdkversion=sorted((sdk/'Lib').iterdir(),key=lambda p:p.name)[-1].name
    env['PATH']=str(vc/'bin/Hostx64/x64')+os.pathsep+env['PATH']
    env['LIB']=';'.join(str(p) for p in [vc/'lib/x64',sdk/'Lib'/sdkversion/'ucrt/x64',sdk/'Lib'/sdkversion/'um/x64'])
    env['INCLUDE']=';'.join(str(p) for p in [vc/'include',sdk/'Include'/sdkversion/'ucrt',sdk/'Include'/sdkversion/'um',sdk/'Include'/sdkversion/'shared'])
    result['build_toolchain']={'msvc':version,'windows_sdk':sdkversion,'environment_keys':sorted(env)}
    def execute(name,argv,timeout=120):
        started=time.monotonic()
        with (run/(name+'.stdout')).open('wb') as out,(run/(name+'.stderr')).open('wb') as err:
            p=subprocess.run([str(v) for v in argv],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err,timeout=timeout)
        result['commands'].append({'name':name,'argv':[str(v) for v in argv],'exit_code':p.returncode,'seconds':time.monotonic()-started,'stdout_sha256':sha(run/(name+'.stdout')),'stderr_sha256':sha(run/(name+'.stderr'))})
        if p.returncode: raise RuntimeError(name+' failed')
    try:
        execute('build',[TC/'cargo.exe','build','--offline','--message-format=json-render-diagnostics','--manifest-path',ROOT/'native_session_owner_002/Cargo.toml','--target-dir',TARGET,'--bins'])
        host=TARGET/'debug/morrow-native-owner-host.exe'
        result['artifacts']={str(host.relative_to(ROOT)).replace(chr(92),'/'):sha(host)}
        result['after']=snapshot();result['frozen_after']=frozen();assert before==result['after']
        result['status']='built_not_runtime_qualified'
    except Exception as e: result['error']=repr(e)
    (run/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':result['status'],'path':str(run/'result.json'),'error':result.get('error')}))
    return 0 if result['status'].startswith('built') else 1
if __name__=='__main__':sys.exit(main())

