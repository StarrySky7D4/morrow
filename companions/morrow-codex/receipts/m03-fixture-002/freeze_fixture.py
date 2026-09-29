"""Freeze local-only fixture002 evidence; never launch host/guest or HTTP."""
from pathlib import Path
import hashlib, importlib.util, json, shutil
ROOT=Path(__file__).resolve().parents[2]
HERE=ROOT/'receipts/m03-fixture-002'
CORE=HERE/'runs/test-core-20260929T012935Z-8ccf982f'
NATIVE=HERE/'runs/test-20260929T014130Z-8d3fee8a'
BUILD=HERE/'runs/build-20260929T014202Z-9ea8b745'
OUT=ROOT/'out/m03-fixture-002/candidates/fixture-002'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p): return json.loads(p.read_text(encoding='utf-8'))
def main():
    core,native,build=[load(p/'result.json') for p in (CORE,NATIVE,BUILD)]
    assert core['status']==native['status']=='revision_local_tests_passed'
    assert build['status']=='revision_native_core_compiled_not_runtime_proof'
    for report in (core,native,build):
        assert report['inputs_before']==report['inputs_after']
        assert report['old_001_002_003_and_kits_unchanged']
    assert native['inputs_after']==build['inputs_after']
    core_inputs={k:v for k,v in core['inputs_after'].items() if k.startswith('qualification/m03-fixture-002/')}
    assert core_inputs
    assert all(build['inputs_after'][k]==v for k,v in core_inputs.items())
    for p,h in build['inputs_after'].items(): assert sha(ROOT/p)==h,p
    assert '14 passed; 0 failed' in (CORE/'stdout.txt').read_text()
    assert '31 passed; 0 failed' in (NATIVE/'stdout.txt').read_text()
    spec=importlib.util.spec_from_file_location('fixture_build',HERE/'build_fixture.py')
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    base=helper.frozen()
    rows=[json.loads(s) for s in (BUILD/'stdout.txt').read_text(encoding='utf-8').splitlines() if s.strip()]
    artifacts=[Path(r['executable']) for r in rows if r.get('reason')=='compiler-artifact'
        and r.get('target',{}).get('name')=='morrow-codex-native-http-client' and r.get('executable')]
    assert len(artifacts)==1
    OUT.mkdir(parents=True,exist_ok=False)
    exe=OUT/artifacts[0].name;shutil.copyfile(artifacts[0],exe)
    assert sha(exe)==sha(artifacts[0])
    files=[*HERE.rglob('*'),*(ROOT/'native/m03-fixture-002').rglob('*'),
        *(ROOT/'qualification/m03-fixture-002').rglob('*'),
        ROOT/'out/m03-fixture-002/cargo-home/config.toml',exe]
    tmp=ROOT/'out/m03-fixture-002/runs/20260929T012935Z-8ccf982f/tmp'
    files += [p for d in tmp.glob('m03-fixture-local-*') for p in d.rglob('*')]
    bound={p.relative_to(ROOT).as_posix():sha(p) for p in files if p.is_file()}
    result={
        'status':'fixed_fixture_002_compiled_local_regressions_only',
        'input_sha256':bound,'executable':str(exe),'executable_sha256':sha(exe),'executable_bytes':exe.stat().st_size,
        'base_candidate_sha256':'06a90c8d1adb8f09d13e83e177d55a4728792f2214000954c296728962c5b689',
        'immediate_frozen_base_inputs_verified':len(base),'immediate_base_unchanged':base==helper.frozen(),
        'historical_transitive_inputs_rehashed':False,
        'core_tests':str(CORE),'native_tests':str(NATIVE),'build':str(BUILD),
        'qualification_local_tests_passed':14,'native_local_tests_passed':31,
        'core_source_inputs_match_final_build':True,'native_test_inputs_match_final_build':True,
        'core_test_reused_after_native_only_changes':True,
        'compiler_artifact_unique_package_ids':len({r['package_id'] for r in rows if r.get('reason')=='compiler-artifact'}),
        'local_test_scope':'synthetic local events, native state/control codec, files and owned local threads; no actual Core HTTP or native pipe pair',
        'fixture_modes':['core-revoke','data-pending'],'fixture_spec_unchanged_from_001':True,
        'client_args':['--fixture-base','http://127.0.0.1:PORT/v1','--evidence-dir','NEW_ABSOLUTE_BATCH_DIR_UNDER_PLUGIN_OUT','--max-chunk','1024','--fixture-spec-sha256','SHA256_OF_EXACT_SPEC_BYTES'],
        'disconnect_wait_cap_ms':500,'authority_ttl_renewed':False,'delivery_resumed_after_pause':False,
        'core_cleanup_budget_ms':2000,'close_control_fixture_budget_ms':500,
        'control_write_boundary':'logical admission under State; blocking stdout outside lock; already admitted frames may issue after pause; not kernel ordering proof',
        'final_credit_reply_kind':'CreditState','final_credit_requires_matching_sequence_and_classifications':True,
        'failed_and_prefinal_local_runs_preserved':True,
        'host_runtime_launched':False,'guest_native_executable_launched':False,'actual_core_http_invoked':False,
        'data_pipe_runtime_tested':False,'A_runtime_verified':False,'B_runtime_verified':False,
        'product_success_claimed':False,'whole_session_release_verified':False,'old_batches_regraded':False,
    }
    target=HERE/'native-candidate-fixture-002.json';assert not target.exists()
    target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'candidate':str(target),'sha256':sha(target),'executable_sha256':sha(exe),
        'executable_bytes':exe.stat().st_size,'bound_files':len(bound),'base_files_verified':len(base)}))
if __name__=='__main__':main()
