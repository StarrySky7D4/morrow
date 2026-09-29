import base64
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
data = json.loads(Path(sys.argv[2]).read_text(encoding='utf-8'))
verified = []
for item in data:
    if not item.get('content'):
        continue
    raw = base64.b64decode(item['content']) if item.get('encoding') == 'base64' else item['content'].encode('utf-8')
    digest = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
    if digest != item['sha']:
        raise SystemExit('blob mismatch: ' + item['path'])
    path = root / item['path']
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)
    verified.append({'path': item['path'], 'git_blob_sha1': digest, 'sha256': hashlib.sha256(raw).hexdigest(), 'url': item.get('display_url')})
Path(sys.argv[3]).write_text(json.dumps({'schema_version': 1, 'files': verified}, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print('verified', len(verified))
