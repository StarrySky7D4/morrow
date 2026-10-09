'use strict';
// Default checks use a repository-owned, source-derived Flutter fixture. Live
// Windows worktree checks are explicit and never substitute UiStrings as truth.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..');
const repo = path.resolve(root, '..');
const reference = path.resolve(root, '../build/win-cloud-20261005');
const uiPath = path.join(root, 'entry/src/main/ets/model/UiStrings.ets');
const fixturePath = path.join(__dirname, 'fixtures/task-decision-flutter-catalog.json');
const locales = ['zh', 'en', 'ja', 'ko', 'de', 'fr', 'es', 'pt', 'ru'];
const keys = ['mainTaskMarkComplete', 'mainTaskMarkIncomplete', 'mainTaskAmbiguousDecision'];
const usedInputs = new Set([__filename, uiPath]);
const sourcePaths = locales.flatMap(locale => [
  path.join(reference, 'l10n/parts', `main.${locale}.arb`),
  path.join(reference, 'packages/morrow_i18n/lib/l10n', `app_${locale}.arb`)
]);
const relative = filename => path.relative(repo, filename).replace(/\\/g, '/');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function validateEntries(entries) {
  assert.deepEqual(Object.keys(entries), locales, 'exact pinned locale set');
  for (const locale of locales) {
    assert.deepEqual(Object.keys(entries[locale]), keys, 'exact task source keys: ' + locale);
    for (const key of keys) {
      const value = entries[locale][key];
      assert.equal(typeof value, 'string', `actual task source exists: ${locale}/${key}`);
      assert.ok(value.length > 0 && !value.includes('{'), `literal task source: ${locale}/${key}`);
    }
  }
  return entries;
}
function readLive() {
  const buffers = sourcePaths.map(filename => {
    usedInputs.add(filename); return fs.readFileSync(filename);
  });
  const sourcesBefore = buffers.map((bytes, i) => ({ path: relative(sourcePaths[i]), bytes: bytes.length, sha256: sha(bytes) }));
  const entries = {};
  for (const [i, locale] of locales.entries()) {
    const parts = JSON.parse(buffers[2 * i].toString('utf8'));
    const combined = JSON.parse(buffers[2 * i + 1].toString('utf8'));
    entries[locale] = {};
    for (const key of keys) {
      const value = parts[key];
      assert.equal(combined[key], value, `parts / combined catalog parity: ${locale}/${key}`);
      entries[locale][key] = value;
    }
  }
  validateEntries(entries);
  const sourcesAfter = sourcePaths.map(filename => {
    const bytes = fs.readFileSync(filename); return { path: relative(filename), bytes: bytes.length, sha256: sha(bytes) };
  });
  assert.deepEqual(sourcesAfter, sourcesBefore, 'all 18 actual ARBs unchanged during extraction');
  return { entries, sourcesBefore, sourcesAfter };
}
function pinnedFixture() {
  usedInputs.add(fixturePath);
  const fixture = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));
  assert.equal(fixture.schema, 1, 'known pinned Flutter fixture schema');
  assert.deepEqual(fixture.locales, locales); assert.deepEqual(fixture.sourceKeys, keys);
  assert.equal(fixture.provenance.referenceRoot, relative(reference));
  assert.equal(fixture.provenance.source, 'actual Flutter main locale parts and assembled ARB catalogs; never UiStrings');
  assert.equal(fixture.provenance.sourcesUnchangedBeforeAfter, true);
  const before = fixture.provenance.sourcesBefore, after = fixture.provenance.sourcesAfter;
  assert.deepEqual(after, before, 'pinned source extraction recorded no drift');
  assert.equal(before.length, 18, '18 actual source manifests');
  for (const [i, input] of before.entries()) {
    assert.equal(input.path, relative(sourcePaths[i]), 'exact actual source path');
    assert.ok(Number.isSafeInteger(input.bytes) && input.bytes > 0, 'actual source length');
    assert.match(input.sha256, /^[a-f0-9]{64}$/, 'actual source SHA256');
  }
  validateEntries(fixture.entries); return fixture;
}
function catalog() { return pinnedFixture().entries; }
function actualStrings() {
  const source = fs.readFileSync(uiPath, 'utf8');
  const typedPrefix = 'const messages: Record<string, Record<string, string>> = ';
  const typedFunction = 'export function uiText(value: string, locale: string): string';
  assert.equal(source.split(typedPrefix).length, 2, 'one actual message declaration');
  assert.equal(source.split(typedFunction).length, 2, 'one actual uiText declaration');
  const start = source.indexOf(typedPrefix) + typedPrefix.length;
  const end = source.indexOf(';\nexport function uiText', start);
  assert.ok(end > start, 'actual messages boundary');
  const messages = JSON.parse(source.slice(start, end));
  // Only remove the two ArkTS type declarations; execute the production lookup
  // body verbatim, including its actual Chinese / unknown-value fallback.
  const executable = source.replace(typedPrefix, 'const messages = ')
    .replace(typedFunction, 'function uiText(value, locale)');
  const context = {};
  vm.runInNewContext(executable, context, { filename: uiPath });
  assert.equal(typeof context.uiText, 'function', 'actual uiText executes');
  return { source, messages, uiText: context.uiText };
}
function check(mode = 'pinned') {
  assert.ok(['pinned', 'live'].includes(mode), 'explicit reference mode');
  const fixture = pinnedFixture(); let entries = fixture.entries;
  if (mode === 'live') {
    const live = readLive();
    assert.deepEqual(live.entries, entries, 'actual current Flutter phrases match pinned source');
    assert.deepEqual(live.sourcesBefore, fixture.provenance.sourcesBefore, 'all 18 current Flutter ARBs match pinned provenance');
    entries = live.entries;
  }
  const actual = actualStrings();
  for (const locale of locales) for (const key of keys) {
    const chinese = entries.zh[key], translated = entries[locale][key];
    if (locale !== 'zh') assert.equal(actual.messages[locale]?.[chinese], translated,
      `actual non-fallback locale entry: ${locale}/${key}`);
    assert.equal(actual.uiText(chinese, locale), translated, `actual uiText: ${locale}/${key}`);
  }
  return { status: 'PASS', referenceMode: mode, locales: [...locales], phraseCount: keys.length,
    lookups: locales.length * keys.length, pinnedFixture: relative(fixturePath), pinnedSourceFileCount: 18,
    liveSourceFileCount: mode === 'live' ? 18 : 0 };
}
module.exports = { locales, keys, catalog, actualStrings, check, usedInputs, uiPath, fixturePath };
if (require.main === module) {
  if (process.argv[2] === '--export-live-fixture') {
    assert.ok(process.argv[3], 'explicit new fixture output path required');
    const live = readLive(), output = path.resolve(process.argv[3]);
    const fixture = { schema: 1, locales, sourceKeys: keys,
      provenance: { source: 'actual Flutter main locale parts and assembled ARB catalogs; never UiStrings',
        referenceRoot: relative(reference), observedAt: new Date().toISOString(),
        extractionCommand: 'node hmos/tool/task-decision-i18n.cjs --export-live-fixture <new-output.json>',
        sourcesUnchangedBeforeAfter: true, sourcesBefore: live.sourcesBefore, sourcesAfter: live.sourcesAfter },
      entries: live.entries };
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, JSON.stringify(fixture, null, 2) + '\n', { flag: 'wx' });
    console.log(JSON.stringify({ status: 'PASS', output: relative(output), sourceFileCount: 18,
      sourcesUnchangedBeforeAfter: true, sha256: sha(fs.readFileSync(output)) }));
  } else {
    assert.equal(process.argv[2], '--check-live', 'use --check-live or --export-live-fixture <new-output.json>');
    console.log(JSON.stringify(check('live')));
  }
}
