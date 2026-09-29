"""Verify qualification locks retain exact public versions/checksums from pinned inputs."""
from pathlib import Path
import hashlib
import json
import tomllib

ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    original = ROOT/'upstream/p02-source-batch-001/codex-source/codex-rs/Cargo.lock'
    kit = ROOT/'upstream/p02-exec-store-002/host-kit-003/Cargo.lock'
    allowed = {}
    for path in (original, kit):
        for package in tomllib.loads(path.read_text(encoding='utf-8'))['package']:
            if package.get('source','').startswith('registry+'):
                key=(package['name'],package['version'])
                old=allowed.setdefault(key, package['checksum'])
                if old != package['checksum']:
                    raise RuntimeError('Conflicting checksum')
    graphs = []
    for name in ('integration',):
        path=ROOT/'qualification/p02-integration-004/Cargo.lock'
        packages=tomllib.loads(path.read_text(encoding='utf-8'))['package']
        registry=[]
        local=[]
        for package in packages:
            source=package.get('source')
            if source is None:
                local.append({'name':package['name'],'version':package['version']})
            elif source=='registry+https://github.com/rust-lang/crates.io-index':
                if allowed.get((package['name'],package['version']))!=package['checksum']:
                    raise RuntimeError(f"Unpinned registry package: {package['name']} {package['version']}")
                archive=ROOT/'out/p02-exec-store-002/cargo-home/registry/cache/index.crates.io-1949cf8c6b5b557f'/f"{package['name']}-{package['version']}.crate"
                if sha(archive)!=package['checksum']:
                    raise RuntimeError('Archive checksum differs')
                registry.append({'name':package['name'],'version':package['version'],'checksum':package['checksum']})
            else:
                raise RuntimeError('Unexpected source: '+source)
        graphs.append({'graph':name,'lock_sha256':sha(path),'packages':len(packages),'registry_count':len(registry),'path_count':len(local),'git_count':0,'registry':registry,'path_packages':local})
    result={'status':'exact_registry_versions_and_archives_verified','upstream_lock_sha256':sha(original),'host_lock_sha256':sha(kit),'graphs':graphs,'limits':['Locks include all-target/optional resolution; counts are not executed modules','Path packages bind through complete source manifests and the explicit patch receipt','Qualification graphs are not product native/Wasm graphs']}
    with (RECEIPTS/'resolved-graphs-001.json').open('x') as output:
        json.dump(result,output,indent=2)
    print(json.dumps({ 'status':result['status'], 'graphs':[{k:v for k,v in g.items() if k not in ('registry','path_packages')} for g in graphs]}))


if __name__=='__main__':
    main()
