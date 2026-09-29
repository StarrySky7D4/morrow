"""Freeze one combined build/run, preserving failures and previous frozen inputs."""
from pathlib import Path
import hashlib,json,os
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
def read(p):return json.loads(p.read_text(encoding='utf-8'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def walk(value):
    if isinstance(value,dict):
        yield value
        for item in value.values():yield from walk(item)
    elif isinstance(value,list):
        for item in value:yield from walk(item)
def main():
    start=read(HERE/'start.json')
    for relative,record in start['frozen_handoffs'].items():
        path=ROOT/relative;assert sha(path)==record['sha256']
        for item,expected in read(path)['input_sha256'].items():assert sha(ROOT/item)==expected,item
    results=[(p,read(p)) for p in (HERE/'runs').glob('*/result.json')]
    builds=[x for x in results if x[1]['stage']=='build' and x[1]['status']=='compiled_not_runtime_proof']
    runs=[x for x in results if x[1]['stage']=='run' and x[1]['status']=='passed_limited_integration_probe']
    build_path,build=max(builds,key=lambda x:x[1]['run_id']);run_path,run=max(runs,key=lambda x:x[1]['run_id'])
    runtime=Path(run['runtime_receipt']);report=read(runtime)
    executable=ROOT/'out/p02-integration-004/target/x86_64-pc-windows-msvc/debug/p02-integration-probe.exe'
    assert sha(executable)==build['artifacts'][0]['sha256']==run['expected_cargo_bin_artifact']['sha256']
    assert sha(runtime)==run['runtime_sha256']
    assert report['status']=='passed_limited_integration_probe' and report['case_count']==24 and report['assertions']==109
    assert len(report['cases'])==24 and sum(c['assertions'] for c in report['cases'])==109
    http_count=0
    for item in walk(report):
        if item.get('method')=='http.stream':
            body=item['body_utf8'].encode();assert hashlib.sha256(body).hexdigest()==item['body_sha256'] and len(body)==item['body_bytes'];http_count+=1
    assert http_count==6
    before=read(HERE/'patch-before-build-003.json');after=read(HERE/'patch-after-build-001.json');assert before==after
    graph=read(HERE/'build-graph-evidence.json');assert graph['executable_sha256']==sha(executable)
    inputs=set();source=ROOT/'upstream/p02-integration-004/codex-work'
    for row in after['changes']:
        p=source/row['path'];assert sha(p)==row['after_sha256'];inputs.add(p)
    deps={}
    for name in ['mxc-verification-005.json','nucleo-verification-002.json']:
        p=ROOT/'receipts/p02-exec-store-002'/name;dep=read(p);base=Path(dep['source_root'])
        for row in dep['files']:
            target=base/row['path'];data=os.readlink(target).encode() if row['git_mode']=='120000' else target.read_bytes()
            assert hashlib.sha256(data).hexdigest()==row['sha256']
        deps[name]=len(dep['files']);inputs.add(p)
    for base in [HERE,ROOT/'qualification/p02-integration-004']:
        inputs.update(p for p in base.rglob('*') if p.is_file() and p.name!='handoff.json' and '__pycache__' not in p.parts)
    inputs.update([executable,ROOT/'out/p02-integration-004/cargo-home/config.toml',ROOT/'receipts/p02-source-batch-001/codex-content-manifest.json'])
    hashes={p.relative_to(ROOT).as_posix():sha(p) for p in sorted(inputs)}
    result={'schema_version':1,'batch':'p02-integration-004','ready_for_review':True,'status':'passed_limited_integration_probe','cases_passed':24,'assertions_passed':109,'http_bodies_rehashed':http_count,'build':build_path.relative_to(ROOT).as_posix(),'run':run_path.relative_to(ROOT).as_posix(),'runtime':runtime.relative_to(ROOT).as_posix(),'runtime_sha256':sha(runtime),'artifact':executable.relative_to(ROOT).as_posix(),'artifact_sha256':sha(executable),'source_delta_files':len(after['changes']),'source_delta_patch_lines':after['patch_lines'],'source_delta_sha256':after['patch_sha256'],'prior_frozen_handoffs':start['frozen_handoffs'],'shared_dependency_files_rechecked':deps,'input_sha256':hashes,'P-02':'not_complete','J-00':'not_complete','G0':'blocked','product_graphs':'0/2','product_84_cases':'not_run','host_003':'unchanged','limits':['Same-source restricted qualification build only, not production Core loop/native/Wasm graphs','New task guard errors and callback observations are not OS-wide isolation','Direct LocalThreadStore/LocalProcess, shell capture, Realtime and other defaults remain outside guarded entries','All LiveThread create refused including injected; no safe creation claimed','No successful process/network/durable commit/writer release; discard Unsupported is not cleanup success','Normal product build, Bazel, all metadata contexts, cancellation/reconnect/auth recovery unverified','M-02/M-03/M-04/M-06/M-08 remain; no alternative schema, credentials, real provider, commits or push']}
    with (HERE/'handoff.json').open('x',encoding='utf-8') as f:json.dump(result,f,indent=2);f.write('\n')
    print(json.dumps({'handoff_sha256':sha(HERE/'handoff.json'),'inputs':len(hashes),'cases':24,'assertions':109,'artifact_sha256':sha(executable)}))
if __name__=='__main__':main()
