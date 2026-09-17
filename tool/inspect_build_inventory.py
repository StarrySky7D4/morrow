"""Read-only engineering snapshot; emits derived Markdown, never a capability grant.

No builds, package refresh, account access, database opens, or test-result promotion.
Python 3.11+; only the standard library and the existing frozen-fixture verifier.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tomllib

import verify_plugin_sdk_baseline as baseline

ROOT = Path(__file__).resolve().parents[1]
CRATES = ('core', 'core-web', 'audit', 'plugin_runtime', 'workbench_host',
          'sdk/rust', 'plugins/workbench', 'network_node')
CONTRACTS = ('runtime.capnp', 'content.proto', 'task.capnp', 'ui.capnp', 'dependency_call.capnp')
PROTOCOLS = (
    ('runtime', 'core/src/runtime.rs', 'PROTOCOL_VERSION', 'version.txt'),
    ('task', 'core/src/task.rs', 'VERSION', 'task-version.txt'),
    ('ui', 'core/src/ui.rs', 'VERSION', 'ui-version.txt'),
    ('dependency-call', 'core/src/dependency_call.rs', 'VERSION', None),
)
REPORTS = ('reports/test.50-application-plugin-management.md',
           'reports/network-node-initial.md', 'docs/PLUGIN_SDK_COMPATIBILITY.md')

class InventoryError(ValueError):
    pass


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def one(pattern: str, text: str, label: str) -> str:
    matches = re.findall(pattern, text, re.MULTILINE)
    if len(matches) != 1:
        raise InventoryError(f'{label}: expected one source declaration, found {len(matches)}')
    return matches[0]


def constant(text: str, name: str, label: str) -> int:
    # Fail closed if declarations move to expressions or become ambiguous.
    value = int(one(r'^pub const ' + re.escape(name) + r': u16 = ([0-9]+);$', text, label))
    if value > 65535:
        raise InventoryError(label + ': u16 declaration out of range')
    return value


class Reader:
    def __init__(self, root: Path):
        self.root = root.resolve()
        self.files: dict[str, bytes] = {}

    def read(self, name: str) -> bytes:
        path = self.root / name
        if not path.resolve().is_relative_to(self.root):
            raise InventoryError('Source escapes project: ' + name)
        if name not in self.files:
            self.files[name] = path.read_bytes()
        return self.files[name]

    def text(self, name: str) -> str:
        return self.read(name).decode('utf-8').replace('\r\n', '\n')

    def recheck(self):
        for name, data in self.files.items():
            if (self.root / name).read_bytes() != data:
                raise InventoryError('Source changed during inspection: ' + name)


def collect(root: Path) -> dict:
    reader = Reader(root)
    pubspec = reader.text('pubspec.yaml')
    version = one(r'^version: ([0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?\+[0-9]+)\s*$', pubspec, 'application version')
    packages = []
    for folder in CRATES:
        path = folder + '/Cargo.toml'
        data = tomllib.loads(reader.text(path))
        package = data['package']
        if not isinstance(package['version'], str):
            raise InventoryError('Unresolved package version: ' + path)
        packages.append((package['name'], package['version'], path))
        lock = folder + '/Cargo.lock'
        if (root / lock).exists():
            reader.read(lock)
    protocols = []
    for name, path, symbol, sdk_file in PROTOCOLS:
        value = constant(reader.text(path), symbol, path)
        sdk_path = 'sdk/rust/contracts/' + sdk_file if sdk_file else 'sdk/rust/src/dependency_call.rs'
        sdk_value = int(reader.text(sdk_path).strip()) if sdk_file else constant(reader.text(sdk_path), 'VERSION', sdk_path)
        if value != sdk_value:
            raise InventoryError('Host/SDK version mismatch: ' + name)
        protocols.append((name, value, path, sdk_path))
    abi_path = 'core/src/plugin_package.rs'
    accepted_abis = one(r'!matches!\(manifest\.guest_abi_version, ([0-9 |]+)\)', reader.text(abi_path), 'guest ABI acceptance')
    accepted_abis = tuple(int(v.strip()) for v in accepted_abis.split('|'))
    if len(set(accepted_abis)) != len(accepted_abis):
        raise InventoryError('Duplicate guest ABI acceptance')
    store = reader.text('core/src/store.rs')
    ranges = re.findall(r'!matches!\(version, ([0-9]+)\.\.=([0-9]+)\)', store)
    if len(ranges) != 3 or len({end for _, end in ranges}) != 1 or ranges[1] != ranges[2]:
        raise InventoryError('Unsupported/ambiguous database source format')
    migrations = [int(v) for v in re.findall(r'pragma_update\(None, "user_version", ([0-9]+)\)', store)]
    database = int(ranges[1][1])
    if not migrations or max(migrations) != database:
        raise InventoryError('Database accepted version and migration target differ')
    mirrors = []
    for name in CONTRACTS:
        host, sdk = 'core/schemas/' + name, 'sdk/rust/contracts/' + name
        a, b = reader.text(host), reader.text(sdk)
        if a != b:
            raise InventoryError('Host/SDK schema mismatch: ' + name)
        mirrors.append((name, digest(a.encode('utf-8'))))
    schemas = []
    for folder in ('core/schemas', 'audit/schemas', 'workbench_host/schemas', 'plugins/workbench/schemas'):
        schema_paths = sorted(path for path in (root / folder).glob('*') if path.suffix in ('.proto', '.capnp'))
        if not schema_paths:
            raise InventoryError('Missing source schema directory: ' + folder)
        for path in schema_paths:
            if path.suffix in ('.proto', '.capnp'):
                name = path.relative_to(root).as_posix()
                raw = reader.read(name)
                schemas.append((name, digest(raw), digest(raw.replace(b'\r\n', b'\n'))))
    manifest = reader.text('core/schemas/plugin_package.proto')
    capability_block = one(r'enum Capability \{([^}]+)\}', manifest, 'package capability enum')
    capabilities = re.findall(r'^\s*([A-Z][A-Z0-9_]*)\s*=\s*([0-9]+);', capability_block, re.MULTILINE)
    if not capabilities:
        raise InventoryError('No package capability declarations')
    if len({name for name, _ in capabilities}) != len(capabilities) or len({int(value) for _, value in capabilities}) != len(capabilities):
        raise InventoryError('Duplicate package capability name or value')
    pin_path = 'sdk/compat/guest-v1-rc1.sha256'
    pin = reader.text(pin_path).strip()
    folder = root / 'sdk/compat/guest-v1-rc1'
    count = baseline.verify(folder, pin)
    provenance = reader.text('sdk/compat/guest-v1-rc1/provenance.txt')
    frozen_abi = int(one(r'^guest_abi=([0-9]+)$', provenance, 'frozen guest ABI'))
    # Integrity is checked independently of compatibility with current source.
    # A changed current ABI is not falsely reported as having executed old guests.
    reports = []
    for name in REPORTS:
        if (root / name).exists():
            reports.append((name, digest(reader.read(name))))
    reader.read('android/app/build.gradle.kts')
    reader.read('lib/plugins/bootstrap_native.dart')
    reader.recheck()
    return dict(reader=reader, version=version, packages=packages, protocols=protocols,
                database=database, database_ranges=ranges, accepted_abis=accepted_abis,
                mirrors=mirrors, schemas=schemas, capabilities=capabilities,
                pin=pin, frozen_count=count, frozen_abi=frozen_abi, reports=reports)


def run_version(command: list[str], cwd: Path) -> str:
    try:
        result = subprocess.run(command, cwd=cwd, capture_output=True, text=True,
                                encoding='utf-8', errors='replace', timeout=15, check=False)
        if result.returncode:
            return f'UNAVAILABLE (exit {result.returncode})'
        return result.stdout.strip() or 'UNAVAILABLE (empty response)'
    except (OSError, subprocess.TimeoutExpired):
        return 'UNAVAILABLE (not installed or timed out)'


def git_state(root: Path) -> tuple[str, str]:
    head = run_version(['git', 'rev-parse', 'HEAD'], root)
    result = subprocess.run(['git', 'status', '--porcelain=v1', '--untracked-files=all'],
                            cwd=root, capture_output=True, text=True, encoding='utf-8',
                            errors='replace', timeout=15, check=False)
    if result.returncode or not re.fullmatch('[0-9a-f]{40,64}', head):
        raise InventoryError('Cannot establish repository identity/state')
    return head, result.stdout.rstrip('\r\n')


def artifact(path: Path) -> tuple[str, int, str]:
    with path.open('rb') as stream:
        before = os.fstat(stream.fileno())
        initial_path = path.stat()
        checksum = hashlib.sha256()
        size = 0
        while chunk := stream.read(1024 * 1024):
            checksum.update(chunk)
            size += len(chunk)
        after = os.fstat(stream.fileno())
        final_path = path.stat()
    identities = {(value.st_size, value.st_mtime_ns, value.st_dev, value.st_ino)
                  for value in (before, initial_path, after, final_path)}
    if len(identities) != 1 or size != before.st_size:
        raise InventoryError('Artifact changed during inspection: ' + str(path))
    return str(path), size, checksum.hexdigest()


def flutter_facts(root: Path, sdk: Path | None) -> list[tuple[str, str]]:
    if sdk is None:
        return [('Flutter / Dart / Android defaults', 'UNRESOLVED: pass --flutter-sdk; no SDK command was run')]
    metadata_raw = (sdk / 'bin/cache/flutter.version.json').read_bytes()
    extension_raw = (sdk / 'packages/flutter_tools/gradle/src/main/kotlin/FlutterExtension.kt').read_bytes()
    data = json.loads(metadata_raw)
    extension = extension_raw.decode('utf-8').replace('\r\n', '\n')
    gradle = (root / 'android/app/build.gradle.kts').read_text(encoding='utf-8')
    facts = [('Flutter cached version (not a build)', str(data['frameworkVersion'])),
             ('Flutter revision', str(data['frameworkRevision'])),
             ('Dart cached version', str(data['dartSdkVersion'])),
             ('Flutter metadata SHA-256', digest(metadata_raw)),
             ('Flutter Android defaults SHA-256', digest(extension_raw))]
    for key in ('minSdk', 'targetSdk', 'compileSdk', 'ndkVersion'):
        expression = one(r'^\s*' + key + r' = (.+)$', gradle, 'Android ' + key).strip()
        expected = 'flutter.' + (key if key == 'ndkVersion' else key + 'Version')
        if expression == expected:
            symbol = expected.split('.')[1]
            declaration = one(r'^\s*val ' + symbol + r': ((?:Int|String) = [^\r\n]+)$', extension, symbol).strip()
            pattern = r'String = "([0-9.]+)"' if key == 'ndkVersion' else r'Int = ([0-9]+)'
            literal = re.fullmatch(pattern, declaration)
            if literal:
                facts.append(('Android source default ' + key, literal[1] + ' (not merged APK metadata)'))
            else:
                facts.append(('Android ' + key, 'UNRESOLVED SDK expression: ' + declaration))
        elif re.fullmatch(r'[0-9]+|"[0-9.]+"', expression):
            facts.append(('Android source literal ' + key, expression))
        else:
            facts.append(('Android ' + key, 'UNRESOLVED expression: ' + expression))
    return facts


def cell(value) -> str:
    return str(value).replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;').replace('|', '&#124;').replace('`', '&#96;').replace('[', '&#91;').replace(']', '&#93;').replace('\r', '').replace('\n', '<br>')


def table(headers, rows) -> str:
    return '\n'.join(['| ' + ' | '.join(map(cell, headers)) + ' |',
                      '| ' + ' | '.join('---' for _ in headers) + ' |'] +
                     ['| ' + ' | '.join(map(cell, row)) + ' |' for row in rows])


def render(data: dict, git: tuple[str, str], artifacts: list, tools: list) -> str:
    reader = data['reader']
    sources = [(name, len(raw), digest(raw)) for name, raw in sorted(reader.files.items())]
    chunks = ['# Morrow engineering inventory', '',
        'Generated UTC: ' + datetime.now(timezone.utc).isoformat(),
        'Source HEAD: `' + git[0] + '`',
        'Working tree: ' + ('DIRTY; HEAD alone does not identify these source bytes.' if git[1] else 'clean at inspection.'),
        '', 'This is derived engineering output, not a runtime manifest, permission grant, signed receipt or release qualification. '
        'No build, guest execution, device test or channel check is performed. Historical reports remain historical. '
        'Hashes below identify inspected bytes; they do not bind an old artifact to current source.',
        '', '## Versions and source contracts', '',
        table(('Item', 'Value', 'Source'), [('application', data['version'], 'pubspec.yaml')] + data['packages'] +
              [(name, value, path + ' = ' + sdk) for name, value, path, sdk in data['protocols']] +
              [('accepted guest ABI (source predicate)', ', '.join(map(str, data['accepted_abis'])), 'core/src/plugin_package.rs'),
               ('frozen guest ABI (provenance)', data['frozen_abi'], 'sdk/compat/guest-v1-rc1/provenance.txt'),
               ('database migration target (source; no user database read)', data['database'], 'core/src/store.rs')]),
        '', '## Current schema digests', '',
        'LF SHA-256 normalizes CRLF only, matching existing contract hashing. Raw SHA-256 identifies file bytes. '
        'All five current host/SDK mirrors and three version snapshots were compared; this is not old-binary compatibility execution.', '',
        table(('Source schema', 'Raw SHA-256', 'LF SHA-256'), data['schemas']),
        '', '## Frozen baseline integrity', '',
        f'PASS_SCOPED: {data["frozen_count"]} pinned files checked by the existing verifier; root `{data["pin"]}`. '
        'Original guests were neither rebuilt nor executed. Integrity does not imply compatibility or publisher trust.',
        '', '## Declared capabilities and qualification', '',
        table(('Package enum name', 'Value', 'Meaning of this observation'),
              [(name, value, 'source declaration only; not a grant or execution result') for name, value in data['capabilities']]),
        '', table(('Platform', 'Compile now', 'Application integration now', 'Device/recovery now', 'Channel now'),
                  [(name, 'NOT_RUN', 'NOT_RUN', 'NOT_RUN', 'NOT_RUN') for name in ('Windows', 'macOS', 'Linux', 'Android', 'iOS/iPadOS', 'Web/PWA')]),
        '', '## Inspected artifacts', '',
        table(('Path', 'Bytes', 'SHA-256', 'Source/build binding'), [(*row, 'UNVERIFIED; not promoted to a current build') for row in artifacts]) if artifacts else 'No artifact paths supplied.',
        '', '## Available toolchain observations', '', table(('Tool/setting', 'Observation'), tools),
        '', '## Historical evidence index', '',
        table(('Report', 'Current file SHA-256', 'Status'), [(name, sha, 'HISTORICAL_REFERENCE; not rerun or automatically endorsed') for name, sha in data['reports']]),
        '', '## Source observation fingerprints', '', table(('Path', 'Bytes', 'Raw SHA-256'), sources),
        '', '## Working-tree changes at inspection', '', table(('Git porcelain status',), [(line,) for line in git[1].splitlines()]) if git[1] else 'No changes reported.', '']
    return '\n'.join(chunks)


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--flutter-sdk', type=Path, help='Read SDK metadata only; does not run Flutter/Gradle')
    parser.add_argument('--artifact', type=Path, action='append', default=[], help='Hash explicitly selected bytes; no inferred source association')
    parser.add_argument('--output', type=Path, help='Create a new derived Markdown report; refuses overwriting existing files')
    args = parser.parse_args(argv)
    try:
        root = args.root.resolve()
        initial = git_state(root)
        data = collect(root)
        artifacts = [artifact(path) for path in args.artifact]
        tools = [('Inventory tool SHA-256', digest(Path(__file__).read_bytes())),
                 ('Python', platform.python_version()), ('Inspection host', platform.system() + '/' + platform.machine())]
        for command in (['rustc', '-Vv'], ['cargo', '-V'], ['capnp', '--version']):
            tools.append((command[0], run_version(command, root)))
        tools.extend(flutter_facts(root, args.flutter_sdk))
        data['reader'].recheck()
        baseline.verify(root / 'sdk/compat/guest-v1-rc1', data['pin'])
        if git_state(root) != initial:
            raise InventoryError('Git identity/state changed during inspection; retry after other edits stop')
        report = render(data, initial, artifacts, tools)
        if args.output:
            with args.output.open('x', encoding='utf-8', newline='\n') as stream:
                stream.write(report)
            print('Created derived inventory: ' + str(args.output))
        else:
            print(report)
        return 0
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print('INVENTORY FAILED: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
