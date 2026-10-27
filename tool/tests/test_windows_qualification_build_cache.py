"""No product/VM execution: cache guard checks with synthetic compiler records."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import windows_qualification_build_cache as cache


class ColdCacheChecks(unittest.TestCase):
    def test_invalid_identifiers_do_not_reserve_a_target(self):
        for run, role, digest in (('../old', 'S', 'a'*64),
                                  ('case', '../S', 'a'*64),
                                  ('case', 'S', 'short')):
            with self.assertRaises(ValueError):
                cache.reserve_target(run, role, digest)

    def test_c_drive_and_parent_traversal_are_rejected(self):
        for path in (Path('C:/cache/output'), Path('D:/build_cache/../escape')):
            with self.assertRaises(ValueError):
                cache._plain_path(path, cache.CACHE_ROOT)

    def test_case080_fresh_library_record_is_rejected(self):
        records = [{'reason': 'compiler-artifact', 'fresh': True},
                   {'reason': 'build-finished', 'success': True}]
        with patch.object(cache, '_plain_path', side_effect=lambda p, r: p):
            with self.assertRaisesRegex(ValueError, 'compile every artifact'):
                cache.verify_compiler_outputs(records, Path('D:/target'),
                                               Path('D:/input/Cargo.toml'), 'setup')

    def test_missing_or_duplicate_compiler_completion_is_rejected(self):
        for records in ([], [{'reason': 'build-finished', 'success': False}],
                        [{'reason': 'build-finished', 'success': True}]*2):
            with patch.object(cache, '_plain_path', side_effect=lambda p, r: p):
                with self.assertRaisesRegex(ValueError, 'compiler completion'):
                    cache.verify_compiler_outputs(records, Path('D:/target'),
                                                   Path('D:/input/Cargo.toml'), 'setup')


if __name__ == '__main__':
    unittest.main()
