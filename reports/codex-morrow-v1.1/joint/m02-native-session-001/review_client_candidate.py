"""Pinned ordinary client source/build review; does not launch it."""
import json,re,sys,tomllib
from pathlib import Path
sys.dont_write_bytecode=True
from check_review import HERE,PLUGIN,sha,read,write,verify_pins
from review_capnp_kit import KIT,MANIFEST_SHA,SCHEMA_SHA,authority
CANDIDATE=PLUGIN/'receipts/m02-native-session-001/candidate-001.json'
PIN='9448e8167737df2ec706999c66a13ce4ce400e2cbcf5aceace1c30a8361058ee'

def main():
    run=HERE/'runs/client-candidate-review-001';run.mkdir(exist_ok=False)
    result={'scope':'pinned_ordinary_client_source_and_producer_build_only','independent_client_run':False,
            'real_ipc_verified':False,'product_pass_credit':0,'candidate_sha256':PIN,'reviewer_sha256':sha(__file__)}
    try:
        if sha(CANDIDATE)!=PIN:raise ValueError('candidate identity changed')
        candidate=read(CANDIDATE)
        pins=[]
        for key in ['input_sha256','old_handoffs']:
            for relative,digest in candidate[key].items():
                path=(PLUGIN/relative).resolve()
                if not path.is_relative_to(PLUGIN):raise ValueError('unsafe candidate path')
                pins.append({'path':str(path),'sha256':digest})
        checked,issues=verify_pins(pins)
        if issues:raise ValueError(issues)
        write(run/'identities-before.json',checked)
        authority()
        if candidate['host_kit_manifest_sha256']!=MANIFEST_SHA or candidate['schema_sha256']!=SCHEMA_SHA:raise ValueError('wrong kit')
        source=Path(candidate['source']);manifest=tomllib.loads((source/'Cargo.toml').read_text())
        wire=(source/manifest['dependencies']['morrow-native-session-wire']['path']).resolve()
        if wire!=source/'vendor/capnp-kit-001/wire':raise ValueError('wrong compiled wire path')
        vendor_manifest=read(KIT/'manifest.json')
        for relative,digest in vendor_manifest['files'].items():
            if sha(source/'vendor/capnp-kit-001'/relative)!=digest:raise ValueError('vendor differs:'+relative)
        def registry(lock):return {(p['name'],p['version'],p.get('checksum')) for p in tomllib.loads(lock.read_text())['package'] if 'source' in p}
        if registry(source/'Cargo.lock')!=registry(KIT/'wire/Cargo.lock'):raise ValueError('registry lock identities differ')
        build_path=Path(candidate['build']);build=read(build_path)
        if build['inputs_before']!=build['inputs_after']:raise ValueError('producer inputs changed')
        command=build['commands'][0]
        if command['exit_code']!=0:raise ValueError('producer build failed')
        for stream in ['stdout','stderr']:
            if sha(build_path.parent/f'0.{stream}')!=command[f'{stream}_sha256']:raise ValueError('build log hash')
        messages=[json.loads(line) for line in (build_path.parent/'0.stdout').read_text(encoding='utf-8').splitlines() if line.startswith('{')]
        artifacts=[m for m in messages if m.get('reason')=='compiler-artifact']
        w=[m for m in artifacts if m['target']['name']=='morrow_native_session_wire']
        if len(w)!=1 or Path(w[0]['manifest_path']).resolve()!=wire/'Cargo.toml':raise ValueError('compiler consumed different wire')
        binaries=[m for m in artifacts if m.get('executable') and m['target']['name']=='morrow-session-client']
        if len(binaries)!=1 or sha(binaries[0]['executable'])!=candidate['exe_sha256']:raise ValueError('executable/build association')
        if sha(candidate['exe'])!=candidate['exe_sha256']:raise ValueError('pinned exe changed')
        generated=[p for p in candidate['input_sha256'] if p.endswith('/out/native_session_capnp.rs')]
        if len(generated)!=1 or sha(PLUGIN/generated[0])!=sha(KIT/'native_session_capnp.rs'):raise ValueError('generated binding identity')
        after,issues=verify_pins(pins)
        if issues or after!=checked:raise ValueError('candidate changed during review')
        write(run/'identities-after.json',after)
        result.update(status='verified_read_only',exit_code=0,client_inputs=len(candidate['input_sha256']),old_handoffs=len(candidate['old_handoffs']),
          registry_lock_packages=len(registry(source/'Cargo.lock')),compiler_artifact_package_ids=len({m['package_id'] for m in artifacts}),
          actual_wire_manifest=w[0]['manifest_path'],exe_sha256=candidate['exe_sha256'],
          source_review=['Challenge checks own PID/schema/self image digest and fixed capabilities',
            'all requests copy initial binding; strict sequence, no pipelining or retry',
            'responses check binding, monotonic generation<=2, remaining TTL/budget nonincrease',
            'input polled before output completion and during bounded interval; Stop supported',
            'one-slot worker queues; failed writer cannot resend; partial/operation deadlines and 60-second safety cap',
            'workers not joined; process exit must be observed independently',
            'CLI cannot set approved/identity/fault modes; withdrawn wire retained as unreferenced evidence only'],
          limits=['No actual host/client launch in this result','No independent client compilation','No trusted installation/image-load race closure',
            'Host authority remains authoritative; client checks are not a grant','Raw process/thread/output lifecycle remains pending real IPC'])
    except Exception as exc:result.update(status='failed',exit_code=1,error=type(exc).__name__,message=str(exc))
    write(run/'result.json',result);print(json.dumps(result,ensure_ascii=False,indent=2));return result['exit_code']

if __name__=='__main__':raise SystemExit(main())
