import base64
import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import stat

ROOT = Path(__file__).resolve().parents[2]
BATCH = ROOT / 'upstream/p02-source-batch-001'
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
STAGING = BATCH / 'cc-switch-source-staging'
DESTINATION = BATCH / 'cc-switch-source'
COMMIT = '846de29c13ac4d65f164db8c15dd5fd58e29f972'
EXPECTED_ROOT = 'e373c27485681c074ddab7d4f085ee9a6667b891'

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')

def digest(kind, raw):
    return hashlib.sha1(kind.encode() + b' ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()

def require(condition, message):
    if not condition:
        raise ValueError(message)

def no_reparse(path):
    data = path.lstat()
    require(not stat.S_ISLNK(data.st_mode) and not (getattr(data, 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT), 'Unexpected reparse point: ' + str(path))

def safe_path(root, path, allow_final_symlink=False):
    relative = PurePosixPath(path)
    require(bool(path) and not relative.is_absolute() and not PureWindowsPath(path).drive and ':' not in path and '\\' not in path and all(x not in ['', '.', '..'] for x in path.split('/')), 'Unsafe path: ' + path)
    require(all(not x.endswith((' ', '.')) for x in relative.parts), 'Windows path normalization: ' + path)
    target = root.joinpath(*relative.parts)
    require(target.absolute().is_relative_to(root.absolute()), 'Target escaped source root: ' + path)
    cursor = root
    no_reparse(cursor)
    for i, component in enumerate(relative.parts):
        cursor = cursor / component
        if cursor.exists() or cursor.is_symlink():
            if i == len(relative.parts)-1 and allow_final_symlink:
                continue
            no_reparse(cursor)
    return target

require(STAGING.is_dir() and not DESTINATION.exists(), 'Expected unpromoted staging only')
for ancestor in [ROOT, ROOT / 'upstream', BATCH, STAGING]:
    no_reparse(ancestor)
require(STAGING.resolve().is_relative_to(BATCH.resolve()) and DESTINATION.absolute().is_relative_to(BATCH.resolve()), 'Promotion boundary invalid')
commit_api = read(RECEIPTS / 'cc-switch-commit-api.json')
require(commit_api['sha'] == COMMIT and commit_api['tree']['sha'] == EXPECTED_ROOT, 'Fixed commit/tree API mismatch')
metadata = read(RECEIPTS / 'cc-switch-complete-git-tree.json')
entries = {e['path']: e for e in metadata['entries']}
require(len(entries) == len(metadata['entries']), 'Duplicate metadata entries')
require(entries['']['sha'] == EXPECTED_ROOT, 'Root metadata mismatch')
children = {}
folded = set()
for path, entry in entries.items():
    if not path:
        continue
    safe_path(STAGING, path)
    require(path.casefold() not in folded, 'Case-insensitive path collision: ' + path)
    folded.add(path.casefold())
    parent, _, label = path.rpartition('/')
    require(parent in entries and entries[parent]['type'] == 'tree', 'Missing parent tree: ' + path)
    require((entry['type'] == 'tree' and entry['mode'] == '040000') or (entry['type'] == 'blob' and entry['mode'] in ['100644', '100755', '120000']), 'Unsupported Git type/mode: ' + path)
    children.setdefault(parent, []).append((label, entry))
checked_trees = {}

def check_tree(path):
    raw_entries = []
    for label, entry in children.get(path, []):
        tree = entry['type'] == 'tree'
        sha = check_tree(entry['path']) if tree else entry['sha']
        key = label.encode('utf-8') + (b'/' if tree else b'')
        raw_entries.append((key, entry['mode'].lstrip('0').encode() + b' ' + label.encode('utf-8') + b'\0' + bytes.fromhex(sha)))
    actual = digest('tree', b''.join(v for _, v in sorted(raw_entries)))
    require(actual == entries[path]['sha'], 'Independent tree mismatch: ' + path)
    checked_trees[path] = actual
    return actual

require(check_tree('') == EXPECTED_ROOT, 'Independent root tree mismatch')
blobs = {p:e for p,e in entries.items() if e['type'] == 'blob'}
records = read(RECEIPTS / 'cc-switch-recovered-file-records.json')
verified = {e['path']: e for e in records['files']}
retry = []
for retry_path in sorted(BATCH.glob('cc-switch-retry-connector-*.json')):
    retry.extend(read(retry_path))
retry_result = []
for item in retry:
    path = item['path']
    entry = blobs[path]
    require(path not in verified and item['content']['encoding'] == 'base64', 'Unexpected retry identity')
    raw = base64.b64decode(item['content']['content'], validate=False)
    require(len(raw) == entry['size'] and digest('blob', raw) == entry['sha'] == item['content']['sha'], 'Retry blob mismatch: ' + path)
    target = safe_path(STAGING, path)
    require(not target.exists(), 'Retry would overwrite source: ' + path)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(raw)
    verified[path] = {'path': path, 'mode': entry['mode'], 'size': len(raw), 'git_blob_sha1': entry['sha'], 'sha256': hashlib.sha256(raw).hexdigest(), 'provenance': 'fixed_commit_github_connector_base64_with_independent_blob_check'}
    retry_result.append(verified[path])
save(RECEIPTS / 'cc-switch-connector-recovery.json', {'schema_version': 1, 'files': retry_result, 'success_count': len(retry_result)})
require(set(verified) == set(blobs), 'Recovered file set is incomplete')
for path, target_value in records.get('pending_symlinks', {}).items():
    link = safe_path(STAGING, path)
    require(not link.exists() and not link.is_symlink(), 'Symlink already materialized')
    relative = PurePosixPath(target_value)
    require(not relative.is_absolute() and not PureWindowsPath(target_value).drive and ':' not in target_value and '\\' not in target_value, 'Unsafe symlink syntax')
    target = link.parent.joinpath(*relative.parts).resolve()
    require(target.is_relative_to(STAGING.resolve()), 'Symlink target escaped source root')
    safe_path(STAGING, target.relative_to(STAGING.resolve()).as_posix())
    require(target.is_file(), 'Missing symlink target')
    link.parent.mkdir(parents=True, exist_ok=True)
    os.symlink(target_value, link, target_is_directory=False)

for path, record in verified.items():
    entry = blobs[path]
    file = safe_path(STAGING, path, allow_final_symlink=entry['mode']=='120000')
    if entry['mode'] == '120000':
        require(file.is_symlink(), 'Expected source symlink: ' + path)
        require(file.resolve().is_relative_to(STAGING.resolve()), 'Materialized symlink escaped root')
        raw = os.readlink(file).encode('utf-8')
    else:
        require(file.is_file() and not file.is_symlink(), 'Expected regular source file: ' + path)
        raw = file.read_bytes()
    require(len(raw) == entry['size'] and digest('blob', raw) == entry['sha'] == record['git_blob_sha1'] and hashlib.sha256(raw).hexdigest() == record['sha256'], 'Final content mismatch: ' + path)
actual_set = set()
for current, dirs, files in os.walk(STAGING, followlinks=False):
    for name in dirs:
        no_reparse(Path(current) / name)
    for name in files:
        actual_set.add((Path(current) / name).relative_to(STAGING).as_posix())
require(actual_set == set(blobs), 'Final directory file set mismatch')
no_reparse(STAGING)
require(STAGING.resolve().is_relative_to(BATCH.resolve()) and DESTINATION.absolute().is_relative_to(BATCH.resolve()) and not DESTINATION.exists(), 'Promotion boundary changed')
STAGING.rename(DESTINATION)
manifest = {'schema_version': 1, 'commit': COMMIT, 'tree_sha1': EXPECTED_ROOT, 'files': sorted(verified.values(), key=lambda e:e['path'])}
manifest_path = RECEIPTS / 'cc-switch-content-manifest.json'
save(manifest_path, manifest)
result = {'schema_version': 1, 'batch': 'p02-source-batch-001', 'repository': 'https://github.com/farion1231/cc-switch', 'commit': COMMIT, 'tree_sha1': EXPECTED_ROOT, 'status': 'complete_fixed_source_verified', 'source_method': '81 complete members from failed archive + 1219 fixed raw files + 22 fixed connector files; every blob verified against independently reconstructed Git tree', 'archive_status': 'failed_incomplete_download_preserved', 'complete_commit_tree_verified': True, 'tree_count': len(checked_trees), 'file_count': len(verified), 'source_bytes': sum(e['size'] for e in verified.values()), 'materialization': DESTINATION.relative_to(ROOT).as_posix(), 'content_manifest': {'path': manifest_path.relative_to(ROOT).as_posix(), 'sha256': hashlib.sha256(manifest_path.read_bytes()).hexdigest()}, 'independent_tree_metadata': {'path': 'receipts/p02-source-batch-001/cc-switch-complete-git-tree.json', 'sha256': hashlib.sha256((RECEIPTS / 'cc-switch-complete-git-tree.json').read_bytes()).hexdigest()}, 'dirty_diff': {'modified': [], 'missing': [], 'extra': [], 'basis': 'Complete file-set and blob equality to independently reconstructed commit tree, not a Git checkout'}, 'mode_materialization': 'Git regular/executable modes recorded in manifest; Windows permission bits are not represented as Unix executable permissions; no source symlinks', 'license_files': [verified[p] for p in ['LICENSE']], 'verification_uses_assert': False, 'upstream_build_or_runtime_executed_by_source_audit': False, 'ended_at': datetime.datetime.now(datetime.timezone.utc).isoformat()}
save(RECEIPTS / 'cc-switch-source-verification.json', result)
print(json.dumps(result, indent=2), flush=True)
