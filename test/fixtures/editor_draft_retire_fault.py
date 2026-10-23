"""One exact private draft UPDATE abort in an isolated native UI fixture."""
import argparse
import hashlib
import json
import sqlite3
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('action', choices=('arm', 'disarm'))
p.add_argument('root', type=Path)
p.add_argument('card')
p.add_argument('draft')
a = p.parse_args()
root = a.root.absolute()
if root.drive.lower() != 'd:' or not root.is_dir():
    raise ValueError('Use an existing isolated D-cache fixture root')
for node in [root, *root.parents]:
    if node.exists() and (node.is_symlink() or node.is_junction()):
        raise ValueError('Fixture root must not traverse a link')
db = root/'store/workbench.db'
if not db.is_file() or not db.resolve().is_relative_to(root.resolve()):
    raise ValueError('Original isolated store not found')
raw = a.card.encode()
key = 'morrow-host-editor-draft-'+hashlib.sha256(len(raw).to_bytes(8, 'little')+raw+a.draft.encode()).hexdigest()
name = 'qualification_original_parent_retire_abort'
sql = f"CREATE TRIGGER {name} BEFORE UPDATE ON cards WHEN OLD.id = '{key}' BEGIN SELECT RAISE(ABORT, 'original parent retirement fixture failure'); END"
with sqlite3.connect(db.as_uri()+'?mode=rw', uri=True) as conn:
    found = conn.execute('SELECT id FROM cards WHERE id=?', (key,)).fetchall()
    if found != [(key,)]: raise ValueError('Exact parent journal is missing')
    existing = conn.execute('SELECT sql FROM sqlite_master WHERE type=? AND name=?', ('trigger', name)).fetchall()
    if a.action == 'arm':
        if existing: raise ValueError('Do not reuse an existing injection')
        conn.execute(sql)
        if conn.execute('SELECT sql FROM sqlite_master WHERE name=?', (name,)).fetchone() != (sql,):
            raise ValueError('Original trigger definition changed')
    else:
        if existing != [(sql,)]: raise ValueError('Only the exact original trigger may be removed')
        conn.execute(f'DROP TRIGGER {name}')
    conn.commit()
proof = dict(action=a.action, database=str(db), card_id=a.card, draft_id=a.draft,
             original_journal_key=key, exact_trigger=sql, complete=True)
with (root/f'retirement-sql-{a.action}.json').open('x', encoding='utf-8') as output:
    json.dump(proof, output, indent=2); output.write('\n')
print(json.dumps(dict(action=a.action, complete=True)))
