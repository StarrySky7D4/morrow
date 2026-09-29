"""Freeze host005 observation-only candidate, without running it."""
import difflib, hashlib, json, pathlib, shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    build=BASE/'native-build-20260929T001444332362Z/result.json'
    tests=BASE/'native-observation-20260929T001441361090Z/result.json'
    b=json.loads(build.read_text()); t=json.loads(tests.read_text())
    assert b['status']=='compiled_native_runtime_not_run'
    assert t['status']=='new_pipe_observation_tests_passed_no_child_or_http'
    assert b['after']==t['after']
    for path,digest in b['after'].items(): assert sha(ROOT/path)==digest,path
    previous=BASE/'native-host-candidate-004'
    pm=json.loads((previous/'manifest.json').read_text())
    for path,digest in pm['source_files'].items(): assert sha(previous/'source'/path)==digest,path
    assert sha(pathlib.Path(pm['executable']))==pm['executable_sha256']
    original=ROOT/b['artifact']['path']; assert sha(original)==b['artifact']['sha256']
    kit=BASE/'native-host-candidate-005';kit.mkdir()
    shutil.copytree(ROOT/'native_session_stream_001',kit/'source')
    exe=kit/'morrow-native-stream-host.exe';shutil.copy2(original,exe)
    changed=[]; diff=[]
    for p in sorted((kit/'source').rglob('*')):
        if not p.is_file(): continue
        rel=p.relative_to(kit/'source');old=previous/'source'/rel
        if not old.exists() or sha(old)!=sha(p):
            changed.append(rel.as_posix())
            diff.extend(difflib.unified_diff(old.read_text().splitlines(True) if old.exists() else [],p.read_text().splitlines(True),fromfile='host004/'+rel.as_posix(),tofile='host005/'+rel.as_posix()))
    (kit/'diff-from-004.patch').write_text(''.join(diff))
    notes='''# Host005: pipe-owner observations

Same v3, platform pipe, native ledger004, Core and limits. The I/O owner records
one fixed Instant domain per unique pipe instance; ns is duration encoding, not
hardware frequency/resolution. Supervisor event at_us remains receipt time.
Bounded records cover each OS write issue, at most three incomplete samples per
actual operation, cancellation receipt and its pre-CancelIoEx poll, cancellation
request, and actual reap. All carry the actual operation ID and issue ordinal,
frame offset/request length and body end. Partial-tail reissue resets sampling.
Samples require poll-before minus previous poll-after >=25ms. No samples are
manufactured during cancellation. Cancellation's extra poll can consume a real
completion; that exact result goes through normal reap handling once. Completed,
no-operation and unconfirmed-error probes are not pending qualification.

Three new local tests pass: real same-process nonreading pipe, 8192-byte write
with original 1024 requested buffers, three spaced incomplete samples, fourth
cancel probe and real error995 reap/join; early cancel remains fewer than three
samples; synthetic completion/error classifier distinction. No child, protocol
guest, Core request or HTTP was launched. The synthetic classifier test is not
a deterministic OS completion-at-cancellation race test. No partial-write fault
qualification or M03/product gate credit. Frozen host004 remains the A baseline.
'''
    (kit/'REVISION-005.md').write_text(notes)
    refs=[build,tests,tests.parent/'compiler.stdout',tests.parent/'compiler.stderr',previous/'manifest.json',ROOT/'tool/m03_native_observation_check_005.py',ROOT/'tool/m03_native_build_001.py',pathlib.Path(__file__)]
    manifest={'status':'host005_compiled_3_local_observation_tests_no_guest_or_http','executable':str(exe),'executable_sha256':sha(exe),'source_files':{p.relative_to(kit/'source').as_posix():sha(p) for p in sorted((kit/'source').rglob('*')) if p.is_file()},'changed_files_from_004':changed,'diff_sha256':sha(kit/'diff-from-004.patch'),'revision_note_sha256':sha(kit/'REVISION-005.md'),'references':{p.relative_to(ROOT).as_posix():sha(p) for p in refs},'wire_changed':False,'platform_pipe_changed':False,'core_changed':False,'ledger_version':4,'local_tests':3,'runtime_http_tested':False,'runtime_guest_tested':False,'product_gate_upgrade':False}
    (kit/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps({'manifest':str(kit/'manifest.json'),'manifest_sha256':sha(kit/'manifest.json'),'exe_sha256':sha(exe),'changed_files':changed}))
if __name__=='__main__': main()
