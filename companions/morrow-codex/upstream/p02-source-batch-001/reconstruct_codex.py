import concurrent.futures
import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import tarfile
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
BATCH = ROOT / 'upstream/p02-source-batch-001'
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
COMMIT = '44fe510ce3ee61c8ef623adcbf89b901c73ddd61'
STAGING = BATCH / 'codex-source-staging'
DESTINATION = BATCH / 'codex-source'
ARCHIVE = BATCH / f'codex-{COMMIT}.tar.gz'

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')

def digest(kind, raw):
    return hashlib.sha1(kind.encode() + b' ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()

commit_metadata = read(RECEIPTS / 'codex-commit-api.json')
assert commit_metadata['sha'] == COMMIT
expected_root = commit_metadata['tree']['sha']
entries = {'': {'path': '', 'type': 'tree', 'mode': '040000', 'sha': expected_root}}

def add(prefix, tree):
    if not tree.get('tree') or tree.get('truncated'):
        return
    for entry in tree['tree']:
        path = (prefix + '/' if prefix else '') + entry['path']
        existing = entries.get(path)
        current = {**entry, 'path': path}
        if existing and (existing['sha'], existing['mode'], existing['type']) != (current['sha'], current['mode'], current['type']):
            raise ValueError('Conflicting Git metadata: ' + path)
        entries[path] = current

add('', read(RECEIPTS / 'codex-root-api.json'))
add('codex-rs', read(RECEIPTS / 'codex-rs-root-api.json'))
for file in sorted(RECEIPTS.glob('codex-tree-part-*.json')):
    for part in read(file):
        add(part['prefix'], part['data'])
for filename in ['codex-large-shallow-api.json', 'codex-large-level2-api.json']:
    for part in read(RECEIPTS / filename):
        add(part['prefix'], part['data'])
add('codex-rs/tui/src', read(RECEIPTS / 'codex-tui-src-shallow-api.json'))
for file in sorted(RECEIPTS.glob('codex-tui-part-*.json')):
    for part in read(file):
        add(part['prefix'], part['data'])

children = {}
for path, entry in entries.items():
    if not path:
        continue
    parent, _, label = path.rpartition('/')
    children.setdefault(parent, []).append((label, entry))
computed_trees = {}
def check_tree(path):
    raw_entries = []
    for label, entry in children.get(path, []):
        is_tree = entry['type'] == 'tree'
        if entry['type'] not in ['tree', 'blob']:
            raise ValueError('Unsupported Git entry type: ' + entry['path'])
        child_digest = check_tree(entry['path']) if is_tree else entry['sha']
        mode = entry['mode'].lstrip('0')
        key = label.encode() + (b'/' if is_tree else b'')
        raw_entries.append((key, mode.encode() + b' ' + label.encode() + b'\0' + bytes.fromhex(child_digest)))
    raw = b''.join(value for _, value in sorted(raw_entries))
    actual = digest('tree', raw)
    if actual != entries[path]['sha']:
        raise ValueError('Incomplete or invalid independent tree metadata: ' + path + ': ' + actual + ' != ' + entries[path]['sha'])
    computed_trees[path] = actual
    return actual

assert check_tree('') == expected_root
blobs = {path: e for path,e in entries.items() if e['type'] == 'blob'}
folded = set()
for path, entry in blobs.items():
    relative = PurePosixPath(path)
    if relative.is_absolute() or '..' in relative.parts or '\\' in path or path.casefold() in folded:
        raise ValueError('Unsafe or colliding source path: ' + path)
    if entry['mode'] not in ['100644', '100755', '120000']:
        raise ValueError('Unsupported source mode: ' + path)
    folded.add(path.casefold())
save(RECEIPTS / 'codex-complete-git-tree.json', {'schema_version': 1, 'commit': COMMIT, 'tree_sha1': expected_root, 'complete_independent_tree_verified': True, 'tree_count': len(computed_trees), 'file_count': len(blobs), 'bytes': sum(e['size'] for e in blobs.values()), 'entries': sorted(entries.values(), key=lambda e:e['path'])})
print(f'Independent tree complete: {len(blobs)} files / {sum(e["size"] for e in blobs.values())} bytes', flush=True)
if STAGING.exists() or DESTINATION.exists():
    raise SystemExit('Refuse to overwrite source staging or completed source')
STAGING.mkdir()
verified = {}
links = {}
def write_verified(path, raw, provenance):
    entry = blobs[path]
    if len(raw) != entry['size'] or digest('blob', raw) != entry['sha']:
        raise ValueError('Fixed Git blob mismatch: ' + path)
    if entry['mode'] == '120000':
        links[path] = raw.decode('utf-8')
    else:
        target = STAGING / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(raw)
    verified[path] = {'path': path, 'mode': entry['mode'], 'size': len(raw), 'git_blob_sha1': entry['sha'], 'sha256': hashlib.sha256(raw).hexdigest(), 'provenance': provenance}

archive_error = None
try:
    with tarfile.open(ARCHIVE, 'r|gz') as tar:
        for member in tar:
            if member.isdir():
                continue
            parts = PurePosixPath(member.name).parts
            if not parts or parts[0] != 'codex-' + COMMIT:
                raise ValueError('Unexpected partial archive prefix')
            path = PurePosixPath(*parts[1:]).as_posix()
            if path not in blobs or path in verified:
                raise ValueError('Unexpected or duplicate partial archive file: ' + path)
            if member.isfile():
                with tar.extractfile(member) as stream:
                    raw = stream.read()
                if len(raw) != member.size:
                    raise EOFError('Incomplete final member')
            elif member.issym():
                raw = member.linkname.encode('utf-8')
            else:
                raise ValueError('Unexpected archive member type')
            write_verified(path, raw, 'complete_member_from_failed_archive_with_independent_blob_check')
except (EOFError, tarfile.ReadError) as error:
    archive_error = type(error).__name__ + ': ' + str(error)
missing = [entry for path,entry in blobs.items() if path not in verified]
recovery = {'schema_version': 1, 'commit': COMMIT, 'tree_sha1': expected_root, 'archive_status': 'failed_incomplete_download', 'archive_recovery_error': archive_error, 'recovered_complete_members': len(verified), 'missing_count': len(missing), 'missing_bytes': sum(e['size'] for e in missing), 'missing': missing, 'raw_workers': 16, 'started_at': datetime.datetime.now(datetime.timezone.utc).isoformat()}
save(RECEIPTS / 'codex-recovery-initial.json', recovery)
print(f'Recovered {len(verified)} complete blob-verified members; missing {len(missing)} / {recovery["missing_bytes"]} bytes', flush=True)

def fetch(entry):
    path = entry['path']
    url = 'https://raw.githubusercontent.com/openai/codex/' + COMMIT + '/' + urllib.parse.quote(path, safe='/')
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        with opener.open(urllib.request.Request(url, headers={'User-Agent': 'morrow-fixed-source-audit/1'}), timeout=60) as response:
            if response.status != 200:
                raise ValueError('Unexpected HTTP status')
            raw = response.read(entry['size'] + 1)
        if len(raw) != entry['size'] or digest('blob', raw) != entry['sha']:
            raise ValueError('Fixed Git blob/size mismatch')
        return path, raw, None
    except Exception as error:
        return path, None, {'path': path, 'url': url, 'error': type(error).__name__ + ': ' + str(error)}

failures = []
with concurrent.futures.ThreadPoolExecutor(max_workers=16) as pool:
    for n, future in enumerate(concurrent.futures.as_completed([pool.submit(fetch, e) for e in missing]), 1):
        path, raw, failure = future.result()
        if failure:
            failures.append(failure)
        else:
            write_verified(path, raw, 'fixed_commit_raw_https_with_independent_blob_check')
        if n % 25 == 0 or n == len(missing):
            print(f'Raw recovery {n}/{len(missing)}; failures {len(failures)}', flush=True)
save(RECEIPTS / 'codex-raw-recovery.json', {'schema_version': 1, 'attempted_files': len(missing), 'success_count': len(missing) - len(failures), 'failures': failures, 'ended_at': datetime.datetime.now(datetime.timezone.utc).isoformat()})
save(RECEIPTS / 'codex-recovered-file-records.json', {'schema_version': 1, 'files': sorted(verified.values(), key=lambda e:e['path']), 'pending_symlinks': links})
if failures or len(verified) != len(blobs):
    raise SystemExit('Raw recovery incomplete; see exact failure inventory')
for path, target_value in links.items():
    link = STAGING / path
    relative = Path(target_value)
    target = (link.parent / relative).resolve()
    if relative.is_absolute() or not target.is_relative_to(STAGING.resolve()):
        raise SystemExit('Unsafe fixed source symlink: ' + path)
    link.parent.mkdir(parents=True, exist_ok=True)
    os.symlink(target_value, link, target_is_directory=target.is_dir())
for path, entry in verified.items():
    file = STAGING / path
    raw = os.readlink(file).encode('utf-8') if entry['mode'] == '120000' else file.read_bytes()
    if digest('blob', raw) != entry['git_blob_sha1'] or hashlib.sha256(raw).hexdigest() != entry['sha256']:
        raise SystemExit('Final materialized content mismatch: ' + path)
actual_set = {p.relative_to(STAGING).as_posix() for p in STAGING.rglob('*') if p.is_file() or p.is_symlink()}
assert actual_set == set(blobs)
assert STAGING.resolve().is_relative_to(BATCH.resolve()) and DESTINATION.resolve().is_relative_to(BATCH.resolve())
STAGING.rename(DESTINATION)
manifest = {'schema_version': 1, 'commit': COMMIT, 'tree_sha1': expected_root, 'files': sorted(verified.values(), key=lambda e:e['path'])}
manifest_path = RECEIPTS / 'codex-content-manifest.json'
save(manifest_path, manifest)
result = {'schema_version': 1, 'batch': 'p02-source-batch-001', 'repository': 'https://github.com/openai/codex', 'commit': COMMIT, 'tree_sha1': expected_root, 'status': 'complete_fixed_source_verified', 'source_method': 'verified complete members from failed archive plus independently blob-verified missing fixed raw files', 'archive_status': 'failed_incomplete_download_preserved', 'complete_commit_tree_verified': True, 'file_count': len(verified), 'source_bytes': sum(e['size'] for e in verified.values()), 'materialization': str(DESTINATION.relative_to(ROOT)), 'content_manifest': {'path': str(manifest_path.relative_to(ROOT)), 'sha256': hashlib.sha256(manifest_path.read_bytes()).hexdigest()}, 'dirty_diff': {'modified': [], 'missing': [], 'extra': [], 'basis': 'All independently authenticated Git-tree blobs and file set, with source modes recorded; not a Git checkout'}, 'license_files': [verified[p] for p in ['LICENSE', 'NOTICE']], 'build_or_runtime_executed': False, 'ended_at': datetime.datetime.now(datetime.timezone.utc).isoformat()}
save(RECEIPTS / 'codex-source-verification.json', result)
print(json.dumps(result, indent=2), flush=True)
