'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process'), assert = require('node:assert/strict');
const repo = path.resolve(__dirname, '../../../..');
const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, 'input-native-inputs.json'), 'utf8'));
for (const item of manifest.repository_source_inventory) {
  assert.ok(item.path.startsWith('hmos/'), 'native staged input scope');
  const staged = cp.execFileSync('git', ['-C', repo, 'show', ':' + item.path], { maxBuffer: 32 * 1024 * 1024 });
  assert.equal(staged.length, item.bytes, 'staged byte length ' + item.path);
  assert.equal(crypto.createHash('sha256').update(staged).digest('hex').toUpperCase(), item.sha256, 'staged SHA ' + item.path);
}
console.log(JSON.stringify({ result: 'PASS', repository_sources: manifest.repository_source_inventory.length,
  staged: true, scope: 'Native repository source bytes; ignored archives and external references are separately checked' }));
