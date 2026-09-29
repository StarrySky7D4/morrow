"""Offline isolated consumer build; only copies pinned public registry archives."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tomllib
sys.dont_write_bytecode=True
from check_review import HERE,sha,write
from review_capnp_kit import KIT,authority

def main():
    out=HERE/'runs/wire-consumer-build-001';out.mkdir(exist_ok=False)
    result={'scope':'independent_new_wire_consumer_only','real_host_client_ipc':False,'product_pass_credit':0,'commands':[]}
    try:
        before=authority();write(out/'identities-before.json',before)
        shutil.copytree(KIT/'wire',out/'wire')
        lock=tomllib.loads((out/'wire/Cargo.lock').read_text())
        vendor=out/'vendor';vendor.mkdir()
        archives=[]
        for package in lock['package']:
            if 'source' not in package:continue
            if package['source']!='registry+https://github.com/rust-lang/crates.io-index':raise ValueError('unexpected source')
            name=f"{package['name']}-{package['version']}"
            matches=list(Path('C:/Users/Administrator/.cargo/registry/cache').glob(f'*/{name}.crate'))
            archive=next((p for p in matches if sha(p)==package['checksum']),None)
            if archive is None:raise ValueError('missing exact cached archive:'+name)
            with tarfile.open(archive,'r:gz') as tf:
                for entry in tf.getmembers():
                    path=(vendor/entry.name).resolve()
                    if not path.is_relative_to(vendor) or not (entry.isfile() or entry.isdir()):raise ValueError('unsafe archive member')
                    if entry.isdir():path.mkdir(parents=True,exist_ok=True)
                    else:
                        path.parent.mkdir(parents=True,exist_ok=True)
                        path.write_bytes(tf.extractfile(entry).read())
            root=vendor/name
            sums={p.relative_to(root).as_posix():sha(p) for p in root.rglob('*') if p.is_file() and p.name!='.cargo-checksum.json'}
            (root/'.cargo-checksum.json').write_text(json.dumps({'package':package['checksum'],'files':sums}),encoding='utf-8')
            archives.append({'name':name,'source_archive':str(archive),'sha256':sha(archive)})
        write(out/'registry-archives.json',archives)
        home=out/'cargo-home';home.mkdir()
        (home/'config.toml').write_text('[source.crates-io]\nreplace-with="pinned"\n[source.pinned]\ndirectory="'+vendor.as_posix()+'"\n[net]\noffline=true\n',encoding='utf-8')
        toolchain=Path('C:/Users/Administrator/.rustup/toolchains/1.97.1-x86_64-pc-windows-msvc/bin')
        profile=out/'profile';profile.mkdir();tmp=out/'tmp';tmp.mkdir()
        env={k:os.environ[k] for k in ['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PATH','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS'] if k in os.environ}
        env.update(PATH=str(toolchain)+';C:/Users/Administrator/capnp-bin;'+env.get('PATH',''),CARGO_HOME=str(home),
          CARGO_TARGET_DIR=str(HERE/'t-wire-001'),RUSTC=str(toolchain/'rustc.exe'),RUSTDOC=str(toolchain/'rustdoc.exe'),
          HOME=str(profile),USERPROFILE=str(profile),APPDATA=str(profile),LOCALAPPDATA=str(profile),TEMP=str(tmp),TMP=str(tmp),
          GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='NUL',GIT_CONFIG_SYSTEM='NUL')
        result['compiler']={'cargo':sha(toolchain/'cargo.exe'),'rustc':sha(toolchain/'rustc.exe')}
        project=HERE/'wire-consumer-001'
        for name,args in [('lock',[str(toolchain/'cargo.exe'),'generate-lockfile','--offline']),
                          ('build',[str(toolchain/'cargo.exe'),'build','--offline','--locked','--message-format=json']),
                          ('run',[str(HERE/'t-wire-001/debug/joint-m02-wire-consumer.exe'),str(KIT/'vectors')])]:
            print('Running '+name,flush=True)
            proc=subprocess.run(args,cwd=project,env=env,capture_output=True,timeout=180)
            (out/f'{name}.stdout').write_bytes(proc.stdout);(out/f'{name}.stderr').write_bytes(proc.stderr)
            result['commands'].append({'name':name,'argv':args,'exit_code':proc.returncode,'stdout_sha256':hashlib.sha256(proc.stdout).hexdigest(),'stderr_sha256':hashlib.sha256(proc.stderr).hexdigest()})
            if proc.returncode:raise RuntimeError(name+' failed')
        generated=list((HERE/'t-wire-001/debug/build').glob('morrow-native-session-wire-*/out/native_session_capnp.rs'))
        if len(generated)!=1 or sha(generated[0])!=sha(KIT/'native_session_capnp.rs'):raise ValueError('independent generated binding differs')
        result['generated_binding_sha256']=sha(generated[0])
        result['consumer_exe_sha256']=sha(HERE/'t-wire-001/debug/joint-m02-wire-consumer.exe')
        result['lock_sha256']=sha(project/'Cargo.lock')
        after=authority();write(out/'identities-after.json',after)
        if before!=after:raise ValueError('authority changed')
        result.update(status='verified_limited',exit_code=0,registry_archives=len(archives),depth_threshold_exercised=False)
    except Exception as exc:result.update(status='failed',exit_code=1,error=type(exc).__name__,message=str(exc))
    write(out/'result.json',result);print(json.dumps(result,ensure_ascii=False,indent=2));return result['exit_code']

if __name__=='__main__':raise SystemExit(main())
