"""Seal the new linked native/Core candidate before any host integration run."""
from pathlib import Path
import hashlib,json,shutil,tomllib
ROOT=Path(__file__).resolve().parents[2]
HERE=ROOT/'receipts/m03-stream-001'
SOURCE=ROOT/'native/m03-stream-001'
BUILD=HERE/'native-runs/build-20260928T215752Z-49e69f5e'
TEST=HERE/'native-runs/test-20260928T215835Z-936ce7f2'
OUT=ROOT/'out/m03-native-001/candidates/native-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):return json.loads(p.read_text(encoding='utf-8'))
def main():
    build,test=load(BUILD/'result.json'),load(TEST/'result.json')
    assert build['status']=='native_compiled_not_runtime_proof' and test['status']=='native_local_unit_passed'
    assert build['protected_unchanged'] and test['protected_unchanged']
    assert build['inputs_after']==test['inputs_before']==test['inputs_after']
    for name,expected in build['inputs_after'].items():assert sha(ROOT/name)==expected,name
    assert '6 passed; 0 failed' in (TEST/'stdout.txt').read_text(encoding='utf-8')
    rows=[json.loads(line) for line in (BUILD/'stdout.txt').read_text(encoding='utf-8').splitlines()]
    outputs=[Path(row['executable']) for row in rows if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')=='morrow-codex-native-http-client' and row.get('executable')]
    assert len(outputs)==1
    generated=[Path(row['out_dir'])/'native_http_capnp.rs' for row in rows if row.get('reason')=='build-script-executed' and 'morrow-native-http-stream-wire' in row.get('out_dir','')]
    assert len(generated)==1
    assert sha(generated[0])=='071955f822b2eab7a803ad2f8d6da50dde4d7371e1c1ef79009b272a4eb842c3'
    OUT.mkdir(parents=True,exist_ok=False)
    exe=OUT/outputs[0].name;shutil.copyfile(outputs[0],exe);assert sha(exe)==sha(outputs[0])
    paths=[*SOURCE.rglob('*'),HERE/'build_native.py',Path(__file__)]
    paths += [p for folder in [BUILD,TEST] for p in folder.iterdir() if p.is_file()]
    paths.append(exe)
    bound={p.relative_to(ROOT).as_posix():sha(p) for p in paths if p.is_file()}
    dep_receipts={name:sha(HERE/name) for name in ['pre-wire-candidate-001.json','wire-intake-001.json','pipe-intake-001.json']}
    old=tomllib.loads((ROOT/'qualification/m03-stream-001/Cargo.lock').read_text(encoding='utf-8'))['package']
    new=tomllib.loads((SOURCE/'Cargo.lock').read_text(encoding='utf-8'))['package']
    identity=lambda ps:{(p['name'],p['version'],p.get('source'),p.get('checksum')) for p in ps}
    receipt={'status':'fixed_native_core_candidate_compiled_ready_for_host_integration_review',
        'input_sha256':bound,'dependency_receipt_sha256':dep_receipts,
        'executable':str(exe),'executable_sha256':sha(exe),'executable_bytes':exe.stat().st_size,
        'compiler_reported_original_executable':str(outputs[0]),
        'actual_core_entry':'main::run -> request_task::start -> real ModelClient session.stream -> upstream Responses/SSE through NativeHttpSession',
        'host_schema_sha256':'8da8f1455200696d8884949f415aae4edd70cee6b583889bba7906c7730cc864',
        'generated_rust_sha256':sha(generated[0]),'generated_matches_host':True,
        'compiler_artifact_unique_package_ids':len({r['package_id'] for r in rows if r.get('reason')=='compiler-artifact'}),
        'lock_packages':len(new),'lock_added_identities':sorted(identity(new)-identity(old),key=str),
        'local_tests_passed':6,'native_host_runtime_invoked':False,'actual_core_http_run_verified':False,
        'product_success_claimed':False,'whole_session_release_verified':False,
        'cli':['--morrow-native-http-v3','--fixture-base','http://127.0.0.1:PORT/v1','--evidence-dir','ABSOLUTE_NEW_BATCH_DIRECTORY_UNDER_PLUGIN_OUT','--max-chunk','1024'],
        'outputs':['core-events.jsonl','result.json'],
        'host_pairing':'HttpPrepare must receive direct State before data RequestChunk; Head remote_address is actual SocketAddr; final ACK precedes normal RequestClosed; Close then actual child exit is observed externally',
        'first_delta_gate':'trusted harness reads a flushed actual_core_output_text_delta record; guest never writes fixture control',
        'evidence_limits':['six tests are local state/async ownership, not HTTP/Core invocation','Started.pending is not sustained OS pending','RequestClosed and local data thread join are not owner release','same-user isolation/process-tree cleanup not claimed'],
        'prior_failed_build':'native-runs/build-20260928T214921Z-8d73e7bf/result.json; MutexGuard Send lifetime fixed; failure retained'}
    path=HERE/'native-candidate-001.json';assert not path.exists()
    path.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'candidate':str(path),'sha256':sha(path),'executable':str(exe),'executable_sha256':sha(exe),
        'bytes':exe.stat().st_size,'bound_files':len(bound),'packages':receipt['compiler_artifact_unique_package_ids'],'lock_packages':len(new)}))
if __name__=='__main__':main()
