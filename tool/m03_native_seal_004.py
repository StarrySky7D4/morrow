"""Freeze host004 revocation provenance repair; never runs a child or HTTP."""
import difflib
import hashlib
import json
import pathlib
import shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
BASE=ROOT/'reports/codex-morrow-v1.1/host/m03-stream-001'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    build=BASE/'native-build-20260928T231828478039Z/result.json'
    tests=BASE/'native-revoke-20260928T231821762264Z/result.json'
    b=json.loads(build.read_text());t=json.loads(tests.read_text())
    assert b['status']=='compiled_native_runtime_not_run'
    assert t['status']=='new_revocation_interleavings_passed_no_child_or_http'
    for p,h in b['after'].items():assert sha(ROOT/p)==h,p
    assert b['after']==t['after']
    exe=ROOT/b['artifact']['path'];assert sha(exe)==b['artifact']['sha256']
    kit=BASE/'native-host-candidate-004';kit.mkdir()
    shutil.copytree(ROOT/'native_session_stream_001',kit/'source')
    shutil.copy2(exe,kit/'morrow-native-stream-host.exe')
    previous=BASE/'native-host-candidate-003/source';changed=[];diff=[]
    for p in sorted((kit/'source').rglob('*')):
        if not p.is_file():continue
        rel=p.relative_to(kit/'source');old=previous/rel
        if not old.exists() or sha(old)!=sha(p):
            changed.append(str(rel).replace('\\','/'))
            diff.extend(difflib.unified_diff(old.read_text().splitlines(True) if old.exists() else [],p.read_text().splitlines(True),fromfile='host003/'+str(rel),tofile='host004/'+str(rel)))
    (kit/'diff-from-003.patch').write_text(''.join(diff))
    notes='''# Host004: first durable revocation provenance

The independent positive run exposed owner Close state3 becoming visible before shared
generation2. The old poll misidentified this local write as external revoke and queued
reason19 after an error0 Close ACK snapshot.

Native ledger004 uses envelope MRNADM04 and SQLite user_version4. Existing experimental
003 profiles are rejected, not migrated or reopened as live authority. The frozen v3
Capnp contract and original Core Store/IO contracts are unchanged.

Approval.revocation_source (0 none,1 operator,2 owner runtime) and revocation_reason are
committed atomically with state3 in the existing IMMEDIATE transaction. Source1 requires
reason19; source2 requires an existing defined reason16..30. Other states require both0.
Unknown/incomplete combinations fail decoding. Parent compares its original full tuple,
normalizing only validated revocation fields for historical cleanup.

The first durable revocation result is immutable. Owner Close cannot replace operator19
that already committed. Poll applies only source1 while generation remains1. Source2 in
that same publication window is not an external event. cancel_http always reads the durable
result and only applies it once; later calls cannot overwrite error/Observed/ACK facts with
their local requested reason. A late redundant operator revoke returns the original source.
Repeated Control::Revoke during Closing also preserves the Closing owner phase.

Five deterministic regressions passed: owner commit before generation publication with
real poll; true external commit through real poll/control delivery; external commit before
Close selects19; owner Close then redundant operator preserves0 and ACK snapshot; invalid
provenance/full-tuple drift/old-profile version fail closed. Real new ledger and native pipe
thread reap/join are exercised. Completed HTTP progress is explicitly supplied test input;
there is no child, HTTP request or proof of actual response material in these tests.

First regression run retained a fixture failure: two tests marked RequestClosed before pipe
cleanup, rejected by the frozen codec. The setup now actually cancels/reaps/joins the data
thread before using RequestClosed. Final5/5 and exact-source build pass. No old component/API
suite was rerun. Commit-uncertainty fault injection is not claimed; receipts are only returned
after successful commit and uncertain errors propagate without creating send authority.

Source README describes the preceding revisions; this note is the004 delta. No new HTTP run
has occurred. Positive harness must additionally require final error_code0 and agreement with
the plugin's final control/cleanup receipt. Original successful and failed scripts stay frozen.
'''
    (kit/'REVISION-004.md').write_text(notes,encoding='utf-8')
    refs=[build,tests,tests.parent/'compiler.stdout',tests.parent/'compiler.stderr',BASE/'native-host-candidate-003/manifest.json',ROOT/'tool/m03_native_revoke_check_004.py',ROOT/'tool/m03_native_build_001.py',pathlib.Path(__file__)]
    manifest={'status':'host004_compiled_5_deterministic_interleavings_passed_no_http_run','executable':str(kit/'morrow-native-stream-host.exe'),'executable_sha256':sha(kit/'morrow-native-stream-host.exe'),'source_files':{str(p.relative_to(kit/'source')).replace('\\','/'):sha(p) for p in sorted((kit/'source').rglob('*')) if p.is_file()},'changed_files_from_003':changed,'diff_sha256':sha(kit/'diff-from-003.patch'),'revision_note_sha256':sha(kit/'REVISION-004.md'),'references':{str(p.relative_to(ROOT)).replace('\\','/'):sha(p) for p in refs},'ledger_version':4,'wire_changed':False,'core_changed':False,'new_deterministic_tests':5,'frozen_inputs_rechecked':b['frozen_count'],'runtime_http_tested':False,'product_gate_upgrade':False}
    (kit/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps({'manifest':str(kit/'manifest.json'),'manifest_sha256':sha(kit/'manifest.json'),'exe_sha256':manifest['executable_sha256'],'changed_files':changed}))
if __name__=='__main__':main()
