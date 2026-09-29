import datetime
import hashlib
import json
from pathlib import Path
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
BATCH = ROOT / 'upstream/p02-source-batch-001'
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
SOURCES = {'codex': ('openai/codex', '44fe510ce3ee61c8ef623adcbf89b901c73ddd61'), 'cc-switch': ('farion1231/cc-switch', '846de29c13ac4d65f164db8c15dd5fd58e29f972')}
name = sys.argv[1]
repo, commit = SOURCES[name]
url = f'https://codeload.github.com/{repo}/tar.gz/{commit}'
path = BATCH / f'{name}-{commit}.tar.gz'
if path.exists():
    raise SystemExit('Refuse to overwrite an earlier acquisition; use a new batch or explicit resume plan')
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
record = {'schema_version': 1, 'batch': 'p02-source-batch-001', 'repository': repo, 'commit': commit, 'url': url, 'method': 'Python urllib HTTPS, no proxy/config/account loading', 'command': f'python upstream/p02-source-batch-001/acquire_archive.py {name}', 'started_at': started, 'path': str(path.relative_to(ROOT)), 'status': 'downloading', 'bytes_received': 0, 'total_limit_seconds': 1800}
deadline = time.monotonic() + 1800
last_report = 0
digest = hashlib.sha256()
try:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    request = urllib.request.Request(url, headers={'User-Agent': 'morrow-fixed-source-audit/1'})
    with opener.open(request, timeout=60) as response, path.open('xb') as target:
        record['http_status'] = response.status
        record['response_headers'] = {k: v for k,v in response.headers.items() if k.lower() in {'content-type', 'content-length', 'etag', 'content-disposition'}}
        while True:
            if time.monotonic() > deadline:
                raise TimeoutError('archive exceeded 1800 second total limit')
            chunk = response.read(1024 * 256)
            if not chunk:
                break
            target.write(chunk)
            target.flush()
            digest.update(chunk)
            record['bytes_received'] += len(chunk)
            if time.monotonic() - last_report >= 15:
                print(f'{name}: received {record["bytes_received"]} bytes', flush=True)
                last_report = time.monotonic()
        expected = response.headers.get('Content-Length')
        if expected is not None and int(expected) != record['bytes_received']:
            raise ValueError('Content-Length mismatch')
    record['status'] = 'download_complete_archive_validation_pending'
    record['exit_code'] = 0
except Exception as error:
    record['status'] = 'failed_incomplete_download'
    record['exit_code'] = 1
    record['error'] = type(error).__name__ + ': ' + str(error)
finally:
    record['ended_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    record['sha256_of_received_bytes'] = digest.hexdigest()
    record['actual_file_bytes'] = path.stat().st_size if path.exists() else 0
    (RECEIPTS / f'{name}-archive-attempt.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(record, indent=2), flush=True)
raise SystemExit(record['exit_code'])
