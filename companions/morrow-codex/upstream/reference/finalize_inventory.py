import hashlib
import json
from pathlib import Path

ROOT = Path.cwd()
def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def jread(path):
    return json.loads(path.read_text(encoding='utf-8'))
def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

sources = []
for source, repo, commit in [('codex', 'openai/codex', '44fe510ce3ee61c8ef623adcbf89b901c73ddd61'), ('cc-switch', 'farion1231/cc-switch', '846de29c13ac4d65f164db8c15dd5fd58e29f972')]:
    base = Path('upstream/reference') / source
    expected = {}
    if source == 'codex':
        for payload in Path('upstream/reference').glob('codex-*-fetch*.json'):
            for item in jread(payload):
                if item.get('content'):
                    expected[item['path']] = item['sha']
    else:
        for receipt in [Path('receipts/upstream-cc-switch-connector.json'), Path('receipts/upstream-cc-switch-local-deps.json'), Path('receipts/upstream-cc-switch-source-final.json')]:
            for item in jread(receipt)['files']:
                expected[item['path']] = item['git_blob_sha1']
    actual = {p.relative_to(base).as_posix(): p for p in base.rglob('*') if p.is_file()}
    if set(actual) != set(expected):
        raise SystemExit(f'{source}: file-set mismatch missing={set(expected)-set(actual)} extra={set(actual)-set(expected)}')
    files = []
    for path, file in sorted(actual.items()):
        raw = file.read_bytes()
        git_blob = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
        if git_blob != expected[path]:
            raise SystemExit('source mismatch: ' + path)
        files.append({'path': path, 'sha256': sha256(file), 'git_blob_sha1': git_blob})
    manifest = Path('receipts') / f'upstream-{source}-tree.json'
    save(manifest, {'schema_version': 1, 'files': files})
    license_paths = ['LICENSE', 'NOTICE'] if source == 'codex' else ['LICENSE']
    sources.append({'id': source, 'kind': 'partial_snapshot', 'url': 'https://github.com/' + repo, 'commit': commit, 'path': base.as_posix(), 'files_manifest': {'path': manifest.as_posix(), 'sha256': sha256(manifest)}, 'license_files': [{'path': p, 'sha256': sha256(base / p)} for p in license_paths], 'dirty': {'allowed': False}, 'verified_files': len(files), 'verification': 'All fetched bytes match GitHub connector git blob SHA-1 and local SHA-256; this verifies only listed fixed-ref files, not a full commit tree', 'license': 'Apache-2.0' if source == 'codex' else 'MIT'})

attempts = [
    {'id': 'codex', 'method': 'git fetch --no-tags --depth 1 fixed SHA', 'status': 'failed', 'error': 'schannel: server closed abruptly (missing close_notify)', 'path': 'upstream/codex'},
    {'id': 'cc-switch', 'method': 'git fetch --no-tags --depth 1 fixed SHA', 'status': 'failed', 'error': 'Failed to connect to github.com port 443 after 21083 ms', 'path': 'upstream/cc-switch'},
    {'id': 'codex', 'method': 'fixed codeload tar.gz', 'status': 'incomplete_download_not_archive', 'received_bytes_reported': 6682470, 'error': 'curl 28 timeout after 240011 milliseconds'},
    {'id': 'cc-switch', 'method': 'fixed codeload tar.gz', 'status': 'incomplete_download_not_archive', 'received_bytes_reported': 4720065, 'error': 'curl 28 timeout after 180004 milliseconds'},
    {'id': 'codex', 'method': 'recursive Git trees REST API', 'status': 'failed_truncated_json', 'path': 'upstream/codex-git-tree-complete.json', 'error': 'curl 18 end of response with 1661998 bytes missing; never use as complete tree'},
    {'id': 'cc-switch', 'method': 'recursive Git trees REST API', 'status': 'complete_metadata_only', 'path': 'upstream/cc-switch-git-tree-complete.json', 'entries': 1454, 'truncated': False},
]
inventory = {'schema_version': 1, 'status': 'partial_snapshot_verified_static_audit_only', 'sources': sources, 'acquisition_attempts': attempts, 'dirty_diff': {'modified_files': [], 'comparison': 'every snapshot file against fixed-ref Git blob SHA; snapshots are not Git checkouts'}, 'runtime_or_build_executed': False, 'registry_sources_downloaded': False, 'limitations': ['Fixed source trees incomplete; full-source preflight must fail source_incomplete.', 'Cargo metadata/build not run; feature/target-resolved minimal graph not qualified.', 'P-02 fake execution/network/store interception not run.', 'M-00/M-01 host-kit acceptance and full G0 remain outside this audit.']}
save(Path('receipts/upstream-inventory.json'), inventory)
print(json.dumps({'sources': sources}, ensure_ascii=False, indent=2))
