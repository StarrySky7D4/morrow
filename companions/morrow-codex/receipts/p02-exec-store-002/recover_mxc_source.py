"""Recover only complete verified blobs, then fetch missing public pinned files."""
from pathlib import Path
import concurrent.futures
import hashlib
import json
import tarfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = Path(__file__).resolve().parent
REV = "6cd3d58f05d3447e67109cfb75e042803b843ca4"
BASE = ROOT / "upstream/p02-exec-store-dependencies-002"
TARGET = BASE / f"mxc-{REV}"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def blob(data):
    return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def main():
    tree_file = RECEIPTS / "mxc-tree.json"
    commit_file = RECEIPTS / "mxc-commit.json"
    tree = json.loads(tree_file.read_text(encoding="utf-8"))
    commit = json.loads(commit_file.read_text(encoding="utf-8"))
    if commit['sha'] != REV or tree['truncated']:
        raise RuntimeError("Unpinned or truncated metadata")
    previous = json.loads((RECEIPTS / 'mxc-attempt-003.json').read_text(encoding="utf-8"))
    if previous['verified_git_tree'] != commit['tree']['sha']:
        raise RuntimeError("Metadata tree not verified")
    entries = {e['path']: e for e in tree['tree'] if e['type'] == 'blob'}
    if any(e['mode'] not in ('100644', '100755') for e in entries.values()):
        raise RuntimeError("Unsupported mode")
    TARGET.mkdir(exist_ok=False)
    archive = BASE / f'mxc-{REV}-attempt-003.tar.gz'
    recovered = []
    archive_error = None
    try:
        with tarfile.open(archive, mode='r|gz') as reader:
            for member in reader:
                if not member.isfile():
                    continue
                prefix = f'mxc-{REV}/'
                if not member.name.startswith(prefix):
                    raise RuntimeError('Wrong archive root')
                relative = member.name[len(prefix):]
                expected = entries[relative]
                if member.size != expected['size']:
                    raise RuntimeError('Wrong member size')
                data = reader.extractfile(member).read()
                if len(data) != expected['size'] or blob(data) != expected['sha']:
                    raise RuntimeError('Wrong member hash')
                target = TARGET / relative
                if not target.resolve().is_relative_to(TARGET.resolve()):
                    raise RuntimeError('Escaping member')
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open('xb') as output:
                    output.write(data)
                recovered.append(relative)
    except (tarfile.TarError, EOFError) as error:
        archive_error = str(error)
    missing = sorted(set(entries) - set(recovered))
    print(json.dumps({'recovered_verified_blobs':len(recovered), 'missing':len(missing), 'archive_error':archive_error}), flush=True)
    attempts = []
    def fetch(relative):
        started = time.monotonic()
        row = {'path':relative, 'url':f'https://raw.githubusercontent.com/microsoft/mxc/{REV}/{relative}'}
        try:
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            request = urllib.request.Request(row['url'], headers={'User-Agent':'morrow-pinned-source-recovery/1', 'Accept-Encoding':'identity'})
            expected = entries[relative]
            with opener.open(request, timeout=30) as response:
                data = response.read(expected['size'] + 1)
            if len(data) != expected['size'] or blob(data) != expected['sha']:
                raise RuntimeError('Pinned raw blob mismatch')
            target = TARGET / relative
            if not target.resolve().is_relative_to(TARGET.resolve()):
                raise RuntimeError('Escaping raw path')
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open('xb') as output:
                output.write(data)
            row.update(status='verified', sha256=sha(data))
        except Exception as error:
            row.update(status='failed', error=f'{type(error).__name__}: {error}')
        row['elapsed_seconds'] = time.monotonic() - started
        return row
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for row in pool.map(fetch, missing):
            attempts.append(row)
            if len(attempts) % 50 == 0:
                print(json.dumps({'raw_completed':len(attempts)}), flush=True)
    result = {'commit':REV,'verified_git_tree':commit['tree']['sha'], 'recovered_blobs':recovered,'archive_sha256':sha(archive.read_bytes()),'archive_error':archive_error,'raw_attempts':attempts, 'tree_response_sha256':sha(tree_file.read_bytes()), 'commit_response_sha256':sha(commit_file.read_bytes())}
    success = all(r['status']=='verified' for r in attempts)
    files = []
    if success:
        for relative, expected in sorted(entries.items()):
            data = (TARGET / relative).read_bytes()
            if blob(data) != expected['sha']:
                raise RuntimeError('Final blob mismatch')
            files.append({'path':relative, 'size':len(data), 'git_mode':expected['mode'], 'git_blob_sha1':expected['sha'], 'sha256':sha(data)})
        if {p.relative_to(TARGET).as_posix() for p in TARGET.rglob('*') if p.is_file()} != set(entries):
            raise RuntimeError('Final file set differs')
    result.update(status='complete_verified' if success else 'failed_preserved', source_root=str(TARGET), files=files, blob_count=len(files), scope='Complete fixed source snapshot recovered from partial archive plus pinned public raw files; not a complete archive or Git checkout')
    with (RECEIPTS / 'mxc-verification-004.json').open('x') as output:
        json.dump(result, output, indent=2)
    print(json.dumps({'status':result['status'], 'blob_count':len(files), 'failed':sum(r['status']!='verified' for r in attempts)}), flush=True)
    raise SystemExit(0 if success else 1)


if __name__ == '__main__':
    main()
