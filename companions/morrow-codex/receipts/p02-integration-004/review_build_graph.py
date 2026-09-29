"""Bind the one successful build to the resolved feature/root identities."""
from pathlib import Path
import hashlib,json
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
def read(p): return json.loads(p.read_text(encoding='utf-8'))
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    results=[(p,read(p)) for p in (HERE/'runs').glob('*/result.json')]
    builds=[x for x in results if x[1]['stage']=='build' and x[1]['status']=='compiled_not_runtime_proof']
    build_path,build=max(builds,key=lambda x:x[1]['run_id'])
    locks=[x for x in results if x[1]['stage']=='lock' and x[1]['status']=='probe_lock_prepared_not_build_proof']
    lock_path,lock=max(locks,key=lambda x:x[1]['run_id'])
    meta=read(lock_path.parent/'stdout.txt')
    packages={p['id']:p for p in meta['packages']}
    artifacts=[]
    for line in (build_path.parent/'stdout.txt').read_text(encoding='utf-8').splitlines():
        msg=json.loads(line)
        if msg.get('reason')=='compiler-artifact': artifacts.append(msg)
    records={}
    source=ROOT/'upstream/p02-integration-004/codex-work/codex-rs'
    for name in ['codex-core','codex-thread-store','codex-exec-server','codex-protocol','codex-rollout']:
        p=[p for p in meta['packages'] if p['name']==name]
        assert len(p)==1
        p=p[0]; Path(p['manifest_path']).relative_to(source)
        actual=[a for a in artifacts if a['package_id']==p['id']]
        assert actual,name
        features=sorted(set(f for a in actual for f in a['features']))
        if name in ['codex-core','codex-thread-store']: assert 'morrow-p02-restricted-qualification' in features
        records[name]={'package_id':p['id'],'manifest_path':p['manifest_path'],'compiler_features':features}
    for p in meta['packages']:
        path=p['manifest_path'].replace('\\','/')
        if '/codex-work/' in path: Path(p['manifest_path']).relative_to(source)
    executables=[a for a in artifacts if a.get('executable') and a['target']['name']=='p02-integration-probe']
    assert len(executables)==1
    exe=Path(executables[0]['executable']);assert sha(exe)==build['artifacts'][0]['sha256']
    result={'status':'same_source_and_compiled_features_verified','metadata':str(lock_path.parent/'stdout.txt'),'metadata_sha256':sha(lock_path.parent/'stdout.txt'),'build':str(build_path),'build_sha256':sha(build_path),'source_root':str(source),'packages':records,'compiler_artifact_unique_packages':len(set(a['package_id'] for a in artifacts)),'executable':str(exe),'executable_sha256':sha(exe),'default_product_build':'not_run; defaults retained in source only','runtime_authorization':'not_established_by_feature'}
    with (HERE/'build-graph-evidence.json').open('x',encoding='utf-8') as f:json.dump(result,f,indent=2)
    print(json.dumps({'status':result['status'],'compiler_artifact_unique_packages':result['compiler_artifact_unique_packages'],'features':records}))
if __name__=='__main__':main()
