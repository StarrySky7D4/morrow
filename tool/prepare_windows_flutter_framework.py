"""Prepare a pinned, isolated Windows Flutter framework candidate.

Example (all outputs stay on D:):
  python -B tool/prepare_windows_flutter_framework.py --flutter-sdk C:/flutter \
    --package-config D:/build_cache/project/.dart_tool/package_config.json \
    --output D:/build_cache/flutter-framework/run-001

Use the returned .dart_tool/package_config.json for compilation or Flutter tests.
The original adjacent package_graph.json and parent pubspec.yaml are copied into
the same relative layout, which Flutter's native asset hooks require.
The global SDK and the input package configuration are never modified. Every
invocation requires a new output directory; a failed preparation is retained.
This tool performs no product execution or acceptance/release decision.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
from urllib.parse import urljoin, urlparse

from windows_qualification_build_cache import CACHE_ROOT, _plain_path, pin

PATCH_ROOT = Path(__file__).with_name('flutter_windows_semantics')
TARGET = 'lib/src/semantics/semantics.dart'


def strict_patch(original: bytes, patch: bytes) -> bytes:
    """Apply the one pinned text patch without fuzzy matching or path handling."""
    source = original.decode('utf-8').replace('\r\n', '\n').splitlines(keepends=True)
    changes = patch.decode('utf-8').splitlines(keepends=True)
    if changes[:2] != [f'--- a/{TARGET}\n', f'+++ b/{TARGET}\n']:
        raise ValueError('Patch must address the single pinned semantics file')
    result: list[str] = []
    consumed = 0
    i = 2
    hunks = 0
    while i < len(changes):
        match = re.fullmatch(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@[^\n]*\n', changes[i])
        if match is None:
            raise ValueError('Malformed or extra-file patch header')
        old_start, old_count, new_start, new_count = (
            int(match[1]), int(match[2] or 1), int(match[3]), int(match[4] or 1)
        )
        old_index = old_start - bool(old_count)
        new_index = new_start - bool(new_count)
        if not consumed <= old_index <= len(source):
            raise ValueError('Patch hunks overlap or exceed the original source')
        result.extend(source[consumed:old_index])
        if len(result) != new_index:
            raise ValueError('Patch output position does not match')
        consumed = old_index
        seen_old = seen_new = 0
        i += 1
        while i < len(changes) and not changes[i].startswith('@@ '):
            line = changes[i]
            prefix = line[:1]
            if prefix not in (' ', '-', '+'):
                raise ValueError('Unexpected patch record')
            if prefix in (' ', '-'):
                if consumed >= len(source) or source[consumed] != line[1:]:
                    raise ValueError('Exact patch context does not match')
                consumed += 1
                seen_old += 1
            if prefix in (' ', '+'):
                result.append(line[1:])
                seen_new += 1
            i += 1
        if (seen_old, seen_new) != (old_count, new_count):
            raise ValueError('Patch hunk line counts do not match')
        hunks += 1
    if not hunks:
        raise ValueError('Empty patch')
    result.extend(source[consumed:])
    return ''.join(result).encode('utf-8')


def _safe_relative(raw: str) -> Path:
    path = PurePosixPath(raw)
    if path.is_absolute() or not path.parts or any(x in ('.', '..') for x in path.parts) or ':' in raw or '\\' in raw:
        raise ValueError('Invalid framework-relative file name')
    return Path(*path.parts)


def _regular_source(root: Path, relative: str) -> Path:
    target = root / _safe_relative(relative)
    for path in (target, *target.parents):
        if path.is_symlink() or path.is_junction():
            raise ValueError('Framework source must not contain reparse paths')
        if path == root:
            break
    if not target.is_file():
        raise ValueError('Missing framework source: ' + relative)
    return target


def relocate_config(value: dict, config_path: Path, framework: Path) -> dict:
    if value.get('configVersion') != 2:
        raise ValueError('Expected a version 2 Dart package configuration')
    result = copy.deepcopy(value)
    entries = result.get('packages', [])
    names = [p['name'] for p in entries]
    if names.count('flutter') != 1 or len(names) != len(set(names)):
        raise ValueError('Expected unique packages and one Flutter package')
    for package in entries:
        uri = urljoin(config_path.as_uri(), package['rootUri'])
        if urlparse(uri).scheme != 'file':
            raise ValueError('Only local package roots are supported')
        package['rootUri'] = framework.as_uri() + '/' if package['name'] == 'flutter' else uri
    return result


def validate_graph(value: dict, config: dict) -> None:
    """Keep Flutter's dependency graph paired with the same package set."""
    if not isinstance(value, dict) or value.get('configVersion') != 1:
        raise ValueError('Expected a version 1 Dart package graph')
    packages = value.get('packages')
    if not isinstance(packages, list) or any(not isinstance(p, dict) for p in packages):
        raise ValueError('Expected package graph records')
    names = [p.get('name') for p in packages]
    if any(not isinstance(n, str) or not n for n in names) or len(names) != len(set(names)):
        raise ValueError('Expected unique graph package names')
    if set(names) != {p['name'] for p in config['packages']}:
        raise ValueError('Package graph and configuration have different packages')
    roots = value.get('roots')
    if not isinstance(roots, list) or not roots or any(not isinstance(n, str) or n not in names for n in roots):
        raise ValueError('Package graph has invalid roots')
    for package in packages:
        for field in ('dependencies', 'devDependencies'):
            edges = package.get(field, [])
            if not isinstance(edges, list) or any(not isinstance(n, str) or n not in names for n in edges):
                raise ValueError('Package graph has an unresolved dependency')


def prepare(sdk: Path, config: Path, output: Path) -> dict:
    if os.environ.get('MORROW_DISPOSABLE_WINDOWS_TEST'):
        raise ValueError('Do not mix build preparation with disposable VM runtime authority')
    sdk, config = sdk.absolute(), config.absolute()
    output = _plain_path(output, CACHE_ROOT)
    if output.exists():
        raise FileExistsError('Each framework preparation needs a new output directory')
    manifest_path = PATCH_ROOT / 'manifest.json'
    manifest = json.loads(manifest_path.read_bytes())
    if manifest.get('format') != 1 or manifest['target_file'] != TARGET or manifest['patch_file'] != 'semantics.patch':
        raise ValueError('Unsupported framework pin manifest')
    version = json.loads((sdk / 'bin/cache/flutter.version.json').read_bytes())
    expected = {
        'frameworkRevision': manifest['framework_revision'],
        'engineRevision': manifest['engine_revision'],
        'frameworkVersion': manifest['framework_version'],
        'dartSdkVersion': manifest['dart_sdk_version'],
    }
    if any(version.get(k) != v for k, v in expected.items()):
        raise ValueError('Flutter SDK revision differs from the reviewed candidate')
    engine = pin(sdk / 'bin/cache/artifacts/engine/windows-x64/flutter_windows.dll')
    if engine['sha256'] != manifest['windows_debug_engine_sha256']:
        raise ValueError('Native Flutter engine differs from the reviewed candidate')
    patch_path = PATCH_ROOT / manifest['patch_file']
    patch = patch_path.read_bytes()
    if hashlib.sha256(patch).hexdigest() != manifest['patch_sha256']:
        raise ValueError('Framework patch bytes changed')
    source = sdk / 'packages/flutter'
    rows = {name: _regular_source(source, name) for name in manifest['input_files']}
    for name, path in rows.items():
        if pin(path)['sha256'] != manifest['input_files'][name]:
            raise ValueError('Framework input changed: ' + name)
    changed = strict_patch(rows[TARGET].read_bytes(), patch)
    if hashlib.sha256(changed).hexdigest() != manifest['output_sha256']:
        raise ValueError('Patched framework differs from the reviewed output')
    config_before = pin(config)
    config_value = json.loads(config.read_bytes())
    packages = relocate_config(config_value, config, output / 'framework')
    graph = config.with_name('package_graph.json')
    graph_before = pin(graph)
    graph_bytes = graph.read_bytes()
    validate_graph(json.loads(graph_bytes), config_value)
    if hashlib.sha256(graph_bytes).hexdigest() != graph_before['sha256']:
        raise ValueError('Package graph changed during validation')
    project_manifest = config.parent.parent / 'pubspec.yaml'
    manifest_before = pin(project_manifest)
    manifest_bytes = project_manifest.read_bytes()
    if hashlib.sha256(manifest_bytes).hexdigest() != manifest_before['sha256']:
        raise ValueError('Project manifest changed during validation')
    # Validate everything before reserving a new directory. Never overwrite,
    # delete, resume, or repair an earlier preparation automatically.
    output.parent.mkdir(parents=True, exist_ok=True)
    _plain_path(output, CACHE_ROOT)
    output.mkdir(exist_ok=False)
    for name, path in rows.items():
        target = output / 'framework' / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
        if pin(target)['sha256'] != manifest['input_files'][name]:
            raise ValueError('Framework changed during copy: ' + name)
    (output / 'framework' / TARGET).write_bytes(changed)
    # The native-assets test runner resolves ../pubspec.yaml from this URI.
    # Preserve that layout without changing any resolved project package roots.
    (output / '.dart_tool').mkdir()
    config_out = output / '.dart_tool/package_config.json'
    config_out.write_text(json.dumps(packages, indent=2) + '\n', encoding='utf-8')
    graph_out = config_out.with_name('package_graph.json')
    graph_out.write_bytes(graph_bytes)
    manifest_out = output / 'pubspec.yaml'
    manifest_out.write_bytes(manifest_bytes)
    for name, path in rows.items():
        if pin(path)['sha256'] != manifest['input_files'][name]:
            raise ValueError('Global framework changed during preparation: ' + name)
    if pin(config) != config_before:
        raise ValueError('Input package configuration changed during preparation')
    if pin(graph) != graph_before or pin(graph_out)['sha256'] != graph_before['sha256']:
        raise ValueError('Input package graph changed during preparation')
    if pin(project_manifest) != manifest_before or pin(manifest_out)['sha256'] != manifest_before['sha256']:
        raise ValueError('Input project manifest changed during preparation')
    result = {
        'status': 'PINNED_FRAMEWORK_PREPARED_NOT_ACCEPTANCE',
        'manifest': pin(manifest_path), 'patch': pin(patch_path),
        'input_package_config': config_before, 'package_config': pin(config_out),
        'input_package_graph': graph_before, 'package_graph': pin(graph_out),
        'input_project_manifest': manifest_before, 'project_manifest': pin(manifest_out),
        'framework': str(output / 'framework'), 'engine': engine,
        'files': {name: pin(output / 'framework' / name) for name in rows},
        'global_sdk_modified': False, 'input_config_modified': False,
        'product_executed': False, 'full_sdk_accepted': False,
        'source_accepted': False, 'release_eligible': False,
    }
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--flutter-sdk', type=Path, required=True)
    parser.add_argument('--package-config', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = prepare(args.flutter_sdk, args.package_config, args.output)
    print(json.dumps({k: result[k] for k in ('status', 'framework', 'package_config')}))
