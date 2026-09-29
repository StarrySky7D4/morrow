"""Local M02 control-slice build/run evidence. Only new outputs; no network or accounts."""
import datetime, hashlib, json, os, pathlib, subprocess, sys, time
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m02-native-session-001'
TC=pathlib.Path(r'C:\Users\Administrator\.rustup\toolchains\1.95.0-x86_64-pc-windows-msvc\bin')
TARGET=ROOT/'target/m02-native-session-001'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def snapshot():
    files=[]
    for root in [ROOT/'native_session',ROOT/'contracts/experimental/agent_host_v2_capnp',ROOT/'core']:
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
    env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATH','PATHEXT','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','LIB','LIBPATH','INCLUDE','VCTOOLSINSTALLDIR','VCTOOLSVERSION','VSINSTALLDIR','VCINSTALLDIR','WINDOWSSDKDIR','WINDOWSSDKVERSION','UNIVERSALCRTSDKDIR','UCRTVERSION'] if k in os.environ}
    profile=run/'profile';profile.mkdir();tmp=run/'tmp';tmp.mkdir()
    env.update({'HOME':str(profile),'USERPROFILE':str(profile),'APPDATA':str(profile/'AppData/Roaming'),'LOCALAPPDATA':str(profile/'AppData/Local'),'TEMP':str(tmp),'TMP':str(tmp),'CARGO_HOME':r'C:\Users\Administrator\.cargo','CARGO_NET_OFFLINE':'true','RUSTC':str(TC/'rustc.exe'),'RUSTDOC':str(TC/'rustdoc.exe'),'RUSTUP_AUTO_INSTALL':'0','GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':os.devnull,'GIT_CONFIG_SYSTEM':os.devnull})
    env['PATH']=str(TC)+os.pathsep+r'C:\Users\Administrator\capnp-bin'+os.pathsep+env.get('PATH','')
    vswhere=pathlib.Path(r'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe')
    install=subprocess.check_output([str(vswhere),'-latest','-products','*','-requires','Microsoft.VisualStudio.Component.VC.Tools.x86.x64','-property','installationPath'],env=env).decode('utf-8-sig').strip()
    script=pathlib.Path(install)/'Common7/Tools/VsDevCmd.bat'
    assert script.is_file() and not any(c in str(script) for c in '\"&|<>^%\r\n')
    cmd='""'+str(script)+'" -no_logo -arch=x64 -host_arch=x64 && set"'
    vc=subprocess.run([env.get('COMSPEC',r'C:\Windows\System32\cmd.exe'),'/d','/s','/c',cmd],env=env,capture_output=True,check=True)
    for line in vc.stdout.decode('utf-8',errors='replace').splitlines():
        key,sep,value=line.partition('=')
        if sep and key.upper() in {'PATH','LIB','LIBPATH','INCLUDE','VCTOOLSINSTALLDIR','VCTOOLSVERSION','VSINSTALLDIR','VCINSTALLDIR','WINDOWSSDKDIR','WINDOWSSDKVERSION','UNIVERSALCRTSDKDIR','UCRTVERSION'}:env[key.upper()]=value
    result['build_environment_keys']=sorted(env)

    def execute(name,argv,timeout=120):
        started=time.monotonic()
        with (run/(name+'.stdout')).open('wb') as out,(run/(name+'.stderr')).open('wb') as err:
            p=subprocess.run([str(v) for v in argv],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=out,stderr=err,timeout=timeout)
        result['commands'].append({'name':name,'argv':[str(v) for v in argv],'exit_code':p.returncode,'seconds':time.monotonic()-started,'stdout_sha256':sha(run/(name+'.stdout')),'stderr_sha256':sha(run/(name+'.stderr'))})
        if p.returncode: raise RuntimeError(name+' failed')
    try:
        execute('wire-test',[TC/'cargo.exe','test','--locked','--offline','--manifest-path',ROOT/'contracts/experimental/agent_host_v2_capnp/Cargo.toml','--target-dir',ROOT/'target/m02-native-session-001-wire'])
        execute('build',[TC/'cargo.exe','build','--locked','--offline','--message-format=json-render-diagnostics','--manifest-path',ROOT/'native_session/Cargo.toml','--target-dir',TARGET,'--bins','--examples'])
        host=TARGET/'debug/morrow-native-session-host.exe';fixture=TARGET/'debug/examples/fixture-client.exe';qualify=TARGET/'debug/examples/qualify.exe'
        result['artifacts']={str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in [host,fixture,qualify]}
        execute('qualify',[qualify,fixture,run/'native-cases'],timeout=45)
        result['qualification_result']=json.loads((run/'native-cases/result.json').read_text())
        cwd=run/'cli-cwd';cwd.mkdir();started=time.monotonic()
        with (run/'cli.stdout').open('wb') as out,(run/'cli.stderr').open('wb') as err:
            process=subprocess.Popen([str(host),'--client',str(fixture),'--sha256',sha(fixture),'--work-dir',str(cwd)],stdin=subprocess.PIPE,stdout=out,stderr=err,cwd=ROOT,env=env)
            try:
                code=process.wait(timeout=5) # DO NOT close host stdin before observing exit.
            except subprocess.TimeoutExpired:
                process.stdin.close();process.wait(timeout=5);raise RuntimeError('host shutdown blocked while control stdin open')
            finally:
                if not process.stdin.closed: process.stdin.close()
        lines=[json.loads(v) for v in (run/'cli.stdout').read_text(encoding='utf-8').splitlines()]
        assert code==0 and lines[-1]['event']=='final' and lines[-1]['snapshot']['phase']=='Released'
        result['cli_stdin_held_open']={'exit_code':code,'seconds':time.monotonic()-started,'stdin_closed_only_after_exit':True}
        result['after']=snapshot();result['frozen_after']=frozen();assert before==result['after']
        result['status']='passed_real_native_control_slice_not_M02_complete'
    except Exception as e: result['error']=repr(e)
    (run/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':result['status'],'path':str(run/'result.json'),'error':result.get('error')}))
    return 0 if result['status'].startswith('passed') else 1
if __name__=='__main__':sys.exit(main())
