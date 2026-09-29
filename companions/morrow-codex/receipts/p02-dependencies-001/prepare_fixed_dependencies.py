"""Prepare two public fixed archives; never runs source or reads Git/Cargo config."""
from __future__ import annotations
import concurrent.futures
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile
import time
import urllib.request

RECEIPTS = Path(__file__).resolve().parent
ROOT = RECEIPTS.parent.parent
OUTPUT = ROOT / 'upstream' / 'p02-dependencies-001'
REPOS = {
    'tokio-tungstenite': '0e5b2d73aa18dd9f0a50ee9ff199d5aef7594186',
    'tungstenite-rs': '4fffad30fe373adbdcffab9545e9e9bf4f2fc19f',
}
MAX_BYTES = 64 * 1024 * 1024

def digest(kind: str, data: bytes) -> str:
    return hashlib.sha1(kind.encode() + b' ' + str(len(data)).encode() + b'\0' + data).hexdigest()

def json_new(path: Path, data: object) -> None:
    with path.open('x', encoding='utf-8', newline='\n') as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
        f.write('\n')

def canonical(path: str) -> str:
    p = PurePosixPath(path)
    if p.is_absolute() or p.as_posix() != path or not p.parts:
        raise ValueError(f'Noncanonical path: {path!r}')
    for part in p.parts:
        if part in ('.', '..') or any(c in part for c in '\\:< >|?*'.replace(' ', '')) or part.endswith(('.', ' ')):
            raise ValueError(f'Unsafe path: {path!r}')
        if part.split('.')[0].upper() in {'CON', 'PRN', 'AUX', 'NUL', *(f'COM{i}' for i in range(1,10)), *(f'LPT{i}' for i in range(1,10))}:
            raise ValueError(f'Windows reserved path: {path!r}')
    return path

def prepare(name: str, commit_sha: str) -> dict:
    started = time.time()
    url = f'https://codeload.github.com/openai-oss-forks/{name}/tar.gz/{commit_sha}'
    attempt = {'repository': f'openai-oss-forks/{name}', 'commit': commit_sha, 'url': url,
               'attempt': 1, 'started_unix': started, 'network': 'stdlib urllib direct HTTPS, ProxyHandler({}), no Git/Cargo invocation'}
    archive = OUTPUT / f'{name}-{commit_sha}-attempt-001.tar.gz'
    attempt_file = RECEIPTS / f'{name}-attempt-001.json'
    if archive.exists() or attempt_file.exists():
        raise FileExistsError('Preserve attempts; choose a new attempt path instead of rerun')
    try:
        commit_path = RECEIPTS / f'{name}-commit.json'
        tree_path = RECEIPTS / f'{name}-tree.json'
        commit = json.loads(commit_path.read_text(encoding='utf-8'))
        tree = json.loads(tree_path.read_text(encoding='utf-8'))
        assert commit['sha'] == commit_sha and tree['truncated'] is False
        entries = {}
        folded = set()
        for entry in tree['tree']:
            p = canonical(entry['path'])
            assert p not in entries and p.casefold() not in folded, f'Duplicate tree path: {p}'
            assert (entry['type'], entry['mode']) in {('blob', '100644'), ('blob', '100755'), ('tree', '040000')}, f'Unsupported tree type: {entry}'
            entries[p] = entry
            folded.add(p.casefold())
        # Reconstruct every Git tree object, binding the returned recursive listing
        # to the exact commit tree SHA even when the API labels its root with a ref.
        directories = [''] + [p for p, e in entries.items() if e['type'] == 'tree']
        reconstructed = {}
        for directory in sorted(directories, key=lambda p: p.count('/') + bool(p), reverse=True):
            children = [(p, e) for p, e in entries.items() if str(PurePosixPath(p).parent).replace('.', '', 1) == directory]
            # Above parent handling must only normalize the root '.', not dot directories.
            children = [(p, e) for p, e in entries.items() if ('' if PurePosixPath(p).parent == PurePosixPath('.') else PurePosixPath(p).parent.as_posix()) == directory]
            ordered = sorted(children, key=lambda pe: (PurePosixPath(pe[0]).name + ('/' if pe[1]['type'] == 'tree' else '')).encode())
            body = b''
            for p, e in ordered:
                if e['type'] == 'tree':
                    assert reconstructed[p] == e['sha'], f'Subtree hash mismatch: {p}'
                body += e['mode'].lstrip('0').encode() + b' ' + PurePosixPath(p).name.encode() + b'\0' + bytes.fromhex(e['sha'])
            reconstructed[directory] = digest('tree', body)
        assert reconstructed[''] == commit['tree']['sha'], 'Root Git tree mismatch'
        attempt['verified_git_tree'] = reconstructed['']
        print(f'{name}: Git tree verified; downloading archive', flush=True)
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        req = urllib.request.Request(url, headers={'User-Agent': 'morrow-codex-fixed-dependency-preparation/1', 'Accept-Encoding': 'identity'})
        with opener.open(req, timeout=30) as response, archive.open('xb') as dest:
            attempt['http_status'] = response.status
            attempt['final_url'] = response.url
            attempt['response_headers'] = dict(response.headers)
            count = 0
            last_notice = time.monotonic()
            while True:
                if time.time() - started > 900:
                    raise TimeoutError('Archive overall 900-second budget exceeded')
                chunk = response.read(16384)
                if not chunk:
                    break
                count += len(chunk)
                if count > MAX_BYTES:
                    raise ValueError('Archive size budget exceeded')
                dest.write(chunk)
                if time.monotonic() - last_notice > 30:
                    print(f'{name}: downloaded {count} bytes', flush=True)
                    last_notice = time.monotonic()
        prefix = f'{name}-{commit_sha}'
        blob_entries = {p:e for p,e in entries.items() if e['type'] == 'blob'}
        files = {}
        seen = set()
        total = 0
        with tarfile.open(archive, mode='r:gz') as tar:
            for member in tar:
                raw = member.name.rstrip('/') if member.isdir() else member.name
                full = canonical(raw)
                assert full not in seen, f'Duplicate archive member {full}'
                seen.add(full)
                assert full == prefix or full.startswith(prefix + '/'), f'Wrong archive prefix: {full}'
                relative = full[len(prefix):].lstrip('/')
                if member.isdir():
                    assert not relative or (relative in entries and entries[relative]['type'] == 'tree'), f'Unknown archive directory {relative}'
                    continue
                assert member.isfile(), f'Nonregular archive member {full}'
                assert relative in blob_entries, f'Unexpected archive member {relative}'
                expected = blob_entries[relative]
                assert member.size == expected['size'], f'Archive size differs {relative}'
                total += member.size
                assert total <= MAX_BYTES, 'Extracted size budget exceeded'
                source = tar.extractfile(member)
                assert source is not None
                data = source.read()
                assert len(data) == member.size and digest('blob', data) == expected['sha'], f'Git blob hash differs {relative}'
                files[relative] = data
        assert set(files) == set(blob_entries), f'Missing archive blobs: {set(blob_entries) - set(files)}'
        source_root = OUTPUT / prefix
        source_root.mkdir(exist_ok=False)
        manifest = []
        for relative, data in sorted(files.items()):
            target = source_root.joinpath(*PurePosixPath(relative).parts)
            assert target.resolve().is_relative_to(source_root.resolve())
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open('xb') as f:
                f.write(data)
            observed = target.read_bytes()
            assert observed == data
            manifest.append({'path':relative, 'size':len(data), 'git_mode':blob_entries[relative]['mode'],
                             'git_blob_sha1':digest('blob', data), 'sha256':hashlib.sha256(data).hexdigest()})
        archive_bytes = archive.read_bytes()
        result = {**attempt, 'status':'complete_verified', 'source_root':str(source_root),
                  'archive_path':str(archive), 'archive_size':len(archive_bytes), 'archive_sha256':hashlib.sha256(archive_bytes).hexdigest(),
                  'git_tree_sha1':reconstructed[''], 'blob_count':len(manifest), 'content_bytes':total,
                  'commit_response_sha256':hashlib.sha256(commit_path.read_bytes()).hexdigest(),
                  'tree_response_sha256':hashlib.sha256(tree_path.read_bytes()).hexdigest(),
                  'elapsed_seconds':round(time.time()-started, 3),
                  'files':manifest,
                  'limits':['Source preparation only; no build, test, Cargo invocation, or runtime proof.', 'Windows files preserve content; Git executable modes retained in this manifest.', 'No existing source/dependency lock modified.']}
        json_new(RECEIPTS / f'{name}-verification.json', result)
        attempt.update({k:v for k,v in result.items() if k != 'files'})
        print(f'{name}: verified {len(manifest)} blobs, archive {len(archive_bytes)} bytes', flush=True)
        return {k:v for k,v in result.items() if k != 'files'}
    except Exception as exc:
        attempt.update(status='failed_preserved', error=f'{type(exc).__name__}: {exc}', elapsed_seconds=round(time.time()-started, 3))
        if archive.exists():
            data = archive.read_bytes()
            attempt.update(partial_archive_size=len(data), partial_archive_sha256=hashlib.sha256(data).hexdigest())
        print(f'{name}: {attempt["error"]}', flush=True)
        return attempt
    finally:
        json_new(attempt_file, attempt)

if __name__ == '__main__':
    OUTPUT.mkdir(exist_ok=False)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda pair:prepare(*pair), REPOS.items()))
    json_new(RECEIPTS / 'summary.json', {'scope':'exactly two fixed public WebSocket forks', 'dependencies':results})
    raise SystemExit(0 if all(r['status'] == 'complete_verified' for r in results) else 1)
