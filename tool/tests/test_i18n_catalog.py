import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

TOOL = Path(__file__).resolve().parents[1] / 'build_i18n.py'
spec = importlib.util.spec_from_file_location('build_i18n_under_test', TOOL)
i18n = importlib.util.module_from_spec(spec)
spec.loader.exec_module(i18n)

class I18nCatalogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.parts = self.root / 'l10n/parts'
        self.parts.mkdir(parents=True)
        self.en = {'@@locale': 'en', 'mainHello': 'Hello {name}', '@mainHello': {'placeholders': {'name': {'type': 'String'}}}}
        self.zh = {'@@locale': 'zh', 'mainHello': '你好 {name}', '@mainHello': copy.deepcopy(self.en['@mainHello'])}
        self.write()
    def write(self):
        for locale in i18n.LOCALES:
            data = copy.deepcopy(self.zh if locale == 'zh' else self.en)
            data['@@locale'] = locale
            (self.parts / f'main.{locale}.arb').write_text(json.dumps(data, ensure_ascii=False), encoding='utf-8')
    def test_real_cli_roundtrip_is_deterministic_and_stale_bytes_fail(self):
        result = subprocess.run([sys.executable, str(TOOL), '--root', str(self.root)], capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        first = i18n.outputs(self.root)
        self.assertEqual(i18n.build(self.root, check=True), 2 * len(i18n.LOCALES) + 1)
        self.assertEqual(first, i18n.outputs(self.root))
        binary = self.root / 'packages/morrow_i18n/assets/languages/en.mlang'
        self.assertTrue(binary.read_bytes().startswith(b'MROWLNG1'))
        binary.write_bytes(binary.read_bytes() + b'x')
        bad = subprocess.run([sys.executable, str(TOOL), '--root', str(self.root), '--check'], capture_output=True)
        self.assertEqual(bad.returncode, 1)
        self.assertIn(b'stale generated artifact', bad.stderr)
    def test_missing_extra_key_and_missing_locale_reject_before_publication(self):
        for mode in ['missing', 'extra', 'locale']:
            with self.subTest(mode=mode):
                self.write()
                if mode == 'locale':
                    (self.parts / 'main.zh.arb').unlink()
                else:
                    data = copy.deepcopy(self.zh)
                    if mode == 'missing':
                        del data['mainHello']; del data['@mainHello']
                    else:
                        data['mainExtra'] = 'extra'
                    (self.parts / 'main.zh.arb').write_text(json.dumps(data), encoding='utf-8')
                with self.assertRaises(i18n.CatalogError):
                    i18n.build(self.root)
                self.assertFalse((self.root / 'packages/morrow_i18n/assets').exists())
    def test_duplicate_json_key_duplicate_fragment_and_namespace_rejected(self):
        path = self.parts / 'main.en.arb'
        path.write_text('{"@@locale":"en","mainHello":"a","mainHello":"b"}')
        with self.assertRaisesRegex(i18n.CatalogError, 'duplicate JSON'):
            i18n.merge(self.root)
        self.write()
        duplicate = self.root/'packages/morrow_i18n/l10n/parts'
        duplicate.mkdir(parents=True)
        (duplicate/'main.en.arb').write_bytes(path.read_bytes())
        with self.assertRaisesRegex(i18n.CatalogError, 'duplicate fragment'):
            i18n.merge(self.root)
        (duplicate/'main.en.arb').unlink()
        self.en['pluginsWrong'] = 'wrong'
        self.write()
        with self.assertRaisesRegex(i18n.CatalogError, 'prefix'):
            i18n.merge(self.root)
    def test_typed_placeholders_and_metadata_must_match(self):
        for spec in [{'type': 'int'}, {'type': 'String', 'format': 'invalid'}, {'type': True}, {}]:
            with self.subTest(spec=spec):
                self.zh['@mainHello']['placeholders']['name'] = spec
                self.write()
                with self.assertRaises(i18n.CatalogError):
                    i18n.merge(self.root)
        self.zh['@mainHello']['placeholders'] = {}
        self.write()
        with self.assertRaisesRegex(i18n.CatalogError, 'placeholder contract mismatch'):
            i18n.merge(self.root)
    def test_size_limits_and_nonstring_messages(self):
        for value in ['x' * (i18n.MAX_TEXT + 1), 1, None]:
            with self.subTest(value_type=type(value).__name__):
                self.en['mainHello'] = value
                self.write()
                with self.assertRaises(i18n.CatalogError):
                    i18n.merge(self.root)
    def test_translation_and_metadata_both_change_compiled_pins(self):
        before = i18n.outputs(self.root)
        self.zh['mainHello'] = '欢迎 {name}'
        self.write()
        after = i18n.outputs(self.root)
        pin = self.root / 'packages/morrow_i18n/lib/src/generated/language_pins.dart'
        self.assertNotEqual(before[pin], after[pin])
        self.zh['@mainHello']['description'] = 'Updated source metadata'
        self.write()
        self.assertNotEqual(after[pin], i18n.outputs(self.root)[pin])

if __name__ == '__main__':
    unittest.main()
