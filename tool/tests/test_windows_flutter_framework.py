"""Drift and output-boundary checks for the isolated framework preparer."""
import copy
import difflib
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from prepare_windows_flutter_framework import TARGET, relocate_config, strict_patch, validate_graph
from windows_qualification_build_cache import CACHE_ROOT, _plain_path


class FrameworkPreparationTests(unittest.TestCase):
    def setUp(self):
        self.original = b'alpha\nbeta\ngamma\ndelta\n'
        self.changed = b'alpha\nreplacement\ngamma\ninserted\ndelta\n'
        self.patch = ''.join(difflib.unified_diff(
            self.original.decode().splitlines(True),
            self.changed.decode().splitlines(True),
            fromfile='a/' + TARGET, tofile='b/' + TARGET,
        )).encode()

    def test_exact_patch_accepts_original_and_preserves_content(self):
        self.assertEqual(strict_patch(self.original, self.patch), self.changed)
        self.assertEqual(strict_patch(self.original.replace(b'\n', b'\r\n'), self.patch), self.changed)

    def test_source_drift_is_never_fuzzily_patched(self):
        with self.assertRaises(ValueError):
            strict_patch(self.original.replace(b'beta', b'other'), self.patch)

    def test_patch_cannot_target_a_second_file(self):
        with self.assertRaises(ValueError):
            strict_patch(self.original, self.patch.replace(b'a/' + TARGET.encode(), b'a/other.dart'))
        with self.assertRaises(ValueError):
            strict_patch(self.original, self.patch + b'--- a/other.dart\n+++ b/other.dart\n')

    def test_patch_count_and_output_position_corruption_fail(self):
        for patch in (self.patch.replace(b'-1,4', b'-1,3'), self.patch.replace(b'+1,5', b'+2,5')):
            with self.assertRaises(ValueError):
                strict_patch(self.original, patch)

    def test_config_preserves_relative_project_identity(self):
        value = {'configVersion': 2, 'packages': [
            {'name': 'flutter', 'rootUri': 'file:///C:/flutter/packages/flutter', 'languageVersion': '3.10'},
            {'name': 'morrow', 'rootUri': '../', 'packageUri': 'lib/'},
        ]}
        original = copy.deepcopy(value)
        out = relocate_config(value, Path('D:/build_cache/example/.dart_tool/package_config.json'), Path('D:/build_cache/fw/framework'))
        self.assertEqual(value, original)
        self.assertEqual(out['packages'][0]['rootUri'], 'file:///D:/build_cache/fw/framework/')
        self.assertEqual(out['packages'][1]['rootUri'], 'file:///D:/build_cache/example/')
        self.assertEqual(out['packages'][1]['packageUri'], 'lib/')

    def test_duplicate_flutter_and_remote_package_roots_fail(self):
        config = {'configVersion': 2, 'packages': [{'name': 'flutter', 'rootUri': 'file:///C:/flutter/'}]}
        config['packages'].append(dict(config['packages'][0]))
        with self.assertRaises(ValueError):
            relocate_config(config, Path('D:/build_cache/p.json'), Path('D:/build_cache/fw'))
        config['packages'] = [{'name': 'flutter', 'rootUri': 'https://example.invalid/framework'}]
        with self.assertRaises(ValueError):
            relocate_config(config, Path('D:/build_cache/p.json'), Path('D:/build_cache/fw'))

    def test_cache_boundary_refuses_c_drive_and_traversal(self):
        for raw in ('C:/build_cache/fw', 'D:/build_cache', 'D:/build_cache/../fw', 'D:/unrelated/fw'):
            with self.assertRaises(ValueError):
                _plain_path(Path(raw), CACHE_ROOT)

    def test_graph_is_validated_without_changing_its_dependencies(self):
        config = {'packages': [{'name': 'app'}, {'name': 'flutter'}]}
        graph = {'configVersion': 1, 'roots': ['app'], 'packages': [
            {'name': 'app', 'dependencies': ['flutter'], 'devDependencies': []},
            {'name': 'flutter', 'dependencies': []},
        ]}
        original = copy.deepcopy(graph)
        validate_graph(graph, config)
        self.assertEqual(graph, original)

    def test_stale_or_malformed_graph_never_matches_the_config(self):
        config = {'packages': [{'name': 'app'}, {'name': 'flutter'}]}
        good = {'configVersion': 1, 'roots': ['app'], 'packages': [
            {'name': 'app', 'dependencies': ['flutter']}, {'name': 'flutter'},
        ]}
        cases = []
        stale = copy.deepcopy(good); stale['packages'].pop(); cases.append(stale)
        duplicate = copy.deepcopy(good); duplicate['packages'].append({'name': 'app'}); cases.append(duplicate)
        wrong_root = copy.deepcopy(good); wrong_root['roots'] = ['missing']; cases.append(wrong_root)
        wrong_edge = copy.deepcopy(good); wrong_edge['packages'][0]['dependencies'] = ['missing']; cases.append(wrong_edge)
        wrong_type = copy.deepcopy(good); wrong_type['packages'][0]['dependencies'] = 'flutter'; cases.append(wrong_type)
        bad_version = copy.deepcopy(good); bad_version['configVersion'] = 2; cases.append(bad_version)
        for case in cases:
            with self.subTest(case=case), self.assertRaises(ValueError):
                validate_graph(case, config)


if __name__ == '__main__':
    unittest.main()
