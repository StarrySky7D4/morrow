'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const contract = require('./task-decision-i18n.cjs');
const entries = contract.catalog(), actual = contract.actualStrings();
for (const locale of contract.locales) {
  test(`task decisions use the pinned exact Flutter design in ${locale}`, () => {
    for (const key of contract.keys) {
      const phrase = entries.zh[key], expected = entries[locale][key];
      if (locale !== 'zh') {
        assert.equal(actual.messages[locale]?.[phrase], expected, 'explicit translated entry exists');
        assert.notEqual(expected, phrase, 'does not silently use Chinese fallback');
      }
      assert.equal(actual.uiText(phrase, locale), expected, 'actual production uiText');
    }
  });
}
test('task menu and pending row call the existing exact catalog phrases', () => {
  const filename = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets');
  contract.usedInputs.add(filename);
  const source = fs.readFileSync(filename, 'utf8');
  for (const key of contract.keys) assert.ok(source.includes(`this.t('${entries.zh[key]}')`),
    `real task UI uses ${key}`);
  for (const invented of ['标记完成', '标记未完成', '待确认，请明确选择完成或未完成']) {
    assert.ok(!source.includes(`this.t('${invented}')`), 'no missing translated task phrase: ' + invented);
  }
});
test('task user text and stable protocol identities remain literal', () => {
  for (const locale of contract.locales) for (const value of [
    'task-stable-id-43', '原始 task 🧪 é 内容', '{"operation":"original-task-set","complete":false}'
  ]) assert.equal(actual.uiText(value, locale), value);
});
test('bounded generator check verifies all nine locales without changing UiStrings', () => {
  const before = fs.readFileSync(contract.uiPath);
  const generator = path.resolve(__dirname, 'generate-ui-strings.cjs');
  contract.usedInputs.add(generator);
  const result = cp.spawnSync(process.execPath, [generator, '--check-task-decisions'], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const report = JSON.parse(result.stdout.trim());
  assert.equal(report.status, 'PASS'); assert.equal(report.lookups, 27);
  assert.equal(report.referenceMode, 'pinned'); assert.equal(report.liveSourceFileCount, 0);
  assert.deepEqual(report.locales, contract.locales);
  assert.deepEqual(fs.readFileSync(contract.uiPath), before, 'read-only check preserves every unrelated translation byte');
});
