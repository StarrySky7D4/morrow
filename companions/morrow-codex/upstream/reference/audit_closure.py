import json
from pathlib import Path
import posixpath
import tomllib

base = Path('upstream/reference/codex')
workspace = tomllib.loads((base / 'codex-rs/Cargo.toml').read_text())['workspace']
workspace_deps = workspace['dependencies']
manifests = {}
for file in sorted(base.rglob('Cargo.toml')):
    doc = tomllib.loads(file.read_text())
    if 'package' in doc:
        manifests[file.relative_to(base).as_posix()] = doc

def dependencies(path, doc):
    groups = [(kind, None, doc.get(kind, {})) for kind in ['dependencies', 'build-dependencies', 'dev-dependencies']]
    for target, table in doc.get('target', {}).items():
        groups += [(kind, target, table.get(kind, {})) for kind in ['dependencies', 'build-dependencies', 'dev-dependencies']]
    result = []
    for kind, target, table in groups:
        for name, value in sorted(table.items()):
            raw = {'version': value} if isinstance(value, str) else value
            resolved = workspace_deps.get(name, {}) if raw.get('workspace') else raw
            if isinstance(resolved, str):
                resolved = {'version': resolved}
            else:
                resolved = dict(resolved)
            features = sorted(set(resolved.get('features', []) + raw.get('features', [])))
            item = {'name': name, 'package': resolved.get('package', name), 'kind': kind, 'target': target, 'optional': raw.get('optional', resolved.get('optional', False)), 'features': features, 'default_features': raw.get('default-features', resolved.get('default-features', True))}
            for key in ['version', 'git', 'rev', 'branch', 'tag']:
                if key in resolved:
                    item[key] = resolved[key]
            if 'path' in resolved:
                parent = 'codex-rs' if raw.get('workspace') else posixpath.dirname(path)
                item['manifest'] = posixpath.normpath(posixpath.join(parent, resolved['path'], 'Cargo.toml'))
            result.append(item)
    return result

packages = {}
for path, doc in manifests.items():
    packages[path] = {'name': doc['package']['name'], 'package': doc['package'], 'features': doc.get('features', {}), 'dependencies': dependencies(path, doc)}

roots = ['core', 'exec-server', 'file-system', 'model-provider', 'model-provider-info', 'models-manager', 'login', 'codex-api', 'codex-client', 'http-client', 'thread-store', 'rollout', 'history', 'protocol', 'apply-patch', 'utils/stream-parser']
closures = {}
missing = set()
for root in roots:
    todo = ['codex-rs/' + root + '/Cargo.toml']
    seen = set()
    registry = set()
    while todo:
        p = todo.pop()
        if p in seen:
            continue
        seen.add(p)
        if p not in packages:
            missing.add(p)
            continue
        for edge in packages[p]['dependencies']:
            if edge['kind'] == 'dev-dependencies':
                continue
            if 'manifest' in edge:
                todo.append(edge['manifest'])
            else:
                registry.add(edge['package'])
    closures[root] = {'workspace_path_manifests': sorted(seen), 'workspace_path_count': len(seen), 'external_direct_names_over_path_closure': sorted(registry)}

lock = tomllib.loads((base / 'codex-rs/Cargo.lock').read_text())
out = {'schema_version': 1, 'commit': '44fe510ce3ee61c8ef623adcbf89b901c73ddd61', 'status': 'static_manifest_closure_not_cargo_resolution', 'definition': 'Conservative union of normal and build path dependencies including every target and optional edge; dev edges recorded but excluded from root closure. Registry transitive records are from fixed Cargo.lock and not a target/feature resolved graph.', 'workspace_package': workspace['package'], 'workspace_manifests': packages, 'candidate_closures': closures, 'missing_path_manifests': sorted(missing), 'lock_packages': lock['package'], 'workspace_patches': tomllib.loads((base / 'codex-rs/Cargo.toml').read_text()).get('patch', {})}
Path('receipts/upstream-codex-dependency-closure.json').write_text(json.dumps(out, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'manifest_count': len(packages), 'closure_counts': {k: v['workspace_path_count'] for k,v in closures.items()}, 'missing': sorted(missing), 'lock_packages': len(lock['package'])}, indent=2))
