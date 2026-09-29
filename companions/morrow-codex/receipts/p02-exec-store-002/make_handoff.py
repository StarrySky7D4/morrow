"""Freeze this completed qualification slice without modifying earlier batches."""
from pathlib import Path
import hashlib
import json

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def main():
    frozen = read(HERE/'start.json')['frozen_handoffs']
    prior_counts = {}
    for relative, expected in frozen.items():
        path = ROOT/relative
        if sha(path) != expected:
            raise RuntimeError('Previous handoff changed')
        inputs = read(path)['input_sha256']
        for item, digest in inputs.items():
            if sha(ROOT/item) != digest:
                raise RuntimeError('Previous frozen input changed: '+item)
        prior_counts[relative] = len(inputs)
    reports = []
    evidence = set()
    for label, run_id, build_id, cases, assertions in (
        ('store','20260928T130333Z-d07636b654','20260928T130311Z-808e61eb41',6,12),
        ('exec','20260928T131658Z-84c272f896','20260928T131255Z-e637b7136f',3,27),
    ):
        base=ROOT/f'receipts/p02-{label}-probe-002'
        run=base/f'runs/run-{run_id}/result.json'
        build=base/f'runs/build-{build_id}/result.json'
        runtime=base/f'runtime-{run_id}.json'
        executable=ROOT/f'out/p02-exec-store-002/target/x86_64-pc-windows-msvc/debug/p02-{label}-probe.exe'
        r=read(runtime)
        if r['status']!=f'passed_limited_{label}_callsite_probe' or r['case_count']!=cases or r['assertions']!=assertions or sum(c['assertions'] for c in r['cases'])!=assertions:
            raise RuntimeError('Incorrect limited runtime result')
        b, rr=read(build),read(run)
        if b['status']!='compiled_not_runtime_proof' or rr['exit_code']!=0 or b['exit_code']!=0:
            raise RuntimeError('Missing actual build/run success')
        if rr['runtime_sha256']!=sha(runtime) or b['artifacts'][0]['sha256']!=sha(executable) or rr['expected_cargo_bin_artifact']['sha256']!=sha(executable):
            raise RuntimeError('Artifact identity changed')
        if not all(x['frozen_after']['matches_frozen_handoff'] for x in (b,rr)):
            raise RuntimeError('Prior inputs not frozen')
        reports.append({'slice':label,'cases_passed':cases,'assertions_passed':assertions,'build':build.relative_to(ROOT).as_posix(),'run':run.relative_to(ROOT).as_posix(),'runtime':runtime.relative_to(ROOT).as_posix(),'runtime_sha256':sha(runtime),'artifact':executable.relative_to(ROOT).as_posix(),'artifact_sha256':sha(executable)})
        for path in (run,build,runtime,executable):
            evidence.add(path)
    before,after=read(HERE/'patch-before-build-001.json'),read(HERE/'patch-after-build-001.json')
    if before!=after or after['status']!='minimal_patch_verified':
        raise RuntimeError('Source patch drifted during build')
    for row in after['changes']:
        path=ROOT/'upstream/p02-exec-store-002/codex-work'/row['path']
        if sha(path)!=row['after_sha256']:
            raise RuntimeError('Source patch drifted after review')
        evidence.add(path)
    for relative in ('receipts/p02-exec-store-002','receipts/p02-store-probe-002','receipts/p02-exec-probe-002','qualification/p02-store-probe-002','qualification/p02-exec-probe-002'):
        for path in (ROOT/relative).rglob('*'):
            if path.is_file() and path.name!='handoff.json' and '__pycache__' not in path.parts:
                evidence.add(path)
    for relative in ('out/p02-exec-store-002/cargo-home/config.toml','receipts/p02-source-batch-001/codex-content-manifest.json','upstream/p02-exec-store-002/host-kit-003/manifest.json','upstream/p02-exec-store-002/host-kit-003/agent_host.capnp','upstream/p02-exec-store-002/codex-work/codex-rs/core/src/client.rs','upstream/p02-exec-store-002/codex-work/codex-rs/core/src/exec.rs'):
        evidence.add(ROOT/relative)
    hashes={p.relative_to(ROOT).as_posix():sha(p) for p in sorted(evidence)}
    result={'schema_version':1,'batch':'p02-exec-store-002','ready_for_review':True,'status':'passed_limited_store_and_exec_callsite_probes','cases_passed':9,'assertions_passed':39,'slices':reports,'upstream_delta_files':5,'upstream_delta_patch_lines':163,'upstream_delta_sha256':after['patch_sha256'],'previous_frozen_handoffs':frozen,'previous_frozen_input_counts':prior_counts,'input_sha256':hashes,'P-02':'not_complete','G0':'blocked','J-00':'not_complete','product_native_wasm_graphs':'0/2','product_84_cases':'not_run','host_schema':'003 unchanged','limits':['Qualification native graphs only; no production session wiring or second agent loop','Local refusal is not IPC disconnection or OS isolation proof','M-04 production storage and M-06 execution lifecycle remain missing','Core ModelClient HTTP/WebSocket/prewarm/fallback and independent direct spawn paths remain uncovered','No accounts, paid model requests, publishing, commits, pushes, tags or releases']}
    with (HERE/'handoff.json').open('x',encoding='utf-8') as output:
        json.dump(result,output,indent=2)
        output.write('\n')
    print(json.dumps({'handoff_sha256':sha(HERE/'handoff.json'),'bound_inputs':len(hashes),'cases':9,'assertions':39,'previous_frozen_input_counts':prior_counts}))


if __name__=='__main__':
    main()
