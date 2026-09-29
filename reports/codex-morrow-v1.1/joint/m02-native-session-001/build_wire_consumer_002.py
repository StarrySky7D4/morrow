"""New isolated retry with explicit compiler-discovery environment allowlist."""
import json,os,subprocess,sys
from pathlib import Path
sys.dont_write_bytecode=True
from check_review import HERE,sha,write
from review_capnp_kit import KIT,authority

def main():
    out=HERE/'runs/wire-consumer-build-002';out.mkdir(exist_ok=False)
    prior=HERE/'runs/wire-consumer-build-001'
    result={'scope':'independent_new_wire_consumer_only','prior_failure_retained':str(prior/'result.json'),
            'real_host_client_ipc':False,'product_pass_credit':0,'commands':[]}
    try:
        before=authority();write(out/'identities-before.json',before)
        for rel in ['Cargo.toml','Cargo.lock','native_session.capnp','build.rs','src/lib.rs','examples/vectors.rs','README.md']:
            if sha(prior/'wire'/rel)!=sha(KIT/'wire'/rel):raise ValueError('consumer copy changed:'+rel)
        home=out/'cargo-home';home.mkdir()
        (home/'config.toml').write_text('[source.crates-io]\nreplace-with="pinned"\n[source.pinned]\ndirectory="'+(prior/'vendor').as_posix()+'"\n[net]\noffline=true\n',encoding='utf-8')
        toolchain=Path('C:/Users/Administrator/.rustup/toolchains/1.97.1-x86_64-pc-windows-msvc/bin')
        profile=out/'profile';profile.mkdir();tmp=out/'tmp';tmp.mkdir()
        keys=['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PATH','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','LIB','LIBPATH','INCLUDE','VCTOOLSINSTALLDIR','VCTOOLSVERSION','VSINSTALLDIR','VCINSTALLDIR','WINDOWSSDKDIR','WINDOWSSDKVERSION','UNIVERSALCRTSDKDIR','UCRTVERSION','VSCMD_ARG_TGT_ARCH','VSCMD_ARG_HOST_ARCH']
        env={k:os.environ[k] for k in keys if k in os.environ}
        env.update(PATH=str(toolchain)+';C:/Users/Administrator/capnp-bin;'+env.get('PATH',''),CARGO_HOME=str(home),
          CARGO_TARGET_DIR=str(HERE/'t-wire-002'),RUSTC=str(toolchain/'rustc.exe'),RUSTDOC=str(toolchain/'rustdoc.exe'),
          HOME=str(profile),USERPROFILE=str(profile),APPDATA=str(profile),LOCALAPPDATA=str(profile),TEMP=str(tmp),TMP=str(tmp),
          GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='NUL',GIT_CONFIG_SYSTEM='NUL')
        result['environment_keys']=sorted(env);result['environment_values_enumerated']=False
        result['compiler']={'cargo':sha(toolchain/'cargo.exe'),'rustc':sha(toolchain/'rustc.exe')}
        project=HERE/'wire-consumer-001'
        result['consumer_sources']={p.relative_to(project).as_posix():sha(p) for p in project.rglob('*') if p.is_file()}
        for name,args in [('build',[str(toolchain/'cargo.exe'),'build','--offline','--locked','--message-format=json']),
                          ('run',[str(HERE/'t-wire-002/debug/joint-m02-wire-consumer.exe'),str(KIT/'vectors')])]:
            print('Running '+name,flush=True)
            proc=subprocess.run(args,cwd=project,env=env,capture_output=True,timeout=180)
            (out/f'{name}.stdout').write_bytes(proc.stdout);(out/f'{name}.stderr').write_bytes(proc.stderr)
            result['commands'].append({'name':name,'argv':args,'exit_code':proc.returncode,'stdout_sha256':sha(out/f'{name}.stdout'),'stderr_sha256':sha(out/f'{name}.stderr')})
            if proc.returncode:raise RuntimeError(name+' failed')
        generated=list((HERE/'t-wire-002/debug/build').glob('morrow-native-session-wire-*/out/native_session_capnp.rs'))
        if len(generated)!=1 or sha(generated[0])!=sha(KIT/'native_session_capnp.rs'):raise ValueError('independent generated binding differs')
        result['generated_binding_sha256']=sha(generated[0])
        result['consumer_exe_sha256']=sha(HERE/'t-wire-002/debug/joint-m02-wire-consumer.exe')
        after=authority();write(out/'identities-after.json',after)
        if before!=after:raise ValueError('authority changed')
        result.update(status='verified_limited',exit_code=0,depth_threshold_exercised=False)
    except Exception as exc:result.update(status='failed',exit_code=1,error=type(exc).__name__,message=str(exc))
    write(out/'result.json',result);print(json.dumps(result,ensure_ascii=False,indent=2));return result['exit_code']

if __name__=='__main__':raise SystemExit(main())
