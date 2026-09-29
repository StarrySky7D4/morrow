"""Freeze compiled host001 only. No client launch or network invocation."""
import hashlib,json,pathlib,shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    build=BASE/'native-build-20260928T221537587036Z/result.json'
    api=BASE/'native-api-20260928T221417555348Z/result.json'
    b=json.loads(build.read_text());assert b['status']=='compiled_native_runtime_not_run'
    assert json.loads(api.read_text())['status']=='new_http_adapter_api_tests_passed_no_runtime_pair'
    for p,h in b['after'].items():assert sha(ROOT/p)==h,p
    exe=ROOT/b['artifact']['path'];assert sha(exe)==b['artifact']['sha256']
    kit=BASE/'native-host-candidate-001';kit.mkdir()
    shutil.copytree(ROOT/'native_session_stream_001',kit/'source')
    shutil.copy2(exe,kit/'morrow-native-stream-host.exe')
    refs=[build,api,BASE/'transport-20260928T210834849150Z/result.json',BASE/'transport-kit-001/manifest.json',BASE/'wire-kit-001/manifest.json',BASE/'pipe-kit-001/manifest.json',ROOT/'tool/m03_native_build_001.py',ROOT/'tool/m03_native_api_check_001.py',pathlib.Path(__file__)]
    frozen={'status':'compiled_host_candidate_001_no_process_or_http_pair','source_files':{str(p.relative_to(kit/'source')).replace('\\','/'):sha(p) for p in sorted((kit/'source').rglob('*')) if p.is_file()},'executable':str(kit/'morrow-native-stream-host.exe'),'executable_sha256':sha(kit/'morrow-native-stream-host.exe'),'references':{str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in refs},'adapter_api_tests':5,'api_test_owner_rows':'simulated, actual Core Store/native ledger, no child and no HTTP','post_test_edits':'rustfmt and documentation only; final exact source rebuilt','frozen_inputs_rechecked_by_build':b['frozen_count'],'pairing_status':'waiting for corrected plugin002; plugin001 must not be run','core_endpoint_mapping':'32-byte v3 opaque endpoint_ref to 64 printable lowercase hex bytes in existing Core request','product_gate_credit':False}
    (kit/'manifest.json').write_text(json.dumps(frozen,indent=2)+'\n')
    print(json.dumps({'manifest':str(kit/'manifest.json'),'manifest_sha256':sha(kit/'manifest.json'),'exe_sha256':frozen['executable_sha256']}))
if __name__=='__main__':main()
