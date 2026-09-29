"""Seal only this new pre-wire source/build result. No runtime acceptance."""
from pathlib import Path
import hashlib
import json
import shutil

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT/'receipts/m03-stream-001'
SOURCE = ROOT/'qualification/m03-stream-001'
BUILD = HERE/'runs/build-20260928T212905Z-e12a3afe'
TEST = HERE/'runs/test-20260928T212937Z-9717237f'
OUT = ROOT/'out/m03-stream-001/candidates/pre-wire-001'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p): return json.loads(p.read_text(encoding='utf-8'))
def main():
    build, test = load(BUILD/'result.json'), load(TEST/'result.json')
    assert build['status'] == 'compiled_pre_wire_only' and test['status'] == 'passed_local_unit_only'
    assert build['frozen_unchanged'] and test['frozen_unchanged']
    assert build['inputs_after'] == test['inputs_before'] == test['inputs_after']
    for name, expected in build['inputs_after'].items(): assert sha(ROOT/name) == expected, name
    assert '3 passed; 0 failed' in (TEST/'stdout.txt').read_text(encoding='utf-8')
    OUT.mkdir(parents=True, exist_ok=False)
    artifacts = []
    for line in (BUILD/'stdout.txt').read_text(encoding='utf-8').splitlines():
        row = json.loads(line)
        if row.get('reason') != 'compiler-artifact' or row['target']['name'] not in ['morrow_codex_m03_stream','morrow-codex-m03-stream']:
            continue
        for name in row.get('filenames',[]):
            path = Path(name)
            if path.suffix not in ['.rlib','.exe']: continue
            destination = OUT/path.name
            shutil.copyfile(path,destination)
            artifacts.append({'compiler_reported_path':str(path),'sealed_path':destination.relative_to(ROOT).as_posix(),
                'sha256':sha(destination),'bytes':destination.stat().st_size,'kind':row['target']['kind']})
    assert any(a['sealed_path'].endswith('.rlib') for a in artifacts)
    assert any(a['sealed_path'].endswith('.exe') for a in artifacts)
    paths = [*SOURCE.rglob('*'), Path(__file__), HERE/'build_candidate.py', HERE/'integration-supplement-002.md']
    paths += [p for folder in [BUILD,TEST] for p in folder.iterdir() if p.is_file() and p.name != 'frozen-before.json']
    bound = {p.relative_to(ROOT).as_posix():sha(p) for p in paths if p.is_file()}
    bound.update({a['sealed_path']:a['sha256'] for a in artifacts})
    receipt = {'status':'compiled_real_core_entry_pre_wire_local_unit_passed',
        'input_sha256':bound,'artifacts':artifacts,'local_unit_tests_passed':3,
        'real_core_entry':'library request_task::start -> ModelClient -> session.stream -> upstream Responses/SSE',
        'cli_entry':'WireNotReady; does not invoke Core or HTTP',
        'build_receipt':(BUILD/'result.json').relative_to(ROOT).as_posix(),
        'test_receipt':(TEST/'result.json').relative_to(ROOT).as_posix(),
        'native_driver_implemented':False,'core_http_runtime_invoked':False,'actual_sse_events_verified':False,
        'product_success_claimed':False,'old_refusal_suite_rerun':False,
        'frozen_source':'qualification/m03-stream-001; implement native driver in a new crate',
        'limits':'32KiB request;64KiB or tighter approved response;8KiB/32 headers;4KiB error;8KiB max chunk',
        'failed_preparation_runs_preserved':['lock-20260928T211556Z-74502337','lock-20260928T211631Z-e2b3c755','lock-20260928T211734Z-daafecfc']}
    path = HERE/'pre-wire-candidate-001.json'
    assert not path.exists()
    path.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'candidate':str(path),'sha256':sha(path),'files':len(bound),'artifacts':artifacts}))
if __name__ == '__main__': main()
