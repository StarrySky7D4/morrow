#!/usr/bin/env python3
"""Compile authoritative ARB fragments to Flutter and pinned PB/LZ4 resources.

No ICU interpreter is implemented here: Flutter gen-l10n validates/compiles ICU.
The LZ4 writer emits a standards-compliant literal block (deterministic, no ratio
claim); runtime decoder also accepts bounded match sequences for codec validation.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess

MAX_RAW = 4 * 1024 * 1024
MAX_ENTRIES = 4096
MAX_TEXT = 64 * 1024

class CatalogError(ValueError):
    pass

def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise CatalogError(f'duplicate JSON key: {key}')
        result[key] = value
    return result

def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':')).encode('utf-8')

def language_config(root):
    path = root / 'packages/morrow_i18n/languages.json'
    if path.stat().st_size > 65536:
        raise CatalogError('language configuration too large')
    value = json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=object_pairs)
    if not isinstance(value, dict) or type(value.get('version')) is not int or value.get('version') != 1 or set(value) != {'version', 'sourceLocale', 'fallbackLocale', 'systemFallbackLocale', 'languages'}:
        raise CatalogError('invalid language configuration')
    languages = value['languages']
    if not isinstance(languages, list) or not 1 <= len(languages) <= 128:
        raise CatalogError('invalid language count')
    codes = set()
    for item in languages:
        if not isinstance(item, dict) or set(item) != {'code', 'nativeName'}:
            raise CatalogError('invalid language entry')
        code, name = item['code'], item['nativeName']
        if not isinstance(code, str) or not re.fullmatch(r'[a-z]{2,3}', code) or code in codes:
            raise CatalogError('invalid or duplicate language code')
        if not isinstance(name, str) or not name.strip() or len(name) > 80 or any(ord(c) < 32 for c in name):
            raise CatalogError('invalid language name')
        codes.add(code)
    if any(not isinstance(value[k], str) or value[k] not in codes for k in ('sourceLocale', 'fallbackLocale', 'systemFallbackLocale')):
        raise CatalogError('unknown fallback/source language')
    return value

def locale_codes(config):
    source = config['sourceLocale']
    return (source, *(v['code'] for v in config['languages'] if v['code'] != source))

def read_fragment(path, locale, prefix):
    if path.stat().st_size > MAX_RAW:
        raise CatalogError(f'fragment too large: {path.name}')
    data = json.loads(path.read_text(encoding='utf-8'), object_pairs_hook=object_pairs)
    if not isinstance(data, dict) or data.get('@@locale') != locale:
        raise CatalogError(f'invalid locale: {path.name}')
    for key, value in data.items():
        if key == '@@locale':
            continue
        if key.startswith('@@'):
            raise CatalogError(f'unsupported global metadata: {key}')
        message = key[1:] if key.startswith('@') else key
        if not re.fullmatch(re.escape(prefix) + r'[A-Z][A-Za-z0-9]*', message):
            raise CatalogError(f'expected {prefix} prefix: {key}')
        if key.startswith('@'):
            if message not in data or not isinstance(value, dict):
                raise CatalogError(f'orphan or invalid metadata: {key}')
            placeholders = value.get('placeholders', {})
            if not isinstance(placeholders, dict):
                raise CatalogError(f'invalid placeholders: {key}')
            for name, spec in placeholders.items():
                if not re.fullmatch(r'[a-zA-Z][a-zA-Z0-9_]*', name) or not isinstance(spec, dict):
                    raise CatalogError(f'invalid placeholder: {key}')
                if spec.get('type') not in ('String', 'int', 'double', 'num', 'DateTime'):
                    raise CatalogError(f'explicit placeholder type required: {key}.{name}')
        elif not isinstance(value, str) or len(value.encode('utf-8')) > MAX_TEXT:
            raise CatalogError(f'invalid message: {key}')
    return data

def merge(root):
    locales = locale_codes(language_config(root))
    sources = [root / 'packages/morrow_i18n/l10n/parts', root / 'l10n/parts']
    fragments = {}
    for directory in sources:
        for path in sorted(directory.glob('*.arb')):
            match = re.fullmatch(r'(common|main|visual|plugins|imports|recovery)\.(' + '|'.join(locales) + r')\.arb', path.name)
            if not match:
                raise CatalogError(f'unrecognized fragment: {path.name}')
            prefix, locale = match.groups()
            if (prefix, locale) in fragments:
                raise CatalogError(f'duplicate fragment: {path.name}')
            fragments[prefix, locale] = read_fragment(path, locale, prefix)
    if not fragments:
        raise CatalogError('no ARB fragments')
    output = {locale: {'@@locale': locale} for locale in locales}
    for prefix in sorted({p for p, _ in fragments}):
        if any((prefix, locale) not in fragments for locale in locales):
            raise CatalogError(f'missing locale for {prefix}')
        catalogs = [fragments[prefix, locale] for locale in locales]
        left = catalogs[0]
        keys = lambda data: {k for k in data if not k.startswith('@')}
        for locale, right in zip(locales[1:], catalogs[1:]):
            if keys(left) != keys(right):
                raise CatalogError(f'missing/extra message in {prefix}.{locale}: {keys(left) ^ keys(right)}')
            for key in keys(left):
                specs = [data.get('@' + key, {}).get('placeholders', {}) for data in (left, right)]
                if specs[0] != specs[1]:
                    raise CatalogError(f'placeholder contract mismatch: {key}')
        for locale, data in zip(locales, catalogs):
            for key, value in data.items():
                if key == '@@locale':
                    continue
                if key in output[locale]:
                    raise CatalogError(f'duplicate message: {key}')
                output[locale][key] = value
    if len([k for k in output[locales[0]] if not k.startswith('@')]) > MAX_ENTRIES:
        raise CatalogError('too many messages')
    return output

def varint(value):
    encoded = bytearray()
    while value > 127:
        encoded.append((value & 127) | 128)
        value >>= 7
    encoded.append(value)
    return bytes(encoded)

def field(number, value):
    return varint(number << 3 | 2) + varint(len(value)) + value

def encode_pack(locale, catalog):
    source = canonical(catalog)
    source_sha = hashlib.sha256(source).digest()
    raw = b'\x08\x01' + field(2, locale.encode()) + field(3, source_sha)
    keys = sorted(k for k in catalog if not k.startswith('@'))
    for key in keys:
        entry = field(1, key.encode()) + field(2, catalog[key].encode()) + field(3, canonical(catalog.get('@'+key, {})))
        raw += field(4, entry)
    if len(raw) > MAX_RAW:
        raise CatalogError('language pack too large')
    # LZ4 final literal sequence. No unbounded or environment-dependent compressor.
    n = len(raw)
    compressed = bytearray([min(n, 15) << 4])
    if n >= 15:
        n -= 15
        while n >= 255:
            compressed.append(255)
            n -= 255
        compressed.append(n)
    compressed.extend(raw)
    digest = hashlib.sha256(raw).digest()
    container = b'MROWLNG1' + struct.pack('<HII', 1, len(raw), len(compressed)) + digest + compressed
    return bytes(container), digest.hex(), source_sha.hex(), len(keys)

def outputs(root):
    package = root / 'packages/morrow_i18n'
    catalogs = merge(root)
    result = {}
    config = language_config(root)
    quote = lambda s: json.dumps(s, ensure_ascii=False).replace('$', r'\$')
    dart = ['// Generated by tool/build_i18n.py from languages.json. Do not edit.',
            'const uiLanguageNames = <String, String>{']
    dart += [f'  {quote(v["code"])}: {quote(v["nativeName"])},' for v in config['languages']]
    dart += ['};', f'const uiFallbackLocale = {quote(config["fallbackLocale"])};',
             f'const uiSystemFallbackLocale = {quote(config["systemFallbackLocale"])};',
             "bool isUiLocale(String value) => value == 'system' || uiLanguageNames.containsKey(value);"]
    result[package/'lib/locale_codes.dart'] = ('\n'.join(dart)+'\n').encode('utf-8')
    rust_codes = ', '.join(json.dumps(v['code']) for v in config['languages'])
    result[root/'workbench_host/src/generated_ui_locales.rs'] = (
        '// Generated by tool/build_i18n.py from languages.json. Do not edit.\n'
        f'const UI_LOCALES: &[&str] = &[{rust_codes}];\n').encode('utf-8')
    result[package/'l10n.yaml'] = (f'''# Generated by tool/build_i18n.py from languages.json. Do not edit.
arb-dir: lib/l10n
template-arb-file: app_{config['sourceLocale']}.arb
output-dir: lib/src/generated
output-localization-file: app_localizations.dart
output-class: AppLocalizations
nullable-getter: true
use-escaping: true
preferred-supported-locales: [{config['systemFallbackLocale']}]
''').encode('utf-8')
    pins = []
    for locale, catalog in catalogs.items():
        result[package / f'lib/l10n/app_{locale}.arb'] = json.dumps(catalog, ensure_ascii=False, sort_keys=True, indent=2).encode('utf-8') + b'\n'
        data, digest, source, count = encode_pack(locale, catalog)
        result[package / f'assets/languages/{locale}.mlang'] = data
        pins.append(f"  '{locale}': (rawSha: '{digest}', catalogSha: '{source}', count: {count}),")
    result[package/'lib/src/generated/language_pins.dart'] = ('// Generated by tool/build_i18n.py. Do not edit.\nconst languagePins = <String, ({String rawSha, String catalogSha, int count})>{\n'+'\n'.join(pins)+'\n};\n').encode()
    return result

def build(root, check=False):
    # Validate everything before publishing any derived file.
    artifacts = outputs(root)
    for path, data in artifacts.items():
        if check:
            if not path.is_file() or path.read_bytes() != data:
                raise CatalogError(f'stale generated artifact: {path.relative_to(root)}')
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    return len(artifacts)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--generate', action='store_true', help='also invoke official Flutter gen-l10n (run flutter pub get first)')
    args = parser.parse_args()
    if args.check and args.generate:
        parser.error('--check and --generate are mutually exclusive')
    try:
        count = build(args.root.resolve(), args.check)
        if args.generate:
            flutter = shutil.which('flutter')
            if not flutter:
                raise CatalogError('Flutter executable not available')
            subprocess.run([flutter, 'gen-l10n'], cwd=args.root/'packages/morrow_i18n', check=True)
        print(f'i18n {"checked" if args.check else "compiled"}: {count} artifacts; {"/".join(locale_codes(language_config(args.root)))}')
    except (CatalogError, OSError, ValueError, subprocess.CalledProcessError) as exc:
        parser.exit(1, f'i18n: {exc}\n')

if __name__ == '__main__':
    main()
