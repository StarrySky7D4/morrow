"""Freeze native003 local observation changes. No host/guest/HTTP execution."""
from pathlib import Path
import hashlib, importlib.util, json, shutil

ROOT=Path(__file__).resolve().parents[2]
HERE=ROOT/'receipts/m03-stream-003'
BUILD=HERE/'runs/build-20260928T233853Z-1b06226d'
TEST=HERE/'runs/test-20260928T233831Z-b862505f'
LOCK=HERE/'runs/lock-20260928T233819Z-11a6110c'
OUT=ROOT/'out/m03-native-003/candidates/native-003'

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p): return json.loads(p.read_text(encoding='utf-8'))

def main():
    build,test=(load(p/'result.json') for p in [BUILD,TEST])
    assert build['status']=='revision_native_core_compiled_not_runtime_proof'
    assert test['status']=='revision_local_tests_passed'
    assert all(r['old_001_002_and_kits_unchanged'] and r['inputs_before']==r['inputs_after'] for r in [build,test])
    assert build['inputs_after']==test['inputs_after']
    assert '16 passed; 0 failed' in (TEST/'stdout.txt').read_text(encoding='utf-8')
    for name,h in build['inputs_after'].items(): assert sha(ROOT/name)==h,name
    spec=importlib.util.spec_from_file_location('native003_build',HERE/'build_revision.py')
    helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
    protected=helper.frozen()
    rows=[json.loads(line) for line in (BUILD/'stdout.txt').read_text(encoding='utf-8').splitlines() if line.strip()]
    paths=[Path(r['executable']) for r in rows if r.get('reason')=='compiler-artifact' and r.get('target',{}).get('name')=='morrow-codex-native-http-client' and r.get('executable')]
    assert len(paths)==1
    OUT.mkdir(parents=True,exist_ok=False)
    exe=OUT/paths[0].name;shutil.copyfile(paths[0],exe)
    assert sha(exe)==sha(paths[0])
    files=[*(ROOT/'native/m03-stream-003').rglob('*'),*(ROOT/'qualification/m03-stream-002').rglob('*'),
           HERE/'build_revision.py',HERE/'revision-notes.md',Path(__file__),ROOT/'out/m03-native-003/cargo-home/config.toml',exe]
    files += [p for folder in [LOCK,TEST,BUILD] for p in folder.iterdir() if p.is_file()]
    bound={p.relative_to(ROOT).as_posix():sha(p) for p in files if p.is_file()}
    result={
        'status':'fixed_native_candidate_003_compiled_with_terminal_observation_regressions',
        'input_sha256':bound,
        'previous_candidate_sha256':'20135ba780b86a344eeb396fd8d8f0ba85ee3600f79308140380c3fc94fbf561',
        'qualification_reused_unchanged':'qualification/m03-stream-002',
        'frozen_001_002_and_kits_unchanged':protected==helper.frozen(),
        'dependency_receipt_sha256':{n:sha(ROOT/'receipts/m03-stream-001'/n) for n in ['wire-intake-001.json','pipe-intake-001.json']},
        'executable':str(exe),'executable_sha256':sha(exe),'executable_bytes':exe.stat().st_size,
        'compiler_reported_original_executable':str(paths[0]),
        'compiler_artifact_unique_package_ids':len({r['package_id'] for r in rows if r.get('reason')=='compiler-artifact'}),
        'native_local_tests_passed':16,'qualification_core_tests_rerun':False,
        'tests_scope':'synthetic state, codec and async wait boundaries; no OS control process, native pipe or HTTP runtime',
        'fixes':[
            'same-lock Close write/ACK identity facts separate from unchanged aggregate native failure',
            'independent sticky control failure by origin survives earlier transport Unknown',
            'boundary EOF distinguished from partial/read/decode errors even after RequestClosed or Close ACK',
            'terminal router observation shares original 500ms Close budget; ACK alone never means clean control',
        ],
        'aggregate_error_semantics':'wait_close/final_control_status retain first aggregate failure; no 503/Unknown success conversion',
        'result_fields':['close_result','final_control_result','control_end_result','close_observation'],
        'clean_control_predicate':'close_written && close_acked && control_stream_ended && control_eof && sticky_control_failure == null',
        'terminal_observation_limit':'router terminal event applied, not OS thread join or own process exit; host authority TTL unchanged',
        'cli':['--morrow-native-http-v3','--fixture-base','http://127.0.0.1:PORT/v1','--evidence-dir','ABSOLUTE_NEW_BATCH_DIRECTORY_UNDER_PLUGIN_OUT','--max-chunk','1024'],
        'host_runtime_launched':False,'guest_native_executable_launched':False,'actual_core_http_run_verified':False,
        'old_s503_regraded':False,'product_success_claimed':False,'whole_session_release_verified':False,
    }
    target=HERE/'native-candidate-003.json';assert not target.exists()
    target.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'candidate':str(target),'sha256':sha(target),'executable':str(exe),'executable_sha256':sha(exe),
                      'bytes':exe.stat().st_size,'bound_files':len(bound),'packages':result['compiler_artifact_unique_package_ids']}))

if __name__=='__main__':main()
