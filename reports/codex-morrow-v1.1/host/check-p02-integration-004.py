"""Read-only host review: never builds or executes the supplied probe."""
import datetime
import difflib
import hashlib
import json
import os
from pathlib import Path

P = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
H = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
R = P / 'receipts/p02-integration-004'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
read = lambda path: json.loads(path.read_text(encoding='utf-8'))

def check(root, mapping):
    bad = [name for name, expected in mapping.items() if not (root/name).is_file() or sha(root/name) != expected]
    assert not bad, bad
    return {'count': len(mapping), 'mismatches': bad}

handoff = read(R/'handoff.json')
assert sha(R/'handoff.json') == '513b1cfbf4b19b32c247b06810430ae29b1e1b779078b9761100a86c7a8d1751'
result = {'kind':'read_only_host_guard_contract_source_receipt_review', 'created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'independent_build':False, 'independent_runtime':False, 'handoff_sha256':sha(R/'handoff.json'), 'batch_inputs':check(P,handoff['input_sha256']), 'prior_inputs':{}}
for rel, item in handoff['prior_frozen_handoffs'].items():
    assert sha(P/rel) == item['sha256']
    result['prior_inputs'][rel] = check(P,read(P/rel)['input_sha256'])
    assert result['prior_inputs'][rel]['count'] == item['input_count']

fixed = P/'upstream/p02-source-batch-001/codex-source'
work = P/'upstream/p02-integration-004/codex-work'
manifest = read(P/'receipts/p02-source-batch-001/codex-content-manifest.json')
patch = read(R/'patch-after-build-001.json')
deltas = {r['path']:r for r in patch['changes']}
known = set()
diff = []
def data(path, mode):
    if mode == '120000':
        assert path.is_symlink()
        return os.readlink(path).encode()
    return path.read_bytes()
for row in manifest['files']:
    name = row['path']; known.add(name)
    old = data(fixed/name,row['mode']); new = data(work/name,row['mode'])
    assert hashlib.sha256(old).hexdigest() == row['sha256']
    assert hashlib.sha256(new).hexdigest() == (deltas[name]['after_sha256'] if name in deltas else row['sha256'])
    if old != new:
        assert deltas[name]['before_sha256'] == row['sha256']
        diff.extend(difflib.unified_diff(old.decode().splitlines(True),new.decode().splitlines(True),fromfile='a/'+name,tofile='b/'+name))
actual = {f.relative_to(work).as_posix() for f in work.rglob('*') if f.is_file() or f.is_symlink()}
assert not known-actual
for name in sorted(actual-known):
    assert name in deltas and deltas[name]['before_sha256'] is None
    new = (work/name).read_bytes()
    assert hashlib.sha256(new).hexdigest() == deltas[name]['after_sha256']
    diff.extend(difflib.unified_diff([],new.decode().splitlines(True),fromfile='/dev/null',tofile='b/'+name))
rebuilt = ''.join(diff).encode()
assert rebuilt == (R/'patch-after-build-001.patch').read_bytes()
result['source'] = {'fixed_files':len(known),'working_files':len(actual),'delta_files':len(deltas),'patch_lines':len(rebuilt.splitlines()),'patch_sha256':hashlib.sha256(rebuilt).hexdigest(),'exact_patch_reconstructed':True}

kit = H/'reports/codex-morrow-v1.1/host/host-kit-003'
km = read(kit/'manifest.json')
result['host_kit'] = check(kit,{r['path']:r['sha256'] for r in km['files']})
result['host_canonical'] = check(H/'contracts/experimental/agent_host_v1',{r['path']:r['sha256'] for r in km['source_files']})
assert sha(kit/'manifest.json') == '5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01'

result['producer_stages'] = {}
for stage in ['build','run']:
    path = P/handoff[stage]; receipt = read(path)
    assert receipt['exit_code'] == 0 and receipt['inputs_before'] == receipt['inputs_after'] and receipt['changed_inputs'] == []
    result['producer_stages'][stage] = {'exit_code':0,'elapsed_seconds':receipt['elapsed_seconds'],'bound_inputs':check(P,receipt['inputs_after'])}
    if stage == 'build':
        messages = [json.loads(line) for line in (path.parent/'stdout.txt').read_text(encoding='utf-8').splitlines()]
        artifacts = [m for m in messages if m.get('reason') == 'compiler-artifact']
        assert any(m.get('reason') == 'build-finished' and m['success'] for m in messages)
graph = read(R/'build-graph-evidence.json')
compiled = {}
for name, package in graph['packages'].items():
    matches = [a for a in artifacts if a['package_id'] == package['package_id']]
    assert matches and all(a['features'] == package['compiler_features'] for a in matches)
    compiled[name] = package['compiler_features']
assert 'morrow-p02-restricted-qualification' in compiled['codex-core']
assert 'morrow-p02-restricted-qualification' in compiled['codex-thread-store']
executables = [a for a in artifacts if a.get('executable')]
assert any(Path(a['executable']) == P/handoff['artifact'] for a in executables)
assert sha(P/handoff['artifact']) == handoff['artifact_sha256']
result['compiled_features_from_raw_build_log'] = compiled
result['compiler_artifact_package_count'] = len({a['package_id'] for a in artifacts})

runtime = read(P/handoff['runtime']); cases = runtime['cases']
assert len(cases) == runtime['case_count'] == 24
assert sum(c['assertions'] for c in cases) == runtime['assertions'] == 109
assert runtime['runtime_dropped_before_receipt'] and runtime['qualification_only']
shared = cases[0]
assert shared['remaining_strong_owners'] == [0,0,0,0]
assert 'Unsupported' in shared['cleanup'] and 'NOT established' in shared['cleanup']
assert len(shared['exec_calls']) == len(shared['network_calls']) == 1
for c in cases[1:3]:
    assert c['result']['spawn_lifecycle_calls'] == [] and 'restricted-qualification: injected exec required' in c['result']['error']
assert cases[3]['result']['after_spawn_calls'] == 0 and 'independent exec disabled' in cases[3]['result']['error']
for c in cases[4:6]:
    assert c['expected_guard'] in c['result']['outcomes'][0]
for c in cases[6:8]:
    assert 'restricted_qualification_local_resume_before_state_db' in c['error']
assert {c['mode'] for c in cases[6:8]} == {'Legacy','Paginated'}
assert cases[8]['adapter_calls'] == [] and not cases[8]['fixture_home_created'] and 'restricted_qualification_create_before_git' in cases[8]['error']
http = []
def inspect(value):
    if isinstance(value,dict):
        if value.get('method') == 'http.stream':
            raw = value['body_utf8'].encode('utf-8'); body = json.loads(raw)
            assert hashlib.sha256(raw).hexdigest() == value['body_sha256'] and len(raw) == value['body_bytes']
            assert value['http_method'] == 'POST' and value['url'] == 'https://fixture.invalid/v1/responses'
            assert body['model'] == 'qualification-model' and body['stream'] is True
            http.append({'bytes':len(raw),'sha256':value['body_sha256']})
        for item in value.values(): inspect(item)
    elif isinstance(value,list):
        for item in value: inspect(item)
inspect(cases)
assert len(http) == 6
result['runtime_receipt_checks'] = {'cases':24,'counted_assertions':109,'new_guard_and_owner_receipts_rechecked':True,'http_bodies_rehashed':http,'runtime_sha256':sha(P/handoff['runtime'])}
result['status'] = 'passed_read_only_review_not_independent_execution'
out = Path(__file__).with_name('p02-integration-004-host-input-check.json')
out.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({k:result[k] for k in ['status','batch_inputs','source','compiler_artifact_package_count']}))
