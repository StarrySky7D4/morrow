"""Synthetic archive/cache primitives only. No Cargo/network calls here."""
import gzip
import hashlib
import io
import json
import pathlib
import tarfile
import unittest
from unittest.mock import patch
import install_registry as i


def toy(extra=None):
    payload = io.BytesIO()
    with tarfile.open(fileobj=payload, mode='w:gz') as archive:
        for name, data in [('Cargo.toml', b'[package]\nname="morrow-cache-toy"\nversion="1.0.0"\nedition="2021"\n'), ('src/lib.rs', b'pub fn toy() -> u8 { 1 }\n')]:
            member = tarfile.TarInfo('morrow-cache-toy-1.0.0/' + name)
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
        if extra is not None:
            archive.addfile(extra, io.BytesIO(b'x' * extra.size) if extra.isfile() else None)
    raw = payload.getvalue()
    record = {'name': 'morrow-cache-toy', 'version': '1.0.0', 'sha256': i.sha(raw)}
    index = json.dumps(dict(name=record['name'], vers=record['version'], cksum=record['sha256'], deps=[], features={}, yanked=False)).encode() + b'\n'
    return record, raw, index


class Offline(unittest.TestCase):
    def root(self):
        return i.ROOT / ('synthetic-' + self._testMethodName)

    def test_layout_checksum_and_real_source_hashes(self):
        record, raw, index = toy()
        i._install_records([(record, raw, index)], self.root())
        base = self.root() / 'registry/src' / i.REGISTRY / 'morrow-cache-toy-1.0.0'
        checksum = json.loads((base / '.cargo-checksum.json').read_bytes())
        self.assertEqual(checksum['package'], i.sha(raw))
        self.assertEqual(set(checksum['files']), {'Cargo.toml', 'src/lib.rs'})
        for name, digest in checksum['files'].items():
            self.assertEqual(i.sha((base / name).read_bytes()), digest)
        self.assertEqual((base / '.cargo-ok').read_bytes(), b'{"v":1}')
        cache = self.root() / 'registry/cache' / i.REGISTRY / 'morrow-cache-toy-1.0.0.crate'
        self.assertEqual(cache.read_bytes(), raw)

    def test_sparse_cache_preserves_exact_json_and_local_empty_validator(self):
        record, _, index = toy()
        cache, selected = i.sparse_cache(index, record)
        self.assertEqual(cache[:6], b'\x03\x02\0\0\0\0')
        self.assertEqual(cache[6:].split(b'\0'), [b'1.0.0', index.rstrip(b'\n'), b''])
        self.assertEqual(selected, i.sha(index))

    def test_existing_cache_root_is_never_adopted(self):
        self.root().mkdir()
        (self.root() / 'sentinel').write_bytes(b'old')
        with self.assertRaisesRegex(i.Rejected, 'fresh_exclusive'):
            i._install_records([toy()], self.root())
        self.assertEqual((self.root() / 'sentinel').read_bytes(), b'old')

    def test_unsafe_components(self):
        for value in ['../x', '/absolute', 'C:/x', 'a\\b', 'nul.rs', 'a./x', 'a:ads', 'a/../b']:
            with self.subTest(value=value), self.assertRaises(i.Rejected):
                i.components(value)

    def test_link_and_generated_checksum_collision_rejected(self):
        for name, kind in [('link', tarfile.SYMTYPE), ('.cargo-checksum.json', tarfile.REGTYPE)]:
            member = tarfile.TarInfo('morrow-cache-toy-1.0.0/' + name)
            member.type, member.linkname = kind, '../outside'
            with self.subTest(name=name), self.assertRaises(i.Rejected):
                root = i.ROOT / ('synthetic-link-' + name.replace('.', '_'))
                i._install_records([toy(member)], root)

    def test_casefold_duplicate_rejected_and_partial_preserved(self):
        member = tarfile.TarInfo('morrow-cache-toy-1.0.0/SRC/LIB.RS')
        member.size = 1
        with self.assertRaisesRegex(i.Rejected, 'duplicate_casefold'):
            i._install_records([toy(member)], self.root())
        self.assertTrue((self.root() / 'registry/src' / i.REGISTRY / 'morrow-cache-toy-1.0.0/src/lib.rs').is_file())
        self.assertFalse((self.root() / 'installation-receipt.json').exists())

    def test_bad_crc_rejected_before_cargo_completion_marker(self):
        record, raw, index = toy()
        bad = bytearray(raw); bad[-8] ^= 1; raw = bytes(bad)
        record['sha256'] = i.sha(raw)
        index = index.replace(json.loads(index)['cksum'].encode(), record['sha256'].encode())
        with self.assertRaises(gzip.BadGzipFile):
            i._install_records([(record, raw, index)], self.root())
        self.assertFalse((self.root() / 'registry/src' / i.REGISTRY / 'morrow-cache-toy-1.0.0/.cargo-ok').exists())

    def test_nonzero_concatenated_tail_rejected(self):
        record, raw, index = toy(); raw += gzip.compress(b'injected')
        prior = record['sha256']; record['sha256'] = i.sha(raw); index = index.replace(prior.encode(), record['sha256'].encode())
        with self.assertRaisesRegex(i.Rejected, 'nonzero_tail'):
            i._install_records([(record, raw, index)], self.root())

    def test_index_wrong_checksum_yanked_or_duplicate_rejected(self):
        record, _, index = toy()
        for bad in [index.replace(record['sha256'].encode(), b'0' * 64), index.replace(b'false', b'true'), index + index]:
            with self.assertRaises(i.Rejected):
                i.sparse_cache(bad, record)

    def test_reparse_rejected_without_real_link(self):
        class Info:
            st_mode, st_file_attributes = 0o040755, 1024
        with patch.object(pathlib.Path, 'lstat', return_value=Info()), self.assertRaisesRegex(i.Rejected, 'reparse'):
            i.plain(self.root())

    def test_decompressed_and_write_bound_reject(self):
        for guard in ['MAX_EXPANDED', 'MAX_DISK']:
            with patch.object(i, guard, 1), self.assertRaises(i.Rejected):
                i._install_records([toy()], i.ROOT / ('synthetic-limit-' + guard))

    def test_fixed_plan_gate_rejects_synthetic_authority_and_cli78(self):
        with self.assertRaisesRegex(i.Rejected, 'fixed_plan'):
            i.install_verified(b'synthetic-not-authorized-plan', b'fake-ready', [], self.root())
        with patch.object(i, '_install_records', side_effect=AssertionError('must not install')):
            self.assertEqual(i.main(), 78)


if __name__ == '__main__':
    unittest.main(verbosity=2)
