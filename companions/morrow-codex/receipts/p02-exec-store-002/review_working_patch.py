"""Bind a minimal source delta to the complete frozen upstream manifest."""
from pathlib import Path
import difflib
import hashlib
import json
import os
import sys

ROOT = Path(__file__).resolve().parents[2]
OLD = ROOT / 'upstream/p02-source-batch-001/codex-source'
NEW = ROOT / 'upstream/p02-exec-store-002/codex-work'
RECEIPTS = Path(__file__).resolve().parent
ALLOWED = {'codex-rs/core/Cargo.toml', 'codex-rs/core/src/lib.rs', 'codex-rs/core/src/morrow_p02_qualification.rs', 'codex-rs/core/src/unified_exec/process_manager.rs', 'codex-rs/exec-server/src/environment.rs'}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    name = sys.argv[1]
    if not name.replace('-', '').isalnum():
        raise RuntimeError('Invalid fresh receipt name')
    manifest = ROOT / 'receipts/p02-source-batch-001/codex-content-manifest.json'
    entries = json.loads(manifest.read_text(encoding='utf-8'))['files']
    records = []
    diffs = []
    known = set()
    for row in entries:
        relative = row['path']
        known.add(relative)
        original = os.readlink(OLD / relative).encode() if row['mode']=='120000' else (OLD / relative).read_bytes()
        current = os.readlink(NEW / relative).encode() if row['mode']=='120000' else (NEW / relative).read_bytes()
        if sha(original) != row['sha256']:
            raise RuntimeError('Frozen original changed: ' + relative)
        if current != original:
            if relative not in ALLOWED:
                raise RuntimeError('Unexpected source delta: ' + relative)
            records.append({'path':relative,'before_sha256':sha(original),'after_sha256':sha(current)})
            diffs.extend(difflib.unified_diff(original.decode().splitlines(keepends=True), current.decode().splitlines(keepends=True), fromfile='a/'+relative, tofile='b/'+relative))
    actual = {p.relative_to(NEW).as_posix() for p in NEW.rglob('*') if p.is_file() or p.is_symlink()}
    if known-actual:
        raise RuntimeError('Upstream file removed')
    for relative in sorted(actual-known):
        if relative not in ALLOWED:
            raise RuntimeError('Unexpected new source: '+relative)
        data = (NEW/relative).read_bytes()
        records.append({'path':relative,'before_sha256':None,'after_sha256':sha(data)})
        diffs.extend(difflib.unified_diff([], data.decode().splitlines(keepends=True), fromfile='/dev/null', tofile='b/'+relative))
    if {r['path'] for r in records} != ALLOWED:
        raise RuntimeError('Expected exactly five changed/added files')
    before = json.loads((RECEIPTS/'working-copy-before.json').read_text())
    for base, files in before['other_copies'].items():
        for relative, expected in files.items():
            if sha((ROOT/base/relative).read_bytes()) != expected:
                raise RuntimeError('Frozen host or fork copy changed')
    patch = ''.join(diffs).encode()
    patch_path = RECEIPTS/(name+'.patch')
    with patch_path.open('xb') as output:
        output.write(patch)
    report = {'status':'minimal_patch_verified','fixed_upstream_files':len(entries),'fixed_upstream_unchanged':True,'working_files':len(actual),'source_manifest_sha256':sha(manifest.read_bytes()),'host_and_fork_copies_unchanged':True,'changes':records,'patch_sha256':sha(patch),'patch_lines':len(patch.splitlines()),'qualification_only':True,'Bazel_validation':'not_run; qualification-only Cargo feature and bridge are not a Bazel/product delivery'}
    with (RECEIPTS/(name+'.json')).open('x') as output:
        json.dump(report,output,indent=2)
    print(json.dumps(report))


if __name__=='__main__':
    main()
