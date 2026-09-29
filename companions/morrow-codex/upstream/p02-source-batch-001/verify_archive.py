import datetime
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import tarfile
import sys

ROOT = Path(__file__).resolve().parents[2]
BATCH = ROOT / 'upstream/p02-source-batch-001'
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
SOURCES = {'codex': ('openai/codex', '44fe510ce3ee61c8ef623adcbf89b901c73ddd61'), 'cc-switch': ('farion1231/cc-switch', '846de29c13ac4d65f164db8c15dd5fd58e29f972')}
name = sys.argv[1]
repo, commit = SOURCES[name]
archive = BATCH / f'{name}-{commit}.tar.gz'
destination = BATCH / f'{name}-source'
commit_receipt = json.loads((RECEIPTS / f'{name}-commit-api.json').read_text(encoding='utf-8'))
expected_tree = commit_receipt['tree']['sha']
assert commit_receipt['sha'] == commit
download = json.loads((RECEIPTS / f'{name}-archive-attempt.json').read_text(encoding='utf-8'))
if download['status'] != 'download_complete_archive_validation_pending':
    raise SystemExit('Archive acquisition has not completed successfully')
if destination.exists():
    raise SystemExit('Refuse to overwrite source materialization')

def sha256_file(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()

def git_hash(kind, raw):
    return hashlib.sha1(kind.encode() + b' ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()

result = {'schema_version': 1, 'batch': 'p02-source-batch-001', 'repository': repo, 'commit': commit, 'expected_tree_sha1': expected_tree, 'archive': str(archive.relative_to(ROOT)), 'archive_bytes': archive.stat().st_size, 'archive_sha256': sha256_file(archive), 'started_at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'status': 'validating'}
files = []
try:
    if result['archive_sha256'] != download['sha256_of_received_bytes']:
        raise ValueError('Download receipt digest differs from archive')
    uncompressed = 0
    with gzip.open(archive, 'rb') as stream:
        while block := stream.read(1024 * 1024):
            uncompressed += len(block)
            if uncompressed > 1024 * 1024 * 1024:
                raise ValueError('Archive exceeds 1 GiB uncompressed budget')
    result['gzip_crc_and_eof_verified'] = True
    result['uncompressed_tar_bytes'] = uncompressed
    root_prefix = name + '-' + commit
    paths = set()
    folded = set()
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar:
            path = PurePosixPath(member.name)
            if not path.parts or path.parts[0] != root_prefix or path.is_absolute() or '..' in path.parts or '\\' in member.name:
                raise ValueError('Unexpected/unsafe archive path: ' + member.name)
            if member.isdir():
                continue
            rel = PurePosixPath(*path.parts[1:]).as_posix()
            if not rel or rel in paths or rel.casefold() in folded:
                raise ValueError('Duplicate or Windows-colliding path: ' + rel)
            paths.add(rel)
            folded.add(rel.casefold())
            if len(paths) > 50000 or member.size > 64 * 1024 * 1024:
                raise ValueError('Archive member budget exceeded')
            if member.isfile():
                with tar.extractfile(member) as source:
                    raw = source.read()
                mode = '100755' if member.mode & 0o111 else '100644'
                kind = 'file'
            elif member.issym():
                raw = member.linkname.encode('utf-8')
                mode, kind = '120000', 'symlink'
            else:
                raise ValueError('Unsupported archive entry type: ' + rel)
            entry = {'path': rel, 'kind': kind, 'mode': mode, 'size': len(raw), 'git_blob_sha1': git_hash('blob', raw), 'sha256': hashlib.sha256(raw).hexdigest()}
            if kind == 'symlink':
                entry['target'] = member.linkname
            files.append(entry)
    tree = {}
    for file in files:
        current = tree
        parts = file['path'].split('/')
        for part in parts[:-1]:
            child = current.setdefault(part, {})
            if not isinstance(child, dict) or 'git_blob_sha1' in child:
                raise ValueError('Path conflicts with file parent')
            current = child
        current[parts[-1]] = file
    tree_ids = {}
    def tree_hash(node, prefix=''):
        entries = []
        for label, item in node.items():
            is_file = 'git_blob_sha1' in item
            digest = item['git_blob_sha1'] if is_file else tree_hash(item, prefix + label + '/')
            mode = item['mode'] if is_file else '40000'
            entries.append((label.encode() + (b'' if is_file else b'/'), mode.encode() + b' ' + label.encode() + b'\0' + bytes.fromhex(digest)))
        raw = b''.join(value for _, value in sorted(entries))
        digest = git_hash('tree', raw)
        tree_ids[prefix.rstrip('/')] = digest
        return digest
    actual_tree = tree_hash(tree)
    result['reconstructed_tree_sha1'] = actual_tree
    result['file_count'] = len(files)
    result['source_bytes'] = sum(x['size'] for x in files)
    result['symlink_count'] = sum(x['kind'] == 'symlink' for x in files)
    if actual_tree != expected_tree:
        raise ValueError('Archive reconstructed Git tree does not match fixed commit tree')
    result['complete_commit_tree_verified'] = True
    manifest_path = RECEIPTS / f'{name}-content-manifest.json'
    manifest_path.write_text(json.dumps({'schema_version': 1, 'repository': repo, 'commit': commit, 'tree_sha1': actual_tree, 'files': sorted(files, key=lambda x:x['path']), 'trees': tree_ids}, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
    result['content_manifest'] = {'path': str(manifest_path.relative_to(ROOT)), 'sha256': sha256_file(manifest_path)}
    destination.mkdir()
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar:
            if not member.isfile():
                continue
            rel = PurePosixPath(*PurePosixPath(member.name).parts[1:])
            target = destination.joinpath(*rel.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            with tar.extractfile(member) as source, target.open('xb') as out:
                while chunk := source.read(1024 * 1024):
                    out.write(chunk)
    for entry in files:
        if entry['kind'] != 'symlink':
            continue
        target = destination / entry['path']
        target.parent.mkdir(parents=True, exist_ok=True)
        relative = Path(entry['target'])
        resolved = (target.parent / relative).resolve()
        if relative.is_absolute() or not resolved.is_relative_to(destination.resolve()):
            raise ValueError('Unsafe source symlink target: ' + entry['path'])
        os.symlink(entry['target'], target, target_is_directory=resolved.is_dir())
    for entry in files:
        path = destination / entry['path']
        raw = os.readlink(path).encode('utf-8') if entry['kind'] == 'symlink' else path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != entry['sha256']:
            raise ValueError('Materialized source mismatch: ' + entry['path'])
    actual_paths = {p.relative_to(destination).as_posix() for p in destination.rglob('*') if p.is_file() or p.is_symlink()}
    if actual_paths != paths:
        raise ValueError('Materialized source set mismatch')
    result['materialization'] = str(destination.relative_to(ROOT))
    result['dirty_diff'] = {'modified': [], 'missing': [], 'extra': [], 'basis': 'Full materialized file set and bytes compared with fixed-commit archive tree; not a Git checkout'}
    result['license_files'] = [{'path': entry['path'], 'sha256': entry['sha256']} for entry in files if entry['path'] in ['LICENSE', 'NOTICE']]
    result['status'] = 'complete_fixed_source_verified'
    result['build_or_runtime_executed'] = False
except Exception as error:
    result['status'] = 'validation_failed'
    result['error'] = type(error).__name__ + ': ' + str(error)
finally:
    result['ended_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    (RECEIPTS / f'{name}-source-verification.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2), flush=True)
raise SystemExit(0 if result['status'] == 'complete_fixed_source_verified' else 1)
