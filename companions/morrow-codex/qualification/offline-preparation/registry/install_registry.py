"""Disabled offline registry installation candidate. No network or code execution."""
import gzip
import hashlib
import io
import json
import os
import pathlib
import re
import stat
import tarfile
import tomllib

ROOT = pathlib.Path(__file__).parent.absolute()
PLAN_SHA = 'd77e275a91424e2671c0ca14a30ab77c5f1c129aa9dcd31c4d22cbad594adff2'
FETCH_READY_SHA = 'f9be52010d9196e6efbf8751c9f78e088ab86098dcc43143a5ffac1eebfa06ed'
REGISTRY = 'index.crates.io-1949cf8c6b5b557f'
CHUNK = 65536
MAX_EXPANDED = 128 * 1024 * 1024
MAX_MEMBERS = 20000
MAX_INDEX = 8 * 1024 * 1024
MAX_ARCHIVE = 32 * 1024 * 1024
MAX_DISK = 512 * 1024 * 1024
CONFIG = b'{"dl":"https://static.crates.io/crates","api":"https://crates.io"}\n'


class Rejected(Exception):
    pass


def require(test, label):
    if not test:
        raise Rejected(label)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def parsed(raw):
    def unique(pairs):
        result = {}
        for k, v in pairs:
            require(k not in result, 'duplicate_json_property')
            result[k] = v
        return result
    return json.loads(raw.decode('utf-8'), object_pairs_hook=unique)


def plain(path):
    p = pathlib.Path(os.path.abspath(path))
    for item in (p, *p.parents):
        try:
            s = item.lstat()
        except FileNotFoundError:
            continue
        require(not stat.S_ISLNK(s.st_mode) and not getattr(s, 'st_file_attributes', 0) & 0x400, 'reparse')
        require(stat.S_ISDIR(s.st_mode) if item != p else stat.S_ISDIR(s.st_mode) or stat.S_ISREG(s.st_mode), 'special_path')
    return p


def components(name):
    require(isinstance(name, str) and '\\' not in name and ':' not in name, 'unsafe_spelling')
    parts = name.split('/')
    require(all(v not in ('', '.', '..') and not v.endswith(('.', ' ')) for v in parts), 'unsafe_components')
    for v in parts:
        require(not any(ord(c) < 32 for c in v), 'control_name')
        require(not re.match(r'^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)', v, re.I), 'reserved_name')
    return parts


class Writes:
    def __init__(self, root):
        self.root, self.used = root, 0

    def new(self, relative, data):
        target = self.root.joinpath(*components(relative))
        require(target.is_relative_to(self.root), 'outside_cache')
        require(self.used + len(data) <= MAX_DISK, 'logical_write_budget')
        plain(target)
        target.parent.mkdir(parents=True, exist_ok=True)
        plain(target)
        with target.open('xb') as f:
            require(f.write(data) == len(data), 'short_write')
            f.flush()
            os.fsync(f.fileno())
        self.used += len(data)
        require(target.read_bytes() == data, 'write_readback')


def index_suffix(name):
    if len(name) == 1:
        return '1/' + name
    if len(name) == 2:
        return '2/' + name
    if len(name) == 3:
        return '3/' + name[0] + '/' + name
    return name[:2] + '/' + name[2:4] + '/' + name


def sparse_cache(raw, record):
    """Observed cache v3/index v2. Empty validator is local, never server ETag."""
    require(isinstance(raw, bytes) and 0 < len(raw) <= MAX_INDEX, 'index_bound')
    result = bytearray(b'\x03\x02\x00\x00\x00\x00')
    versions, selected = set(), None
    for line in raw.splitlines():
        require(line and b'\x00' not in line, 'index_line')
        item = parsed(line)
        require(item.get('name') == record['name'] and isinstance(item.get('vers'), str), 'index_identity')
        version = item['vers']
        require(re.fullmatch(r'[0-9A-Za-z.+-]{1,128}', version) is not None and version not in versions, 'index_version')
        require(re.fullmatch(r'[a-f0-9]{64}', item.get('cksum', '')) is not None and isinstance(item.get('deps'), list) and type(item.get('yanked')) is bool, 'index_record')
        versions.add(version)
        if version == record['version']:
            require(item['cksum'] == record['sha256'] and item['yanked'] is False, 'selected_index_checksum')
            selected = line + b'\n'
        result.extend(version.encode('ascii') + b'\x00' + line + b'\x00')
    require(selected is not None and len(result) <= MAX_INDEX + 262144, 'selected_index_missing')
    return bytes(result), sha(selected)


def extract_checked(raw, record, writes):
    require(isinstance(raw, bytes) and len(raw) <= MAX_ARCHIVE and sha(raw) == record['sha256'], 'archive_checksum')
    prefix = record['name'] + '-' + record['version']
    base = 'registry/src/' + REGISTRY + '/' + prefix
    seen, kinds, checksums, total, count = set(), {}, {}, 0, 0

    class Bounded:
        def __init__(self, stream):
            self.stream, self.used = stream, 0

        def read(self, size):
            require(0 <= size <= CHUNK, 'reader_bound')
            data = self.stream.read(min(size, MAX_EXPANDED - self.used + 1))
            require(self.used + len(data) <= MAX_EXPANDED, 'decompressed_bound')
            self.used += len(data)
            return data

    with gzip.GzipFile(fileobj=io.BytesIO(raw), mode='rb') as zipped:
        bounded = Bounded(zipped)
        with tarfile.open(fileobj=bounded, mode='r|') as archive:
            for member in archive:
                count += 1
                require(count <= MAX_MEMBERS, 'member_bound')
                name = member.name.rstrip('/') if member.isdir() else member.name
                parts = components(name)
                require(parts[0] == prefix and (len(parts) > 1 or member.isdir()), 'archive_root')
                key = name.casefold()
                require(key not in seen, 'duplicate_casefold')
                seen.add(key)
                require(member.isfile() or member.isdir(), 'link_or_special')
                require(member.size >= 0 and total + member.size <= MAX_EXPANDED, 'expanded_bound')
                total += member.size
                for length in range(1, len(parts)):
                    parent = '/'.join(parts[:length]).casefold()
                    require(kinds.get(parent) != 'file', 'file_directory_collision')
                    kinds[parent] = 'directory'
                require(kinds.get(key) != 'directory' or member.isdir(), 'file_directory_collision')
                kinds[key] = 'directory' if member.isdir() else 'file'
                if member.isdir():
                    require(member.size == 0, 'directory_data')
                    continue
                relative = '/'.join(parts[1:])
                require(relative not in ('.cargo-ok', '.cargo-checksum.json'), 'cargo_generated_collision')
                reader = archive.extractfile(member)
                require(reader is not None, 'member_read')
                # Stream each member to CreateNew; no archive modes, links or executable bits copied.
                target = writes.root.joinpath(*components(base + '/' + relative))
                plain(target)
                target.parent.mkdir(parents=True, exist_ok=True)
                plain(target)
                digest, copied = hashlib.sha256(), 0
                with reader, target.open('xb') as f:
                    while True:
                        chunk = reader.read(CHUNK)
                        if not chunk:
                            break
                        require(writes.used + len(chunk) <= MAX_DISK, 'logical_write_budget')
                        require(f.write(chunk) == len(chunk), 'short_write')
                        digest.update(chunk)
                        copied += len(chunk)
                        writes.used += len(chunk)
                    f.flush()
                    os.fsync(f.fileno())
                require(copied == member.size, 'member_length')
                checksums[relative] = digest.hexdigest()
                require(sha(target.read_bytes()) == checksums[relative], 'member_readback')
            # Streaming tar caches TarInfo (bounded by MAX_MEMBERS plus one rejected).
            # Drain its read-ahead AND the same bounded gzip to EOF for CRC/tails.
            while True:
                tail = archive.fileobj.read(CHUNK)
                if not tail:
                    break
                require(not any(tail), 'nonzero_tail')
    require('Cargo.toml' in checksums, 'manifest_missing')
    manifest_path = writes.root / base / 'Cargo.toml'
    require(manifest_path.stat().st_size <= 1048576, 'manifest_bound')
    package = tomllib.loads(manifest_path.read_text(encoding='utf-8'))['package']
    require(package['name'] == record['name'] and package['version'] == record['version'], 'manifest_identity')
    checksum = json.dumps({'files': checksums, 'package': record['sha256']}, sort_keys=True, separators=(',', ':')).encode() + b'\n'
    writes.new(base + '/.cargo-checksum.json', checksum)
    # Actual fixed local Cargo sample marker: {"v":1}. Written only after full validation.
    writes.new(base + '/.cargo-ok', b'{"v":1}')
    return {'members': count, 'file_count': len(checksums), 'expanded_files': total, 'decompressed_input': bounded.used}


def _install_records(records, root):
    """Private primitive: synthetic tests only; does not establish authorization."""
    root = plain(root)
    require(root.parent == ROOT and not root.exists(), 'fresh_exclusive_child_required')
    root.mkdir(exist_ok=False)
    writes = Writes(root)
    observations = []
    writes.new('registry/index/' + REGISTRY + '/config.json', CONFIG)
    for record, raw, index in records:
        cache, selected = sparse_cache(index, record)
        writes.new('registry/cache/' + REGISTRY + '/' + record['name'] + '-' + record['version'] + '.crate', raw)
        observation = extract_checked(raw, record, writes)
        writes.new('registry/index/' + REGISTRY + '/.cache/' + index_suffix(record['name']), cache)
        observations.append(dict(observation, name=record['name'], version=record['version'], sha256=record['sha256'], selected_index_sha256=selected))
    writes.new('installation-receipt.json', json.dumps({'status': 'VERIFIED_NEW_LOCAL_CACHE_ONLY_NOT_RESOLVED_OR_AUTHORIZED', 'objects': observations, 'logical_bytes_before_receipt': writes.used, 'validator': 'LOCAL_OFFLINE_EMPTY_NOT_SERVER_OBSERVED'}, sort_keys=True).encode())
    return observations


def install_verified(plan_raw, fetch_ready_raw, bundles, root):
    """Prepared fixed-plan gate. CLI cannot reach it; no current effect grant."""
    require(isinstance(plan_raw, bytes) and sha(plan_raw) == PLAN_SHA, 'fixed_plan')
    require(isinstance(fetch_ready_raw, bytes) and sha(fetch_ready_raw) == FETCH_READY_SHA, 'fixed_fetch_source_ready')
    plan = parsed(plan_raw)
    require(1 <= len(bundles) <= 25, 'bundle_count')
    records, ordinals, names = [], set(), set()
    for bundle in bundles:
        ordinal = bundle['ordinal']
        require(type(ordinal) is int and 0 <= ordinal < 25 and ordinal not in ordinals, 'fixed_ordinal')
        ordinals.add(ordinal)
        record = plan['registry_payloads'][ordinal]
        require(record['name'] not in names, 'single_index_per_name')
        names.add(record['name'])
        receipt_raw, raw, index = bundle['receipt_raw'], bundle['crate_bytes'], bundle['index_bytes']
        require(len(receipt_raw) <= 16384 and sha(receipt_raw) == bundle['receipt_sha256'], 'verified_receipt_identity')
        receipt = parsed(receipt_raw)
        require(receipt['status'] == 'VERIFIED_STAGING_ONLY_NOT_CARGO_INSTALLATION' and receipt['plan_sha256'] == PLAN_SHA, 'verified_receipt_scope')
        require(receipt['name'] == record['name'] and receipt['version'] == record['version'] and receipt['sha256'] == record['sha256'] and receipt['bytes'] == len(raw), 'verified_record')
        require(len(raw) <= MAX_ARCHIVE and sha(raw) == record['sha256'] and sha(index) == receipt['index_raw_sha256'], 'verified_bytes')
        _, selected = sparse_cache(index, record)
        require(selected == receipt['selected_index_sha256'], 'verified_selected_index')
        records.append((record, raw, index))
    return _install_records(records, root)


def main():
    print('{"status":"HARD_DISABLED_PENDING_NEW_AUTHORIZATION","cache_install":"NOT_RUN","dependency_resolution":"NOT_PROVEN","Git":"NOT_IMPLEMENTED"}')
    return 78


if __name__ == '__main__':
    raise SystemExit(main())
