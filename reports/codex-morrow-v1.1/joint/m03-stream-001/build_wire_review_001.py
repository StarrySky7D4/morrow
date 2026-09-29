"""Offline isolated codec build and targeted reviewer tests; writes this new batch only."""
import hashlib,json,os,shutil,struct,subprocess,tarfile,tomllib
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
KIT=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001/wire-kit-001'
RUN=HERE/'wire-independent-001'
def sha(p):
    with open(p,'rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def save(p,x):p.write_text(json.dumps(x,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def jread(p):return json.loads(p.read_text(encoding='utf-8-sig'))
assert not RUN.exists();RUN.mkdir()
result={'status':'started','commands':[],'scope':'codec-only offline independent build','runtime_host_core':False}
try:
    identities=jread(HERE/'wire-structure-review-001/identities.json')['files']
    def pin(label):
        rows=[{'path':x['path'],'sha256':sha(x['path'])} for x in identities]
        assert rows==identities
        save(RUN/f'identities-{label}.json',rows)
    pin('before')
    source=RUN/'source';shutil.copytree(KIT/'source',source)
    shutil.copyfile(HERE/'joint_wire_tests_001.rs',source/'tests/joint.rs')
    result['source_inputs']={p.relative_to(source).as_posix():sha(p) for p in source.rglob('*') if p.is_file()}
    lock=tomllib.loads((source/'Cargo.lock').read_text(encoding='utf-8-sig'))
    vendor=RUN/'vendor';vendor.mkdir();archives=[]
    for pkg in lock['package']:
        if 'source' not in pkg:continue
        assert pkg['source']=='registry+https://github.com/rust-lang/crates.io-index'
        name=f"{pkg['name']}-{pkg['version']}"
        matches=list(Path('C:/Users/Administrator/.cargo/registry/cache').glob(f'*/{name}.crate'))
        archive=next((p for p in matches if sha(p)==pkg['checksum']),None)
        assert archive is not None,name
        with tarfile.open(archive,'r:gz') as tf:
            for member in tf.getmembers():
                target=(vendor/member.name).resolve()
                assert target.is_relative_to(vendor) and (member.isfile() or member.isdir())
                if member.isdir():target.mkdir(parents=True,exist_ok=True)
                else:target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(tf.extractfile(member).read())
        vend=vendor/name
        sums={p.relative_to(vend).as_posix():sha(p) for p in vend.rglob('*') if p.is_file() and p.name!='.cargo-checksum.json'}
        save(vend/'.cargo-checksum.json',{'package':pkg['checksum'],'files':sums})
        archives.append({'name':name,'archive':str(archive),'sha256':sha(archive)})
    save(RUN/'registry-archives.json',archives)
    home=RUN/'cargo-home';home.mkdir()
    (home/'config.toml').write_text('[source.crates-io]\nreplace-with="pinned"\n[source.pinned]\ndirectory="'+vendor.as_posix()+'"\n[net]\noffline=true\n')
    tool=Path('C:/Users/Administrator/.rustup/toolchains/1.95.0-x86_64-pc-windows-msvc/bin')
    capnp=Path('C:/Users/Administrator/capnp-bin/capnp.exe')
    assert sha(capnp)=='c41ed4ec9c9d8ed6f3334fd9a8186d7196d890db2bd540bb15ed5cec8b49ef34'
    profile=RUN/'profile';profile.mkdir();tmp=RUN/'tmp';tmp.mkdir();cases=RUN/'cases';cases.mkdir()
    keys=['SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PATH','PROGRAMFILES','PROGRAMFILES(X86)','PROGRAMW6432','PROGRAMDATA','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','LIB','LIBPATH','INCLUDE','VCTOOLSINSTALLDIR','VCTOOLSVERSION','VSINSTALLDIR','VCINSTALLDIR','WINDOWSSDKDIR','WINDOWSSDKVERSION','UNIVERSALCRTSDKDIR','UCRTVERSION','VSCMD_ARG_TGT_ARCH','VSCMD_ARG_HOST_ARCH']
    env={k:os.environ[k] for k in keys if k in os.environ}
    env.update(PATH=str(tool)+';'+str(capnp.parent)+';'+env.get('PATH',''),CARGO_HOME=str(home),CARGO_TARGET_DIR=str(RUN/'target'),RUSTC=str(tool/'rustc.exe'),RUSTDOC=str(tool/'rustdoc.exe'),USERPROFILE=str(profile),APPDATA=str(profile),LOCALAPPDATA=str(profile),TEMP=str(tmp),TMP=str(tmp),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='NUL',GIT_CONFIG_SYSTEM='NUL',JOINT_CASE_DIR=str(cases),JOINT_VECTORS=str(KIT/'vectors'))
    result['tools']={str(p):sha(p) for p in [tool/'cargo.exe',tool/'rustc.exe',capnp]}
    result['environment_keys']=sorted(env)
    hashes=[]
    for limit in [65536,1024]:
        field=lambda x:struct.pack('<I',len(x))+x
        headers=[(b'content-type',b'application/json'),(b'x-raw',bytes([255]))]
        raw=b'Morrow/native-http-proposal/v1\0'+struct.pack('<QQ',1,2)+field(b'op')+struct.pack('<Q',1)+field(b'POST')+field(b'http://127.0.0.1:12345/responses')+struct.pack('<I',2)+b''.join(field(n)+field(v) for n,v in headers)+struct.pack('<I',limit)+field(b'{}')
        (cases/f'preimage-{limit}.bin').write_bytes(raw);hashes.append(hashlib.sha256(raw).hexdigest())
    (cases/'hash-oracle.txt').write_text('\n'.join(hashes)+'\n')
    def command(label,args):
        print('Running '+label,flush=True)
        p=subprocess.run(args,cwd=source,env=env,capture_output=True,timeout=180,creationflags=subprocess.CREATE_NO_WINDOW)
        (RUN/f'{label}.stdout').write_bytes(p.stdout);(RUN/f'{label}.stderr').write_bytes(p.stderr)
        result['commands'].append({'name':label,'args':args,'exit_code':p.returncode,'stdout_sha256':sha(RUN/f'{label}.stdout'),'stderr_sha256':sha(RUN/f'{label}.stderr')})
        assert p.returncode==0,label+' failed'
        return p
    p=command('build',[str(tool/'cargo.exe'),'test','--locked','--offline','--test','joint','--no-run','--message-format=json'])
    messages=[json.loads(line) for line in p.stdout.decode().splitlines() if line.startswith('{')]
    artifacts=[m for m in messages if m.get('reason')=='compiler-artifact']
    save(RUN/'compiler-artifacts.json',artifacts)
    executable=next(Path(m['executable']) for m in artifacts if m['target']['name']=='joint' and m.get('executable'))
    generated=list((RUN/'target/debug/build').glob('morrow-native-http-stream-wire-*/out/native_http_capnp.rs'))
    assert len(generated)==1 and sha(generated[0])=='071955f822b2eab7a803ad2f8d6da50dde4d7371e1c1ef79009b272a4eb842c3'
    p=command('review-tests',[str(executable),'--nocapture','--test-threads=1'])
    assert '7 passed; 0 failed; 0 ignored' in p.stdout.decode()
    assert result['source_inputs']=={p.relative_to(source).as_posix():sha(p) for p in source.rglob('*') if p.is_file()}
    pin('after')
    result.update(status='independent_codec_review_passed',passed=7,registry_archives=len(archives),compiler_artifact_packages=len(set(m['package_id'] for m in artifacts)),executable_sha256=sha(executable),generated_sha256=sha(generated[0]),old_inputs_verified=len(identities),product_pass_credit=0)
except Exception as exc:
    result.update(status='failed',error=type(exc).__name__,message=str(exc))
save(RUN/'result.json',result)
print(json.dumps({k:v for k,v in result.items() if k in ['status','passed','message','registry_archives','compiler_artifact_packages','old_inputs_verified']}),flush=True)
raise SystemExit(0 if result['status']=='independent_codec_review_passed' else 1)
