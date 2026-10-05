"""Export/verify a bounded standalone diagnostic source ZIP, never execute it.

Repository-side tool only. A verified manifest is finite local byte identity, not
a signature, host qualification, protocol authority or an SDK freeze.
"""
import argparse
import io
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import zipfile
import zlib

from package_plugin_sdk import DistributionError, regular, safe_name, sha, unique_object

MANIFEST = 'SDK_DIAGNOSTICS_MANIFEST.json'
PROFILE = 'morrow-sdk-diagnostics-source-v1'
SOURCES = {'morrow_sdk_diagnostics.py': 'tool/morrow_sdk_diagnostics.py',
           'sdk_profiles.py': 'tool/sdk_profiles.py', 'LICENSE': 'LICENSE', 'NOTICE': 'NOTICE'}
PAYLOAD = frozenset(SOURCES) | {'README.md'}
MAX_FILES = 6
MAX_FILE_BYTES = 128 * 1024
MAX_MANIFEST_BYTES = 8 * 1024
MAX_TOTAL_BYTES = 256 * 1024
MAX_ARCHIVE_BYTES = MAX_TOTAL_BYTES + 32 * 1024
README = b'''# Standalone Morrow SDK host diagnostics

Python 3.11+ and the standard library are sufficient for these two source files.
Use an absolute Python executable and an explicitly selected trusted local host:

    /absolute/python3 -I -B morrow_sdk_diagnostics.py profiles --host /trusted/morrow-workbench-host
    /absolute/python3 -I -B morrow_sdk_diagnostics.py preflight /selected/package.mplugin --host /trusted/morrow-workbench-host

The entry point loads only its adjacent original sdk_profiles.py. No repository,
Core, runtime, SDK libraries, exporter, compiler or network is needed by this
running tool. No native host or precompiled guest is bundled. There is no host
discovery, download, build, installation or fallback. You must trust the selected
host executable and local files; this tool is not a sandbox for malicious code
or a hostile filesystem.

profiles returns compiled metadata, with no authority. preflight first validates
the advertised known capability and only then requests static preparation.
An old host lacking that capability receives only --sdk-capabilities, never the
new diagnostic command. Prepared means static preparation only: no guest run,
installation, grants, route qualification or production qualification.

After valid argument parsing, exits are 0 for validated profiles/prepared, 2 for
a validated rejected preflight, and 1 for local/discovery/protocol/process errors
without receipt stdout. Argument usage errors retain argparse exit 2 and stderr;
exit 2 alone is not a rejection receipt. stdout contains one validated JSON receipt
only for a successful query or a protocol-valid prepared/rejected preflight.

The unchanged consumer enforces a five-second deadline per subprocess/pipe and
bounded cleanup. This is not a five-second end-to-end guarantee or a guarantee of
whole process-tree reclamation. Host and archive before/after byte observations
do not protect against an attacker replacing/restoring files between reads.

SDK_DIAGNOSTICS_MANIFEST.json inventories the five payload files only. It is not
signed and establishes finite local byte identity, not trusted provenance, host
qualification, SDK freeze, Windows readiness or production readiness. Rehashing
modified sources does not establish their safety or semantic correctness.
Verify the ZIP before extraction using a separately trusted repository copy:

    python3 -B tool/package_sdk_diagnostics.py verify-zip /selected/diagnostics.zip

After extracting to a new directory, the same trusted repository tool supports
verify-directory /selected/directory. Do not execute an unverified archive's tools
to establish trust in that archive. The exporter/verifier is deliberately absent
from this running bundle. Keep projects, hosts, packages and caches outside it.

This is a separate diagnostics source bundle. The existing SDK source-v1 export,
sdk.lock.toml, schemas, fixed originals and SDK-only pack/check/transform refusals
remain unchanged; adding this bundle does not turn them into host-aware routes.
Original LICENSE and NOTICE apply to the bundled sources.
'''


def identity(info):
    # Windows path/descriptor stat disagree on ctime in Python 3.12; birth
    # time is consistent. Python 3.11 ctime already represents birth time.
    timestamp = (getattr(info, 'st_birthtime_ns', info.st_ctime_ns)
                 if os.name == 'nt' else info.st_ctime_ns)
    # Path stat alone synthesizes filename-based execute bits on Windows.
    mode = info.st_mode & ~0o111 if os.name == 'nt' else info.st_mode
    return (info.st_dev, info.st_ino, mode, info.st_size,
            info.st_mtime_ns, timestamp)


def checked_directory(path):
    path = path.absolute()
    for parent in (path, *path.parents):
        regular(parent, True)
    return path.resolve(strict=True)


def read_regular(path, maximum):
    """Bound the open/read too; reject substitutions and nonregular inputs."""
    path = path.absolute()
    checked_directory(path.parent)
    selected = regular(path)
    flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_NONBLOCK', 0)
    with os.fdopen(os.open(path, flags), 'rb') as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or identity(before) != identity(selected):
            raise DistributionError('input changed or is not regular: ' + str(path))
        if before.st_size > maximum:
            raise DistributionError('input size limit: ' + str(path))
        data = stream.read(maximum + 1)
        after = os.fstat(stream.fileno())
    if (len(data) > maximum or len(data) != after.st_size
            or identity(before) != identity(after) or identity(after) != identity(regular(path))):
        raise DistributionError('input changed or exceeds size limit: ' + str(path))
    return data, identity(after)


def parse_manifest(data):
    if len(data) > MAX_MANIFEST_BYTES:
        raise DistributionError('manifest size limit')
    try:
        value = json.loads(data.decode('utf-8'), object_pairs_hook=unique_object)
    except RecursionError as error:
        raise DistributionError('manifest nesting limit') from error
    if (not isinstance(value, dict) or set(value) != {'schema', 'profile', 'scope', 'files'}
            or type(value['schema']) is not int or value['schema'] != 1
            or value['profile'] != PROFILE or value['scope'] != 'local-byte-identity-only'
            or not isinstance(value['files'], dict) or set(value['files']) != PAYLOAD):
        raise DistributionError('unsupported diagnostics manifest or file closure')
    total = len(data)
    for name, item in value['files'].items():
        safe_name(name)
        if (not isinstance(item, dict) or set(item) != {'size', 'sha256'}
                or type(item['size']) is not int or not 0 <= item['size'] <= MAX_FILE_BYTES
                or not isinstance(item['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', item['sha256'])):
            raise DistributionError('invalid diagnostics manifest entry: ' + name)
        total += item['size']
    if total > MAX_TOTAL_BYTES:
        raise DistributionError('total size limit')
    return value


def snapshot(root):
    root = checked_directory(root)
    files, identities = {'README.md': README}, {}
    total = len(README)
    if total > MAX_FILE_BYTES or len(PAYLOAD) + 1 != MAX_FILES:
        raise DistributionError('diagnostics payload limit')
    for name, source in SOURCES.items():
        data, identities[source] = read_regular(root / source, min(MAX_FILE_BYTES, MAX_TOTAL_BYTES - total))
        files[name] = data
        total += len(data)
    manifest = {'schema': 1, 'profile': PROFILE, 'scope': 'local-byte-identity-only',
                'files': {name: {'size': len(data), 'sha256': sha(data)} for name, data in sorted(files.items())}}
    encoded = (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode('utf-8')
    parse_manifest(encoded)
    return files, encoded, identities


def verify_bytes(data):
    if len(data) > MAX_ARCHIVE_BYTES:
        raise DistributionError('archive size limit')
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len(entries) != MAX_FILES:
            raise DistributionError('archive file count differs')
        seen, total = set(), 0
        for entry in entries:
            if entry.orig_filename != entry.filename:
                raise DistributionError('archive filename was normalized or truncated')
            safe_name(entry.filename)
            mode = entry.external_attr >> 16
            if (entry.filename.casefold() in seen or entry.is_dir()
                    or stat.S_IFMT(mode) not in (0, stat.S_IFREG)
                    or entry.external_attr & (0x10 | 0x400) or entry.flag_bits & 1):
                raise DistributionError('duplicate/nonregular/encrypted archive entry')
            seen.add(entry.filename.casefold())
            maximum = MAX_MANIFEST_BYTES if entry.filename == MANIFEST else MAX_FILE_BYTES
            if not 0 <= entry.file_size <= maximum:
                raise DistributionError('archive file size limit')
            total += entry.file_size
        if total > MAX_TOTAL_BYTES or set(archive.namelist()) != PAYLOAD | {MANIFEST}:
            raise DistributionError('archive total limit or fixed closure differs')
        manifest = parse_manifest(archive.read(MANIFEST))
        for name, item in manifest['files'].items():
            # Metadata has bounded every decompressed entry before read; CRC is checked.
            content = archive.read(name)
            if len(content) != item['size'] or sha(content) != item['sha256']:
                raise DistributionError('archive byte identity differs: ' + name)
    return manifest


def verify_zip(path):
    data, _ = read_regular(path, MAX_ARCHIVE_BYTES)
    return verify_bytes(data)


def verify_directory(path):
    root = checked_directory(path)
    # Flat fixed closure: no recursive walk, links, caches or extra directories.
    names = set()
    for item in root.iterdir():
        if len(names) >= MAX_FILES:
            raise DistributionError('directory file count limit')
        regular(item)
        names.add(item.name)
    if names != PAYLOAD | {MANIFEST}:
        raise DistributionError('directory fixed closure differs')
    raw, _ = read_regular(root / MANIFEST, MAX_MANIFEST_BYTES)
    manifest = parse_manifest(raw)
    for name, item in manifest['files'].items():
        content, _ = read_regular(root / name, MAX_FILE_BYTES)
        if len(content) != item['size'] or sha(content) != item['sha256']:
            raise DistributionError('directory byte identity differs: ' + name)
    return manifest


def create_archive(root, output):
    root, output = checked_directory(root), output.absolute()
    output = checked_directory(output.parent) / output.name
    if os.path.lexists(output):
        raise DistributionError('refuses existing output: ' + str(output))
    if root == output or root in output.parents:
        raise DistributionError('output must be outside source root')
    before = snapshot(root)
    files, manifest, _ = before
    temporary = None
    published = False
    try:
        descriptor, name = tempfile.mkstemp(prefix='.morrow-diagnostics-', suffix='.zip', dir=output.parent)
        temporary = Path(name)
        os.close(descriptor)
        with zipfile.ZipFile(temporary, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name, content in sorted({**files, MANIFEST: manifest}.items()):
                entry = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                entry.create_system = 3
                entry.external_attr = (stat.S_IFREG | 0o644) << 16
                entry.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(entry, content)
        verify_zip(temporary)  # Validate before publication; never execute contents.
        archive_bytes, _ = read_regular(temporary, MAX_ARCHIVE_BYTES)
        receipt = {'bytes': len(archive_bytes), 'sha256': sha(archive_bytes), 'files': MAX_FILES,
                   'profile': PROFILE, 'scope': 'local-byte-identity-only'}
        if snapshot(root) != before:
            raise DistributionError('diagnostics source inputs changed while packaging')
        os.link(temporary, output)  # Atomic no-overwrite publication, including races.
        published = True
        return receipt
    finally:
        if temporary is not None:
            try:
                temporary.unlink(missing_ok=True)
            except OSError as error:
                if not published:
                    raise
                # link is the publication commit. Never delete the destination:
                # even its name could already refer to another actor's file.
                print('WARNING: archive published; temporary cleanup failed:', error, file=sys.stderr)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    create = commands.add_parser('create')
    create.add_argument('--source-root', required=True, type=Path)
    create.add_argument('--output', required=True, type=Path)
    for command in ('verify-zip', 'verify-directory'):
        commands.add_parser(command).add_argument('path', type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == 'create':
            result = create_archive(args.source_root, args.output)
        else:
            manifest = verify_zip(args.path) if args.command == 'verify-zip' else verify_directory(args.path)
            result = {'status': 'verified', 'scope': manifest['scope'], 'files': MAX_FILES,
                      'profile': PROFILE}
        print(json.dumps(result, sort_keys=True))
        return 0
    except (OSError, ValueError, UnicodeError, RuntimeError, zipfile.BadZipFile,
            NotImplementedError, EOFError, zlib.error) as error:
        print('ERROR:', error, file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
