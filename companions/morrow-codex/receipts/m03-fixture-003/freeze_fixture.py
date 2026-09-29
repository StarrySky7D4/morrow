"""Seal fixture003 source/local evidence and executable. No host/guest/HTTP launch."""
from pathlib import Path
import difflib
import hashlib
import importlib.util
import json
import shutil

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / 'receipts/m03-fixture-003'
CORE = HERE / 'runs/test-core-20260929T221031Z-5e54aa28'
NATIVE = HERE / 'runs/test-20260929T221930Z-863b46cb'
BUILD = HERE / 'runs/build-20260929T222011Z-7d1a4b8e'
OUT = ROOT / 'out/m03-fixture-003/candidates/fixture-003'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(path):
    return json.loads(path.read_text(encoding='utf-8'))


def main():
    target = HERE / 'native-candidate-fixture-003.json'
    assert not target.exists(), 'candidate is already sealed'
    core, native, build = [load(path / 'result.json') for path in (CORE, NATIVE, BUILD)]
    assert core['status'] == native['status'] == 'revision_local_tests_passed'
    assert build['status'] == 'revision_native_core_compiled_not_runtime_proof'
    for report in (core, native, build):
        assert report['inputs_before'] == report['inputs_after']
        assert report['immediate_frozen_fixture002_inputs_unchanged']
    assert native['inputs_after'] == build['inputs_after']
    core_inputs = {k: v for k, v in core['inputs_after'].items()
                   if k.startswith('qualification/m03-fixture-003/')}
    assert core_inputs and all(build['inputs_after'][k] == v for k, v in core_inputs.items())
    for path, digest in build['inputs_after'].items():
        assert sha(ROOT / path) == digest, path
    assert '15 passed; 0 failed' in (CORE / 'stdout.txt').read_text()
    assert '35 passed; 0 failed' in (NATIVE / 'stdout.txt').read_text()
    module = importlib.util.spec_from_file_location('fixture003_build', HERE / 'build_fixture.py')
    helper = importlib.util.module_from_spec(module)
    module.loader.exec_module(helper)
    protected = helper.frozen()

    diff = []
    for area in ('qualification', 'native'):
        new = ROOT / area / 'm03-fixture-003'
        old = ROOT / area / 'm03-fixture-002'
        for path in sorted(p for p in new.rglob('*') if p.is_file()):
            relative = path.relative_to(new)
            before = (old / relative).read_text(encoding='utf-8').splitlines(keepends=True)
            after = path.read_text(encoding='utf-8').splitlines(keepends=True)
            diff.extend(difflib.unified_diff(before, after,
                        fromfile=(old / relative).relative_to(ROOT).as_posix(),
                        tofile=path.relative_to(ROOT).as_posix()))
    patch = HERE / 'diff-from-fixture-002.patch'
    with patch.open('x', encoding='utf-8', newline='\n') as stream:
        stream.writelines(diff)

    rows = [json.loads(line) for line in (BUILD / 'stdout.txt').read_text(encoding='utf-8').splitlines()
            if line.strip()]
    artifacts = [Path(row['executable']) for row in rows
                 if row.get('reason') == 'compiler-artifact'
                 and row.get('target', {}).get('name') == 'morrow-codex-native-http-client'
                 and row.get('executable')]
    assert len(artifacts) == 1
    OUT.mkdir(parents=True, exist_ok=False)
    exe = OUT / artifacts[0].name
    shutil.copyfile(artifacts[0], exe)
    assert sha(exe) == sha(artifacts[0])

    paths = [*HERE.rglob('*'), *(ROOT / 'native/m03-fixture-003').rglob('*'),
             *(ROOT / 'qualification/m03-fixture-003').rglob('*'),
             ROOT / 'out/m03-fixture-003/cargo-home/config.toml', exe,
             ROOT / 'receipts/m03-fixture-003-design-001/scenario-specs.example.json',
             ROOT / 'receipts/m03-fixture-003-design-001/proposal-manifest.json']
    for report in (core, native):
        # Build helper owns a fresh TEMP for each stage; use its receipt's unique suffix.
        prefix = 'test-core-' if report['stage'] == 'test-core' else 'test-'
        receipt = CORE if report['stage'] == 'test-core' else NATIVE
        run = ROOT / 'out/m03-fixture-003/runs' / receipt.name.removeprefix(prefix) / 'tmp'
        for folder in run.glob('m03-fixture003-*'):
            paths.extend(folder.rglob('*'))
    bound = {p.relative_to(ROOT).as_posix(): sha(p) for p in paths if p.is_file()}
    assert protected == helper.frozen()
    result = {
        'status': 'fixed_fixture_003_compiled_local_regressions_only',
        'input_sha256': bound, 'executable': str(exe), 'executable_sha256': sha(exe),
        'executable_bytes': exe.stat().st_size,
        'base_candidate_sha256': '6a8d4d07aab156d72f5fce73dea7750f8b5424707dae84595182ff7f55c211a5',
        'immediate_frozen_base_inputs_verified': len(protected), 'immediate_base_unchanged': True,
        'historical_transitive_inputs_rehashed': False,
        'core_tests': str(CORE), 'native_tests': str(NATIVE), 'build': str(BUILD),
        'qualification_local_tests_passed': 15, 'native_local_tests_passed': 35,
        'core_source_inputs_match_final_build': True, 'native_test_inputs_match_final_build': True,
        'core_test_reused_after_native_only_changes': True,
        'compiler_artifact_unique_package_ids': len({r['package_id'] for r in rows
                                                   if r.get('reason') == 'compiler-artifact'}),
        'fixture_mode': 'passive-observe', 'fixture_spec_version': 2,
        'scenarios': ['authority-deadline', 'network-abort', 'pipe-partial-close'],
        'client_args': ['--fixture-base', 'http://127.0.0.1:PORT/v1', '--evidence-dir',
                        'NEW_ABSOLUTE_BATCH_DIR_UNDER_PLUGIN_OUT', '--max-chunk', '1024',
                        '--fixture-spec-sha256', 'SHA256_OF_EXACT_SPEC_BYTES'],
        'no_fixture_hold_release_or_read_pause': True,
        'partial_prefix_raw_bytes_limit': 12, 'partial_prefix_fragment_limit': 12,
        'partial_fragments_reference_actual_read_completions': True,
        'data_worker_failure_and_actual_join_separate': True,
        'control_failure_before_missing_request_closed_wait': True,
        'authority_ttl_renewed': False, 'first_cancellation_reason_rewritten': False,
        'core_cleanup_budget_ms': 2000, 'close_control_fixture_budget_ms': 500,
        'final_credit_reply_kind': 'CreditState', 'missing_ack_not_waived': True,
        'control_write_boundary': 'logical State admission; existing blocking stdout issue gap retained',
        'local_test_scope': 'synthetic typed events/completions, native state/control codec, files and actual owned local threads; no ModelClient HTTP or host/guest pipe pair',
        'prefinal_local_runs_preserved': True,
        'host_runtime_launched': False, 'guest_native_executable_launched': False,
        'actual_core_http_invoked': False, 'http_requests': 0, 'data_pipe_runtime_tested': False,
        'authority_deadline_runtime_verified': False, 'network_abort_runtime_verified': False,
        'pipe_partial_close_runtime_verified': False, 'whole_session_release_verified': False,
        'product_success_claimed': False, 'runtime_qualified': False, 'product_accepted': False,
        'old_batches_regraded': False, 'host_upstream_schema_modified': False, 'git_commits_created': 0,
    }
    with target.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(result, stream, indent=2)
        stream.write('\n')
    for path, digest in result['input_sha256'].items():
        assert sha(ROOT / path) == digest, path
    print(json.dumps({'candidate': str(target), 'sha256': sha(target),
                      'executable_sha256': sha(exe), 'executable_bytes': exe.stat().st_size,
                      'bound_files': len(bound), 'base_files_verified': len(protected),
                      'compiler_package_ids': result['compiler_artifact_unique_package_ids']}))


if __name__ == '__main__':
    main()
