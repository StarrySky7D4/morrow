"""Freeze revision002 and its concrete local fixes; no end-to-end claim."""
from pathlib import Path
import hashlib,json,shutil,importlib.util
ROOT=Path(__file__).resolve().parents[2]
HERE=ROOT/'receipts/m03-stream-002'
BUILD=HERE/'runs/build-20260928T221836Z-7f00cee4'
CORETEST=HERE/'runs/test-core-20260928T221614Z-72673c77'
NATIVETEST=HERE/'runs/test-20260928T221630Z-f26c2ac2'
OUT=ROOT/'out/m03-native-002/candidates/native-002'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):return json.loads(p.read_text(encoding='utf-8'))
def main():
    build,core,native=(load(p/'result.json') for p in [BUILD,CORETEST,NATIVETEST])
    assert build['status']=='revision_native_core_compiled_not_runtime_proof'
    assert core['status']==native['status']=='revision_local_tests_passed'
    assert all(r['old_001_and_kits_unchanged'] and r['inputs_before']==r['inputs_after'] for r in [build,core,native])
    docs=[]
    for tested in [core,native]:
        for name,expected in build['inputs_after'].items():
            if tested['inputs_after'].get(name)!=expected:
                assert name.endswith('/README.md'),name
                docs.append(name)
    for name,h in build['inputs_after'].items():assert sha(ROOT/name)==h,name
    assert '4 passed; 0 failed' in (CORETEST/'stdout.txt').read_text(encoding='utf-8')
    assert '9 passed; 0 failed' in (NATIVETEST/'stdout.txt').read_text(encoding='utf-8')
    spec=importlib.util.spec_from_file_location('revision_build',HERE/'build_revision.py');helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    protected=helper.frozen()
    rows=[json.loads(line) for line in (BUILD/'stdout.txt').read_text(encoding='utf-8').splitlines()]
    paths=[Path(r['executable']) for r in rows if r.get('reason')=='compiler-artifact' and r.get('target',{}).get('name')=='morrow-codex-native-http-client' and r.get('executable')]
    assert len(paths)==1
    OUT.mkdir(parents=True,exist_ok=False);exe=OUT/paths[0].name;shutil.copyfile(paths[0],exe)
    assert sha(exe)==sha(paths[0])
    files=[*(ROOT/'qualification/m03-stream-002').rglob('*'),*(ROOT/'native/m03-stream-002').rglob('*'),
        HERE/'build_revision.py',HERE/'revision-notes.md',Path(__file__),exe]
    files += [p for folder in [BUILD,CORETEST,NATIVETEST] for p in folder.iterdir() if p.is_file()]
    bound={p.relative_to(ROOT).as_posix():sha(p) for p in files if p.is_file()}
    result={'status':'fixed_native_core_candidate_002_compiled_with_targeted_local_regressions',
        'input_sha256':bound,'supersedes_for_pairing':'native-candidate-001; original remains frozen',
        'previous_candidate_sha256':'7d2dc5319e4adeda829c8c80579fc09f56b84ba68eb6d746fbc9f6700b5514e1',
        'review_input_sha256':'12696ee0e2a171fceeaa0279992de611253fea4bee70704a411225c22966b364',
        'dependency_receipt_sha256':{n:sha(ROOT/'receipts/m03-stream-001'/n) for n in ['wire-intake-001.json','pipe-intake-001.json']},
        'old_001_and_consumed_inputs_unchanged':protected==helper.frozen(),
        'executable':str(exe),'executable_sha256':sha(exe),'executable_bytes':exe.stat().st_size,
        'compiler_reported_original_executable':str(paths[0]),
        'compiler_artifact_unique_package_ids':len({r['package_id'] for r in rows if r.get('reason')=='compiler-artifact'}),
        'core_local_tests_passed':4,'native_local_tests_passed':9,
        'after_test_documentation_only_changes':sorted(set(docs)),
        'fixes':['host cancellation synchronously binds and closes the shared Core event gate; pre-binding replay; reserved and queued events suppressed',
                 'cleanup continues but terminal fatal survives valid Close ACK; result and final control decision recorded after Close'],
        'actual_core_entry':'main::run -> revision002 request_task::start -> fixed real ModelClient/Responses/SSE using native Session',
        'cli':['--morrow-native-http-v3','--fixture-base','http://127.0.0.1:PORT/v1','--evidence-dir','ABSOLUTE_NEW_BATCH_DIRECTORY_UNDER_PLUGIN_OUT','--max-chunk','1024'],
        'close_contract':'Matching Close sequence State/code0 ACK must fully write before host closes control; no fatal reset',
        'result_snapshot':'after_close_observation with close_result and final_control_result',
        'host_runtime_launched':False,'actual_core_http_run_verified':False,'original_failures_reproduced_with_real_host':False,
        'product_success_claimed':False,'whole_session_release_verified':False}
    target=HERE/'native-candidate-002.json';assert not target.exists()
    target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'candidate':str(target),'sha256':sha(target),'executable':str(exe),'executable_sha256':sha(exe),
        'bytes':exe.stat().st_size,'bound_files':len(bound),'packages':result['compiler_artifact_unique_package_ids']}))
if __name__=='__main__':main()
