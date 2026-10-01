from pathlib import Path
import os,subprocess,json,hashlib,shutil,time,argparse,sys
root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description='Local actual Windows explicit owner recovery qualification; fresh libraries; no CI or replay')
parser.add_argument('--out',required=True)
args=parser.parse_args()
if os.name != "nt":raise RuntimeError("This actual product qualification requires Windows")
base=(root/args.out).resolve();base.relative_to(root);assert not base.exists();base.mkdir(parents=True)
check=base/'check';check.mkdir()
roots=['workbench_host','native_pipe_win_001','native_session_stream_001','plugin_runtime','network_node','core','audit','lib','test','windows','tool','plugins/workbench','sdk/rust/contracts','contracts','packages','shaders','l10n','third_party/um_decrypt']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def inventory():
 out={}
 for part in roots:
  for directory,folders,names in os.walk(root/part):
   folders[:]=[v for v in folders if v not in {'target','build','.git','__pycache__'}]
   for name in names:
    p=Path(directory)/name
    if p.suffix in {'.rs','.toml','.lock','.proto','.capnp','.dart','.ps1','.py','.txt','.yaml','.yml','.h','.c','.cc','.cpp','.frag','.cmake','.dll'}:out[p.relative_to(root).as_posix()]=sha(p)
 for rel in ['pubspec.yaml','pubspec.lock','analysis_options.yaml']:
  p=root/rel
  if p.is_file():out[rel]=sha(p)
 return out
before=inventory();commands=[]
def run(label,args,env=None):
 if label=='flutter-analyze':
  analyzer_local=check/'analyzer-local';analyzer_local.mkdir()
  env=dict(os.environ,LOCALAPPDATA=str(analyzer_local))
 started=time.time()
 with (check/(label+'.log')).open('wb') as output:code=subprocess.run(args,cwd=root,env=env,stdout=output,stderr=subprocess.STDOUT).returncode
 row={'label':label,'args':args,'exit_code':code,'elapsed_seconds':time.time()-started,'log_sha256':sha(check/(label+'.log'))};commands.append(row);print(json.dumps(row),flush=True)
 if code:raise RuntimeError(label+' failed')
try:
 run('owner-recovery-evidence-auditor-tests',[sys.executable,'tool/tests/test_m03_owner_recovery_011_process.py','-v'])
 run('flutter-analyze',[r'C:\flutter\bin\flutter.bat','analyze','--no-pub','lib/main_rust.dart','lib/plugins/application_shutdown.dart','lib/plugins/session_coordinator.dart','lib/plugins/workbench_native.dart','lib/plugins/workbench_channel_supervised.dart','lib/plugins/workbench_supervision.dart','lib/plugins/workbench_recovery.dart','test/workbench_supervised_native_test.dart','test/workbench_supervised_service_native_test.dart','test/workbench_supervision_test.dart','lib/plugins/workbench_owner_management.dart','test/workbench_owner_recovery_native_test.dart','test/workbench_owner_recovery_test.dart'])
 run('product-build',['cargo','build','--offline','--locked','--manifest-path','workbench_host/Cargo.toml','--bin','morrow-workbench-supervisor','--bin','morrow-workbench-host','--jobs','1','--target-dir','target/m03-product-supervisor-009'])
 run('service-fixture-build',['cargo','build','--offline','--locked','--manifest-path','workbench_host/Cargo.toml','--example','package_service_run_fixture','--jobs','1','--target-dir','target/m03-product-supervisor-009'])
 candidate=base/'candidate';assert not candidate.exists();candidate.mkdir()
 for name in ['morrow-workbench-host.exe','morrow-workbench-supervisor.exe']:
  source=root/'target/m03-product-supervisor-009/debug'/name;target=candidate/name;shutil.copy2(source,target);assert sha(source)==sha(target)
 package=root/'build/workbench-host/bundle/workbench.morrowplugin';shutil.copy2(package,candidate/package.name)
 packager=root/'target/m03-product-supervisor-009/debug/examples/package_service_run_fixture.exe';shutil.copy2(packager,candidate/packager.name)
 run('service-fixture-package',[str(candidate/packager.name),str(candidate/'service-bootstrap.mplugin')])
 for rel,digest in before.items():
  source=root/rel;assert sha(source)==digest;target=candidate/'source'/rel;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target);assert sha(target)==digest
 pins={p.name:sha(p) for p in candidate.iterdir() if p.is_file()}
 (candidate/'manifest.json').write_text(json.dumps({'source_files':before,'files':pins,'build_check':str(check),'ci':False,'production_path':'Flutter -> independent workbench-supervisor -> actual Rust workbench-host -> Wasmi'},indent=2)+'\n',encoding='utf-8')
 run('host-ledger-and-gate-tests',['cargo','test','--offline','--locked','--manifest-path','workbench_host/Cargo.toml','--lib','--jobs','1','--target-dir','target/m03-product-supervisor-009','workbench_supervision::','--','--test-threads=1'])
 run('host-product-gate-tests',['cargo','test','--offline','--locked','--manifest-path','workbench_host/Cargo.toml','--lib','--jobs','1','--target-dir','target/m03-product-supervisor-009','product_gate::','--','--test-threads=1'])
 run('host-real-worker-shutdown',['cargo','test','--offline','--locked','--manifest-path','workbench_host/Cargo.toml','--bin','morrow-workbench-host','--jobs','1','--target-dir','target/m03-product-supervisor-009','shutdown_tests::','--','--test-threads=1'])
 run('flutter-ui-and-session',[r'C:\flutter\bin\flutter.bat','test','--no-pub','--concurrency=1','test/workbench_supervision_test.dart','test/session_coordinator_test.dart','test/application_shutdown_test.dart','test/workbench_channel_test.dart','test/workbench_owner_recovery_test.dart'])
 evidence=base/'run';assert not evidence.exists();evidence.mkdir()
 env=dict(os.environ,MORROW_WORKBENCH_HOST=str(candidate/'morrow-workbench-host.exe'),MORROW_WORKBENCH_SUPERVISOR=str(candidate/'morrow-workbench-supervisor.exe'),MORROW_WORKBENCH_PACKAGE=str(candidate/package.name),MORROW_PRODUCT_SUPERVISOR_EVIDENCE=str(evidence),MORROW_SERVICE_RUN_PACKAGE=str(candidate/'service-bootstrap.mplugin'),MORROW_SERVICE_RUN_PACKAGER=str(candidate/'package_service_run_fixture.exe'))
 run('flutter-real-workbench',[r'C:\flutter\bin\flutter.bat','test','--no-pub','--concurrency=1','test/workbench_supervised_native_test.dart','test/workbench_supervised_service_native_test.dart','test/workbench_owner_recovery_native_test.dart'],env)
 run('owner-recovery-raw-sqlite-audit',[sys.executable,'tool/m03_owner_recovery_011_process.py','audit-native',str(evidence),'--out',str(check/'owner-raw-audit'),'--candidate',str(candidate),'--expected-manifest-sha256',sha(candidate/'manifest.json')])
finally:
 after=inventory();receipt={'sources_before':before,'sources_after':after,'source_unchanged':before==after,'commands':commands};(check/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')

 files={p.relative_to(base).as_posix():sha(p) for p in base.rglob('*') if p.is_file()}
 (base/'evidence-manifest.json').write_text(json.dumps({'files':files,'source_unchanged':before==after,'ci':False,'scope':'focused actual Windows product and explicit owner recovery qualification; no full G0 or SDK freeze'},indent=2)+'\n',encoding='utf-8')
