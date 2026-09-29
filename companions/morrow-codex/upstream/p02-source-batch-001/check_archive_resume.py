import datetime
import json
from pathlib import Path
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'
name = sys.argv[1]
receipt = json.loads((RECEIPTS / f'{name}-archive-attempt.json').read_text(encoding='utf-8'))
offset = receipt['actual_file_bytes']
etag = receipt.get('response_headers', {}).get('ETag')
headers = {'User-Agent': 'morrow-fixed-source-audit/1', 'Range': f'bytes={offset}-{offset}'}
if etag:
    headers['If-Range'] = etag
result = {'schema_version': 1, 'source': name, 'url': receipt['url'], 'requested_range': headers['Range'], 'if_range': etag, 'started_at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'maximum_body_read_bytes': 1}
try:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(urllib.request.Request(receipt['url'], headers=headers), timeout=30) as response:
        result['http_status'] = response.status
        result['headers'] = {k: v for k,v in response.headers.items() if k.lower() in {'content-length', 'content-range', 'etag', 'accept-ranges'}}
        result['resume_supported'] = response.status == 206 and (response.headers.get('Content-Range') or '').startswith(f'bytes {offset}-{offset}/') and response.headers.get('ETag') == etag
        if result['resume_supported']:
            result['probe_bytes_read'] = len(response.read(1))
        else:
            result['probe_bytes_read'] = 0
except Exception as error:
    result['resume_supported'] = False
    result['error'] = type(error).__name__ + ': ' + str(error)
result['ended_at'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
(RECEIPTS / f'{name}-range-check.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result, indent=2), flush=True)
