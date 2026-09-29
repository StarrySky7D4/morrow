"""Read-only receipt/source verification; never runs the supplied executable."""
import datetime
import hashlib
import json
import os
from pathlib import Path

ROOT = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
HOST = Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
OUT = Path(__file__).parent

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

def bound_check(root, mapping, symlinks=frozenset()):
    failures = []
    for name, expected in mapping.items():
        path = root / name
        if name in symlinks:
            actual = hashlib.sha256(os.readlink(path).encode()).hexdigest() if path.is_symlink() else None
        else:
            actual = sha(path) if path.is_file() else None
        if actual != expected:
            failures.append(name)
    return {'count': len(mapping), 'mismatches': failures}

handoff_path = ROOT / 'receipts/p02-core-network-003/handoff.json'
handoff = read(handoff_path)
assert sha(handoff_path) == '97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb'
result = {'kind': 'independent_read_only_receipt_and_source_check',
          'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'independent_build': False, 'independent_probe_execution': False,
          'handoff_sha256': sha(handoff_path),
          'batch_inputs': bound_check(ROOT, handoff['input_sha256'])}
result['prior_inputs'] = {}
for rel, expected in handoff['prior_frozen_handoffs'].items():
    path = ROOT / rel
    assert sha(path) == expected['sha256']
    check = bound_check(ROOT, read(path)['input_sha256'])
    assert check['count'] == expected['input_count']
    result['prior_inputs'][rel] = check

manifest = read(ROOT / 'receipts/p02-source-batch-001/codex-content-manifest.json')
fixed = ROOT / 'upstream/p02-source-batch-001/codex-source'
work = ROOT / 'upstream/p02-core-network-003/codex-work'
expected = {row['path']: row['sha256'] for row in manifest['files']}
source_links = {row['path'] for row in manifest['files'] if row['mode'] == '120000'}
result['fixed_source'] = bound_check(fixed, expected, source_links)
result['source_symlink_check'] = 'Git mode 120000 hashes link text, not target bytes'
patch = read(ROOT / 'receipts/p02-core-network-003/patch-after-build-001.json')
for row in patch['changes']:
    assert expected.get(row['path']) == row['before_sha256']
    expected[row['path']] = row['after_sha256']
result['working_source'] = bound_check(work, expected, source_links)
actual_paths = {p.relative_to(work).as_posix() for p in work.rglob('*') if p.is_file()}
result['working_source']['unlisted_files'] = sorted(actual_paths - expected.keys())
result['working_source']['missing_files'] = sorted(expected.keys() - actual_paths)
result['source_delta_files'] = len(patch['changes'])
result['source_delta_patch_sha256'] = sha(ROOT / 'receipts/p02-core-network-003/patch-after-build-001.patch')
assert result['source_delta_patch_sha256'] == handoff['source_delta_sha256']

kit = HOST / 'reports/codex-morrow-v1.1/host/host-kit-003'
km = read(kit / 'manifest.json')
result['host_manifest_sha256'] = sha(kit / 'manifest.json')
assert result['host_manifest_sha256'] == '5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01'
result['host_canonical'] = bound_check(HOST / 'contracts/experimental/agent_host_v1', {x['path']: x['sha256'] for x in km['source_files']})
if isinstance(km.get('files'), list):
    result['host_kit'] = bound_check(kit, {x['path']: x['sha256'] for x in km['files']})

runtime = read(ROOT / handoff['runtime'])
assert runtime['case_count'] == 6 and runtime['assertions'] == 47
expected_cases = [
    ('http_direct', ['http.stream'], [False, False], 7),
    ('websocket_disconnect', ['websocket.connect'], [True, True], 5),
    ('prewarm_disconnect', ['websocket.connect'], [True, True], 5),
    ('stream_426_natural_fallback_and_next_session', ['websocket.connect', 'http.stream', 'http.stream'], [True, False, False], 12),
    ('prewarm_426_natural_fallback_then_stream', ['websocket.connect', 'http.stream'], [True, False, False], 9),
    ('preconnect_426_natural_fallback_then_stream', ['websocket.connect', 'http.stream'], [True, False, False], 9),
]
result['runtime_case_checks'] = []
http_count = 0
for case, (name, methods, states, assertions) in zip(runtime['cases'], expected_cases, strict=True):
    assert case['case'] == name and case['assertions'] == assertions
    core = case['core_result']
    assert core['websocket_enabled_states'] == states
    assert core['credential_environment_absent'] and core['provider_auth_fields_absent']
    calls = case['backend_calls']
    assert [x['method'] for x in calls] == methods
    outcomes = core['outcomes']
    assert len(outcomes) == (2 if '426' in name else 1)
    for index, outcome in enumerate(outcomes):
        if index == 0 and name.startswith(('prewarm_426', 'preconnect_426')):
            assert outcome == name.split('_')[0] + '_setup_ok_without_network_success'
        else:
            assert 'qualification-disconnected-' + ('websocket' if name in ['websocket_disconnect', 'prewarm_disconnect'] else 'http') in outcome
    for call in calls:
        if call['method'] == 'http.stream':
            raw = call['body_utf8'].encode('utf-8')
            assert hashlib.sha256(raw).hexdigest() == call['body_sha256']
            assert len(raw) == call['body_bytes']
            body = json.loads(raw)
            assert body['model'] == 'qualification-model' and body['stream'] is True
            assert call['http_method'] == 'POST' and call['url'] == 'https://fixture.invalid/v1/responses'
            http_count += 1
        else:
            assert call['url'] == 'wss://fixture.invalid/v1/responses'
            assert call['response'] == ('synthetic_426' if '426' in name else 'local_disconnect')
    result['runtime_case_checks'].append({'case': name, 'assertions_recorded': assertions, 'receipt_values_rechecked': True})
result['http_bodies_independently_rehashed'] = http_count
result['producer_build_run'] = {}
for stage in ['build', 'run']:
    receipt = read(ROOT / handoff[stage])
    assert receipt['exit_code'] == 0 and receipt['changed_inputs'] == []
    assert receipt['inputs_before'] == receipt['inputs_after']
    result['producer_build_run'][stage] = {'exit_code': receipt['exit_code'], 'elapsed_seconds': receipt['elapsed_seconds'], 'bound_inputs': bound_check(ROOT, receipt['inputs_after'])}
checks = [result['batch_inputs'], result['fixed_source'], result['working_source'], result['host_canonical'], *result['prior_inputs'].values(), *[v['bound_inputs'] for v in result['producer_build_run'].values()]]
if 'host_kit' in result:
    checks.append(result['host_kit'])
if any(c['mismatches'] for c in checks):
    print(json.dumps({'failed_checks': [c for c in checks if c['mismatches']]}, ensure_ascii=True))
assert all(not c['mismatches'] for c in checks)
assert not result['working_source']['unlisted_files'] and not result['working_source']['missing_files']
result['status'] = 'passed_read_only_source_and_receipt_review_not_runtime_reproduction'
destination = OUT / 'p02-core-network-003-host-input-check.json'
destination.write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
print(json.dumps({'status': result['status'], 'batch_inputs': result['batch_inputs'], 'fixed_source': result['fixed_source'], 'working_source': result['working_source'], 'host_kit': result.get('host_kit'), 'http_bodies_checked': http_count, 'output': str(destination)}))
