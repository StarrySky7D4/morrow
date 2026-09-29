"""Seal the locally tested fixture candidate. Never execute host/guest/HTTP."""
from pathlib import Path
import hashlib,importlib.util,json,shutil
ROOT=Path(__file__).resolve().parents[2]
HERE=ROOT/'receipts/m03-fixture-001'
BUILD=HERE/'runs/build-20260929T003432Z-e47fce6a'
CORE=HERE/'runs/test-core-20260929T003426Z-523e3ae9'
NATIVE=HERE/'runs/test-20260929T003429Z-4a7dc7d2'
LOCKS=[HERE/'runs/lock-core-20260929T002006Z-6e3e9314',HERE/'runs/lock-20260929T002008Z-7c99cdfd']
OUT=ROOT/'out/m03-fixture-001/candidates/fixture-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):return json.loads(p.read_text(encoding='utf-8'))
def main():
    build,core,native=(load(p/'result.json') for p in [BUILD,CORE,NATIVE])
    assert build['status']=='revision_native_core_compiled_not_runtime_proof'
    assert core['status']==native['status']=='revision_local_tests_passed'
    assert all(r['old_001_002_003_and_kits_unchanged'] and r['inputs_before']==r['inputs_after'] for r in [build,core,native])
    assert build['inputs_after']==core['inputs_after']==native['inputs_after']
    for name,h in build['inputs_after'].items():assert sha(ROOT/name)==h,name
    assert '11 passed; 0 failed' in (CORE/'stdout.txt').read_text(encoding='utf-8')
    assert '18 passed; 0 failed' in (NATIVE/'stdout.txt').read_text(encoding='utf-8')
    spec=importlib.util.spec_from_file_location('fixture_build',HERE/'build_fixture.py')
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    protected=helper.frozen()
    rows=[json.loads(s) for s in (BUILD/'stdout.txt').read_text(encoding='utf-8').splitlines() if s.strip()]
    paths=[Path(r['executable']) for r in rows if r.get('reason')=='compiler-artifact' and r.get('target',{}).get('name')=='morrow-codex-native-http-client' and r.get('executable')]
    assert len(paths)==1
    OUT.mkdir(parents=True,exist_ok=False);exe=OUT/paths[0].name;shutil.copyfile(paths[0],exe)
    assert sha(exe)==sha(paths[0])
    files=[*(ROOT/'native/m03-fixture-001').rglob('*'),*(ROOT/'qualification/m03-fixture-001').rglob('*'),
        HERE/'build_fixture.py',HERE/'revision-notes.md',HERE/'fixture-interface.md',HERE/'fixture-spec.example.json',Path(__file__),
        ROOT/'out/m03-fixture-001/cargo-home/config.toml',exe]
    files += [p for folder in [*LOCKS,CORE,NATIVE,BUILD] for p in folder.iterdir() if p.is_file()]
    local_artifacts=ROOT/'out/m03-fixture-001/runs/20260929T003426Z-523e3ae9/tmp'
    files += [p for folder in local_artifacts.glob('m03-fixture-local-*') for p in folder.rglob('*') if p.is_file()]
    bound={p.relative_to(ROOT).as_posix():sha(p) for p in files if p.is_file()}
    changed={}
    for old,new in [('qualification/m03-stream-002','qualification/m03-fixture-001'),('native/m03-stream-003','native/m03-fixture-001')]:
        changed[new]=[p.relative_to(ROOT/new).as_posix() for p in (ROOT/new).rglob('*') if p.is_file() and (not (ROOT/old/p.relative_to(ROOT/new)).is_file() or sha(p)!=sha(ROOT/old/p.relative_to(ROOT/new)))]
    result={
        'status':'fixed_fixture_001_compiled_local_regressions_only','input_sha256':bound,
        'executable':str(exe),'executable_sha256':sha(exe),'executable_bytes':exe.stat().st_size,
        'compiler_reported_original_executable':str(paths[0]),
        'compiler_artifact_unique_package_ids':len({r['package_id'] for r in rows if r.get('reason')=='compiler-artifact'}),
        'qualification_local_tests_passed':11,'native_local_tests_passed':18,
        'local_test_scope':'synthetic events/state/codec; local files and owned local threads; no Core HTTP, guest native launch, host or data pipe',
        'source_changes':changed,'old_frozen_inputs_verified':len(protected),
        'frozen_001_002_003_kits_upstream004_unchanged':protected==helper.frozen(),
        'base_candidate_sha256':'54f4c6cd38e1968d1dbc54d26b8bf40460ee660498f8590c7f44c10fe04396c5',
        'design_sha256':{'plugin':'c69823c6e9f210dcf8b9de3bc4207a0ce7feba1c45ed869bacc11244ffd5df31','host':'39bf723fd17cf96d836dde7aaf432d18fbe7d4390c59aebfba58497fe0526db6'},
        'fixture_modes':['core-revoke','data-pending'],'fixture_spec':'fixture-spec.json exact bytes SHA-bound in final argument pair',
        'client_args':['--fixture-base','http://127.0.0.1:PORT/v1','--evidence-dir','NEW_ABSOLUTE_BATCH_DIR_UNDER_PLUGIN_OUT','--max-chunk','1024','--fixture-spec-sha256','SHA256_OF_EXACT_SPEC_BYTES'],
        'actual_core_entry':'main -> fixture request_task::start -> unchanged real ModelClient/Responses/SSE; no synthetic ResponseEvent creation in runtime fixture path',
        'close_and_join_budget_ms':500,'authority_ttl_renewed':False,'consumer_event_slots':8,
        'host_runtime_launched':False,'guest_native_executable_launched':False,'actual_core_http_invoked':False,
        'data_pipe_runtime_tested':False,'A_runtime_verified':False,'B_runtime_verified':False,
        'product_success_claimed':False,'whole_session_release_verified':False,'old_batches_regraded':False,
    }
    target=HERE/'native-candidate-fixture-001.json';assert not target.exists()
    target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'candidate':str(target),'sha256':sha(target),'executable_sha256':sha(exe),'bytes':exe.stat().st_size,
        'bound_files':len(bound),'frozen_inputs_verified':len(protected),'packages':result['compiler_artifact_unique_package_ids']}))
if __name__=='__main__':main()
