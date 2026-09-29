"""Freeze host003 Welcome-code fix with one new strict positive process check. No client launch or network invocation."""
import hashlib,json,pathlib,shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    build=BASE/'native-build-20260928T225754496446Z/result.json'
    api=BASE/'native-api-20260928T221417555348Z/result.json'
    b=json.loads(build.read_text());assert b['status']=='compiled_native_runtime_not_run'
    assert json.loads(api.read_text())['status']=='new_http_adapter_api_tests_passed_no_runtime_pair'
    for p,h in b['after'].items():
        if not p.endswith('/README.md'):assert sha(ROOT/p)==h,p
    exe=ROOT/b['artifact']['path'];assert sha(exe)==b['artifact']['sha256']
    kit=BASE/'native-host-candidate-003';kit.mkdir()
    shutil.copytree(ROOT/'native_session_stream_001',kit/'source')
    shutil.copy2(exe,kit/'morrow-native-stream-host.exe')
    close=BASE/'native-welcome-20260928T225819377451Z/result.json';assert json.loads(close.read_text())['status']=='passed'
    refs=[build,api,close,ROOT/'tool/m03_native_welcome_check_003.py',BASE/'native-code-audit-003.md',BASE/'native-host-candidate-002/manifest.json',BASE/'transport-20260928T210834849150Z/result.json',BASE/'transport-kit-001/manifest.json',BASE/'wire-kit-001/manifest.json',BASE/'pipe-kit-001/manifest.json',ROOT/'tool/m03_native_build_001.py',ROOT/'tool/m03_native_api_check_001.py',pathlib.Path(__file__)]
    frozen={'status':'compiled_host_candidate_003_strict_Welcome_positive_passed_no_http_pair','source_files':{str(p.relative_to(kit/'source')).replace('\\','/'):sha(p) for p in sorted((kit/'source').rglob('*')) if p.is_file()},'executable':str(kit/'morrow-native-stream-host.exe'),'executable_sha256':sha(kit/'morrow-native-stream-host.exe'),'references':{str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in refs},'adapter_api_tests':5,'api_test_owner_rows':'simulated, actual Core Store/native ledger, no child and no HTTP','post_test_edits':'No source edit after final build/new positive; previous API/Close checks referenced only, not rerun', 'new_strict_welcome_positive_cases':1, 'previous_close_cases_not_rerun':3, 'partial_write_fault_injected':False,'frozen_inputs_rechecked_by_build':b['frozen_count'],'pairing_status':'awaiting joint delta review and coordinator resume with fixed plugin002; prior candidates not reused for HTTP','core_endpoint_mapping':'32-byte v3 opaque endpoint_ref to 64 printable lowercase hex bytes in existing Core request','product_gate_credit':False}
    import difflib
    previous=BASE/'native-host-candidate-002/source';diffs=[];changed=[]
    for p in sorted((kit/'source').rglob('*')):
        if p.is_file():
            rel=p.relative_to(kit/'source');old=previous/rel
            if not old.exists() or sha(old)!=sha(p):
                changed.append(str(rel).replace('\\','/'))
                diffs.extend(difflib.unified_diff(old.read_text().splitlines(True) if old.exists() else [],p.read_text().splitlines(True),fromfile='host002/'+str(rel),tofile='host003/'+str(rel)))
    assert sorted(changed)==['src/bin/morrow-native-close-peer.rs','src/supervisor.rs'],changed
    (kit/'diff-from-002.patch').write_text(''.join(diffs))
    frozen['changed_files_from_002']=changed;frozen['diff_sha256']=sha(kit/'diff-from-002.patch')
    (kit/'manifest.json').write_text(json.dumps(frozen,indent=2)+'\n')
    print(json.dumps({'manifest':str(kit/'manifest.json'),'manifest_sha256':sha(kit/'manifest.json'),'exe_sha256':frozen['executable_sha256']}))
if __name__=='__main__':main()
