"""Fifth-round fixed source/producer-build review and original-executable replay."""
from pathlib import Path
import sys, os, json, hashlib, difflib, tomllib, subprocess, shutil, traceback, copy
sys.dont_write_bytecode=True
from review_p02_batch import verify_files, members
from review_p02_exec_store import source_review as previous_source_review
HERE=Path(__file__).resolve().parent
PLUGIN=Path(r'C:\Users\Administrator\Desktop\CodeXProjext\morrow-codex')
RUN=HERE/'runs/p02-core-network-003-review-001'
BATCH='receipts/p02-core-network-003'
PIN='97501761be789decabc52de2a89311d1d6a877b6dd38fa8c795f649a7b91aacb'
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def sha(b): return hashlib.sha256(b).hexdigest()
def digest(p): return sha(p.read_bytes())
def require(ok,msg):
    if not ok: raise RuntimeError(msg)
def write(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def snapshot():
    path=PLUGIN/BATCH/'handoff.json'; require(digest(path)==PIN,'new handoff drift')
    h=read(path); expected=dict(h['input_sha256']); require(len(expected)==53,'input count')
    expected[BATCH+'/handoff.json']=PIN
    for path,pin in h['prior_frozen_handoffs'].items():
        require(digest(PLUGIN/path)==pin['sha256'],'prior handoff drift')
        inputs=read(PLUGIN/path)['input_sha256']; require(len(inputs)==pin['input_count'],'old input count')
        expected.update(inputs); expected[path]=pin['sha256']
    for stage in ('build','run'):
        r=read(PLUGIN/h[stage]); require(r['exit_code']==0 and not r['changed_inputs'],'producer command failed/changed')
        require(r['inputs_before']==r['inputs_after'],'producer input drift')
        for path,d in r['inputs_before'].items():
            require(d is not None and (path not in expected or expected[path]==d),'conflicting pin '+path)
            expected[path]=d
    for path,d in expected.items(): require(digest(PLUGIN/path)==d,'current pin mismatch '+path)
    return expected
def source_review():
    old=PLUGIN/'upstream/p02-source-batch-001/codex-source'; new=PLUGIN/'upstream/p02-core-network-003/codex-work'
    manifest=read(PLUGIN/'receipts/p02-source-batch-001/codex-content-manifest.json')
    original=verify_files(old,manifest['files']); require(original['root_tree']==manifest['tree_sha1'],'original root tree')
    added={'codex-rs/core/src/morrow_network.rs','codex-rs/core/src/morrow_network_qualification.rs'}
    require(members(new)=={f['path'] for f in manifest['files']}|added,'working membership')
    changes=[]; chunks=[]
    for f in manifest['files']:
        name=f['path']; p=new/name
        require(p.resolve().is_relative_to(new.resolve()) and p.is_symlink()==(f['mode']=='120000'),'working link')
        a=os.readlink(old/name).encode() if f['mode']=='120000' else (old/name).read_bytes()
        b=os.readlink(p).encode() if f['mode']=='120000' else p.read_bytes()
        if a!=b:
            changes.append(dict(path=name,before_sha256=sha(a),after_sha256=sha(b)))
            chunks.extend(difflib.unified_diff(a.decode().splitlines(True),b.decode().splitlines(True),fromfile='a/'+name,tofile='b/'+name))
    for name in sorted(added):
        b=(new/name).read_bytes(); changes.append(dict(path=name,before_sha256=None,after_sha256=sha(b)))
        chunks.extend(difflib.unified_diff([],b.decode().splitlines(True),fromfile='/dev/null',tofile='b/'+name))
    patch=''.join(chunks).encode(); declared=read(PLUGIN/BATCH/'patch-after-build-001.json')
    require(changes==declared['changes'] and len(changes)==5,'source changes')
    require(patch==(PLUGIN/BATCH/'patch-after-build-001.patch').read_bytes() and len(patch.splitlines())==391,'patch bytes/lines')
    bridge=(new/'codex-rs/core/src/morrow_network_qualification.rs').read_text(encoding='utf-8')
    require(not any(s in bridge for s in ('force_http_fallback','try_switch_fallback_transport','disable_websockets')),'bridge manipulates fallback')
    previous=previous_source_review()
    return dict(original_files=8697,new_working_files=8699,changes=changes,patch_sha256=sha(patch),patch_lines=391,bridge_no_fallback_helper_or_flag_access=True,prior_batch002_source_and_shared_copies=previous)
def producer_review(h):
    lock=tomllib.loads((PLUGIN/'qualification/p02-core-network-003/Cargo.lock').read_text(encoding='utf-8'))['package']
    declared=read(PLUGIN/BATCH/'resolved-graphs-001.json')['graphs'][0]
    reg=[p for p in lock if p.get('source')]; local=[p for p in lock if not p.get('source')]
    require((len(lock),len(reg),len(local))==(1113,1010,103),'lock counts')
    upstream=tomllib.loads((PLUGIN/'upstream/p02-source-batch-001/codex-source/codex-rs/Cargo.lock').read_text(encoding='utf-8'))['package']
    identity=lambda pp:{(p['name'],p['version'],p.get('source'),p.get('checksum')) for p in pp}
    require(identity(reg)<=identity(upstream),'registry identity outside original lock')
    require({(p['name'],p['version'],p['checksum']) for p in reg}=={(p['name'],p['version'],p['checksum']) for p in declared['registry']},'registry report')
    for stage in ('build','run'):
        path=PLUGIN/h[stage]; r=read(path)
        for stream in ('stdout','stderr'): require(digest(path.parent/(stream+'.txt'))==r[stream+'_sha256'],'producer log binding')
        require('--locked' in r['argv'] and '--offline' in r['argv'],'producer flags')
    br=read(PLUGIN/h['build']); exe=PLUGIN/h['artifact']
    require(any(Path(a['path'])==exe and a['sha256']==h['artifact_sha256'] for a in br['artifacts']),'binary association')
    messages=[json.loads(line) for line in (PLUGIN/h['build']).parent.joinpath('stdout.txt').read_text(encoding='utf-8').splitlines() if line.startswith('{')]
    require(any(m.get('reason')=='build-finished' and m.get('success') for m in messages),'build finish')
    require(any(m.get('executable') and Path(m['executable'])==exe for m in messages),'compiler artifact')
    return dict(locked_packages=1113,registry=1010,local=103,compiler_artifact_packages=len({m['package_id'] for m in messages if m.get('reason')=='compiler-artifact'}),producer_build_evidence_verified=True,independent_compilation=False)
def validate_runtime(value):
    require(value['status']=='passed_limited_core_network_probe' and value['case_count']==6 and value['assertions']==47,'runtime result')
    names=['http_direct','websocket_disconnect','prewarm_disconnect','stream_426_natural_fallback_and_next_session','prewarm_426_natural_fallback_then_stream','preconnect_426_natural_fallback_then_stream']
    orders=[['http.stream'],['websocket.connect'],['websocket.connect'],['websocket.connect','http.stream','http.stream'],['websocket.connect','http.stream'],['websocket.connect','http.stream']]
    states=[[False,False],[True,True],[True,True],[True,False,False],[True,False,False],[True,False,False]]
    counts=[7,5,5,12,9,9]
    require(len(value['cases'])==6,'cases count')
    normalized=copy.deepcopy(value); bodies=[]
    for i,case in enumerate(normalized['cases']):
        require(case['case']==names[i] and case['assertions']==counts[i],'case identity/count')
        calls=case['backend_calls']; core=case['core_result']
        require([c['method'] for c in calls]==orders[i],'complete backend call order')
        require(core['websocket_enabled_states']==states[i],'Core fallback states')
        require(core['credential_environment_absent'] is True and core['provider_auth_fields_absent'] is True and core['personal_auth_manager'] is False and core['factory_policy']=='unavailable','auth prerequisites')
        error_http='stream_error:stream disconnected before completion: qualification-disconnected-http'
        expected_outcomes=[[error_http],['stream_error:stream disconnected before completion: qualification-disconnected-websocket'],['prewarm_error:stream disconnected before completion: qualification-disconnected-websocket'],[error_http,'next_session_error:stream disconnected before completion: qualification-disconnected-http'],['prewarm_setup_ok_without_network_success',error_http],['preconnect_setup_ok_without_network_success',error_http]][i]
        require(core['outcomes']==expected_outcomes,'Core outcomes')
        for call in calls:
            if call['method']=='http.stream':
                body=call['body_utf8'].encode('utf-8'); parsed=json.loads(body)
                require(len(body)==call['body_bytes'] and sha(body)==call['body_sha256'],'HTTP exact body binding')
                require(call['http_method']=='POST' and call['url']=='https://fixture.invalid/v1/responses','HTTP request target')
                require(parsed['model']=='qualification-model' and parsed['stream'] is True and parsed['store'] is False and parsed['input']==[],'Core request body')
                require(call['response']=='local_refusal_before_successful_stream','HTTP outcome')
                headers=dict(call['headers']); require(headers.get('accept')=='text/event-stream' and headers.get('content-type')=='application/json','HTTP headers')
                bodies.append(dict(case=names[i],bytes=len(body),sha256=sha(body)))
                call['body_utf8']=parsed; call['body_sha256']='<independently checked exact bytes; JSON object order may vary>'
                groups=[headers]
            else:
                require(call['url']=='wss://fixture.invalid/v1/responses' and call['boundary']=='before_default_API_header_merge_and_handshake','WebSocket boundary')
                require(call['response']==('local_disconnect' if i in (1,2) else 'synthetic_426'),'WebSocket injected result')
                groups=[dict(call[k]) for k in ('provider_headers','extra_headers','default_headers')]
            for headers in groups: require(not {'authorization','proxy-authorization','cookie','x-api-key'}&set(headers),'credential header')
    return normalized,bodies
def replay(h):
    profile=RUN/'profile'; temp=RUN/'tmp'
    for p in (profile/'AppData/Local',profile/'AppData/Roaming',temp): p.mkdir(parents=True,exist_ok=False)
    # Read only known OS keys individually; never enumerate environment values.
    env={}
    for key in ('SYSTEMROOT','WINDIR','COMSPEC','PATHEXT','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS'):
        value=os.environ.get(key)
        if value is not None: env[key]=value
    env.update(PATH=str(Path(env['SYSTEMROOT'])/'System32'),HOME=str(profile),USERPROFILE=str(profile),LOCALAPPDATA=str(profile/'AppData/Local'),APPDATA=str(profile/'AppData/Roaming'),TEMP=str(temp),TMP=str(temp),GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_SYSTEM=os.devnull,GIT_CONFIG_GLOBAL=os.devnull,GIT_TERMINAL_PROMPT='0')
    exe=PLUGIN/h['artifact']; require(digest(exe)==h['artifact_sha256'],'exe before run')
    output=RUN/'runtime.json'; argv=[str(exe),str(output)]
    result=subprocess.run(argv,cwd=RUN,env=env,capture_output=True,timeout=30)
    (RUN/'stdout.txt').write_bytes(result.stdout); (RUN/'stderr.txt').write_bytes(result.stderr)
    require(result.returncode==0,'independent runtime nonzero')
    actual,bodies=validate_runtime(read(output)); producer,_=validate_runtime(read(PLUGIN/h['runtime']))
    require(actual==producer,'independent runtime semantic mismatch')
    return dict(argv=argv,exit_code=0,exe_sha256=digest(exe),runtime_sha256=digest(output),producer_runtime_sha256=h['runtime_sha256'],exact_bytes_equal=output.read_bytes()==(PLUGIN/h['runtime']).read_bytes(),semantic_equal=True,comparison='HTTP bodies parsed only after independent exact byte length/hash checks; JSON object member order normalized; all remaining fields exact',HTTP_body_records=bodies,environment_keys=sorted(env),environment_values_enumerated=False,profile=str(profile),cases=6,counted_assertions=47)
def main():
    RUN.mkdir(parents=True,exist_ok=False); shutil.copyfile(__file__,RUN/'reviewer-source.py')
    r=dict(status='failed',scope='selected Core HTTP/WS refusal and synthetic426 fallback branches',independent_compilation=False,P02='blocked',J00='blocked',G0='blocked',product_graphs='0/2',product_acceptance_verified=0,product_acceptance_not_run=84)
    try:
        before=snapshot(); write(RUN/'inputs-before.json',before); h=read(PLUGIN/BATCH/'handoff.json')
        r['sources']=source_review(); r['producer_build']=producer_review(h); write(RUN/'pre-run-review.json',r)
        r['independent_runtime']=replay(h)
        after=snapshot(); write(RUN/'inputs-after.json',after); require(after==before,'frozen input drift')
        require(source_review()==r['sources'],'source or shared dependency drift')
        r.update(status='verified_limited',input_count=len(before),frozen_inputs_unchanged=True,cases=6,counted_assertions=47)
    except Exception as error:
        r['error']=str(error); (RUN/'exception.txt').write_text(traceback.format_exc(),encoding='utf-8')
    write(RUN/'result.json',r); print(json.dumps(r,ensure_ascii=False)); return 0 if r['status']=='verified_limited' else 1
if __name__=='__main__': raise SystemExit(main())
