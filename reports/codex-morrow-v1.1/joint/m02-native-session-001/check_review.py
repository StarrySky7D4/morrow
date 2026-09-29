"""Read-only M02 review preflight; never executes a delivered program.

This describes reviewer evidence, not a transport schema or authorization API.
0 = integrity only (with --integrity-only), 1 = mismatch, 2 = runtime review pending.
"""
import argparse
import hashlib
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
JOINT = HERE.parent
HOST = JOINT.parents[2]
PLUGIN = HOST.parents[2] / 'morrow-codex'
BASE = '88557916aabf2e10619b1022110035178498898a'

def sha(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as f:
        for chunk in iter(lambda: f.read(1048576), b''):
            h.update(chunk)
    return h.hexdigest()

def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))

def write(path, data):
    path = Path(path).resolve()
    if not path.is_relative_to(HERE):
        raise ValueError('output outside current joint batch')
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

def verify_pins(pins):
    issues, results, seen = [], [], set()
    for item in pins:
        path, expected = Path(item['path']), item['sha256']
        if not path.is_absolute() or not re.fullmatch(r'[0-9a-f]{64}', expected):
            raise ValueError('invalid file identity')
        resolved = path.resolve()
        if str(resolved).casefold() in seen:
            raise ValueError('duplicate file identity')
        seen.add(str(resolved).casefold())
        # These are explicitly enumerated evidence files, never environment/config discovery.
        try:
            actual = sha(path)
        except OSError as exc:
            actual = None
            issues.append({'path': str(path), 'error': type(exc).__name__})
        match = actual == expected
        if not match and actual is not None:
            issues.append({'path': str(path), 'error': 'sha256_mismatch'})
        results.append({'path': str(path), 'expected': expected, 'actual': actual, 'match': match})
    return results, issues

def check_matrix(matrix, catalogue):
    issues = []
    cases = matrix['diagnostics']
    if {c['id'] for c in cases} != {f'M02D-{n:02}' for n in range(1, 15)} or len(cases) != 14:
        issues.append('diagnostic_ids')
    product = {i['id']: i for i in catalogue['acceptance']}
    if len(product) != 84 or any(i['status'] != 'not_run' for i in product.values()):
        issues.append('product_84_not_run_changed')
    for anchor in matrix['original_acceptance']:
        item = product.get(anchor['id'])
        if item is None or any(anchor[k] != item[k] for k in ['scenario', 'must_prove', 'source']):
            issues.append('original_source_binding:' + anchor['id'])
        source = anchor['source']
        lines = Path(source['path']).read_text(encoding='utf-8-sig').splitlines()
        if lines[source['line']-1] != source['text']:
            issues.append('original_source_line:' + anchor['id'])
    valid = {i['id'] for i in matrix['original_acceptance']}
    for case in cases:
        if not set(case['related_original_ids']).issubset(valid):
            issues.append('invalid_mapping:' + case['id'])
        if case['status'] != 'not_run' or case['product_pass_credit'] != 0:
            issues.append('preflight_cannot_grant_runtime_credit:' + case['id'])
        if not case['required_observations'] or not case['rejection_or_limit']:
            issues.append('incomplete_diagnostic:' + case['id'])
    return issues

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--run-id', required=True)
    ap.add_argument('--integrity-only', action='store_true')
    ap.add_argument('--pin-file', nargs=2, action='append', default=[], metavar=('PATH', 'SHA256'))
    args = ap.parse_args()
    if not re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,90}', args.run_id):
        ap.error('unsafe run id')
    run = HERE / 'runs' / args.run_id
    run.parent.mkdir(exist_ok=True)
    run.mkdir(exist_ok=False)
    result = {'captured_at': datetime.now(timezone.utc).isoformat(), 'scope': 'identity_and_matrix_only',
              'independent_host_run': False, 'independent_client_run': False,
              'product_pass_credit': 0, 'M02_complete': False, 'G1_passed': False,
              'checker_sha256': sha(__file__), 'issues': []}
    try:
        baseline = read(HERE / 'baseline.json')
        matrix = read(HERE / 'matrix.json')
        result['baseline_sha256'] = sha(HERE / 'baseline.json')
        result['matrix_sha256'] = sha(HERE / 'matrix.json')
        head = subprocess.run(['git', '-C', str(HOST), 'rev-parse', 'HEAD'], check=True, capture_output=True, timeout=15).stdout.decode().strip()
        branch = subprocess.run(['git', '-C', str(HOST), 'branch', '--show-current'], check=True, capture_output=True, timeout=15).stdout.decode().strip()
        if head != BASE or branch != 'codex/io-safety-refactor':
            result['issues'].append('worktree_identity_changed')
        result.update(head=head, branch=branch)
        pins, issues = verify_pins(baseline['files'])
        result['issues'].extend(issues)
        write(run / 'frozen-identities.json', pins)
        result['frozen_file_count'] = len(pins)
        result['issues'].extend(check_matrix(matrix, read(JOINT / 'index.json')))
        extra, issues = verify_pins([{'path': p, 'sha256': s.lower()} for p, s in args.pin_file])
        write(run / 'additional-identities.json', extra)
        result['issues'].extend(issues)
        result['additional_file_count'] = len(extra)
        result['diagnostic_cases'] = len(matrix['diagnostics'])
        result['diagnostic_cases_executed'] = 0
        result['pending'] = ['host_authoritative_new_contract', 'ready_handoff_source_review',
                             'independent_real_host_child_ipc_and_lifecycle']
        result['integrity_status'] = 'failed' if result['issues'] else 'verified'
        result['status'] = 'failed' if result['issues'] else 'blocked_pending_runtime_review'
        result['exit_code'] = 1 if result['issues'] else (0 if args.integrity_only else 2)
    except Exception as exc:
        result.update(status='failed', integrity_status='failed', exit_code=1)
        result['issues'].append({'error': type(exc).__name__, 'message': str(exc)})
    write(run / 'result.json', result)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return result['exit_code']

if __name__ == '__main__':
    raise SystemExit(main())
