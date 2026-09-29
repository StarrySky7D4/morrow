"""Read-only fixed-source verification; optional receipt stays within this batch."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import stat

ROOT = Path(__file__).resolve().parents[2]
BATCH = ROOT / 'upstream/p02-source-batch-001'
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
IDENTITIES = {
    'codex': ('44fe510ce3ee61c8ef623adcbf89b901c73ddd61', '3b868fad63be6ac5db91402b579fab37f587d7d5'),
    'cc-switch': ('846de29c13ac4d65f164db8c15dd5fd58e29f972', 'e373c27485681c074ddab7d4f085ee9a6667b891'),
}

def require(condition, message):
    if not condition:
        raise ValueError(message)

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def git_hash(kind, raw):
    return hashlib.sha1(kind.encode()+b' '+str(len(raw)).encode()+b'\0'+raw).hexdigest()

def no_reparse(path):
    data = path.lstat()
    require(not stat.S_ISLNK(data.st_mode) and not (getattr(data, 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT), 'Unexpected reparse point: '+str(path))

parser = argparse.ArgumentParser()
parser.add_argument('source', choices=IDENTITIES)
parser.add_argument('--receipt-name', help='New receipt basename; existing receipts are never overwritten')
args = parser.parse_args()
commit, root_tree = IDENTITIES[args.source]
source = BATCH / (args.source+'-source')
manifest_path = RECEIPTS / (args.source+'-content-manifest.json')
tree_path = RECEIPTS / (args.source+'-complete-git-tree.json')
commit_path = RECEIPTS / (args.source+'-commit-api.json')
for ancestor in [ROOT, ROOT/'upstream', BATCH, source]:
    no_reparse(ancestor)
require(source.resolve().is_relative_to(BATCH.resolve()), 'Source root escaped batch')
api = read(commit_path)
require(api['sha']==commit and api['tree']['sha']==root_tree, 'Commit API identity mismatch')
tree = read(tree_path)
manifest = read(manifest_path)
require(tree['commit']==commit and tree['tree_sha1']==root_tree and manifest['commit']==commit and manifest['tree_sha1']==root_tree, 'Manifest identity mismatch')
entries = {e['path']:e for e in tree['entries']}
require(len(entries)==len(tree['entries']), 'Duplicate tree entries')
files = {e['path']:e for e in manifest['files']}
blobs = {p:e for p,e in entries.items() if e['type']=='blob'}
require(len(files)==len(manifest['files']) and set(files)==set(blobs), 'Manifest file-set mismatch')
folded = set()
children = {}
actual_blob_hashes = {}
reserved = {'CON','PRN','AUX','NUL',*['COM'+str(i) for i in range(1,10)],*['LPT'+str(i) for i in range(1,10)]}
for path, entry in entries.items():
    if not path:
        continue
    relative = PurePosixPath(path)
    require(not relative.is_absolute() and not PureWindowsPath(path).drive and ':' not in path and '\\' not in path and all(p not in ['', '.', '..'] and not p.endswith((' ', '.')) and p.split('.')[0].upper() not in reserved for p in path.split('/')), 'Unsafe metadata path: '+path)
    require(path.casefold() not in folded, 'Windows filename collision: '+path)
    folded.add(path.casefold())
    parent, _, label = path.rpartition('/')
    require(parent in entries and entries[parent]['type']=='tree', 'Missing parent metadata')
    children.setdefault(parent, []).append((label, entry))
    file = source.joinpath(*relative.parts)
    require(file.absolute().is_relative_to(source.absolute()), 'File escaped source root')
    for ancestor in file.parents:
        if ancestor == source:
            break
        no_reparse(ancestor)
    if entry['type']=='tree':
        require(entry['mode']=='040000' and file.is_dir(), 'Missing source directory')
        no_reparse(file)
        continue
    require(entry['type']=='blob' and entry['mode'] in ['100644','100755','120000'], 'Unsupported tree object type/mode')
    if entry['mode']=='120000':
        require(file.is_symlink(), 'Expected source symlink: '+path)
        target = os.readlink(file)
        require(not PurePosixPath(target).is_absolute() and not PureWindowsPath(target).drive and ':' not in target and '\\' not in target and file.resolve().is_relative_to(source.resolve()), 'Unsafe source symlink')
        raw = target.encode('utf-8')
    else:
        no_reparse(file)
        require(file.is_file(), 'Missing source file: '+path)
        raw = file.read_bytes()
    digest = git_hash('blob', raw)
    expected = files[path]
    require(len(raw)==entry['size']==expected['size'] and digest==entry['sha']==expected['git_blob_sha1'] and entry['mode']==expected['mode'] and hashlib.sha256(raw).hexdigest()==expected['sha256'], 'Source content mismatch: '+path)
    actual_blob_hashes[path] = digest

actual_files = set()
actual_dirs = set()
for current, dirs, names in os.walk(source, followlinks=False):
    for name in dirs:
        directory = Path(current)/name
        no_reparse(directory)
        actual_dirs.add(directory.relative_to(source).as_posix())
    for name in names:
        actual_files.add((Path(current)/name).relative_to(source).as_posix())
require(actual_files==set(blobs), 'Source has missing or extra files')
expected_dirs = {p for p,e in entries.items() if p and e['type']=='tree'}
require(actual_dirs==expected_dirs, 'Source has missing or extra directories')
checked_trees = {}

def check_tree(path):
    raw_entries = []
    for label, entry in children.get(path, []):
        is_tree = entry['type']=='tree'
        sha = check_tree(entry['path']) if is_tree else actual_blob_hashes[entry['path']]
        raw_entries.append((label.encode()+(b'/' if is_tree else b''),entry['mode'].lstrip('0').encode()+b' '+label.encode()+b'\0'+bytes.fromhex(sha)))
    actual = git_hash('tree', b''.join(v for _,v in sorted(raw_entries)))
    require(actual==entries[path]['sha'], 'Materialized Git tree mismatch: '+path)
    checked_trees[path] = actual
    return actual

require(check_tree('')==root_tree, 'Materialized root tree mismatch')
result = {'schema_version':1,'status':'complete_fixed_source_reverified','source':args.source,'commit':commit,'tree_sha1':root_tree,'file_count':len(blobs),'tree_count':len(checked_trees),'source_bytes':sum(e['size'] for e in blobs.values()),'dirty_diff':{'modified':[],'missing':[],'extra':[]},'content_manifest_sha256':sha256(manifest_path),'tree_metadata_sha256':sha256(tree_path),'verifier_sha256':sha256(Path(__file__)),'source_read_only':True,'executed_upstream_code':False,'windows_note':'Git executable modes are verified from immutable metadata; Unix filesystem execute-bit parity is not asserted','verified_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
output = json.dumps(result,indent=2)+'\n'
if args.receipt_name:
    require(Path(args.receipt_name).name==args.receipt_name and ':' not in args.receipt_name and args.receipt_name.endswith('.json'), 'Receipt must be a JSON basename')
    target = RECEIPTS/args.receipt_name
    require(not target.exists() and target.absolute().is_relative_to(RECEIPTS.resolve()), 'Refuse receipt overwrite or escape')
    target.write_text(output,encoding='utf-8')
print(output,end='')
