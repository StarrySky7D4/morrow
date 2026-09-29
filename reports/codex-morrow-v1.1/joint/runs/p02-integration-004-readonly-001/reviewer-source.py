"""Read-only batch004 audit. Does not invoke Cargo, probe executables or networks."""
from pathlib import Path
import sys, json, hashlib, os, difflib, tomllib, shutil, traceback
sys.dont_write_bytecode=True
from review_p02_batch import verify_files, members
from review_p02_core_network import source_review as previous_source_review, validate_runtime as check_network
from review_p02_exec_store import check_store, check_exec
HERE=Path(__file__).resolve().parent
PLUGIN=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
RUN=HERE/'runs/p02-integration-004-readonly-001'
BATCH='receipts/p02-integration-004'
PIN='513b1cfbf4b19b32c247b06810430ae29b1e1b779078b9761100a86c7a8d1751'
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def sha(b): return hashlib.sha256(b).hexdigest()
def digest(p): return sha(p.read_bytes())
def require(ok,msg):
    if not ok: raise RuntimeError(msg)
def write(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def snapshot():
    p=PLUGIN/BATCH/'handoff.json'; require(digest(p)==PIN,'new handoff identity')
    h=read(p); values=dict(h['input_sha256']); require(len(values)==83,'new input count')
    values[BATCH+'/handoff.json']=PIN
    for p,pin in h['prior_frozen_handoffs'].items():
        require(digest(PLUGIN/p)==pin['sha256'],'prior handoff drift')
        old=read(PLUGIN/p)['input_sha256']; require(len(old)==pin['input_count'],'prior input count')
        values.update(old); values[p]=pin['sha256']
    for stage in ('build','run'):
        r=read(PLUGIN/h[stage]); require(r['exit_code']==0 and not r['changed_inputs'],'producer outcome')
        require(r['inputs_before']==r['inputs_after'],'producer input drift')
        for p,d in r['inputs_before'].items():
            require(d is not None and (p not in values or values[p]==d),'conflicting input '+p); values[p]=d
    for p,d in values.items(): require(digest(PLUGIN/p)==d,'input mismatch '+p)
    return values
def source_review():
    old=PLUGIN/'upstream/p02-source-batch-001/codex-source'; new=PLUGIN/'upstream/p02-integration-004/codex-work'
    manifest=read(PLUGIN/'receipts/p02-source-batch-001/codex-content-manifest.json')
    original=verify_files(old,manifest['files']); require(original['root_tree']==manifest['tree_sha1'],'original git tree')
    added={'codex-rs/core/src/'+n for n in ('morrow_network.rs','morrow_network_qualification.rs','morrow_p02_qualification.rs')}
    require(members(new)=={f['path'] for f in manifest['files']}|added,'integration membership')
    changes=[]; diff=[]
    for f in manifest['files']:
        name=f['path']; p=new/name
        require(p.resolve().is_relative_to(new.resolve()) and p.is_symlink()==(f['mode']=='120000'),'source link')
        a=os.readlink(old/name).encode() if f['mode']=='120000' else (old/name).read_bytes()
        b=os.readlink(p).encode() if f['mode']=='120000' else p.read_bytes()
        if a!=b:
            changes.append(dict(path=name,before_sha256=sha(a),after_sha256=sha(b)))
            diff.extend(difflib.unified_diff(a.decode().splitlines(True),b.decode().splitlines(True),fromfile='a/'+name,tofile='b/'+name))
    for name in sorted(added):
        b=(new/name).read_bytes(); changes.append(dict(path=name,before_sha256=None,after_sha256=sha(b)))
        diff.extend(difflib.unified_diff([],b.decode().splitlines(True),fromfile='/dev/null',tofile='b/'+name))
    patch=''.join(diff).encode(); declared=read(PLUGIN/BATCH/'patch-after-build-001.json')
    require(len(changes)==12 and changes==declared['changes'],'actual integration delta')
    require(len(patch.splitlines())==693 and patch==(PLUGIN/BATCH/'patch-after-build-001.patch').read_bytes(),'actual patch identity')
    previous=previous_source_review()
    return dict(original_files=8697,working_files=8700,modified_files=9,added_files=3,changes=changes,patch_lines=693,patch_sha256=sha(patch),previous_source_audit=previous)
def guard_order():
    root=PLUGIN/'upstream/p02-integration-004/codex-work/codex-rs'; declared=read(PLUGIN/BATCH/'guard-order-and-host-inputs.json')
    specs=[('core/src/unified_exec/process_manager.rs','restricted-qualification: injected exec required','let inherited_fds = spawn_lifecycle.inherited_fds();'),('core/src/exec.rs','restricted-qualification: independent exec disabled','let ExecRequest {'),('core/src/client.rs','restricted-qualification: injected HTTP required','let client = create_client_for_route('),('core/src/client.rs','restricted-qualification: injected WebSocket required','ApiWebSocketResponsesClient::new(api_provider, api_auth)'),('thread-store/src/live_thread.rs','restricted_qualification_create_before_git','let metadata_sync = ThreadMetadataSync::for_create(&params).await;'),('thread-store/src/live_thread.rs','restricted_qualification_local_resume_before_state_db','let should_load_history = params.history.is_none();')]
    checked=[]
    for name,guard,effect in specs:
        path=root/name; text=path.read_text(encoding='utf-8'); start=text.index(guard); finish=text.index(effect,start)
        require(start<finish,'guard placement')
        entry=next(c for c in declared['checks'] if c['file']==name and c['guard_line']==text[:start].count('\n')+1)
        require(digest(path)==entry['file_sha256'],'guard file hash')
        checked.append(dict(file=name,guard_line=text[:start].count('\n')+1,effect_line=text[:finish].count('\n')+1,source_order_only=True))
    for h in declared['host_preflight']:
        require(digest(Path(h['original']))==digest(PLUGIN/h['copy'])==h['sha256'],'host original/copy identity')
    return checked
def build_graph(h):
    evidence=read(PLUGIN/BATCH/'build-graph-evidence.json'); meta_path=Path(evidence['metadata'])
    require(digest(meta_path)==evidence['metadata_sha256'],'metadata binding'); meta=read(meta_path)
    lock=tomllib.loads((PLUGIN/'qualification/p02-integration-004/Cargo.lock').read_text(encoding='utf-8'))['package']
    reg=[p for p in lock if p.get('source')]; local=[p for p in lock if not p.get('source')]
    require((len(lock),len(reg),len(local))==(1117,1013,104),'lock counts')
    originals=[]
    for p in ('upstream/p02-source-batch-001/codex-source/codex-rs/Cargo.lock','upstream/p02-exec-store-002/host-kit-003/Cargo.lock'):
        originals+=tomllib.loads((PLUGIN/p).read_text(encoding='utf-8'))['package']
    ident=lambda pp:{(p['name'],p['version'],p.get('source'),p.get('checksum')) for p in pp}
    require(ident(reg)<=ident(originals),'registry identities')
    require({(p['name'],p['version'],p.get('source')) for p in lock}=={(p['name'],p['version'],p.get('source')) for p in meta['packages']},'metadata identities')
    root=PLUGIN/'upstream/p02-integration-004/codex-work/codex-rs'
    codex=[p for p in meta['packages'] if p['source'] is None and p['name'].startswith('codex-')]
    require(codex and all(Path(p['manifest_path']).is_relative_to(root) for p in codex),'multiple Codex source roots')
    for stage in ('build','run'):
        path=PLUGIN/h[stage]; r=read(path)
        require('--locked' in r['argv'] and '--offline' in r['argv'],'producer flags')
        for label in ('stdout','stderr'): require(digest(path.parent/(label+'.txt'))==r[label+'_sha256'],'log identity')
    build_path=PLUGIN/h['build']; br=read(build_path)
    require(digest(build_path)==evidence['build_sha256'],'build evidence binding')
    messages=[json.loads(line) for line in build_path.parent.joinpath('stdout.txt').read_text(encoding='utf-8').splitlines() if line.startswith('{')]
    require(any(m.get('reason')=='build-finished' and m.get('success') for m in messages),'build finished')
    artifacts=[m for m in messages if m.get('reason')=='compiler-artifact']; require(len({m['package_id'] for m in artifacts})==905,'artifact count')
    exe=PLUGIN/h['artifact']; require(digest(exe)==h['artifact_sha256'],'exe current identity')
    require(any(m.get('executable') and Path(m['executable'])==exe for m in artifacts),'compiler exe binding')
    require(any(Path(a['path'])==exe and a['sha256']==h['artifact_sha256'] for a in br['artifacts']),'build receipt exe binding')
    features={}
    nodes={n['id']:n for n in meta['resolve']['nodes']}
    for name,details in evidence['packages'].items():
        p=next(p for p in meta['packages'] if p['name']==name)
        require(p['id']==details['package_id'] and Path(p['manifest_path'])==Path(details['manifest_path']),'package binding')
        compiled={tuple(sorted(m['features'])) for m in artifacts if m['package_id']==p['id']}
        require(compiled=={tuple(sorted(details['compiler_features']))},'actual compiler features')
        require(sorted(nodes[p['id']]['features'])==sorted(details['compiler_features']),'resolved feature binding')
        features[name]=details['compiler_features']
    require(features['codex-core']==['morrow-p02-network-qualification','morrow-p02-qualification','morrow-p02-restricted-qualification'] and features['codex-thread-store']==['morrow-p02-restricted-qualification'],'restriction not actually compiled')
    return dict(lock_packages=1117,registry=1013,local=104,Codex_path_packages_same_root=len(codex),compiler_artifact_identities=905,compiled_features=features,producer_build_verified=True,independent_build=False,independent_run=False,executable_sha256=digest(exe))
def runtime_review(h):
    path=PLUGIN/h['runtime']; require(digest(path)==h['runtime_sha256'],'runtime identity'); v=read(path)
    require(v['status']=='passed_limited_integration_probe' and v['case_count']==24 and v['assertions']==109 and len(v['cases'])==24,'producer runtime totals')
    cases=v['cases']; require(sum(c['assertions'] for c in cases)==109,'assertion sum')
    require(v['runtime_dropped_before_receipt'] is True and v['qualification_only'] is True,'runtime scope fields')
    shared=cases[0]; require(shared['case']=='three_seams_shared_lifetime' and shared['assertions']==9,'shared case')
    require(shared['remaining_strong_owners']==[0,0,0,0] and 'Unsupported' in shared['cleanup'] and 'NOT established' in shared['cleanup'],'owner/writer distinction')
    require(shared['persist_error']=='thread-store unsupported operation: persist_thread_requires_M04','shared persist result')
    require('qualification-disconnected:Invalid' in shared['exec_error'] and len(shared['exec_calls'])==1 and len(shared['network_calls'])==1,'shared real calls')
    for i in (1,2):
        c=cases[i]; require(c['case']=='default_exec_new_task' and c['assertions']==2 and c['tty']==(i==2),'default guard case')
        require(c['result']['spawn_lifecycle_calls']==[] and 'injected exec required' in c['result']['error'],'default guard outcome')
    c=cases[3]; require(c['case']=='independent_exec_new_task' and c['result']['after_spawn_calls']==0 and 'independent exec disabled' in c['result']['error'],'independent exec guard')
    for i,expected in [(4,'injected HTTP required'),(5,'injected WebSocket required')]:
        c=cases[i]; require(c['case']=='missing_network_new_task' and c['expected_guard']==expected and len(c['result']['outcomes'])==1 and expected in c['result']['outcomes'][0],'missing network guard')
        require(c['result']['credential_environment_absent'] and c['result']['provider_auth_fields_absent'] and c['result']['personal_auth_manager'] is False,'missing network auth')
    for i,mode in [(6,'Legacy'),(7,'Paginated')]:
        c=cases[i]; require(c['case']=='local_store_resume_new_task' and c['mode']==mode and 'restricted_qualification_local_resume_before_state_db' in c['error'],'local resume guard')
    c=cases[8]; require(c['case']=='create_before_git_new_task' and c['adapter_calls']==[] and c['fixture_home_created'] is False and 'restricted_qualification_create_before_git' in c['error'],'create guard')
    check_exec(dict(status='passed_limited_exec_callsite_probe',case_count=3,assertions=27,cases=cases[9:12]))
    check_store(dict(status='passed_limited_store_callsite_probe',case_count=6,assertions=12,cases=cases[12:18]))
    _,bodies=check_network(dict(status='passed_limited_core_network_probe',case_count=6,assertions=47,cases=cases[18:24]))
    c=shared['network_calls'][0]; b=c['body_utf8'].encode(); require(len(b)==c['body_bytes'] and sha(b)==c['body_sha256'],'shared HTTP body')
    parsed=json.loads(b); require(parsed['model']=='qualification-model' and parsed['stream'] is True and parsed['store'] is False,'shared HTTP semantics')
    require(c['method']=='http.stream' and c['http_method']=='POST' and c['url']=='https://fixture.invalid/v1/responses','shared HTTP target')
    bodies.insert(0,dict(case='three_seams_shared_lifetime',bytes=len(b),sha256=sha(b)))
    # Validate actual exec parameters from shared lifetime using the same semantic checker.
    imported=read(PLUGIN/'receipts/p02-exec-probe-002/runtime-20260928T131658Z-84c272f896.json')
    imported['cases'][0]['calls']=shared['exec_calls']; imported['cases'][0]['upstream_returned_error']=shared['exec_error']
    check_exec(imported)
    return dict(producer_cases=24,producer_counted_assertions=109,shared_lifetime_cases=1,new_task_guard_cases=8,rebuilt_prior_cases=15,HTTP_bodies_rehashed=bodies,explicit_discard_and_complete_store_sequence='source assertions verified; sequence not serialized in shared case',owner_release_only=True,writer_release=False,independent_execution=False)
def main():
    RUN.mkdir(parents=True,exist_ok=False); shutil.copyfile(__file__,RUN/'reviewer-source.py')
    r=dict(status='failed',scope='read-only source, same-build feature and producer runtime audit',independent_build=False,independent_run=False,P02='blocked',J00='blocked',G0='blocked',product_graphs='0/2',product_acceptance_verified=0,product_acceptance_not_run=84)
    try:
        before=snapshot(); write(RUN/'inputs-before.json',before); h=read(PLUGIN/BATCH/'handoff.json')
        r['sources']=source_review(); r['guards']=guard_order(); r['same_build']=build_graph(h); r['producer_runtime']=runtime_review(h)
        after=snapshot(); write(RUN/'inputs-after.json',after); require(before==after,'frozen inputs changed')
        require(source_review()==r['sources'],'source/copies/dependencies changed')
        r.update(status='verified_read_only',input_count=len(before),inputs_unchanged=True)
    except Exception as error:
        r['error']=str(error); (RUN/'exception.txt').write_text(traceback.format_exc(),encoding='utf-8')
    write(RUN/'result.json',r); print(json.dumps({k:v for k,v in r.items() if k!='sources'},ensure_ascii=False)); return 0 if r['status']=='verified_read_only' else 1
if __name__=='__main__': raise SystemExit(main())
