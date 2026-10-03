"""Bounded SDK source distribution; integrity inventory, not a signature or SDK freeze.

Python 3.11+. No compiler, host, network or guest is invoked by this module.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import stat
import tempfile
import unicodedata
import zipfile
import plugin_sdk_lock as sdk_lock

MANIFEST = 'SDK_DISTRIBUTION_MANIFEST.json'
PROFILE = 'morrow-sdk-source-distribution-v1'
MAX_FILES = 4096
MAX_FILE_BYTES = 8 * 1024 * 1024
MAX_TOTAL_BYTES = 64 * 1024 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_ARCHIVE_BYTES = MAX_TOTAL_BYTES + 4 * 1024 * 1024
LANGUAGES = ('c', 'cpp', 'rust')
STARTERS = ('task', 'transform', 'ui', 'dependency-caller', 'io', 'service', 'service-http', 'channel', 'channel-directory')
SDK_DOCS = ('README.md', 'CONTENT_API.md', 'IO_API.md', 'CHANNEL_API.md', 'SERVICE_API.md')
TOOLS = ('morrow_plugin.py', 'plugin_sdk_lock.py', 'package_plugin_sdk.py')
CONTRACTS = ('runtime.capnp', 'content.proto', 'task.capnp', 'ui.capnp', 'dependency_call.capnp', 'io.capnp', 'channel.capnp', 'mutation.capnp', 'service.capnp', 'service_resources.capnp', 'version.txt', 'task-version.txt', 'ui-version.txt')
README = b'''# Morrow SDK source distribution\n\nThis source-only developer bundle supports C, C++ and Rust guests and declarative UI starters. It contains the original SDK library sources, fixed contracts, licenses, existing starters and project/lock tooling. No native DLL, precompiled guest, SDK tests, Core, runtime or host source is bundled.\n\nPython 3.11+ is required. Use `python -B tool/package_plugin_sdk.py verify-directory .` before using this directory. The bounded manifest covers source, tools, examples and documentation. It verifies local byte identity; it is not a supply-chain signature, a new contract authority, SDK freeze or platform/product qualification. Existing sdk.lock.toml retains morrow-sdk-source-v1 and its original limits.\n\n`python -B tool/morrow_plugin.py new ../plugin --sdk-only --language c --kind transform --id example.transform --lock-sdk`\n`python -B tool/morrow_plugin.py validate ../plugin --sdk-only --require-sdk-lock`\n\nPass --sdk-root PATH/sdk if the SDK is not beside this tool. Generated projects belong outside the bundle so the exact inventory remains valid. When relocating a Rust project, update its Cargo dependency path and pass the same SDK path. new/validate/lock-sdk in SDK-only mode use the bundle inventory and narrow internal source binding observations; they do not compare against or qualify a host. Source export requires SDK/Core canonical schemas and declared versions to agree before export. Internal binding observations do not prove compiler expansion, schema/manual decoder semantics, or behavior of self-rehashed malicious source. The default mode still requires repository Core contracts.\n\nbuild and doctor additionally require a trusted local compiler/toolchain; they are not source-only checks. Rust/Cargo may need an already provisioned offline dependency cache; C/C++ additionally need Clang and a WASI sysroot. They have not been run as part of distribution validation. pack/check/transform require trusted Core/runtime tools and are rejected in SDK-only mode before subprocesses or output. profiles is not included.\n\nTemplates declare ceilings, never grants. UI requires a host renderer/session validator. Content, IO, service, channel and dependencies require host-controlled bindings, grants and lifecycle. This bundle does not complete filesystem/network/plugin lifecycle capabilities or test them. Existing sdk/README.md and API documents describe repository workflows and their qualification limits; historical repository links may require the repository.\n'''

class DistributionError(ValueError):
    pass

def sha(data):
    return hashlib.sha256(data).hexdigest()

def regular(path, directory=False):
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
        raise DistributionError('links/reparse points are not distribution inputs: ' + str(path))
    if not (stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)):
        raise DistributionError('expected ordinary ' + ('directory: ' if directory else 'file: ') + str(path))
    return info

def safe_name(name):
    if not isinstance(name, str) or not name or len(name) > 512 or name != unicodedata.normalize('NFC', name):
        raise DistributionError('invalid distribution path')
    if any(ord(c) < 32 or ord(c) == 127 for c in name) or '\\' in name or ':' in name:
        raise DistributionError('unsafe distribution path: ' + repr(name))
    parts = name.split('/')
    if any(p in ('', '.', '..') or p.rstrip(' .') != p or len(p) > 255 for p in parts):
        raise DistributionError('unsafe distribution path: ' + repr(name))
    if PurePosixPath(name).is_absolute() or PureWindowsPath(name).drive:
        raise DistributionError('absolute distribution path')
    for p in parts:
        if re.fullmatch(r'(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?', p, re.I):
            raise DistributionError('reserved distribution path')
    return name

def checked_file(root, name, limit=None):
    safe_name(name)
    limit = MAX_FILE_BYTES if limit is None else min(MAX_FILE_BYTES, limit)
    regular(root, True)
    path = root
    parts = name.split('/')
    for p in parts[:-1]:
        path /= p
        regular(path, True)
    path /= parts[-1]
    if regular(path).st_size > limit:
        raise DistributionError('file size limit: ' + name)
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise DistributionError('file changed or exceeds size limit: ' + name)
    return data


PUBLIC_HEADERS = ('morrow_plugin_sdk', 'morrow_plugin_codec', 'morrow_plugin_task', 'morrow_plugin_ui', 'morrow_plugin_dependency', 'morrow_plugin_io', 'morrow_plugin_service', 'morrow_plugin_mutation', 'morrow_channel_v1')
SOURCE_ENTRIES = ('rust/src/lib.rs', 'rust/src/protocol.rs', 'rust/src/task.rs', 'rust/src/ui.rs', 'rust/src/dependency_call.rs', 'rust/src/io.rs', 'rust/src/service.rs', 'rust/src/service_resources.rs', 'rust/src/mutation.rs', 'rust/src/channel.rs', 'rust/src/descriptor_prefix.rs', 'rust/src/ffi.rs', 'rust/src/io_ffi.rs', 'rust/src/service_ffi.rs', 'rust/src/mutation_ffi.rs', 'rust/src/channel_ffi.rs', 'rust/src/wasm.rs', 'rust/src/wasm_alloc.rs', 'c/include/morrow_plugin_wasm.h', 'c/src/morrow_plugin_sdk.c', 'c/src/morrow_plugin_task.c', 'c/src/morrow_channel_v1.c', 'c/src/morrow_plugin_wasm.c', 'c/src/morrow_plugin_wasm_libc.c', 'cpp/src/morrow_plugin_wasm_runtime.cpp')


def public_inputs():
    # File closure only; versions are read from original source, never a second configuration.
    return {'sdk/' + name for name in SOURCE_ENTRIES} | {'sdk/' + lang + '/include/' + stem + suffix for stem in PUBLIC_HEADERS for lang, suffix in (('c', '.h'), ('cpp', '.hpp'))}


def source_text(root, name):
    return checked_file(root, name).decode('utf-8').replace('\r\n', '\n')


def rust_code(text):
    # Reuse the project's existing bounded comment/literal masking observation.
    import morrow_plugin
    return morrow_plugin.source_code(text, 'SDK distribution source binding')


def exact_constant(text, name, bits):
    import morrow_plugin
    code = rust_code(text)
    pattern = r'^pub const ' + name + r': u' + str(bits) + r' = ([0-9]+);$'
    matches = list(re.finditer(pattern, code, re.MULTILINE))
    if len(re.findall(r'\bconst\s+' + name + r'\b', code)) != 1 or len(matches) != 1 or not morrow_plugin.source_top_level(code, matches[0].start()):
        raise DistributionError('expected one top-level Rust u' + str(bits) + ' constant: ' + name)
    value = int(matches[0][1])
    if value >= 2 ** bits:
        raise DistributionError('out-of-range Rust version')
    return value


def require_source(pattern, text, label):
    if not re.search(pattern, text, re.MULTILINE):
        raise DistributionError('missing SDK source binding: ' + label)


def validate_sdk_bindings(root):
    """Narrow source binding observations; no Rust/C parsing/compilation/semantic proof."""
    for name in public_inputs():
        checked_file(root, name)
    lib = source_text(root, 'sdk/rust/src/lib.rs')
    # Follow file-backed Rust module declarations to catch missing private/child modules too.
    pending, seen = [('sdk/rust/src/lib.rs', 'sdk/rust/src')], set()
    while pending:
        name, directory = pending.pop()
        if name in seen:
            continue
        seen.add(name)
        if len(seen) > sdk_lock.MAX_FILES:
            raise DistributionError('Rust module observation limit')
        code = rust_code(source_text(root, name))
        for module in re.findall(r'^(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z_0-9]*)\s*;', code, re.MULTILINE):
            direct, nested = f'{directory}/{module}.rs', f'{directory}/{module}/mod.rs'
            choices = [candidate for candidate in (direct, nested) if (root / candidate).is_file()]
            if len(choices) != 1:
                raise DistributionError('missing/ambiguous Rust source module: ' + module)
            pending.append((choices[0], f'{directory}/{module}'))
    build = source_text(root, 'sdk/rust/build.rs')
    versions = {}
    for file, variable, symbol in (('version.txt', 'version', 'VERSION'), ('task-version.txt', 'task_version', 'TASK_VERSION'), ('ui-version.txt', 'ui_version', 'UI_VERSION')):
        value = checked_file(root, 'sdk/rust/contracts/' + file).decode('ascii').strip()
        if not re.fullmatch('[0-9]{1,5}', value) or int(value) > 65535:
            raise DistributionError('invalid SDK compiler version input: ' + file)
        versions[symbol] = int(value)
        require_source(r'let\s+' + variable + r'\s*=\s*std::fs::read_to_string\("contracts/' + re.escape(file) + r'"\)\?\s*\.trim\(\)\s*\.parse::<u16>\(\)\?', build, file + ' -> u16 compiler input')
        require_source(r'pub const ' + symbol + r':u16=\{' + variable + r'\};', build, variable + ' -> generated constant')
    require_source(r'include!\(concat!\(env!\("OUT_DIR"\), "/contract.rs"\)\)', lib, 'generated contract module')
    protocol = rust_code(source_text(root, 'sdk/rust/src/protocol.rs'))
    require_source(r'^pub const PROTOCOL_VERSION: u16 = contract::VERSION;$', protocol, 'runtime generated alias')
    task = rust_code(source_text(root, 'sdk/rust/src/task.rs'))
    require_source(r'get_version\(\) != contract::TASK_VERSION', task, 'task generated decode version')
    require_source(r'set_version\(contract::TASK_VERSION\)', task, 'task generated encode version')
    ui = rust_code(source_text(root, 'sdk/rust/src/ui.rs'))
    require_source(r'^pub const VERSION: u16 = crate::contract::UI_VERSION;$', ui, 'UI generated alias')
    require_source(r'set_protocol_version\(contract::VERSION\)', protocol, 'runtime version write')
    require_source(r'get_protocol_version\(\) != contract::VERSION', protocol, 'runtime version read')
    for stem in ('ui', 'dependency_call', 'io', 'service', 'service_resources', 'channel'):
        code = rust_code(source_text(root, 'sdk/rust/src/' + stem + '.rs'))
        if stem != 'ui':
            versions[stem] = exact_constant(source_text(root, 'sdk/rust/src/' + stem + '.rs'), 'VERSION', 32 if stem == 'channel' else 16)
        require_source(r'set_version\(VERSION\)', code, stem + ' version write')
        require_source(r'(?:get_version\(\)|\bversion) != VERSION', code, stem + ' version read')
    mutation_schema = source_text(root, 'sdk/rust/contracts/mutation.capnp')
    match = re.findall(r'^const version :UInt16 = ([0-9]+);$', mutation_schema, re.MULTILINE)
    if len(match) != 1 or int(match[0]) > 65535:
        raise DistributionError('invalid canonical mutation version declaration')
    versions['mutation'] = int(match[0])
    require_source(r'^pub const VERSION: u16 = wire::VERSION;$', rust_code(source_text(root, 'sdk/rust/src/mutation.rs')), 'mutation schema-generated version alias')
    # Native descriptor ABI is separate from all wire versions above.
    prefix = rust_code(source_text(root, 'sdk/rust/src/descriptor_prefix.rs'))
    values = re.findall(r'prefix\.abi_version == ([0-9]+)', prefix)
    if len(values) != 1:
        raise DistributionError('missing native descriptor ABI check')
    native_abi = int(values[0])
    for stem, symbol in (('morrow_plugin_io', 'MP_IO_ABI_VERSION'), ('morrow_plugin_service', 'MP_SERVICE_ABI_VERSION'), ('morrow_plugin_mutation', 'MP_MUTATION_ABI_VERSION'), ('morrow_channel_v1', 'MP_CHANNEL_VERSION')):
        c = rust_code(source_text(root, 'sdk/c/include/' + stem + '.h'))
        values = re.findall(r'^#define ' + symbol + r' ([0-9]+)u$', c, re.MULTILINE)
        if len(values) != 1 or int(values[0]) != native_abi:
            raise DistributionError('C/Rust native descriptor ABI differs: ' + stem)
    for stem in PUBLIC_HEADERS:
        cpp = source_text(root, 'sdk/cpp/include/' + stem + '.hpp')
        require_source(r'^#include "' + stem + r'\.h"$', cpp, 'C++ public header delegates to C: ' + stem)
    sdk_c = rust_code(source_text(root, 'sdk/c/include/morrow_plugin_sdk.h'))
    values = re.findall(r'^#define MP_SDK_ABI_V1 ([0-9]+)u$', sdk_c, re.MULTILINE)
    host = re.findall(r'host\.abi_version != ([0-9]+)', rust_code(lib))
    if len(values) != 1 or len(host) != 1 or values != host:
        raise DistributionError('C/Rust transport adapter ABI differs')
    cpp_codec = rust_code(source_text(root, 'sdk/cpp/include/morrow_plugin_codec.hpp'))
    values = re.findall(r'\.abi_version\s*=\s*([0-9]+)', cpp_codec)
    if not values or any(int(value) != native_abi for value in values):
        raise DistributionError('C++/Rust native codec descriptor ABI differs')
    cpp_channel = rust_code(source_text(root, 'sdk/cpp/include/morrow_channel_v1.hpp'))
    values = re.findall(r'\.abi_version\s*=\s*([0-9]+)', cpp_channel)
    if not values or any(int(value) != native_abi for value in values):
        raise DistributionError('C++/Rust native channel descriptor ABI differs')
    cpp_dependency = rust_code(source_text(root, 'sdk/cpp/include/morrow_plugin_dependency.hpp'))
    values = re.findall(r'return\s*\{([0-9]+),sizeof\(mp_dependency_request_v1\)', cpp_dependency)
    if len(values) != 1 or int(values[0]) != native_abi:
        raise DistributionError('C++/Rust native dependency descriptor ABI differs')
    ffi = rust_code(source_text(root, 'sdk/rust/src/ffi.rs'))
    ui_abi = re.findall(r'if abi != ([0-9]+) \|\| node_size', ffi)
    cpp_ui = rust_code(source_text(root, 'sdk/cpp/include/morrow_plugin_ui.hpp'))
    ui_cpp = re.findall(r'mp_ui_document_encode\(\s*([0-9]+),', cpp_ui)
    if len(ui_abi) != 1 or ui_abi != ui_cpp:
        raise DistributionError('C++/Rust native UI ABI differs')
    return {'scope': 'narrow-source-binding-observations', 'wire_versions': versions, 'native_descriptor_abi': native_abi,
            'rust_modules_observed': len(seen), 'host_agreement': False, 'semantics': 'NOT_PROVED',
            'limitations': ['No compiler or macro expansion performed.', 'C/C++ runtime/task/UI wire versions are supplied by the shared Rust FFI codec, not independent public numeric constants.', 'IO/service/resources/dependency/channel have one Rust wire VERSION declaration; no independent SDK schema wire-version constant exists to compare.', 'Schema/manual decoder semantics and arbitrary self-rehashed malicious source behavior are NOT_PROVED. The inventory is not signed.']}


def validate_source_authority(root):
    """Export requires the repository's single Core contract authority; never bundled."""
    internal = validate_sdk_bindings(root)
    compared = []
    for name in CONTRACTS:
        if name.endswith('.txt'):
            continue
        sdk_data = checked_file(root, 'sdk/rust/contracts/' + name).replace(b'\r\n', b'\n')
        core_data = checked_file(root, 'core/schemas/' + name).replace(b'\r\n', b'\n')
        if sdk_data != core_data:
            raise DistributionError('SDK/Core canonical schema differs before export: ' + name)
        compared.append({'name': name, 'sha256_lf': sha(core_data)})
    for stem, constant, sdk_key, bits in (('runtime', 'PROTOCOL_VERSION', 'VERSION', 16), ('task', 'VERSION', 'TASK_VERSION', 16), ('ui', 'VERSION', 'UI_VERSION', 16), ('dependency_call', 'VERSION', 'dependency_call', 16), ('io', 'VERSION', 'io', 16), ('service', 'VERSION', 'service', 16), ('service_resources', 'VERSION', 'service_resources', 16), ('channel', 'VERSION', 'channel', 32)):
        core = exact_constant(source_text(root, 'core/src/' + stem + '.rs'), constant, bits)
        if core != internal['wire_versions'][sdk_key]:
            raise DistributionError('SDK/Core wire version differs before export: ' + stem)
    require_source(r'^pub const VERSION: u16 = wire::VERSION;$', rust_code(source_text(root, 'core/src/mutation.rs')), 'Core mutation canonical version binding')
    return {'scope': 'source-root SDK/Core canonical contracts and declared versions compared before export', 'schemas': compared,
            'wire_versions': internal['wire_versions'], 'core_bundled': False, 'signature': False, 'semantics': 'NOT_PROVED'}


def validate_payload_names(names):
    fixed = {'README.md', 'NOTICE', 'docs/PLUGIN_PROJECT_TOOLS.md', 'docs/PLUGIN_SDK_COMPATIBILITY.md'}
    fixed.update(public_inputs())
    fixed.update('sdk/' + name for name in sdk_lock.ROOT_FILES + SDK_DOCS)
    fixed.update('sdk/rust/contracts/' + name for name in CONTRACTS)
    fixed.update('tool/' + name for name in TOOLS)
    for lang in LANGUAGES:
        for profile in STARTERS:
            prefix = f'sdk/examples/{lang}-{profile}/'
            fixed.add(prefix + ('plugin.c' if lang == 'c' else 'plugin.cpp' if lang == 'cpp' else 'src/lib.rs'))
            if lang == 'rust':
                fixed.update((prefix + 'Cargo.toml', prefix + 'Cargo.lock'))
    if not fixed <= set(names):
        raise DistributionError('missing required distribution input: ' + ', '.join(sorted(fixed - set(names))))
    forbidden = {'.dll', '.exe', '.lib', '.pdb', '.obj', '.o', '.a', '.so', '.dylib', '.wasm', '.mplugin', '.zip'}
    for name in names:
        safe_name(name)
        if (name not in fixed and not any(name.startswith('sdk/' + tree + '/') for tree in sdk_lock.TREES)) or Path(name).suffix.lower() in forbidden:
            raise DistributionError('outside SDK source distribution closure: ' + name)


def selected_names(root):
    # Reuse the existing SDK source-lock inventory and constraints unchanged.
    library = sdk_lock.inventory(root / 'sdk')
    validate_sdk_bindings(root)
    for name in CONTRACTS:
        checked_file(root, 'sdk/rust/contracts/' + name)
    names = {'sdk/' + name for name in library}
    names.update('sdk/' + name for name in SDK_DOCS)
    names.update('tool/' + name for name in TOOLS)
    names.update(('NOTICE', 'docs/PLUGIN_PROJECT_TOOLS.md', 'docs/PLUGIN_SDK_COMPATIBILITY.md'))
    for lang in LANGUAGES:
        for profile in STARTERS:
            prefix = f'sdk/examples/{lang}-{profile}/'
            names.add(prefix + ('plugin.c' if lang == 'c' else 'plugin.cpp' if lang == 'cpp' else 'src/lib.rs'))
            if lang == 'rust':
                names.update((prefix + 'Cargo.toml', prefix + 'Cargo.lock'))
    if len(names) + 1 > MAX_FILES:
        raise DistributionError('file count limit')
    # Version files are compiler inputs, not a new version configuration.
    for name in ('version.txt', 'task-version.txt', 'ui-version.txt'):
        value = checked_file(root, 'sdk/rust/contracts/' + name).decode('ascii').strip()
        if not re.fullmatch('[0-9]{1,5}', value) or int(value) > 65535:
            raise DistributionError('invalid SDK u16 compiler version input: ' + name)
    # Validate compiler inputs in ordinary and target-specific Cargo tables.
    import tomllib
    for name in ['sdk/rust/Cargo.toml'] + [f'sdk/examples/rust-{p}/Cargo.toml' for p in STARTERS]:
        cargo = tomllib.loads(checked_file(root, name).decode('utf-8'))
        # Additional Cargo source graphs are not included or resolved by this bundle.
        if any(key in cargo for key in ('patch', 'replace', 'bin', 'example', 'test', 'bench')):
            raise DistributionError('Cargo patch/replace/explicit target inputs unsupported in source closure: ' + name)
        workspace = cargo.get('workspace', {})
        if (not isinstance(workspace, dict) or set(workspace) - {'resolver', 'members', 'default-members', 'exclude'}
                or any(value != [] for key, value in workspace.items() if key != 'resolver')):
            raise DistributionError('Cargo workspace members/inherited inputs unsupported in source closure: ' + name)
        parent = (root / name).parent
        library = name == 'sdk/rust/Cargo.toml'
        package, lib = cargo.get('package', {}), cargo.get('lib', {})
        if not isinstance(package, dict) or not isinstance(lib, dict):
            raise DistributionError('invalid Cargo package/lib tables: ' + name)
        if 'workspace' in package:
            raise DistributionError('Cargo package.workspace unsupported in standalone source closure: ' + name)
        def compiler_path(value, expected, label):
            if not isinstance(value, str) or Path(value).is_absolute() or PureWindowsPath(value).drive:
                raise DistributionError('unsupported absolute/non-string Cargo ' + label + ': ' + name)
            path = parent / value
            for ancestor in [path, *path.parents]:
                if ancestor.exists() or os.path.lexists(ancestor):
                    regular(ancestor, ancestor != path)
            if path.resolve(strict=True) != expected.resolve(strict=True):
                raise DistributionError('Cargo ' + label + ' escapes supported source entry: ' + name)
        compiler_path(lib.get('path', 'src/lib.rs'), parent / 'src/lib.rs', 'lib.path')
        build = package.get('build', True)
        if library:
            if build is False:
                raise DistributionError('SDK Cargo package.build disables canonical contract generation')
            compiler_path('build.rs' if build is True else build, parent / 'build.rs', 'package.build')
        elif build is not False and (build is not True or (parent / 'build.rs').exists()):
            raise DistributionError('starter Cargo package.build input is outside supported source closure: ' + name)
        tables = [cargo]
        targets = cargo.get('target', {})
        if not isinstance(targets, dict) or any(not isinstance(value, dict) for value in targets.values()):
            raise DistributionError('invalid Cargo target tables: ' + name)
        tables.extend(targets.values())
        for table in tables:
            for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
                dependencies = table.get(section, {})
                if not isinstance(dependencies, dict):
                    raise DistributionError('invalid Cargo dependency table: ' + name)
                for dependency in dependencies.values():
                    if isinstance(dependency, dict):
                        if dependency.get('workspace'):
                            raise DistributionError('workspace-inherited Cargo dependency unsupported in standalone closure: ' + name)
                        if 'path' in dependency:
                            # This bundle's only supported path dependency is its SDK library.
                            value = dependency['path']
                            if not isinstance(value, str) or Path(value).is_absolute() or PureWindowsPath(value).drive:
                                raise DistributionError('absolute Cargo dependency in SDK bundle: ' + name)
                            path = parent / value
                            regular(path, True)
                            if path.resolve(strict=True) != (root / 'sdk/rust').resolve(strict=True):
                                raise DistributionError('Cargo dependency escapes SDK library: ' + name)
    return sorted(names)

def unique_object(pairs):
    result = {}
    for name, value in pairs:
        if name in result:
            raise DistributionError('duplicate manifest key: ' + name)
        result[name] = value
    return result

def parse_manifest(data):
    if len(data) > MAX_MANIFEST_BYTES:
        raise DistributionError('manifest size limit')
    try:
        value = json.loads(data.decode('utf-8'), object_pairs_hook=unique_object)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise DistributionError('invalid manifest') from error
    if not isinstance(value, dict) or set(value) != {'schema', 'profile', 'qualification', 'native_libraries_bundled', 'files'}:
        raise DistributionError('unexpected manifest fields')
    if type(value['schema']) is not int or value['schema'] != 1 or value['profile'] != PROFILE or value['qualification'] != 'source-only-not-frozen' or value['native_libraries_bundled'] is not False:
        raise DistributionError('unsupported distribution manifest')
    files = value['files']
    if not isinstance(files, dict) or not 1 <= len(files) <= MAX_FILES:
        raise DistributionError('manifest file count limit')
    seen, total = set(), 0
    for name, entry in files.items():
        safe_name(name)
        if name == MANIFEST or name.casefold() in seen:
            raise DistributionError('duplicate/colliding manifest path: ' + name)
        seen.add(name.casefold())
        if not isinstance(entry, dict) or set(entry) != {'size', 'sha256'} or type(entry['size']) is not int or not 0 <= entry['size'] <= MAX_FILE_BYTES or not isinstance(entry['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', entry['sha256']):
            raise DistributionError('invalid manifest entry: ' + name)
        total += entry['size']
    if total + len(data) > MAX_TOTAL_BYTES:
        raise DistributionError('total size limit including manifest')
    return value

def snapshot(root):
    root = root.absolute()
    names = selected_names(root)
    files = {'README.md': README}
    total = len(README)
    if total > MAX_TOTAL_BYTES:
        raise DistributionError('total size limit before payload reads')
    for name in names:
        data = checked_file(root, name, MAX_TOTAL_BYTES - total)
        total += len(data)
        files[name] = data
    validate_payload_names(files)
    manifest = {'schema': 1, 'profile': PROFILE, 'qualification': 'source-only-not-frozen', 'native_libraries_bundled': False,
                'files': {name: {'size': len(data), 'sha256': sha(data)} for name, data in sorted(files.items())}}
    encoded = (json.dumps(manifest, ensure_ascii=True, indent=2, sort_keys=True) + '\n').encode()
    parse_manifest(encoded)
    return files, encoded

def verify_zip(path):
    info = regular(path)
    if info.st_size > MAX_ARCHIVE_BYTES:
        raise DistributionError('archive size limit')
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        if not 2 <= len(entries) <= MAX_FILES + 1:
            raise DistributionError('archive file count limit')
        seen, total = set(), 0
        for entry in entries:
            safe_name(entry.filename)
            mode = entry.external_attr >> 16
            if entry.filename.casefold() in seen or entry.is_dir() or stat.S_ISLNK(mode) or (mode and stat.S_IFMT(mode) not in (0, stat.S_IFREG)) or entry.flag_bits & 1:
                raise DistributionError('duplicate/nonregular/encrypted archive entry: ' + entry.filename)
            seen.add(entry.filename.casefold())
            if entry.file_size > (MAX_MANIFEST_BYTES if entry.filename == MANIFEST else MAX_FILE_BYTES):
                raise DistributionError('archive file size limit')
            total += entry.file_size
        if total > MAX_TOTAL_BYTES or MANIFEST not in archive.namelist():
            raise DistributionError('archive total limit or missing manifest')
        manifest = parse_manifest(archive.read(MANIFEST))
        if set(archive.namelist()) != set(manifest['files']) | {MANIFEST}:
            raise DistributionError('archive inventory differs')
        validate_payload_names(manifest['files'])
        for name, item in manifest['files'].items():
            data = archive.read(name)  # ZipFile checks CRC on read; prechecked size bounds.
            if len(data) != item['size'] or sha(data) != item['sha256']:
                raise DistributionError('archive hash/size differs: ' + name)
    return manifest

def directory_files(root):
    regular(root, True)
    result = set()
    pending = [root]
    visited = 0
    while pending:
        parent = pending.pop()
        for path in parent.iterdir():
            visited += 1
            if visited > MAX_FILES * 4:
                raise DistributionError('directory traversal limit')
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
                raise DistributionError('links/reparse points are not distribution inputs: ' + str(path))
            if stat.S_ISDIR(info.st_mode):
                pending.append(path)
            elif stat.S_ISREG(info.st_mode):
                result.add(safe_name(path.relative_to(root).as_posix()))
                if len(result) > MAX_FILES + 1:
                    raise DistributionError('file count limit')
            else:
                raise DistributionError('nonregular directory input')
    return result

def verify_distribution(root):
    root = root.absolute()
    for parent in [root, *root.parents]:
        regular(parent, True)
    data = checked_file(root, MANIFEST)
    manifest = parse_manifest(data)
    required = set(selected_names(root)) | {'README.md'}
    if set(manifest['files']) != required or directory_files(root) != required | {MANIFEST}:
        raise DistributionError('distribution closure differs (keep projects and caches outside bundle)')
    for name, entry in manifest['files'].items():
        current = checked_file(root, name)
        if len(current) != entry['size'] or sha(current) != entry['sha256']:
            raise DistributionError('distribution hash/size differs: ' + name)
    return manifest

def create_archive(root, output):
    root, output = root.absolute(), output.absolute()
    regular(root, True)
    regular(output.parent, True)
    # Validate the full parent chain, including ancestors above the input root.
    for path in [root, *root.parents, output.parent, *output.parent.parents]:
        regular(path, True)
    if os.path.lexists(output):
        raise DistributionError('refuses existing output: ' + str(output))
    if output == root or root in output.parents:
        raise DistributionError('output must be outside the source root')
    authority = validate_source_authority(root)
    files, manifest = snapshot(root)  # Missing inputs fail before any output/temp file.
    temporary = None
    try:
        descriptor, name = tempfile.mkstemp(prefix='.morrow-sdk-', suffix='.zip', dir=output.parent)
        temporary = Path(name)
        os.close(descriptor)
        with zipfile.ZipFile(temporary, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name, data in sorted({**files, MANIFEST: manifest}.items()):
                entry = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                entry.create_system = 3
                entry.external_attr = (stat.S_IFREG | 0o644) << 16
                entry.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(entry, data)
        verify_zip(temporary)
        # Recheck selected inputs before publishing. No output overwrite is possible.
        if snapshot(root) != (files, manifest):
            raise DistributionError('distribution inputs changed while packaging')
        if validate_source_authority(root) != authority:
            raise DistributionError('source-root Core authority changed while packaging')
        os.link(temporary, output)
        return {'bytes': output.stat().st_size, 'sha256': sha(output.read_bytes()), 'files': len(files), 'source_authority': authority}
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    create = sub.add_parser('create')
    create.add_argument('--source-root', type=Path, required=True)
    create.add_argument('--output', type=Path, required=True)
    for name in ('verify-zip', 'verify-directory'):
        sub.add_parser(name).add_argument('path', type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == 'create':
            result = create_archive(args.source_root, args.output)
        else:
            manifest = verify_zip(args.path) if args.command == 'verify-zip' else verify_distribution(args.path)
            result = {'status': 'verified', 'scope': 'local-byte-inventory', 'files': len(manifest['files']), 'host_agreement': False, 'sdk_frozen': False, 'source_binding_semantics': 'NOT_PROVED'}
        print(json.dumps(result, sort_keys=True))
        return 0
    except (DistributionError, sdk_lock.SdkLockError, OSError, zipfile.BadZipFile, RuntimeError, UnicodeError, ValueError) as error:
        print('ERROR:', error, file=__import__('sys').stderr)
        return 1

if __name__ == '__main__':
    raise SystemExit(main())
