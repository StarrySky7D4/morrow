"""Fourth-round source/receipt review and isolated replay of pinned producer binaries."""
from pathlib import Path
import json, hashlib, os, sys, subprocess, difflib, tomllib, re, uuid, copy, traceback, shutil
sys.dont_write_bytecode=True
from review_p02_batch import verify_files, members
HERE=Path(__file__).resolve().parent
PLUGIN=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
RUN=HERE/'runs/p02-exec-store-002-review-001'
PIN='d3c19e26c5c04461eed188ff1d84518add5d8af711c3dfc78e6e49ad8312cd76'
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def sha(b): return hashlib.sha256(b).hexdigest()
def digest(p): return sha(p.read_bytes())
def require(ok,msg):
    if not ok: raise RuntimeError(msg)
def write(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def snapshot():
    file=PLUGIN/'receipts/p02-exec-store-002/handoff.json'; require(digest(file)==PIN,'new handoff drift')
    handoff=read(file); expected=dict(handoff['input_sha256']); require(len(expected)==129,'handoff count')
    expected['receipts/p02-exec-store-002/handoff.json']=PIN
    for path,h in handoff['previous_frozen_handoffs'].items():
        require(digest(PLUGIN/path)==h,'prior handoff changed')
        values=read(PLUGIN/path)['input_sha256']; require(len(values)==handoff['previous_frozen_input_counts'][path],'prior input count')
        expected.update(values); expected[path]=h
    for s in handoff['slices']:
        for stage in ('build','run'):
            result=read(PLUGIN/s[stage]); require(result['exit_code']==0 and not result['changed_inputs'],'producer command failure/drift')
            require(result['inputs_before']==result['inputs_after'],'producer input snapshots')
            for path,h in result['inputs_before'].items():
                require(h is not None,'missing producer input'); require(path not in expected or expected[path]==h,'inconsistent source pin')
                expected[path]=h
    for path,h in expected.items(): require(digest(PLUGIN/path)==h,'input mismatch '+path)
    return expected
def source_review():
    manifest=read(PLUGIN/'receipts/p02-source-batch-001/codex-content-manifest.json')
    old=PLUGIN/'upstream/p02-source-batch-001/codex-source'; new=PLUGIN/'upstream/p02-exec-store-002/codex-work'
    original=verify_files(old,manifest['files']); require(original['root_tree']==manifest['tree_sha1'],'original tree')
    declared=read(PLUGIN/'receipts/p02-exec-store-002/patch-after-build-001.json')
    known={f['path'] for f in manifest['files']}; added='codex-rs/core/src/morrow_p02_qualification.rs'
    require(members(new)==known|{added},'worktree source membership')
    changes=[]; chunks=[]
    for f in manifest['files']:
        name=f['path']; p=new/name; require(p.resolve().is_relative_to(new.resolve()),'working link escape')
        require(p.is_symlink()==(f['mode']=='120000'),'working link mode')
        a=os.readlink(old/name).encode() if f['mode']=='120000' else (old/name).read_bytes()
        b=os.readlink(p).encode() if f['mode']=='120000' else p.read_bytes()
        if a!=b:
            changes.append(dict(path=name,before_sha256=sha(a),after_sha256=sha(b)))
            chunks.extend(difflib.unified_diff(a.decode().splitlines(True),b.decode().splitlines(True),fromfile='a/'+name,tofile='b/'+name))
    b=(new/added).read_bytes(); changes.append(dict(path=added,before_sha256=None,after_sha256=sha(b)))
    chunks.extend(difflib.unified_diff([],b.decode().splitlines(True),fromfile='/dev/null',tofile='b/'+added))
    patch=''.join(chunks).encode()
    require(changes==declared['changes'] and len(changes)==5,'actual source delta')
    require(patch==(PLUGIN/'receipts/p02-exec-store-002/patch-after-build-001.patch').read_bytes(),'patch exact bytes')
    require(len(patch.splitlines())==163,'patch lines')
    copies=read(PLUGIN/'receipts/p02-exec-store-002/working-copy-before.json')['other_copies']
    for base,files in copies.items():
        require(members(PLUGIN/base)==set(files),'copy membership')
        for name,h in files.items(): require(digest(PLUGIN/base/name)==h,'host/fork copy drift')
    dependencies=[]
    for label,receipt in [('mxc','mxc-verification-005'),('nucleo','nucleo-verification-002')]:
        proof=read(PLUGIN/('receipts/p02-exec-store-002/'+receipt+'.json'))
        checked=verify_files(Path(proof['source_root']),proof['files'])
        commit=read(PLUGIN/('receipts/p02-exec-store-002/'+label+'-commit.json'))
        require(checked['root_tree']==commit['tree']['sha'],'dependency root tree '+label)
        require(proof['commit']==commit['sha'],'dependency commit')
        dependencies.append(dict(name=label,commit=proof['commit'],files=checked['files'],tree=checked['root_tree']))
    return dict(original_files=8697,working_files=8698,source_tree=original['root_tree'],patch_sha256=sha(patch),patch_lines=163,changes=changes,copy_files=sum(len(f) for f in copies.values()),additional_dependencies=dependencies)
def graphs_and_builds(handoff):
    report=read(PLUGIN/'receipts/p02-exec-store-002/resolved-graphs-001.json'); result=[]
    upstream=tomllib.loads((PLUGIN/'upstream/p02-source-batch-001/codex-source/codex-rs/Cargo.lock').read_text(encoding='utf-8'))['package']
    host=tomllib.loads((PLUGIN/'upstream/p02-exec-store-002/host-kit-003/Cargo.lock').read_text(encoding='utf-8'))['package']
    identities=lambda packages:{(p['name'],p['version'],p.get('source'),p.get('checksum')) for p in packages}
    for s in handoff['slices']:
        label=s['slice']; lock=tomllib.loads((PLUGIN/f'qualification/p02-{label}-probe-002/Cargo.lock').read_text(encoding='utf-8'))['package']
        declared=next(g for g in report['graphs'] if g['graph']==label); reg=[p for p in lock if p.get('source')]; local=[p for p in lock if not p.get('source')]
        require((len(lock),len(reg),len(local))==(declared['packages'],declared['registry_count'],declared['path_count']),'graph counts')
        require(all(p['source']=='registry+https://github.com/rust-lang/crates.io-index' for p in reg),'unexpected remote source')
        require(identities(reg)<=identities(upstream)|identities(host),'registry identity drift')
        require({(p['name'],p['version'],p['checksum']) for p in reg}=={(p['name'],p['version'],p['checksum']) for p in declared['registry']},'declared registry tuples')
        for stage in ('build','run'):
            receipt_path=PLUGIN/s[stage]; r=read(receipt_path)
            for stream in ('stdout','stderr'): require(digest(receipt_path.parent/(stream+'.txt'))==r[stream+'_sha256'],'producer log binding')
            require('--locked' in r['argv'] and '--offline' in r['argv'],'producer cargo flags')
        br=read(PLUGIN/s['build']); exe=PLUGIN/s['artifact']
        require(any(Path(a['path'])==exe and a['sha256']==s['artifact_sha256'] for a in br['artifacts']),'producer binary association')
        messages=[json.loads(line) for line in (PLUGIN/s['build']).parent.joinpath('stdout.txt').read_text(encoding='utf-8').splitlines() if line.startswith('{')]
        require(any(m.get('reason')=='build-finished' and m.get('success') for m in messages),'producer build finished')
        require(any(m.get('executable') and Path(m['executable'])==exe for m in messages),'compiler executable path')
        result.append(dict(slice=label,resolved_packages=len(lock),registry_packages=len(reg),local_packages=len(local),producer_compilation='evidence_verified_not_independently_rebuilt',executable_sha256=digest(exe),producer_input_count=len(br['inputs_before'])))
    return result
def check_store(v):
    require((v['status'],v['case_count'],v['assertions'])==('passed_limited_store_callsite_probe',6,12),'store result')
    sequence=[
      ('resume_disconnected',['resume_thread'],'qualification-disconnected:open_writer'),
      ('resume_history_failure_discards',['resume_thread','load_history','discard_thread'],'qualification-disconnected:read_after'),
      ('append_disconnected_no_metadata',['resume_thread','load_history','append_items'],'qualification-disconnected:append_batch'),
      ('standard_persist_unsupported',['resume_thread','load_history','persist_context:Standard','persist_thread_requires_M04'],'thread-store unsupported operation: persist_thread_requires_M04'),
      ('flush_unsupported',['resume_thread','load_history','flush_thread'],'thread-store unsupported operation: flush_thread'),
      ('live_history_disconnected',['resume_thread','load_history','load_history'],'qualification-disconnected:read_after')]
    require(len(v['cases'])==6,'store cases')
    for case,(name,calls,error) in zip(v['cases'],sequence):
        require(case['case']==name and case['calls']==calls and error in case['error'] and case['assertions']==2,'store semantics '+name)
        require(not any('metadata' in c for c in case['calls']),'unexpected metadata call')
def check_exec(v):
    require((v['status'],v['case_count'],v['assertions'])==('passed_limited_exec_callsite_probe',3,27),'exec result')
    require(len(v['cases'])==3,'exec cases')
    order=['processId','argv','cwd','envPolicy','env','tty','pipeStdin','arg0','sandbox','enforceManagedNetwork','managedNetwork']
    for index,case in enumerate(v['cases']):
        require(len(case['calls'])==1 and case['filesystem_http_adapter_calls']==[],'adapter calls')
        require(case['assertions']==[8,8,11][index] and case['tty']==[False,True,False][index],'exec assertions')
        call=case['calls'][0]; p=call['params']
        require(call['method']=='ExecBackend::start' and set(p)==set(order),'parameter fields')
        require(p['argv']==['morrow-qualification-never-launch','fixture-argument'] and p['cwd']=='file:///C:/morrow-qualification-nonexistent','actual argv/cwd')
        require(p['env']=={} and p['tty']==case['tty'] and p['pipeStdin'] is False and p['enforceManagedNetwork'] is False,'actual environment/tty')
        require(all(p[k] is None for k in ['envPolicy','arg0','sandbox','managedNetwork']),'sandbox fixture identity')
        require(p['processId'].startswith('2-') and uuid.UUID(p['processId'][2:]).version==4,'upstream generated process handle')
        body=json.dumps({k:p[k] for k in order},separators=(',',':')).encode()
        require(sha(body)==call['params_sha256'],'independent ExecParams digest')
        require(call['proposal_bytes']==440 and re.fullmatch('[0-9a-f]{64}',call['proposal_sha256']),'proposal digest shape')
        expected='qualification-disconnected:Invalid' if index<2 else 'qualification-unsupported:003 has no argv execution, PTY or process IO contract; requires M-06 (related M-02)'
        require(expected in case['error'],'exec original refusal')
    normalized=copy.deepcopy(v)
    for case in normalized['cases']:
        call=case['calls'][0]; call['params']['processId']='<upstream UUID>'
        call['params_sha256']='<validated digest of UUID-bearing parameters>'; call['proposal_sha256']='<UUID-dependent wire digest; not independently decoded>'
    return normalized
def replay(handoff):
    profile=RUN/'profile'; tmp=RUN/'tmp'
    for path in (profile/'AppData/Local',profile/'AppData/Roaming',tmp): path.mkdir(parents=True,exist_ok=False)
    env={k:v for k,v in os.environ.items() if k.upper() in {'SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS'}}
    env.update(PATH=str(Path(os.environ['SYSTEMROOT'])/'System32'),HOME=str(profile),USERPROFILE=str(profile),LOCALAPPDATA=str(profile/'AppData/Local'),APPDATA=str(profile/'AppData/Roaming'),TEMP=str(tmp),TMP=str(tmp),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_SYSTEM=os.devnull,GIT_CONFIG_GLOBAL=os.devnull,GIT_TERMINAL_PROMPT='0')
    result=[]
    for s in handoff['slices']:
        label=s['slice']; exe=PLUGIN/s['artifact']; require(digest(exe)==s['artifact_sha256'],'exe changed before run')
        output=RUN/(label+'-runtime.json'); argv=[str(exe),str(output)]
        executed=subprocess.run(argv,cwd=RUN,env=env,capture_output=True,timeout=30)
        (RUN/(label+'-stdout.txt')).write_bytes(executed.stdout); (RUN/(label+'-stderr.txt')).write_bytes(executed.stderr)
        require(executed.returncode==0,'independent runtime failed '+label)
        actual=read(output); producer=read(PLUGIN/s['runtime'])
        if label=='store':
            check_store(actual); check_store(producer); require(actual==producer,'store receipt differs')
            comparison='byte_equal'; require(output.read_bytes()==(PLUGIN/s['runtime']).read_bytes(),'store byte mismatch')
        else:
            require(check_exec(actual)==check_exec(producer),'exec receipt semantic difference')
            comparison='equal after normalizing upstream UUID processId and dependent params/proposal digests; params digest independently recomputed'
        result.append(dict(slice=label,argv=argv,exit_code=0,exe_sha256=digest(exe),runtime_sha256=digest(output),cases=s['cases_passed'],assertions=s['assertions_passed'],comparison=comparison))
    write(RUN/'environment-policy.json',dict(keys=sorted(env),personal_environment_inherited=False,profile=str(profile),cwd=str(RUN),no_os_monitor=True))
    return result
def main():
    RUN.mkdir(parents=True,exist_ok=False); shutil.copyfile(__file__,RUN/'reviewer-source.py')
    result=dict(status='failed',scope='source/producer-build evidence review and independent replay of pinned executables',independent_compilation=False,P02='blocked',J00='blocked',G0='blocked',product_graphs='0/2',product_acceptance_verified=0,product_acceptance_not_run=84)
    try:
        before=snapshot(); write(RUN/'inputs-before.json',before)
        result['sources']=source_review(); h=read(PLUGIN/'receipts/p02-exec-store-002/handoff.json')
        result['producer_builds_and_locks']=graphs_and_builds(h); write(RUN/'pre-run-review.json',result)
        result['independent_runtime']=replay(h)
        after=snapshot(); write(RUN/'inputs-after.json',after); require(after==before,'frozen inputs changed')
        require(source_review()==result['sources'],'source/copies/dependencies changed')
        result.update(status='verified_limited',input_count=len(before),frozen_inputs_unchanged=True,cases=9,assertions=39)
    except Exception as error:
        result['error']=str(error); (RUN/'exception.txt').write_text(traceback.format_exc(),encoding='utf-8')
    write(RUN/'result.json',result); print(json.dumps(result,ensure_ascii=False)); return 0 if result['status']=='verified_limited' else 1
if __name__=='__main__': raise SystemExit(main())
