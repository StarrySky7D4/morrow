import concurrent.futures
import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import stat
import tarfile
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
BATCH = ROOT / 'upstream/p02-source-batch-001'
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
COMMIT = '846de29c13ac4d65f164db8c15dd5fd58e29f972'
EXPECTED_ROOT = 'e373c27485681c074ddab7d4f085ee9a6667b891'
STAGING = BATCH / 'cc-switch-source-staging'
DESTINATION = BATCH / 'cc-switch-source'
ARCHIVE = BATCH / f'cc-switch-{COMMIT}.tar.gz'

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')

def require(condition, message):
    if not condition:
        raise ValueError(message)

def digest(kind, raw):
    return hashlib.sha1(kind.encode() + b' ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()

def no_reparse(path):
    s = path.lstat()
    require(not stat.S_ISLNK(s.st_mode) and not (getattr(s, 'st_file_attributes', 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT), 'Unexpected reparse point: ' + str(path))

def safe_path(root, path):
    parts = PurePosixPath(path)
    require(bool(path) and not parts.is_absolute() and not PureWindowsPath(path).drive and ':' not in path and '\\' not in path and all(x not in ['', '.', '..'] for x in path.split('/')), 'Unsafe source path: ' + path)
    reserved = {'CON', 'PRN', 'AUX', 'NUL', *['COM'+str(i) for i in range(1,10)], *['LPT'+str(i) for i in range(1,10)]}
    require(all(not x.endswith((' ', '.')) and x.split('.')[0].upper() not in reserved for x in parts.parts), 'Windows path normalization: ' + path)
    target = root.joinpath(*parts.parts)
    require(target.absolute().is_relative_to(root.absolute()), 'Target escaped source root')
    cursor = root
    if cursor.exists():
        no_reparse(cursor)
    for part in parts.parts:
        cursor = cursor / part
        if cursor.exists() or cursor.is_symlink():
            no_reparse(cursor)
    return target

for ancestor in [ROOT, ROOT / 'upstream', BATCH]:
    no_reparse(ancestor)
require(not STAGING.exists() and not DESTINATION.exists(), 'Refuse to overwrite source')
commit_api = read(RECEIPTS / 'cc-switch-commit-api.json')
require(commit_api['sha'] == COMMIT and commit_api['tree']['sha'] == EXPECTED_ROOT, 'Fixed commit identity mismatch')
old_metadata_path = ROOT / 'upstream/cc-switch-git-tree-complete.json'
old_metadata_bytes = old_metadata_path.read_bytes()
metadata = json.loads(old_metadata_bytes)
require(metadata.get('truncated') is False and metadata['sha'] == COMMIT, 'Incomplete or wrong fixed tree metadata')
(RECEIPTS / 'cc-switch-reused-tree-api.json').write_bytes(old_metadata_bytes)
entries = {'': {'path':'', 'type':'tree', 'mode':'040000', 'sha':EXPECTED_ROOT}}
folded = set()
for entry in metadata['tree']:
    path = entry['path']
    safe_path(STAGING, path)
    require(path not in entries and path.casefold() not in folded, 'Duplicate source path')
    require((entry['type']=='tree' and entry['mode']=='040000') or (entry['type']=='blob' and entry['mode'] in ['100644','100755']), 'Unsupported Git type/mode')
    entries[path] = entry
    folded.add(path.casefold())
children = {}
for path, entry in entries.items():
    if not path:
        continue
    parent, _, label = path.rpartition('/')
    require(parent in entries and entries[parent]['type']=='tree', 'Missing parent tree')
    children.setdefault(parent, []).append((label, entry))
checked_trees = {}

def check_tree(path):
    raw_entries = []
    for label, entry in children.get(path, []):
        is_tree = entry['type']=='tree'
        sha = check_tree(entry['path']) if is_tree else entry['sha']
        raw_entries.append((label.encode()+(b'/' if is_tree else b''), entry['mode'].lstrip('0').encode()+b' '+label.encode()+b'\0'+bytes.fromhex(sha)))
    actual = digest('tree', b''.join(v for _,v in sorted(raw_entries)))
    require(actual == entries[path]['sha'], 'Independent Git tree mismatch: '+path)
    checked_trees[path] = actual
    return actual

require(check_tree('') == EXPECTED_ROOT, 'Independent root tree mismatch')
blobs = {p:e for p,e in entries.items() if e['type']=='blob'}
save(RECEIPTS / 'cc-switch-complete-git-tree.json', {'schema_version':1, 'commit':COMMIT, 'tree_sha1':EXPECTED_ROOT, 'complete_independent_tree_verified':True, 'tree_count':len(checked_trees), 'file_count':len(blobs), 'bytes':sum(e['size'] for e in blobs.values()), 'metadata_provenance':{'path':old_metadata_path.relative_to(ROOT).as_posix(), 'sha256':hashlib.sha256(old_metadata_bytes).hexdigest(), 'method':'Read-only reuse of prior fixed GitHub API tree, independently recomputed against current-batch fixed commit API'}, 'entries':sorted(entries.values(), key=lambda e:e['path'])})
print(f'Independent CC tree complete: {len(blobs)} files / {sum(e["size"] for e in blobs.values())} bytes', flush=True)
STAGING.mkdir()
verified = {}

def write_verified(path, raw, provenance):
    entry = blobs[path]
    require(len(raw)==entry['size'] and digest('blob',raw)==entry['sha'], 'Fixed blob mismatch: '+path)
    target = safe_path(STAGING,path)
    require(not target.exists(), 'Refuse source overwrite: '+path)
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_bytes(raw)
    verified[path] = {'path':path, 'mode':entry['mode'], 'size':len(raw), 'git_blob_sha1':entry['sha'], 'sha256':hashlib.sha256(raw).hexdigest(), 'provenance':provenance}

archive_error = None
try:
    with tarfile.open(ARCHIVE, 'r|gz') as tar:
        for member in tar:
            if member.isdir():
                continue
            parts = PurePosixPath(member.name).parts
            require(bool(parts) and parts[0]=='cc-switch-'+COMMIT, 'Unexpected archive root')
            path = PurePosixPath(*parts[1:]).as_posix()
            require(path in blobs and path not in verified and member.isfile(), 'Unexpected or duplicate tar member')
            require(member.size==blobs[path]['size'], 'Archive metadata size mismatch: '+path)
            with tar.extractfile(member) as stream:
                raw = stream.read(member.size+1)
            require(len(raw)==member.size, 'Incomplete archive member: '+path)
            write_verified(path,raw,'complete_member_from_failed_archive_with_independent_blob_check')
except (EOFError,tarfile.ReadError) as error:
    archive_error = type(error).__name__+': '+str(error)
missing = [e for p,e in blobs.items() if p not in verified]
initial = {'schema_version':1,'commit':COMMIT,'tree_sha1':EXPECTED_ROOT,'archive_status':'failed_incomplete_download','archive_recovery_error':archive_error,'recovered_complete_members':len(verified),'missing_count':len(missing),'missing_bytes':sum(e['size'] for e in missing),'missing':missing,'raw_workers':12,'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
save(RECEIPTS/'cc-switch-recovery-initial.json',initial)
print(f'Recovered {len(verified)} complete blob-verified members; missing {len(missing)} / {initial["missing_bytes"]} bytes',flush=True)

def fetch(entry):
    path = entry['path']
    url = 'https://raw.githubusercontent.com/farion1231/cc-switch/'+COMMIT+'/'+urllib.parse.quote(path,safe='/')
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        with opener.open(urllib.request.Request(url,headers={'User-Agent':'morrow-fixed-source-audit/1'}),timeout=60) as response:
            require(response.status==200,'Unexpected HTTP status')
            raw = response.read(entry['size']+1)
        require(len(raw)==entry['size'] and digest('blob',raw)==entry['sha'],'Fixed blob mismatch')
        return path,raw,None
    except Exception as error:
        return path,None,{'path':path,'url':url,'error':type(error).__name__+': '+str(error)}

failures=[]
with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
    for n,future in enumerate(concurrent.futures.as_completed([pool.submit(fetch,e) for e in missing]),1):
        path,raw,failure=future.result()
        if failure:
            failures.append(failure)
        else:
            write_verified(path,raw,'fixed_commit_raw_https_with_independent_blob_check')
        if n%25==0 or n==len(missing):
            print(f'CC raw recovery {n}/{len(missing)}; failures {len(failures)}',flush=True)
save(RECEIPTS/'cc-switch-raw-recovery.json',{'schema_version':1,'attempted_files':len(missing),'success_count':len(missing)-len(failures),'failures':failures,'ended_at':datetime.datetime.now(datetime.timezone.utc).isoformat()})
save(RECEIPTS/'cc-switch-recovered-file-records.json',{'schema_version':1,'files':sorted(verified.values(),key=lambda e:e['path'])})
require(not failures and set(verified)==set(blobs),'CC recovery incomplete; precise failure inventory preserved')
for path,record in verified.items():
    raw=safe_path(STAGING,path).read_bytes()
    require(len(raw)==blobs[path]['size'] and digest('blob',raw)==record['git_blob_sha1'] and hashlib.sha256(raw).hexdigest()==record['sha256'],'Final source content mismatch')
actual=set()
for current,dirs,files in os.walk(STAGING,followlinks=False):
    for name in dirs+files:
        no_reparse(Path(current)/name)
    actual.update((Path(current)/name).relative_to(STAGING).as_posix() for name in files)
require(actual==set(blobs),'Final source file-set mismatch')
require(STAGING.resolve().is_relative_to(BATCH.resolve()) and DESTINATION.absolute().is_relative_to(BATCH.resolve()) and not DESTINATION.exists(),'Source promotion boundary invalid')
STAGING.rename(DESTINATION)
manifest_path=RECEIPTS/'cc-switch-content-manifest.json'
save(manifest_path,{'schema_version':1,'commit':COMMIT,'tree_sha1':EXPECTED_ROOT,'files':sorted(verified.values(),key=lambda e:e['path'])})
result={'schema_version':1,'batch':'p02-source-batch-001','repository':'https://github.com/farion1231/cc-switch','commit':COMMIT,'tree_sha1':EXPECTED_ROOT,'status':'complete_fixed_source_verified','source_method':'Verified complete members from failed archive plus independently blob-verified missing fixed raw files','archive_status':'failed_incomplete_download_preserved','complete_commit_tree_verified':True,'tree_count':len(checked_trees),'file_count':len(verified),'source_bytes':sum(e['size'] for e in verified.values()),'materialization':DESTINATION.relative_to(ROOT).as_posix(),'content_manifest':{'path':manifest_path.relative_to(ROOT).as_posix(),'sha256':hashlib.sha256(manifest_path.read_bytes()).hexdigest()},'dirty_diff':{'modified':[],'missing':[],'extra':[],'basis':'Complete file-set and blob equality to independently reconstructed commit tree, not a Git checkout'},'mode_materialization':'Git regular/executable modes recorded; Windows Unix executable permission equivalence is not claimed; no source symlinks','license_files':[verified['LICENSE']],'verification_uses_assert':False,'upstream_build_or_runtime_executed_by_source_audit':False,'ended_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
save(RECEIPTS/'cc-switch-source-verification.json',result)
print(json.dumps(result,indent=2),flush=True)
