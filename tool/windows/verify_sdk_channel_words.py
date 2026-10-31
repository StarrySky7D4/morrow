"""Fresh Windows Wasmi qualification of the current SDK word-equality implementation.
The historical fixtures and artifact pins stay immutable; only copied SDK locks refresh.
The new profile expects the measured 11-call/ACK3 fuel fault; limits stay fixed.
"""
from pathlib import Path
import argparse, hashlib, importlib.util, json, os, re, shutil, subprocess, sys, time
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--run-id',required=True)
parser.add_argument('--evidence-root',type=Path,required=True)
parser.add_argument('--refresh-copied-sdk-locks',action='store_true',required=True)
args=parser.parse_args()
assert re.fullmatch(r'[0-9]{4,8}',args.run_id)
C=Path(__file__).resolve().parents[2];B=args.evidence_root.resolve()
cache_root=Path('D:/build_cache')
assert B.is_relative_to(cache_root) and B.is_dir()
for parent in (B,*B.parents):assert not parent.is_symlink() and not parent.is_junction()
O=B/('windows-sdk-words-'+args.run_id);W=cache_root/('sdk-words'+args.run_id)
assert not O.exists() and not W.exists() and not os.environ.get('MORROW_DISPOSABLE_WINDOWS_TEST')
sys.path.insert(0,str(C/'tool'))
import windows_qualification_build_cache as cache
import plugin_sdk_lock as sdk_lock
spec=importlib.util.spec_from_file_location('source1218',C/'tool/verify_structural_repairs.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
SOURCE=v.source_digest()
assert shutil.disk_usage('D:/build_cache').free>=6*2**30,'ACTUAL_D_6GIB_RESERVE'
modes=['bounded-reusable-001','bounded-oneshot-001','standard-reusable-002','standard-oneshot-002']
projects=[C/'sdk/fixtures'/('rust-channel-'+x) for x in modes]
sdk_inventory=sdk_lock.inventory(C/'sdk')
locks=[]
for project in projects:
    prior=sdk_lock.parse((project/sdk_lock.LOCK_NAME).read_bytes())
    assert set(prior)==set(sdk_inventory)
    changed={name for name in prior if prior[name]!=sdk_inventory[name]}
    assert changed <= {'rust/src/channel.rs'}, ('UNREVIEWED_SDK_LOCK_DRIFT',sorted(changed))
    locks.append(dict(original_lock_sha256=hashlib.sha256((project/sdk_lock.LOCK_NAME).read_bytes()).hexdigest(),reviewed_source_changes=sorted(changed)))
names=[x.decode() for x in subprocess.check_output(['git','ls-files','-z','--','sdk'],cwd=C).split(b'\0') if x]
assert len(names)==344 and all((C/n).is_file() for n in names)
O.mkdir();W.mkdir()
def put(n,x):
    with (O/n).open('x',encoding='utf-8') as f:json.dump(x,f,indent=2);f.flush();os.fsync(f.fileno())
def check_source():
    assert v.source_digest()==SOURCE,'CANONICAL_SOURCE_CHANGED'
def run(label,command,env,timeout=None):
    put(label+'-reserved.json',dict(command=[str(x) for x in command],source_digest=SOURCE))
    with (O/(label+'.stdout')).open('xb') as out,(O/(label+'.stderr')).open('xb') as err:
        p=subprocess.Popen(command,cwd=C,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
        put(label+'-process.json',dict(pid=p.pid));start=time.monotonic()
        try:code=p.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            put(label+'-unknown.json',dict(status='UNKNOWN_ORIGINAL_PROCESS_RETAINED_NO_KILL_OR_RETRY',pid=p.pid,timeout_seconds=timeout))
            raise RuntimeError('ORIGINAL_PROCESS_UNKNOWN_NO_REPLAY')
    row=dict(exit_code=code,original_process_returned=True,seconds=round(time.monotonic()-start,3),stdout=cache.pin(O/(label+'.stdout')),stderr=cache.pin(O/(label+'.stderr')))
    put(label+'-result.json',row);return row

copied=[]
for n in names:
    src=C/n
    assert not src.is_symlink() and not src.is_junction()
    dst=W/n;dst.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dst)
    pin=cache.pin(src);assert cache.pin(dst)['sha256']==pin['sha256'];copied.append(dict(relative=n,source=pin,copy=cache.pin(dst)))
for p,lock in zip(projects,locks):
    project=W/p.relative_to(C)
    assert hashlib.sha256((project/sdk_lock.LOCK_NAME).read_bytes()).hexdigest()==lock['original_lock_sha256']
    sdk_lock.write(project,W/'sdk',update=True)
    sdk_lock.verify(project,W/'sdk',required=True)
for row in copied:
    if row['relative'].endswith('/sdk.lock.toml') and any(row['relative']==(p.relative_to(C)/sdk_lock.LOCK_NAME).as_posix() for p in projects):
        row['copy']=cache.pin(W/row['relative'])
binding=dict(source_digest=SOURCE,sdk_inputs=copied,programs=['plugin_package','plugin_check','channel_word_equality_sdk'],core_manifest=cache.pin(C/'core/Cargo.toml'),core_lock=cache.pin(C/'core/Cargo.lock'),runtime_manifest=cache.pin(C/'plugin_runtime/Cargo.toml'),runtime_lock=cache.pin(C/'plugin_runtime/Cargo.lock'),fixture_modes=modes,historical_assertions_unchanged=True,historical_test=cache.pin(C/'plugin_runtime/tests/channel_bounded_sdk.rs'),limits=dict(fuel=20000000,memory_bytes=16777216,host_calls=16),historical_profile='SEPARATE_ORIGINAL_FILE_AND_PINS_UNMODIFIED',copied_source_lock_refresh=locks,negative_boundary=dict(host_calls=11,last_acked=3))
sha=hashlib.sha256(json.dumps(binding,sort_keys=True).encode()).hexdigest()
target=cache.reserve_target('sdkwords'+args.run_id,'WINDOWS',sha)
assert not list(target.iterdir())
compact=subprocess.run(['C:/Windows/System32/compact.exe','/C','/Q',str(target)],capture_output=True,creationflags=subprocess.CREATE_NO_WINDOW);assert compact.returncode==0 and not list(target.iterdir())
env={k:val for k,val in cache.environment(O/'tmp').items() if not k.startswith('MORROW_')}
tc=Path('C:/Users/Administrator/.rustup/toolchains/1.96.0-x86_64-pc-windows-msvc/bin')
env.update(RUSTUP_TOOLCHAIN='1.96.0',CAPNP='C:/Users/Administrator/capnp-bin/capnp.exe',CARGO_TARGET_DIR=str(target),CARGO_BUILD_TARGET='x86_64-pc-windows-msvc',CARGO_PROFILE_RELEASE_DEBUG='0',PYTHONDONTWRITEBYTECODE='1',PYTHONUTF8='1')
env['PATH']=str(tc)+os.pathsep+env['PATH']
put('input-binding.json',binding)
put('target.json',dict(target=str(target),input_sha256=sha,empty_before_first_compiler=True,compiled_cache_shared_with_prior_runs=False,source_copy=str(W)))
phases=[];artifacts={};failure=None
try:
    for i,mode in enumerate(modes,1):
        check_source();project=W/'sdk/fixtures'/('rust-channel-'+mode)
        assert not (project/'build').exists() and not (project/'dist').exists()
        print('FRESH_WINDOWS_WASM_BUILD_AND_TOOL_PACK '+mode,flush=True)
        result=run('pack-'+str(i),[sys.executable,'-B',str(C/'tool/morrow_plugin.py'),'pack',str(project),'--sdk-root',str(W/'sdk'),'--require-sdk-lock'],env)
        phases.append(result);assert result['exit_code']==0,'ORIGINAL_PACKAGER_FAILURE'
        packages=list((project/'dist').glob('*.mplugin'));assert len(packages)==1
        wasm=project/'build/plugin.wasm';assert wasm.read_bytes()[:8]==b'\0asm\x01\0\0\0'
        key=mode.removesuffix('-001').removesuffix('-002')
        prefix='MORROW_WORD_'+key.removeprefix('bounded-').replace('-','_').upper()
        artifacts[mode]=dict(wasm=cache.pin(wasm),package=cache.pin(packages[0]),copied_sdk_lock=cache.pin(project/'sdk.lock.toml'))
        for suffix,pin in [('WASM',artifacts[mode]['wasm']),('PACKAGE',artifacts[mode]['package'])]:env[prefix+'_'+suffix]=pin['path'];env[prefix+'_'+suffix+'_SHA256']=pin['sha256']
        put('artifacts-'+str(i)+'.json',artifacts[mode])
    check_source()
    command=[str(tc/'cargo.exe'),'rustc','--manifest-path',str(C/'plugin_runtime/Cargo.toml'),'--locked','--offline','--release','--features','packages','--test','channel_word_equality_sdk','--target','x86_64-pc-windows-msvc','--target-dir',str(target),'--message-format=json','--jobs','1','--','-C','metadata=morrow_windows_words_'+args.run_id+'_'+SOURCE]
    print('BUILD_CURRENT_WINDOWS_WASMI_HARNESS',flush=True)
    build=run('harness-build',command,env);assert build['exit_code']==0
    rows=[json.loads(x) for x in (O/'harness-build.stdout').read_bytes().splitlines() if x.startswith(b'{')]
    chosen=[x for x in rows if x.get('reason')=='compiler-artifact' and x['target']['name']=='channel_word_equality_sdk' and x.get('executable')]
    assert len(chosen)==1 and chosen[0]['fresh'] is False and chosen[0]['profile']['test']
    exe=Path(chosen[0]['executable']);dest=O/exe.name;shutil.copy2(exe,dest);exe_pin=cache.pin(dest);assert exe_pin['sha256']==cache.pin(exe)['sha256']
    put('harness-artifact.json',dict(executable=exe_pin,compiler=chosen[0]))
    listed=run('list',[str(dest),'--list','--format','terse'],env,30);assert listed['exit_code']==0
    methods=[line[:-6] for line in (O/'list.stdout').read_text().splitlines() if line.endswith(': test')]
    assert len(methods)==8 and len(set(methods))==8
    put('runtime-plan.json',dict(methods=methods,artifacts=artifacts,host=exe_pin,each_exact_current_profile_assertion=True,no_budget_or_semantic_change=True,automatic_retry=False,VM_calls=0))
    runtime=[]
    for i,name in enumerate(methods,1):
        assert cache.pin(dest)==exe_pin
        print('WINDOWS_WASMI_WORD_PROFILE_ASSERTION '+name,flush=True)
        label=f'test-{i:02d}'
        result=run(label,[str(dest),name,'--exact','--ignored','--test-threads=1','--nocapture'],env,180)
        log=(O/(label+'.stdout')).read_text(errors='replace')
        passed=result['exit_code']==0 and 'test result: ok. 1 passed; 0 failed; 0 ignored;' in log
        assert len(re.findall(r'^test '+re.escape(name)+r' \.\.\. ',log,re.M))==1
        runtime.append(dict(name=name,passed=passed,**result));put(label+'-acceptance.json',runtime[-1])
    check_source()
    for item in copied:assert cache.pin(C/item['relative'])==item['source'] and cache.pin(W/item['relative'])==item['copy']
    passed=all(x['passed'] for x in runtime)
    put('result.json',dict(status='PASS_SCOPED_WINDOWS_WASMI_WORD_EQUALITY_MATRIX' if passed else 'FAILED_WINDOWS_WASMI_WORD_EQUALITY_MATRIX',source_digest=SOURCE,source_unchanged=True,canonical_sdk_inputs_unchanged=len(copied),copied_fixture_locks_refreshed=4,actual_methods=len(runtime),passed=sum(x['passed'] for x in runtime),failed=sum(not x['passed'] for x in runtime),methods=runtime,artifacts=artifacts,all_original_processes_returned=True,budgets_unchanged=True,old_fixture_pins_not_rewritten=True,VM_calls=0,full_SDK26_accepted=False,SDK_FREEZE_accepted=False,source_accepted=False,release_eligible=False))
    print('WINDOWS_WASMI_MATRIX_COMPLETE_PASS='+str(sum(x['passed'] for x in runtime))+' FAIL='+str(sum(not x['passed'] for x in runtime)),flush=True)
except BaseException as e:
    put('failure.json',dict(status='STOPPED_NOT_ACCEPTED_NO_AUTOMATIC_RETRY',error=type(e).__name__,message=str(e),completed_packaging_phases=len(phases),VM_calls=0))
    raise

if not passed: raise SystemExit(1)
