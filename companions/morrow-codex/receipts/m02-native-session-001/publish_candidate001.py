from pathlib import Path
import hashlib,json,shutil,subprocess,sys,tomllib
sys.dont_write_bytecode=True
from run_client import ROOT,HERE,SOURCE,OUT,environment,frozen
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
candidate=OUT/'candidates/client-001';candidate.mkdir(parents=True,exist_ok=False)
before=frozen()
build=HERE/'runs/build-20260928T192142Z-8bc832b6/result.json'
test=HERE/'runs/test-20260928T192141Z-db65a8e1/result.json'
cli=HERE/'runs/offline-cli-20260928T192144Z-68ba0663/result.json'
for p in [build,test,cli]:
    r=read(p);assert r['status'].startswith('passed_')
    assert r['inputs_before']==r['inputs_after']
    for name,want in r['inputs_after'].items():assert sha(ROOT/name)==want,name
kit=SOURCE/'vendor/capnp-kit-001';manifest=read(kit/'manifest.json')
assert sha(kit/'manifest.json')=='2a0146a8f18b658b906c6a5f2d0334272a794edf43b0b6ac5757d9ba45cb66d9'
for name,want in manifest['files'].items():assert sha(kit/name)==want
generated=list((OUT/'target/x86_64-pc-windows-msvc/debug/build').glob('morrow-native-session-wire-*/out/native_session_capnp.rs'))
assert generated and all(sha(p)==manifest['files']['native_session_capnp.rs'] for p in generated)
allowed={(p['name'],p['version']):p['checksum'] for p in tomllib.loads((kit/'wire/Cargo.lock').read_text(encoding='utf-8'))['package'] if p.get('source')}
packages=tomllib.loads((SOURCE/'Cargo.lock').read_text(encoding='utf-8'))['package']
for p in packages:
    if p.get('source'):assert allowed.get((p['name'],p['version']))==p['checksum']
original=Path(read(build)['artifact']['path']);assert sha(original)==read(build)['artifact']['sha256']
exe=candidate/'morrow-session-client.exe';shutil.copyfile(original,exe);assert sha(exe)==sha(original)
nohost=subprocess.run([str(exe),'--morrow-native-session-v2'],cwd=candidate,env=environment(candidate/'no-host'),stdin=subprocess.DEVNULL,capture_output=True,timeout=10)
(candidate/'no-host.stdout').write_bytes(nohost.stdout);(candidate/'no-host.stderr').write_bytes(nohost.stderr)
assert nohost.returncode==24 and nohost.stdout==b''
inputs={p.relative_to(ROOT).as_posix():sha(p) for p in SOURCE.rglob('*') if p.is_file()}
for p in [Path(__file__),HERE/'run_client.py',build,test,cli,OUT/'cargo-home/config.toml',*generated,exe]:inputs[p.relative_to(ROOT).as_posix()]=sha(p)
assert frozen()==before
result={'status':'candidate_ready_for_real_host_not_ipc_verified','candidate':'client-001','exe':str(exe),'exe_sha256':sha(exe),'source':str(SOURCE),'host_kit_manifest_sha256':sha(kit/'manifest.json'),'schema_sha256':manifest['schema_sha256'],'generated_bindings_sha256':manifest['files']['native_session_capnp.rs'],'packages':len(packages),'registry_packages':sum(bool(p.get('source')) for p in packages),'build':str(build),'local_tests':str(test),'offline_cli':str(cli),'no_host':{'exit':nohost.returncode,'stdout_bytes':len(nohost.stdout),'stderr_sha256':hashlib.sha256(nohost.stderr).hexdigest(),'stdout_eof':True,'stderr_eof':True},'host_arguments':['--morrow-native-session-v2'],'observation_arguments':['--queries','16','--interval-ms','200'],'exit_codes':{'valid_host_stop_25':0,'bad_cli':2,'protocol':16,'identity':17,'sequence':18,'revoked':19,'expired':20,'quota':21,'unsupported':22,'handshake':23,'disconnected':24,'local_timeout':26},'input_sha256':inputs,'old_handoffs':before,'limits':['No real host integration run yet','No local approval/state-file authority','Process-wide cap60s, operation/frame max5s; no retry','No installer/image attestation/OS isolation/descendant containment','M02 partial; M03/M04/M06/M08 unsupported; no Core business integration']}
dest=HERE/'candidate-001.json';assert not dest.exists();dest.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'status':result['status'],'manifest':str(dest),'manifest_sha256':sha(dest),'exe':str(exe),'exe_sha256':sha(exe),'inputs':len(inputs)}))
