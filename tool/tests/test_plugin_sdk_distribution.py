"""Source/ZIP/metadata tests only; no compiler, SDK build, guest or host execution."""
import contextlib
import hashlib
import io
import json
import re
import os
from pathlib import Path
import shutil
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import zipfile

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / 'tool'))
import morrow_plugin as tool
import package_plugin_sdk as distribution


class SdkDistributionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        source = Path(os.environ.get('MORROW_DISTRIBUTION_TEST_ROOT', str(REPO)))
        cls.temporary = tempfile.TemporaryDirectory(prefix='morrow-sdk-distribution-', dir=os.environ.get('MORROW_DISTRIBUTION_TEST_TEMP'))
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(cls.temporary.name)
        cls.source = source
        cls.archive = cls.root / 'source.zip'
        cls.receipt = distribution.create_archive(source, cls.archive)
        cls.bundle = cls.root / 'bundle'
        cls.bundle.mkdir()
        with zipfile.ZipFile(cls.archive) as zipped:
            for entry in zipped.infolist():
                path = cls.bundle / entry.filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(zipped.read(entry))
        cls.original_manifest = (cls.bundle / distribution.MANIFEST).read_bytes()

    def rehash(self):
        names = distribution.directory_files(self.bundle) - {distribution.MANIFEST}
        files = {name: {'size': len((self.bundle/name).read_bytes()), 'sha256': hashlib.sha256((self.bundle/name).read_bytes()).hexdigest()} for name in names}
        value = {'schema': 1, 'profile': distribution.PROFILE, 'qualification': 'source-only-not-frozen', 'native_libraries_bundled': False, 'files': files}
        (self.bundle/distribution.MANIFEST).write_text(json.dumps(value), encoding='utf-8')

    def test_source_export_checks_single_core_authority_without_bundling_core(self):
        self.assertEqual(len(self.receipt['source_authority']['schemas']), 10)
        self.assertFalse(self.receipt['source_authority']['core_bundled'])
        self.assertEqual(self.receipt['source_authority']['semantics'], 'NOT_PROVED')
        self.assertFalse((self.bundle/'core').exists())
        self.assertFalse((self.bundle/'plugin_runtime').exists())
        self.assertNotIn('sdk/rust/tests', distribution.directory_files(self.bundle))

    def test_export_and_relocated_source_bindings_verify(self):
        manifest = distribution.verify_zip(self.archive)
        self.assertEqual(distribution.verify_distribution(self.bundle), manifest)
        report = distribution.validate_sdk_bindings(self.bundle)
        self.assertFalse(report['host_agreement'])
        self.assertEqual(report['semantics'], 'NOT_PROVED')
        self.assertEqual(tool.sdk_lock.inventory(self.bundle/'sdk'), tool.sdk_lock.inventory(self.source/'sdk'))

    def test_self_rehashed_missing_public_input_is_rejected(self):
        for name in ('sdk/c/include/morrow_plugin_task.h', 'sdk/cpp/include/morrow_plugin_ui.hpp', 'sdk/rust/src/lib.rs', 'sdk/rust/src/service_resources.rs', 'sdk/rust/src/channel/transport.rs'):
            path = self.bundle/name
            saved = path.read_bytes()
            try:
                path.unlink()
                self.rehash()
                with self.assertRaises((distribution.DistributionError, OSError)):
                    distribution.verify_distribution(self.bundle)
            finally:
                path.write_bytes(saved)
                (self.bundle/distribution.MANIFEST).write_bytes(self.original_manifest)

    def test_self_rehashed_versions_and_generated_binding_drift_are_rejected(self):
        for name, old, new in (
            ('sdk/c/include/morrow_plugin_io.h', '#define MP_IO_ABI_VERSION 1u', '#define MP_IO_ABI_VERSION 2u'),
            ('sdk/cpp/include/morrow_channel_v1.hpp', 'r.abi_version=1', 'r.abi_version=2'),
            ('sdk/rust/src/protocol.rs', 'pub const PROTOCOL_VERSION: u16 = contract::VERSION;', 'pub const PROTOCOL_VERSION: u16 = 9;'),
            ('sdk/rust/src/ui.rs', 'pub const VERSION: u16 = crate::contract::UI_VERSION;', 'pub const VERSION: u16 = 9;'),
            ('sdk/rust/src/mutation.rs', 'pub const VERSION: u16 = wire::VERSION;', 'pub const VERSION: u16 = 9;'),
            ('sdk/rust/build.rs', 'pub const TASK_VERSION:u16={task_version};', 'pub const TASK_VERSION:u16=9;')):
            path = self.bundle/name
            saved = path.read_bytes()
            try:
                self.assertIn(old, saved.decode('utf-8'))
                path.write_text(saved.decode('utf-8').replace(old, new), encoding='utf-8')
                self.rehash()
                with self.assertRaises(distribution.DistributionError):
                    distribution.verify_distribution(self.bundle)
            finally:
                path.write_bytes(saved)
                (self.bundle/distribution.MANIFEST).write_bytes(self.original_manifest)

    def test_sdk_only_host_commands_reject_before_mocked_children_and_output(self):
        for operation in ('pack', 'check', 'transform'):
            output = self.root / 'must-not-exist-output'
            args = [operation, str(self.root/'missing-project')] if operation=='pack' else [operation, str(self.root/'missing-package')] if operation=='check' else [operation, str(self.root/'missing-package'), 'handler', 'bytes', 'bytes', str(self.root/'missing-input'), str(output)]
            stdout, stderr = io.StringIO(), io.StringIO()
            with mock.patch.object(tool, 'compile_project', side_effect=AssertionError('compiler ran')), mock.patch.object(tool, 'run', side_effect=AssertionError('child ran')), mock.patch.object(tool, 'executable', side_effect=AssertionError('tool probed')), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                self.assertEqual(tool.main([*args, '--sdk-only']), 1)
            self.assertEqual(stdout.getvalue(), '')
            self.assertIn('before compilers', stderr.getvalue())
            self.assertFalse(output.exists())

    def test_default_host_gate_does_not_silently_select_sdk_only(self):
        with mock.patch.object(tool, 'ROOT', self.root/'missing-host'):
            with self.assertRaises(tool.ToolError):
                tool.require_host_commands(SimpleNamespace(command='check', sdk_only=False))

    def test_core_authority_change_before_publication_leaves_no_output(self):
        import copy
        first = distribution.validate_source_authority(self.source)
        second = copy.deepcopy(first)
        second['wire_versions']['io'] += 1
        output = self.root/'authority-change.zip'
        existing = set(self.root.iterdir())
        with mock.patch.object(distribution, 'validate_source_authority', side_effect=[first, second]) as authority:
            with self.assertRaisesRegex(distribution.DistributionError, 'Core authority changed'):
                distribution.create_archive(self.source, output)
            self.assertEqual(authority.call_count, 2)
        self.assertFalse(output.exists())
        self.assertEqual(set(self.root.iterdir()), existing)

    def test_existing_output_is_not_replaced(self):
        before = self.archive.read_bytes()
        with self.assertRaises(distribution.DistributionError):
            distribution.create_archive(self.source, self.archive)
        self.assertEqual(self.archive.read_bytes(), before)

    def test_paths_and_duplicate_manifest_keys_fail_closed(self):
        for name in ('../escape', '/absolute', 'C:/drive', 'a\\b', 'a//b', 'a/../b', 'CON', 'a./b', 'a/b ', 'a\x00b'):
            with self.assertRaises(distribution.DistributionError):
                distribution.safe_name(name)
        with self.assertRaises(distribution.DistributionError):
            distribution.parse_manifest(b'{"schema":1,"schema":1}')

    def test_reparse_attribute_rejected(self):
        info = SimpleNamespace(st_mode=stat.S_IFDIR | 0o755, st_file_attributes=0x400)
        with mock.patch.object(Path, 'lstat', return_value=info):
            with self.assertRaises(distribution.DistributionError):
                distribution.regular(self.bundle, True)

    def test_bounded_file_read_rejects_stat_size_before_open(self):
        with mock.patch.object(Path, 'open', side_effect=AssertionError('oversized file opened')):
            with self.assertRaises(distribution.DistributionError):
                distribution.checked_file(self.bundle, 'NOTICE', 1)

    def test_cumulative_payload_budget_checked_before_next_read(self):
        ordinary = self.root/'budget-fixture'
        ordinary.mkdir()
        (ordinary/'first').write_bytes(b'1234')
        (ordinary/'second').write_bytes(b'123456789')
        with mock.patch.object(distribution, 'selected_names', return_value=['first', 'second']), mock.patch.object(distribution, 'MAX_TOTAL_BYTES', len(distribution.README)+8):
            with self.assertRaises(distribution.DistributionError):
                distribution.snapshot(ordinary)

    def test_cargo_target_build_and_lib_escape_rejected_with_rehashed_manifest(self):
        path = self.bundle/'sdk/examples/rust-task/Cargo.toml'
        saved = path.read_bytes()
        text = saved.decode('utf-8')
        external = self.root/'external'
        external.mkdir()
        (external/'lib.rs').write_text('// unsupported compiler input', encoding='utf-8')
        # The fixture exists; this proves containment checks instead of missing-file failure.
        value = external.as_posix()
        variants = (text+'\n[target.\'cfg(windows)\'.dependencies]\nescape = {path = "'+value+'"}\n', text.replace('[package]', '[package]\nbuild = "'+value+'/lib.rs"'), text.replace('[lib]', '[lib]\npath = "'+value+'/lib.rs"'))
        try:
            for changed in variants:
                path.write_text(changed, encoding='utf-8')
                self.rehash()
                with self.assertRaises(distribution.DistributionError):
                    distribution.verify_distribution(self.bundle)
        finally:
            path.write_bytes(saved)
            (self.bundle/distribution.MANIFEST).write_bytes(self.original_manifest)

    def test_self_rehashed_patch_replace_workspace_and_explicit_target_rejected(self):
        path = self.bundle/'sdk/examples/rust-task/Cargo.toml'
        saved = path.read_bytes()
        text = saved.decode('utf-8')
        package_workspace = re.sub(r'\[workspace\]\s*resolver = "2"\s*', '', text).replace('[package]', '[package]\nworkspace = "../../../outside"')
        variants = (package_workspace, text+'\n[patch.crates-io]\nsha2 = {path = "../../../outside"}\n',
                    text+'\n[replace]\n"sha2:0.10.9" = {path = "../../../outside"}\n',
                    text.replace('[workspace]', '[workspace]\nmembers = ["../../../outside"]'),
                    text+'\n[[bin]]\nname = "outside"\npath = "../../../outside/lib.rs"\n',
                    text+'\n[[example]]\nname = "outside"\npath = "../../../outside/lib.rs"\n',
                    text+'\n[[test]]\nname = "outside"\npath = "../../../outside/lib.rs"\n',
                    text+'\n[[bench]]\nname = "outside"\npath = "../../../outside/lib.rs"\n')
        try:
            for changed in variants:
                path.write_text(changed, encoding='utf-8')
                self.rehash()
                with self.assertRaises(distribution.DistributionError):
                    distribution.verify_distribution(self.bundle)
        finally:
            path.write_bytes(saved)
            (self.bundle/distribution.MANIFEST).write_bytes(self.original_manifest)

    def test_zip_duplicate_traversal_symlink_and_hash_rejected(self):
        with zipfile.ZipFile(self.archive) as zipped:
            payload = {entry.filename: zipped.read(entry) for entry in zipped.infolist()}
        for index, kind in enumerate(('duplicate', 'traversal', 'symlink', 'hash')):
            path = self.root/f'bad-{index}.zip'
            with zipfile.ZipFile(path, 'w') as zipped:
                for name, data in payload.items():
                    zipped.writestr(name, b'changed' if kind=='hash' and name=='NOTICE' else data)
                if kind=='duplicate':
                    import warnings
                    with warnings.catch_warnings():
                        warnings.simplefilter('ignore')
                        zipped.writestr('NOTICE', payload['NOTICE'])
                elif kind=='traversal':
                    zipped.writestr('../escape', b'x')
                elif kind=='symlink':
                    item=zipfile.ZipInfo('sdk/link');item.create_system=3;item.external_attr=(stat.S_IFLNK|0o777)<<16
                    zipped.writestr(item,b'../escape')
            with self.assertRaises(distribution.DistributionError):
                distribution.verify_zip(path)


if __name__ == '__main__':
    unittest.main()
