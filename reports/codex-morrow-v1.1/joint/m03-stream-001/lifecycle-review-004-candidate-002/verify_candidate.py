"""Verify saved candidate/build bindings; no runner imports or builds."""
from pathlib import Path
import hashlib, json

W=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow\build\io-safety-refactor')
P=W.parent.parent.parent/'morrow-codex'
J=Path(__file__).resolve().parent
C=W/'reports/codex-morrow-v1.1/host/m03-lifecycle-004/candidate-002'
EXPECTED='fa206f562d30b59686fcb7abfad7ff84467a0b7659f7a32db343cd782f4c2a13'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def save(name,value):
    with (J/name).open('x',encoding='utf-8') as f:
        json.dump(value,f,indent=2);f.write('\n')
pins={}
def pin(p,h):
    assert sha(p)==h,str(p)
    pins[str(p)]=h
pin(C/'manifest.json',EXPECTED)
m=read(C/'manifest.json')
pin(C/'build-receipt.json',m['build_receipt_sha256'])
b=read(C/'build-receipt.json')
assert b['status']=='passed' and b['source_unchanged'] is True
assert b['sources_before']==b['sources_after']==m['source_files']
assert len(b['commands'])==2 and all(x['exit_code']==0 for x in b['commands'])
assert [x['command'][1] for x in b['commands']]==['build','test']
assert all('--locked' in x['command'] and '--offline' in x['command'] for x in b['commands'])
for rel,h in m['source_files'].items():
    pin(W/rel,h);pin(C/'source'/rel,h)
for rel,h in m['helper_sha256'].items(): pin(W/rel,h)
pin(Path(m['executable']),m['executable_sha256'])
assert Path(m['executable'])==C/'morrow-native-stream-host.exe'
assert '28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out' in (C/'tests.log').read_text(encoding='utf-8')
pm=P/'receipts/m03-fixture-002/native-candidate-fixture-002.json'
pin(pm,m['guest_manifest_sha256']);pin(Path(read(pm)['executable']),m['guest_exe_sha256'])
assert m['guest_manifest_sha256']=='6a8d4d07aab156d72f5fce73dea7750f8b5424707dae84595182ff7f55c211a5'
assert m['guest_exe_sha256']=='b895a7d78cd6c3c3641e502ad63b167cf5b78bdfaab75bfde079710efb0a08bc'
assert m['product_G0']=='not_run' and m['sdk_frozen'] is False

runs=[]
for mode,stamp in [('core-revoke','20260929T213241184115Z'),('data-pending','20260929T213307996034Z')]:
    r=C/'runs'/('revoke-011-'+mode+'-'+stamp)
    locked=read(r/'locked-expectations.json');bind=read(r/'candidate-binding.json')
    assert bind['manifest']==str(C/'manifest.json') and bind['manifest_sha256']==EXPECTED
    assert bind['mode']==mode and bind['exit_code']==0 and bind['automatic_retry'] is False
    assert bind['helper_sha256']==m['helper_sha256']
    assert bind['runner_sha256']==m['source_files']['tool/m03_lifecycle_004.py']
    assert locked['host']==['lifecycle004',EXPECTED,m['executable_sha256']]
    assert locked['plugin_manifest_sha256']==m['guest_manifest_sha256'] and locked['plugin_exe_sha256']==m['guest_exe_sha256']
    assert locked['original_ttl_ms']==10000 and locked['handshake_ms']==6000 and locked['close_ms']==1000
    assert locked['one_post_only'] and locked['automatic_retry'] is False
    launch=read(r/'launch.json')['args']
    assert launch[0]==m['executable'] and launch[launch.index('--client')+1]==read(pm)['executable']
    assert launch[launch.index('--sha256')+1]==m['guest_exe_sha256']
    runs.append({'mode':mode,'path':str(r),'binding_sha256':sha(r/'candidate-binding.json')})

old=C.parent/'candidate-001';old_m=read(old/'manifest.json')
changed=[k for k,v in m['source_files'].items() if old_m['source_files'].get(k)!=v]
assert set(old_m['source_files'])==set(m['source_files'])
assert changed==['tool/m03_lifecycle_004.py'] and old_m['executable_sha256']==m['executable_sha256']
failed=old/'runs/revoke-011-core-revoke-20260929T213056059898Z/result.json'
pin(failed,'2d01efa63f11a671814e7e8df3d5d5b8d80f248317b594afaed6bb781803dee1')
fr=read(failed)
assert fr['status']=='failed' and fr['post_count']==0 and fr['host_exit']==2 and fr['host_reader_joined']
assert all(x['joined'] for x in fr['fixture_threads'])
runner=(C/'source/tool/m03_lifecycle_004.py').read_text(encoding='utf-8')
assert 'harness.PLUGIN = GUEST_ROOT' in runner
assert 'choices=("core-revoke", "data-pending")' in runner
for p,h in pins.items(): assert sha(Path(p))==h,p
save('candidate-inputs.json',{'pins':pins,'source_count':len(m['source_files']),
     'build_log_sha256':sha(C/'build.log'),'tests_log_sha256':sha(C/'tests.log'),
     'full_dependency_compiler_trace_verified':False,'old_fixture79_rescanned':False})
save('candidate-result.json',{'status':'saved_candidate_bindings_verified','manifest_sha256':EXPECTED,
     'host_exe_sha256':m['executable_sha256'],'current_and_snapshot_source_count':len(m['source_files']),
     'before_after_sources_equal':True,'producer_library_tests':28,'reviewer_builds_or_tests':0,
     'runtime_cases_bound':runs,'old_candidate001_still_failed':True,'old_candidate001_post_count':0,
     'candidate001_002_changed_sources':changed,'fixed_helpers_and_fixture_unchanged':True,
     'original_deadline_chain_qualified':False,'nonzero_short_completion_qualified':False,
     'product_gate_upgrade':False,'sdk_frozen':False})
print(json.dumps({'status':'candidate_verified','source_count':len(m['source_files']),
                  'candidate_result_sha256':sha(J/'candidate-result.json')}))
