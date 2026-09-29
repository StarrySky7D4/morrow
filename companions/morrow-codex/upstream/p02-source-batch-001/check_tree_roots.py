import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2]
receipts = root / 'receipts/p02-source-batch-001'
checks = []
for name in ['codex', 'cc-switch']:
    commit = json.loads((receipts / f'{name}-commit-api.json').read_text(encoding='utf-8'))
    tree = json.loads((receipts / f'{name}-root-api.json').read_text(encoding='utf-8'))
    if tree.get('truncated'):
        raise SystemExit('Truncated root metadata')
    entries = []
    for entry in tree['tree']:
        mode = entry['mode'].lstrip('0')
        key = entry['path'].encode() + (b'/' if entry['type'] == 'tree' else b'')
        encoded = mode.encode() + b' ' + entry['path'].encode() + b'\0' + bytes.fromhex(entry['sha'])
        entries.append((key, encoded))
    raw = b''.join(value for _, value in sorted(entries))
    actual = hashlib.sha1(b'tree ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
    expected = commit['tree']['sha']
    if actual != expected:
        raise SystemExit(f'{name} root tree mismatch: {actual} != {expected}')
    checks.append({'source': name, 'commit': commit['sha'], 'expected_tree': expected, 'root_api_reconstructed_tree': actual, 'entries': len(entries), 'matched': True})
(receipts / 'root-tree-identity.json').write_text(json.dumps({'schema_version': 1, 'checks': checks}, indent=2) + '\n', encoding='utf-8')
print(json.dumps(checks, indent=2))
