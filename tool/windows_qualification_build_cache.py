"""Cold, single-use Cargo outputs for Windows qualification variants.

Source hashes and Cargo manifest paths alone cannot prove which library bytes
were linked. In particular, copying an overlay with its original timestamps
can reuse a library from an earlier variant in a shared target directory.
Keep the download cache, but never share compiled outputs between these runs.
This module performs no product execution and grants no runtime authority.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re

CACHE_ROOT = Path('D:/build_cache')


def pin(path: Path) -> dict:
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return {'path': str(path), 'bytes': path.stat().st_size,
            'sha256': digest.hexdigest()}


def _plain_path(path: Path, root: Path) -> Path:
    """Reject alternate drives, traversal and reparse redirections."""
    if not path.is_absolute() or '..' in path.parts:
        raise ValueError('An absolute, traversal-free cache path is required')
    absolute = Path(os.path.abspath(path))
    if not absolute.is_relative_to(root) or absolute == root:
        raise ValueError('Qualification outputs must stay below D:/build_cache')
    for candidate in (absolute, *absolute.parents):
        if candidate.is_symlink() or candidate.is_junction():
            raise ValueError('Reparse paths are not qualification cache roots')
        if candidate == root:
            break
    if absolute.resolve() != absolute:
        raise ValueError('Cache path resolves outside its literal location')
    return absolute


def reserve_target(run: str, role: str, input_sha256: str) -> Path:
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,55}', run):
        raise ValueError('Invalid build run identifier')
    if not re.fullmatch(r'[A-Z][A-Z0-9-]{0,15}', role):
        raise ValueError('Invalid role')
    if not re.fullmatch(r'[0-9a-f]{64}', input_sha256):
        raise ValueError('A complete input digest is required')
    parent = _plain_path(CACHE_ROOT / 'qualification-targets', CACHE_ROOT)
    parent.mkdir(exist_ok=True)
    target = _plain_path(parent / f'{run}-{role}-{input_sha256[:16]}', CACHE_ROOT)
    # Atomic reservation: an earlier success, failure or interrupted build is
    # never resumed under this identity. Do not clean/retry it automatically.
    target.mkdir(exist_ok=False)
    if any(target.iterdir()):
        raise ValueError('The new compiler output directory must be empty')
    return target


def environment(temporary: Path) -> dict[str, str]:
    temporary = _plain_path(temporary, CACHE_ROOT)
    temporary.mkdir(exist_ok=False)
    result = dict(os.environ)
    result.update(CARGO_HOME=str(CACHE_ROOT / 'cargo'), CARGO_NET_OFFLINE='true',
                  CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='1',
                  RUSTC_WRAPPER='', RUSTC_WORKSPACE_WRAPPER='',
                  TEMP=str(temporary), TMP=str(temporary), TMPDIR=str(temporary))
    return result


def verify_compiler_outputs(records: list[dict], target: Path,
                            manifest: Path, binary: str,
                            required_libraries: tuple[str, ...] = ()) -> dict:
    """Require actual compilation, then pin every reported compiler output."""
    target = _plain_path(target, CACHE_ROOT)
    finished = [r for r in records if r.get('reason') == 'build-finished']
    if len(finished) != 1 or finished[0].get('success') is not True:
        raise ValueError('Missing unique successful original compiler completion')
    artifacts = [r for r in records if r.get('reason') == 'compiler-artifact']
    if not artifacts or any(r.get('fresh') is not False for r in artifacts):
        raise ValueError('A cold qualification build must compile every artifact')
    selected = [r for r in artifacts if r.get('executable')
                and r['target']['name'] == binary
                and Path(r['manifest_path']).resolve() == manifest.resolve()]
    if len(selected) != 1 or selected[0]['profile']['test']:
        raise ValueError('Missing unique expected non-test binary')
    for library in required_libraries:
        matches = [r for r in artifacts if r['target']['name'] == library
                   and 'lib' in r['target']['kind']
                   and Path(r['manifest_path']).resolve() == manifest.resolve()]
        if len(matches) != 1:
            raise ValueError('Expected library was not freshly compiled: ' + library)
    outputs = {}
    for artifact in artifacts:
        for raw in [*artifact['filenames'], *([artifact['executable']]
                                            if artifact.get('executable') else [])]:
            path = _plain_path(Path(raw), target)
            if not path.is_file():
                raise ValueError('Compiler output is missing: ' + raw)
            outputs[str(path)] = pin(path)
    return {'target_dir': str(target), 'compiler_artifacts': len(artifacts),
            'all_artifacts_freshly_compiled': True,
            'required_libraries': list(required_libraries),
            'selected': selected[0], 'outputs': outputs,
            'runtime': 'NOT_RUN', 'source_accepted': False,
            'release_eligible': False}


def verify_markers(path: Path, *, required: tuple[bytes, ...] = (),
                   forbidden: tuple[bytes, ...] = ()) -> dict:
    raw = path.read_bytes()
    for marker in required:
        if marker not in raw:
            raise ValueError('Expected qualification marker absent: ' + marker.decode())
    for marker in forbidden:
        if marker in raw:
            raise ValueError('Unexpected qualification marker: ' + marker.decode())
    return {'artifact': pin(path),
            'required': [m.decode() for m in required],
            'forbidden': [m.decode() for m in forbidden],
            'passed': True, 'semantic_acceptance': False}


def write_receipt(path: Path, value: dict) -> None:
    with path.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2)
