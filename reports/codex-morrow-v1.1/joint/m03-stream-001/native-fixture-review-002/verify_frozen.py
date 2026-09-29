"""Read-only fixed-source and existing-log verification; no tests or runtime launch."""
from pathlib import Path
import hashlib,json
P=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
J=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text(encoding='utf-8'))
def save(n,v):
    with (J/n).open('x',encoding='utf-8') as f:json.dump(v,f,indent=2);f.write('\n')
mp=P/'receipts/m03-fixture-002/native-candidate-fixture-002.json'
assert sha(mp)=='6a8d4d07aab156d72f5fce73dea7750f8b5424707dae84595182ff7f55c211a5'
m=read(mp); pins=[]
for rel,h in m['input_sha256'].items():
    assert sha(P/rel)==h,rel
    pins.append(dict(path=str(P/rel),sha256=h))
assert len(pins)==79
exe=Path(m['executable']);assert sha(exe)==m['executable_sha256']=='b895a7d78cd6c3c3641e502ad63b167cf5b78bdfaab75bfde079710efb0a08bc'
assert exe.stat().st_size==66073600
results={k:read(Path(m[k])/'result.json') for k in ('core_tests','native_tests','build')}
for kind,r in results.items():
    assert r['exit_code']==0 and r['inputs_before']==r['inputs_after'] and not r['changed_inputs']
    for name,h in r['log_sha256'].items():assert sha(Path(m[kind])/name)==h
build=results['build']['inputs_after']
assert results['native_tests']['inputs_after']==build
core_inputs={k:v for k,v in results['core_tests']['inputs_after'].items() if k.startswith('qualification/m03-fixture-002/')}
assert core_inputs and all(build[k]==v==m['input_sha256'][k] for k,v in core_inputs.items())
assert all(m['input_sha256'][k]==v for k,v in build.items())
for kind,n in [('core_tests',14),('native_tests',31)]:
    log=(Path(m[kind])/'stdout.txt').read_text(encoding='utf-8')
    assert f'test result: ok. {n} passed; 0 failed;' in log
    assert sum(line.startswith('test ') and line.endswith(' ... ok') for line in log.splitlines())==n
artifacts=[]
for line in (Path(m['build'])/'stdout.txt').read_text(encoding='utf-8').splitlines():
    try:v=json.loads(line)
    except json.JSONDecodeError:continue
    if v.get('reason')=='compiler-artifact':artifacts.append(v)
packages={v['package_id'] for v in artifacts};assert len(packages)==907
compiled=[v for v in artifacts if v.get('executable') and v['target']['name']=='morrow-codex-native-http-client']
assert len(compiled)==1 and sha(Path(compiled[0]['executable']))==m['executable_sha256']
old=P/'receipts/m03-fixture-001/native-candidate-fixture-001.json'
assert sha(old)==m['base_candidate_sha256']
# Immediate unchanged module claims, not a historical full-tree scan.
for rel in ('native/src/admission.rs','native/src/delivery.rs','native/src/lib.rs','qualification/src/fixture.rs','qualification/src/fixture_tests.rs','qualification/src/limits.rs'):
    package,tail=rel.split('/',1)
    assert sha(P/package/'m03-fixture-001'/tail)==sha(P/package/'m03-fixture-002'/tail)
save('inputs.json',dict(candidate_manifest=str(mp),candidate_manifest_sha256=sha(mp),pins=pins,build_inputs=build,core_source_inputs=core_inputs,compiler_package_count=907))
save('result.json',dict(status='limited_source_review_no_remaining_blocker',candidate_manifest_sha256=sha(mp),candidate_exe_sha256=m['executable_sha256'],verified_candidate_pins=79,producer_core_tests=14,producer_native_tests=31,compiler_package_count=907,source_bound_to_tests_and_build=True,independent_tests_run=0,http_requests=0,A_runtime_verified=False,B_runtime_verified=False,old_A010_failed_preserved=True,product_gate_upgrade=False,control_admission_is_not_os_issue=True))
print(json.dumps(dict(status='verified',pins=79,core_tests=14,native_tests=31,packages=907,result_sha256=sha(J/'result.json'))))
