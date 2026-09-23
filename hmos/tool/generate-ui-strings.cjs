// Run from any directory. Reads the active Flutter worktree without editing it.
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const arb = path.resolve(root, '../build/io-safety-refactor/packages/morrow_i18n/lib/l10n');
const zh = JSON.parse(fs.readFileSync(path.join(arb, 'app_zh.arb'), 'utf8'));
const messages = {};
for (const locale of ['en', 'ja', 'ko', 'de', 'fr', 'es', 'pt', 'ru']) {
  const translated = JSON.parse(fs.readFileSync(path.join(arb, `app_${locale}.arb`), 'utf8'));
  messages[locale] = {};
  for (const [key, value] of Object.entries(zh)) {
    if (!key.startsWith('@') && typeof value === 'string' && !value.includes('{') &&
        typeof translated[key] === 'string' && !translated[key].includes('{')) {
      messages[locale][value] = translated[key];
    }
  }
}
fs.writeFileSync(path.join(root, 'entry/src/main/ets/model/UiStrings.ets'),
  '// Generated from the active Flutter ARB catalogs; protocol values remain Chinese.\n' +
  'const messages: Record<string, Record<string, string>> = ' + JSON.stringify(messages, null, 2) + ';\n' +
  'export function uiText(value: string, locale: string): string { return messages[locale]?.[value] ?? value; }\n');
