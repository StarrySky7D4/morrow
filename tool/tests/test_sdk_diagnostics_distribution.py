"""Bounded source ZIP tests; verification never executes archive code."""
import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import warnings
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tool'))
import package_sdk_diagnostics as distribution


class DiagnosticsDistributionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / 'source'
        (self.source / 'tool').mkdir(parents=True)
        for name in distribution.SOURCES.values():
            shutil.copyfile(ROOT / name, self.source / name)
        self.archive = self.root / 'diagnostics.zip'
        self.bundle = self.root / 'extracted'
        self.bundle.mkdir()

    def create(self):
        return distribution.create_archive(self.source, self.archive)

    def extract_verified(self):
        distribution.verify_zip(self.archive)
        with zipfile.ZipFile(self.archive) as archive:
            for name in archive.namelist():
                (self.bundle / name).write_bytes(archive.read(name))

    def rewrite_zip(self, change):
        self.create()
        with zipfile.ZipFile(self.archive) as archive:
            members = [(entry, archive.read(entry)) for entry in archive.infolist()]
        self.archive.unlink()
        with warnings.catch_warnings(), zipfile.ZipFile(self.archive, 'w') as archive:
            warnings.simplefilter('ignore', UserWarning)
            for entry, content in change(members):
                archive.writestr(entry, content)

    def test_export_has_only_fixed_six_files_and_original_byte_identity(self):
        receipt = self.create()
        manifest = distribution.verify_zip(self.archive)
        self.assertEqual(receipt['files'], 6)
        self.assertEqual(receipt['sha256'], distribution.sha(self.archive.read_bytes()))
        self.assertEqual(set(manifest['files']), distribution.PAYLOAD)
        with zipfile.ZipFile(self.archive) as archive:
            self.assertEqual(set(archive.namelist()), distribution.PAYLOAD | {distribution.MANIFEST})
            for name, source in distribution.SOURCES.items():
                self.assertEqual(archive.read(name), (ROOT / source).read_bytes())
        self.extract_verified()
        self.assertEqual(distribution.verify_directory(self.bundle), manifest)
        for absent in ('core', 'sdk', 'plugin_runtime', 'package_sdk_diagnostics.py', 'morrow_plugin.py'):
            self.assertFalse((self.bundle / absent).exists())

    def test_same_sources_make_deterministic_archive(self):
        first = self.create()
        second = distribution.create_archive(self.source, self.root / 'second.zip')
        self.assertEqual(first, second)
        self.assertEqual(self.archive.read_bytes(), (self.root / 'second.zip').read_bytes())

    def test_missing_source_failure_creates_no_success_or_temp_artifact(self):
        (self.source / 'tool/sdk_profiles.py').unlink()
        with self.assertRaises(OSError):
            self.create()
        self.assertFalse(self.archive.exists())
        self.assertEqual(list(self.root.glob('.morrow-diagnostics-*')), [])

    def test_existing_output_and_output_inside_source_are_not_overwritten(self):
        self.archive.write_bytes(b'keep this')
        with self.assertRaisesRegex(distribution.DistributionError, 'existing output'):
            self.create()
        self.assertEqual(self.archive.read_bytes(), b'keep this')
        with self.assertRaisesRegex(distribution.DistributionError, 'outside source'):
            distribution.create_archive(self.source, self.source / 'archive.zip')
        for source, output in ((self.source / '..' / 'source', self.source / 'inside.zip'),
                               (self.source, self.bundle / '..' / 'source' / 'inside.zip')):
            with self.subTest(source=source, output=output), self.assertRaisesRegex(distribution.DistributionError, 'outside source'):
                distribution.create_archive(source, output)
        self.assertFalse((self.source / 'inside.zip').exists())

    def test_source_and_output_ancestor_links_and_nonregular_inputs_are_refused(self):
        selected = self.source / 'tool/sdk_profiles.py'
        saved = selected.read_bytes()
        selected.unlink()
        selected.symlink_to(ROOT / 'tool/sdk_profiles.py')
        with self.assertRaises(distribution.DistributionError):
            self.create()
        selected.unlink()
        selected.write_bytes(saved)
        link = self.root / 'linked-root'
        link.symlink_to(self.source, target_is_directory=True)
        with self.assertRaises(distribution.DistributionError):
            distribution.create_archive(link, self.archive)
        output = self.root / 'linked-output'
        output.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(distribution.DistributionError):
            distribution.create_archive(self.source, output / 'archive.zip')
        selected.unlink()
        if hasattr(os, 'mkfifo'):
            os.mkfifo(selected)
            with self.assertRaises(distribution.DistributionError):
                self.create()

    def test_file_and_total_limits_are_enforced_before_publication(self):
        selected = self.source / 'tool/sdk_profiles.py'
        with selected.open('wb') as stream:
            stream.truncate(distribution.MAX_FILE_BYTES + 1)
        with self.assertRaisesRegex(distribution.DistributionError, 'size limit'):
            self.create()
        self.assertFalse(self.archive.exists())
        shutil.copyfile(ROOT / 'tool/sdk_profiles.py', selected)
        with mock.patch.object(distribution, 'MAX_TOTAL_BYTES', 4096), self.assertRaises(distribution.DistributionError):
            self.create()
        self.assertFalse(self.archive.exists())

    def test_source_byte_or_same_byte_identity_change_during_packaging_aborts(self):
        original_verify = distribution.verify_zip
        for identical in (False, True):
            def mutate(path):
                result = original_verify(path)
                selected = self.source / 'tool/sdk_profiles.py'
                replacement = self.source / 'tool/replacement'
                replacement.write_bytes(selected.read_bytes() + (b'' if identical else b'\n# changed\n'))
                replacement.replace(selected)
                return result
            with self.subTest(identical=identical), mock.patch.object(distribution, 'verify_zip', side_effect=mutate), \
                    self.assertRaisesRegex(distribution.DistributionError, 'changed while packaging'):
                self.create()
            self.assertFalse(self.archive.exists())
            self.assertEqual(list(self.root.glob('.morrow-diagnostics-*')), [])

    def test_verification_failure_never_publishes(self):
        with mock.patch.object(distribution, 'verify_zip', side_effect=distribution.DistributionError('injected verify failure')), \
                self.assertRaisesRegex(distribution.DistributionError, 'injected'):
            self.create()
        self.assertFalse(self.archive.exists())
        self.assertEqual(list(self.root.glob('.morrow-diagnostics-*')), [])

    def test_publication_race_does_not_overwrite_other_output(self):
        original_link = os.link
        def racing_link(source, output):
            output.write_bytes(b'other writer')
            return original_link(source, output)
        with mock.patch.object(distribution.os, 'link', side_effect=racing_link), self.assertRaises(FileExistsError):
            self.create()
        self.assertEqual(self.archive.read_bytes(), b'other writer')
        self.assertEqual(list(self.root.glob('.morrow-diagnostics-*')), [])

    def test_cleanup_failure_after_publication_does_not_delete_destination(self):
        original_unlink = Path.unlink
        def fail_temporary(path, *args, **kwargs):
            if path.name.startswith('.morrow-diagnostics-'):
                raise PermissionError('injected temporary cleanup failure')
            return original_unlink(path, *args, **kwargs)
        with mock.patch.object(Path, 'unlink', fail_temporary), contextlib.redirect_stderr(io.StringIO()) as stderr:
            receipt = self.create()
        self.assertEqual(distribution.sha(self.archive.read_bytes()), receipt['sha256'])
        distribution.verify_zip(self.archive)
        self.assertIn('archive published; temporary cleanup failed', stderr.getvalue())

    def test_raw_nul_filename_is_rejected_before_normalized_closure(self):
        self.create()
        # Produce a same-length raw ZIP name that ZipInfo normalizes to LICENSE.
        with zipfile.ZipFile(self.archive) as archive:
            members = [(entry, archive.read(entry)) for entry in archive.infolist()]
        self.archive.unlink()
        with zipfile.ZipFile(self.archive, 'w') as archive:
            for entry, content in members:
                if entry.filename == 'LICENSE': entry.filename = 'LICENSE_x'
                archive.writestr(entry, content)
        self.archive.write_bytes(self.archive.read_bytes().replace(b'LICENSE_x', b'LICENSE\x00x'))
        with self.assertRaisesRegex(distribution.DistributionError, 'normalized or truncated'):
            distribution.verify_zip(self.archive)

    def test_archive_path_count_type_size_and_hash_rejections(self):
        def renamed(members, name):
            members[0][0].filename = name
            return members
        for name in ('../escape', '/absolute', 'C:/drive', 'tool\\x', 'CON', 'a/../b'):
            self.rewrite_zip(lambda members: renamed(members, name))
            with self.subTest(name=name), self.assertRaises(distribution.DistributionError):
                distribution.verify_zip(self.archive)
            self.archive.unlink()
        for operation in ('extra', 'missing', 'duplicate', 'casefold', 'symlink', 'fifo', 'directory', 'oversize', 'changed'):
            def change(members):
                if operation == 'extra': return members + [(zipfile.ZipInfo('extra'), b'x')]
                if operation == 'missing': return members[1:]
                if operation == 'duplicate': return [members[1], *members[1:]]
                if operation == 'casefold':
                    members[0][0].filename = members[1][0].filename.swapcase()
                elif operation in ('symlink', 'fifo', 'directory'):
                    mode = {'symlink': stat.S_IFLNK, 'fifo': stat.S_IFIFO, 'directory': stat.S_IFDIR}[operation]
                    members[0][0].external_attr = (mode | 0o644) << 16
                elif operation == 'oversize': members[0] = (members[0][0], b'x' * (distribution.MAX_FILE_BYTES + 1))
                elif operation == 'changed': members[0] = (members[0][0], b'wrong')
                return members
            self.rewrite_zip(change)
            with self.subTest(operation=operation), self.assertRaises(distribution.DistributionError):
                distribution.verify_zip(self.archive)
            self.archive.unlink()

    def test_manifest_strict_fields_and_limits(self):
        self.create()
        with zipfile.ZipFile(self.archive) as archive:
            original = archive.read(distribution.MANIFEST)
        self.assertEqual(distribution.parse_manifest(original)['scope'], 'local-byte-identity-only')
        for operation in ('schema_bool', 'extra', 'missing', 'bad_hash', 'size_bool', 'size_big', 'scope'):
            value = json.loads(original)
            if operation == 'schema_bool': value['schema'] = True
            elif operation == 'extra': value['files']['extra'] = value['files']['LICENSE']
            elif operation == 'missing': del value['files']['LICENSE']
            elif operation == 'bad_hash': value['files']['LICENSE']['sha256'] = 'F' * 64
            elif operation == 'size_bool': value['files']['LICENSE']['size'] = True
            elif operation == 'size_big': value['files']['LICENSE']['size'] = distribution.MAX_FILE_BYTES + 1
            elif operation == 'scope': value['scope'] = 'signed'
            with self.subTest(operation=operation), self.assertRaises(distribution.DistributionError):
                distribution.parse_manifest(json.dumps(value).encode())
        with self.assertRaises(distribution.DistributionError):
            distribution.parse_manifest(original.replace(b'"schema": 1', b'"schema": 1, "schema": 1'))
        with self.assertRaises(distribution.DistributionError):
            distribution.parse_manifest(b' ' * (distribution.MAX_MANIFEST_BYTES + 1))
        with self.assertRaises(distribution.DistributionError):
            distribution.parse_manifest(b'[' * 2000 + b'0' + b']' * 2000)

    def test_directory_changed_extra_and_link_are_refused(self):
        self.create()
        self.extract_verified()
        selected = self.bundle / 'NOTICE'
        saved = selected.read_bytes()
        selected.write_bytes(b'changed')
        with self.assertRaises(distribution.DistributionError):
            distribution.verify_directory(self.bundle)
        selected.write_bytes(saved)
        extra = self.bundle / '__pycache__'
        extra.mkdir()
        with self.assertRaises(distribution.DistributionError):
            distribution.verify_directory(self.bundle)
        extra.rmdir()
        selected.unlink()
        selected.symlink_to(ROOT / 'NOTICE')
        with self.assertRaises(distribution.DistributionError):
            distribution.verify_directory(self.bundle)

    def test_verifier_never_executes_archive_and_verification_is_not_a_signature(self):
        # Self-rehashed arbitrary content may match an unsigned inventory. The
        # verifier neither executes that content nor claims semantic correctness.
        marker = self.root / 'must-not-exist'
        payload = ('from pathlib import Path\nPath(' + repr(str(marker)) + ').write_text("executed")\n').encode()
        (self.source / 'tool/morrow_sdk_diagnostics.py').write_bytes(payload)
        self.create()
        distribution.verify_zip(self.archive)
        self.extract_verified()
        distribution.verify_directory(self.bundle)
        self.assertFalse(marker.exists())

    def test_extracted_cli_help_works_isolated_without_repository(self):
        self.create()
        self.extract_verified()
        cwd, home, temporary = (self.root / name for name in ('cwd', 'home', 'tmp'))
        for path in (cwd, home, temporary): path.mkdir()
        env = {k: v for k, v in os.environ.items() if k not in ('PYTHONPATH', 'PYTHONHOME')}
        env.update(PATH='', HOME=str(home), TMPDIR=str(temporary))
        result = subprocess.run([sys.executable, '-I', '-B', str(self.bundle / 'morrow_sdk_diagnostics.py'), '--help'],
                                cwd=cwd, env=env, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(b'{profiles,preflight}', result.stdout)
        distribution.verify_directory(self.bundle)

    def test_cli_bad_archive_exit_one_empty_stdout_and_no_artifact(self):
        self.archive.write_bytes(b'not a zip')
        with contextlib.redirect_stdout(io.StringIO()) as stdout, contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertEqual(distribution.main(['verify-zip', str(self.archive)]), 1)
        self.assertEqual(stdout.getvalue(), '')
        self.assertTrue(stderr.getvalue().startswith('ERROR:'))


if __name__ == '__main__':
    unittest.main()
