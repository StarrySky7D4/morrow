"""Read-only frozen runtime and peer audit before independent execution."""
import json,sys
from pathlib import Path
sys.dont_write_bytecode=True
from check_review import HERE,HOST,PLUGIN,sha,read,write,verify_pins
from review_capnp_kit import authority,MANIFEST_SHA,SCHEMA_SHA
ROOT=HOST/'reports/codex-morrow-v1.1/host/m02-native-session-001'
RUNTIME=ROOT/'runtime-kit-001'
HANDOFF_SHA='09ec3b47a4425cff6bd7347314e2844862ed2412bf416adc3ba521c7360ce51d'
RUNTIME_SHA='178044ee85d0f1c15e0168e8fcf6b728b5105ac5b2c63b8d2134b16b49a23be9'

def main():
    out=HERE/'runs/runtime-ready-review-001';out.mkdir(exist_ok=False)
    result={'scope':'frozen_source_build_and_identity_only','independent_runtime':False,'product_pass_credit':0,
            'reviewer_sha256':sha(__file__),'runtime_manifest_sha256':RUNTIME_SHA}
    try:
        if sha(ROOT/'runtime-handoff-001.json')!=HANDOFF_SHA or sha(RUNTIME/'manifest.json')!=RUNTIME_SHA:raise ValueError('runtime identity changed')
        handoff=read(ROOT/'runtime-handoff-001.json');manifest=read(RUNTIME/'manifest.json')
        if sha(handoff['delivery'])!=handoff['delivery_sha256']:raise ValueError('delivery changed')
        if manifest['wire_manifest_sha256']!=MANIFEST_SHA or manifest['schema_sha256']!=SCHEMA_SHA:raise ValueError('wrong wire')
        authority();pins={}
        def add(path,digest):
            key=str(Path(path).resolve())
            if key in pins and pins[key]!=digest:raise ValueError('conflicting file pin')
            pins[key]=digest
        for section,root in [('files',RUNTIME),('build_inputs_sha256',HOST),('evidence_sha256',HOST)]:
            for relative,digest in manifest[section].items():
                path=(root/relative).resolve()
                if not path.is_relative_to(root):raise ValueError('unsafe manifest path')
                add(path,digest)
        actual={p.relative_to(RUNTIME).as_posix() for p in RUNTIME.rglob('*') if p.is_file()}
        if actual!=set(manifest['files'])|{'manifest.json'}:raise ValueError('runtime file inventory differs')
        for rel in manifest['files']:
            if rel.startswith('native_session/') and sha(RUNTIME/rel)!=sha(HOST/rel):raise ValueError('source copy differs')
        build=read(manifest['build_result'])
        if build['before']!=build['after']:raise ValueError('producer build changed inputs')
        for relative,digest in build['after'].items():add(HOST/relative,digest)
        if any(c['exit_code']!=0 for c in build['commands']):raise ValueError('producer build or qualification failed')
        log=Path(manifest['build_result']).parent/'build.stdout'
        if sha(log)!=build['commands'][0]['stdout_sha256']:raise ValueError('build log changed')
        artifacts=[json.loads(line) for line in log.read_text(encoding='utf-8').splitlines() if line.startswith('{')]
        artifacts=[m for m in artifacts if m.get('reason')=='compiler-artifact']
        for name,source in [('morrow_core',HOST/'core/Cargo.toml'),('morrow_native_session_wire',HOST/'contracts/experimental/agent_host_v2_capnp/Cargo.toml'),('morrow_native_session',HOST/'native_session/Cargo.toml')]:
            found=[m for m in artifacts if m['target']['name']==name]
            if len(found)!=1 or Path(found[0]['manifest_path']).resolve()!=source:raise ValueError('actual compiled source differs:'+name)
        binary=[m for m in artifacts if m['target']['name']=='morrow-native-session-host' and m.get('executable')]
        if len(binary)!=1 or sha(binary[0]['executable'])!=manifest['host_sha256'] or sha(manifest['host_executable'])!=manifest['host_sha256']:raise ValueError('host executable association')
        peer_path=PLUGIN/'receipts/m02-native-session-adversary-002/candidate-002.json'
        peer_pin='afc3f57c1c430d61f88d22ecbe71c170254f6dc47477d67054aff4fde8e1ef42'
        if sha(peer_path)!=peer_pin:raise ValueError('peer002 manifest identity')
        peer=read(peer_path)
        add(peer_path,peer_pin)
        for rel,digest in peer['input_sha256'].items():add(PLUGIN/rel,digest)
        client=read(PLUGIN/'receipts/m02-native-session-001/candidate-001.json')
        for rel,digest in client['input_sha256'].items():add(PLUGIN/rel,digest)
        # One diagnostic-only delta between frozen peer sources: hide the child console.
        p1=PLUGIN/'native/m02-native-session-adversary-001/src/main.rs'
        p2=PLUGIN/'native/m02-native-session-adversary-002/src/main.rs'
        text=p2.read_text(encoding='utf-8')
        if 'creation_flags(0x08000000)' not in text or 'Duration::from_millis(1500)' not in text or 'Duration::from_secs(10)' not in text:raise ValueError('peer bounds/hidden descendant not present')
        checked,issues=verify_pins([{'path':p,'sha256':s} for p,s in pins.items()])
        if issues:raise ValueError(issues)
        write(out/'identities.json',checked)
        result.update(status='verified_read_only',exit_code=0,identity_count=len(checked),
          runtime_files=len(manifest['files']),host_build_inputs=len(manifest['build_inputs_sha256']),producer_evidence_files=len(manifest['evidence_sha256']),
          host_compiler_artifact_packages=len({m['package_id'] for m in artifacts}),
          host_executable=manifest['host_executable'],host_sha256=manifest['host_sha256'],
          client_executable=client['exe'],client_sha256=client['exe_sha256'],peer_executable=peer['exe'],peer_sha256=peer['exe_sha256'],
          peer_manifest=str(peer_path),peer_manifest_sha256=peer_pin,
          source_review=['shared core HostPolicy activation/ready/revocation/stop/retire; no child-created admission',
            'first authorize Instant predates hashing/spawn; exact allowlisted environment captured and hashed then reused',
            'private piped actual Child PID; initial binding and code/kind/sequence checked separately',
            'bounded control queue8 prioritized over timer/data; one input frame and pending response',
            'timer branch refreshes Instant; release requires exit_observed and both EOF',
            'stderr details16 plus totals and event_overflow; no silent completeness assertion',
            'dedicated std control reader prevents Tokio stdin runtime shutdown wait',
            'ordinary client has no fault mode; peer002 only fixed own-exe descendant1500ms and watchdog10s'],
          limits=['Production approval UI, cross-process owner registry, crash recovery absent',
            'Hash-to-image-load race and OS/descendant containment not closed',
            'Partial write cancellation and blocked operator stdout not dynamically verified',
            'In-process no-replacement and delayed admission tests currently producer evidence only'])
    except Exception as exc:result.update(status='failed',exit_code=1,error=type(exc).__name__,message=str(exc))
    write(out/'result.json',result);print(json.dumps(result,ensure_ascii=False,indent=2));return result['exit_code']

if __name__=='__main__':raise SystemExit(main())
