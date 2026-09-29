import hashlib
import json
from pathlib import Path
import re
import tomllib

def save(path, data):
    Path(path).write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

path = Path('receipts/upstream-codex-dependency-closure.json')
data = json.loads(path.read_text(encoding='utf-8'))
by_name = {}
for package in data['lock_packages']:
    by_name.setdefault(package['name'], []).append(package)
for closure in data['candidate_closures'].values():
    queue = [(name, None) for name in closure['external_direct_names_over_path_closure']]
    seen = set()
    while queue:
        name, version = queue.pop()
        for package in by_name.get(name, []):
            if version and version != package['version']:
                continue
            key = (name, package['version'], package.get('source', 'workspace'))
            if key in seen:
                continue
            seen.add(key)
            for dep in package.get('dependencies', []):
                parts = dep.split(' ')
                queue.append((parts[0], parts[1] if len(parts)>1 and parts[1][0].isdigit() else None))
    closure['conservative_lock_reachable_packages'] = [{'name': p[0], 'version': p[1], 'source': p[2]} for p in sorted(seen)]
    closure['conservative_lock_reachable_count'] = len(seen)
data['lock_closure_caveat'] = 'Lock adjacency is a conservative graph, not Cargo target/feature resolution; all versions are included when an edge does not disambiguate its version. Registry crate source/build.rs bodies have not been downloaded or audited.'
save(path, data)

cc = Path('upstream/reference/cc-switch/src-tauri')
modules = {}
for file in sorted((cc/'src').rglob('*.rs')):
    text = file.read_text(encoding='utf-8')
    modules[file.relative_to(cc).as_posix()] = {'sha256': hashlib.sha256(file.read_bytes()).hexdigest(), 'use_lines': [{'line': i, 'text': line} for i,line in enumerate(text.splitlines(), 1) if line.startswith('use ')], 'status': 'static source candidate, not compiled extraction'}
save('receipts/upstream-cc-switch-dependency-closure.json', {'schema_version': 1, 'commit': '846de29c13ac4d65f164db8c15dd5fd58e29f972', 'root_manifest': tomllib.loads((cc/'Cargo.toml').read_text(encoding='utf-8')), 'modules': modules, 'selected_registry_names': ['serde', 'serde_json', 'sha2', 'bytes', 'futures', 'async-stream', 'log'], 'tokio': 'Only pin! used in production streaming adapter; replace wrapper or retain minimal macro surface after actual resolution, never copy upstream rt-multi-thread feature set into pure Wasm slice.', 'replace': ['provider::CodexChatReasoningConfig with copied licensed pure type only', 'proxy::error::ProxyError with pure error type', 'stream EOF/DONE/finish handling and think-tag inference', 'network-driven async wrapper with byte-in/state-out parser boundary'], 'exclude': ['Tauri and src-tauri/build.rs', 'full provider.rs including application configuration readers', 'axum response adapters', 'proxy routing/failover/global configuration writers', 'reqwest/rusqlite/OS integrations'], 'lock_packages': tomllib.loads((cc/'Cargo.lock').read_text(encoding='utf-8'))['package'], 'qualification': 'No source extraction/build or semantic differential tests executed.'})

patterns = {'env': r'(?:std::)?env::(?:var|var_os|vars)|option_env!|env!\(', 'fs': r'(?:std|tokio)::fs|OpenOptions|File::|create_dir|read_to_string|write_all|rollout_path|SqlitePool|keyring', 'process': r'Command::|spawn_process|spawn_child|ExecBackend|\.start\(params|webbrowser::open', 'net': r'reqwest|HttpTransport|\.send\(\)|\.post\(|WebSocket|websocket|TcpListener', 'runtime_init': r'tokio::spawn|spawn_blocking|OnceLock|OnceCell|Runtime::|LocalThreadStore::new|Environment::local'}
hits = []
for root in ['upstream/reference/codex', 'upstream/reference/cc-switch']:
    for file in sorted(Path(root).rglob('*.rs')):
        for i, line in enumerate(file.read_text(encoding='utf-8').splitlines(), 1):
            classes = [k for k,v in patterns.items() if re.search(v, line)]
            if classes:
                hits.append({'path': file.as_posix(), 'line': i, 'classes': classes, 'text': line.strip()})
save('receipts/upstream-side-effect-candidates.json', {'schema_version': 1, 'qualification': 'Keyword candidate inventory of the fetched source subset, including comments/tests; not a complete call graph, reachability proof, or absence-of-side-effects proof.', 'matches': hits})
print(json.dumps({'side_effect_candidate_lines': len(hits), 'codex_lock_counts': {k: v['conservative_lock_reachable_count'] for k,v in data['candidate_closures'].items()}, 'cc_lock_packages': len(tomllib.loads((cc/'Cargo.lock').read_text(encoding='utf-8'))['package'])}, indent=2))
