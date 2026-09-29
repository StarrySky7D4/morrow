"""Final ready source/build identity audit before independent execution."""
import json
from pathlib import Path
from check_review import HERE,HOST,read,write,sha,verify
def main():
    run=HERE/'runs/runtime-ready-001';run.mkdir(parents=True,exist_ok=False)
    root=HOST/'reports/codex-morrow-v1.1/host/m02-admission-owner-002';hand=root/'ready-handoff-001.json'
    if sha(hand)!='ee8986c3ab614b77d9f321a6d4961e605374c66e1e52bbf91b48f5070e6bc52c':raise ValueError('handoff hash')
    h=read(hand);mp=Path(h['manifest']);m=read(mp)
    if sha(mp)!='ee257b1cc15dcb72a4f92343406431fbc4a35153915e23e5f73e9265917e373e' or mp.parent.name!='runtime-kit-002':raise ValueError('only ready kit002')
    pins={str(HOST/p):s for p,s in m['build_inputs_sha256'].items()}
    for p,s in m['files'].items():
        f=(mp.parent/p).resolve()
        if not f.is_relative_to(mp.parent):raise ValueError('kit path escape')
        pins[str(f)]=s
        if p.startswith('native_session_owner_002/') and sha(HOST/p)!=s:raise ValueError('kit/source copy mismatch')
    pins.update(m['evidence_sha256'])
    for p,s in [(hand,sha(hand)),(mp,sha(mp)),(Path(h['delivery']),h['delivery_sha256'])]:pins[str(p)]=s
    rows=verify([{'path':p,'sha256':s}for p,s in pins.items()]);write(run/'identities.json',rows)
    if not all(x['match']for x in rows):raise ValueError('ready identity drift')
    if {p.relative_to(mp.parent).as_posix()for p in mp.parent.rglob('*')if p.is_file()}!=set(m['files'])|{'manifest.json'}:raise ValueError('kit inventory')
    build=read(m['receipts']['build_api'])
    if build['before']!=build['after'] or build['before']!=m['build_inputs_sha256']:raise ValueError('build input association')
    for c in build['commands']:
        if c['exit_code']!=0:raise ValueError('producer build/API failure')
        for ext in ['stdout','stderr']:
            if sha(Path(m['receipts']['build_api']).parent/(c['name']+'.'+ext))!=c[ext+'_sha256']:raise ValueError('build log hash')
    messages=[json.loads(x)for x in (Path(m['receipts']['build_api']).parent/'build.stdout').read_text().splitlines()if x.startswith('{')]
    artifacts=[x for x in messages if x.get('reason')=='compiler-artifact'];exes=[x for x in artifacts if x.get('executable')]
    if len(exes)!=1 or Path(exes[0]['target']['src_path'])!=HOST/'native_session_owner_002/src/main.rs' or sha(exes[0]['executable'])!=h['host_sha256']:raise ValueError('actual compiled host association')
    if not any(Path(x['target']['src_path'])==HOST/'core/src/lib.rs'for x in artifacts):raise ValueError('shared Core not compiled')
    if not any(Path(x['target']['src_path'])==HOST/'contracts/experimental/agent_host_v2_capnp/src/lib.rs'for x in artifacts):raise ValueError('host Capnp authority not compiled')
    source=(mp.parent/'native_session_owner_002/src/authority.rs').read_text();lib=(mp.parent/'native_session_owner_002/src/lib.rs').read_text()
    repaired={
      'Revoked_legal_persisted_phase':'| "Revoked"'in source,
      'complete_live_record_compared':'g != live.record'in source and 'latest != g'in source,
      'canonical_profile_bound':'profile.canonical_root != root.to_string_lossy()'in source,
      'direct_bypass_types_crate_private':all('pub(crate) struct '+s in lib for s in ['Admission','NativeHost','Session']),
    }
    if not all(repaired.values()):raise ValueError('review correction missing')
    result={'status':'verified_read_only_ready_host','host_executable':h['host_executable'],'host_sha256':h['host_sha256'],'handoff':str(hand),'handoff_sha256':sha(hand),'manifest_sha256':sha(mp),'identities':str(run/'identities.json'),'input_count':len(rows),'build_input_count':len(m['build_inputs_sha256']),'kit_file_count':len(m['files']),'evidence_file_count':len(m['evidence_sha256']),'actual_compiler_package_ids':sorted({x['package_id']for x in artifacts}),'pre_ready_corrections':repaired,'source_review_scope':'full authority/CLI, storage codec and transaction/lock/crash paths read; supervisor delta against frozen001; producer counts not independent credit','clock_origins':'authority at_us from authority open; session at_us from first Admission; compare separately','independent_build':False,'independent_runtime':False,'product_pass_credit':0,'reviewer_sha256':sha(__file__)}
    write(run/'result.json',result);print(json.dumps({k:v for k,v in result.items()if k!='actual_compiler_package_ids'},indent=2))
if __name__=='__main__':main()
