"""Freeze only actual successful build/run evidence plus exact source identity."""
from pathlib import Path
import hashlib
import json
import os

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

def main():
    start=read(HERE/'start.json')
    for relative, record in start['frozen_handoffs'].items():
        path=ROOT/relative
        if sha(path)!=record['sha256']:
            raise RuntimeError('Previous handoff changed')
        for item, expected in read(path)['input_sha256'].items():
            if sha(ROOT/item)!=expected:
                raise RuntimeError('Previous input changed: '+item)
    successful=[]
    for path in (HERE/'runs').glob('*/result.json'):
        result=read(path)
        if result['status'] in ('compiled_not_runtime_proof','passed_limited_core_network_probe'):
            successful.append((path,result))
    builds=[x for x in successful if x[1]['stage']=='build']
    runs=[x for x in successful if x[1]['stage']=='run']
    if not builds or not runs:
        raise RuntimeError('Missing actual successful build/run')
    build_path,b=max(builds,key=lambda x:x[1]['run_id'])
    run_path,r=max(runs,key=lambda x:x[1]['run_id'])
    runtime=Path(r['runtime_receipt'])
    report=read(runtime)
    executable=ROOT/'out/p02-core-network-003/target/x86_64-pc-windows-msvc/debug/p02-core-network-probe.exe'
    if b['artifacts'][0]['sha256']!=sha(executable) or r['expected_cargo_bin_artifact']['sha256']!=sha(executable) or r['runtime_sha256']!=sha(runtime):
        raise RuntimeError('Runtime or executable identity changed')
    if report['status']!='passed_limited_core_network_probe' or report['case_count']!=6 or sum(c['assertions'] for c in report['cases'])!=report['assertions']:
        raise RuntimeError('Incorrect runtime result')
    for case in report['cases']:
        for call in case['backend_calls']:
            if call['method']=='http.stream':
                body=call['body_utf8'].encode()
                if hashlib.sha256(body).hexdigest()!=call['body_sha256'] or len(body)!=call['body_bytes']:
                    raise RuntimeError('Captured body mismatch')
        if not case['core_result']['credential_environment_absent'] or not case['core_result']['provider_auth_fields_absent']:
            raise RuntimeError('Credential preconditions absent')
    before=read(HERE/'patch-before-build-002.json')
    after=read(HERE/'patch-after-build-001.json')
    if before!=after:
        raise RuntimeError('Source patch drift during build')
    source=ROOT/'upstream/p02-core-network-003/codex-work'
    inputs=set()
    for row in after['changes']:
        path=source/row['path']
        if sha(path)!=row['after_sha256']:
            raise RuntimeError('Patched source drifted')
        inputs.add(path)
    # Recheck shared source dependencies rather than relying only on old receipts.
    shared_counts={}
    for name in ('mxc-verification-005.json','nucleo-verification-002.json'):
        path=ROOT/'receipts/p02-exec-store-002'/name
        dep=read(path)
        base=Path(dep['source_root'])
        for row in dep['files']:
            target=base/row['path']
            data=os.readlink(target).encode() if row['git_mode']=='120000' else target.read_bytes()
            if hashlib.sha256(data).hexdigest()!=row['sha256']:
                raise RuntimeError('Shared dependency changed')
        shared_counts[name]=len(dep['files'])
        inputs.add(path)
    for base in (HERE,ROOT/'qualification/p02-core-network-003'):
        inputs.update(p for p in base.rglob('*') if p.is_file() and p.name!='handoff.json' and '__pycache__' not in p.parts)
    inputs.update((executable,ROOT/'out/p02-core-network-003/cargo-home/config.toml',ROOT/'receipts/p02-source-batch-001/codex-content-manifest.json'))
    hashes={p.relative_to(ROOT).as_posix():sha(p) for p in sorted(inputs)}
    result={'schema_version':1,'batch':'p02-core-network-003','ready_for_review':True,'status':'passed_limited_core_network_probe','cases_passed':6,'assertions_passed':report['assertions'],'build':build_path.relative_to(ROOT).as_posix(),'run':run_path.relative_to(ROOT).as_posix(),'runtime':runtime.relative_to(ROOT).as_posix(),'runtime_sha256':sha(runtime),'artifact':executable.relative_to(ROOT).as_posix(),'artifact_sha256':sha(executable),'source_delta_files':len(after['changes']),'source_delta_patch_lines':after['patch_lines'],'source_delta_sha256':after['patch_sha256'],'prior_frozen_handoffs':start['frozen_handoffs'],'shared_dependency_files_rechecked':shared_counts,'input_sha256':hashes,'P-02':'not_complete','G0':'blocked','J-00':'not_complete','product_graphs':'0/2','product_84_cases':'not_run','host_003':'unchanged, not a successful network adapter','limits':['Actual Core selected branch qualification, not full agent loop or production session integration','Synthetic426 is local injected error; setup Ok is not network success','No successful HTTP/SSE/WebSocket, established-connection reconnect, auth recovery, Realtime/unary or real host IPC','Only bridge feature-gated; backend API and routing changes require future product review','M-03/M-08 gaps remain; no OS isolation or global bypass proof; no commit/push/release']}
    with (HERE/'handoff.json').open('x',encoding='utf-8') as output:
        json.dump(result,output,indent=2)
        output.write('\n')
    print(json.dumps({'handoff_sha256':sha(HERE/'handoff.json'),'inputs':len(hashes),'cases':6,'assertions':report['assertions'],'artifact_sha256':sha(executable)}))

if __name__=='__main__':
    main()
