'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, repo = path.resolve(report, '../../../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const tracked = cp.execFileSync('git', ['-C', repo, 'ls-files', '--', 'hmos/tool', 'hmos/entry/src/main/ets'])
  .toString().trim().split('\n').filter(Boolean);
const owned = ['hmos/entry/src/main/ets/model/EditorBusinessHandoff.ets',
  'hmos/tool/editor-business-handoff-model.test.cjs', 'hmos/tool/index-business-integration.test.cjs',
  'hmos/tool/index-business-test-harness.cjs', 'hmos/reports/ui-source/v25/editor-handoff-store-fixture.json',
  'hmos/reports/ui-source/v25/run-models.cjs'];
const names = [...new Set([...tracked, ...owned])].sort();
const inventory = () => names.map(name => { const bytes = fs.readFileSync(path.join(repo, name));
  return { path: name, bytes: bytes.length, sha256: sha(bytes) }; });
const write = (name, value) => fs.writeFileSync(path.join(report, name), JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const before = inventory(); write('models-inputs-before.json', before);
const tests = fs.readdirSync(path.join(repo, 'hmos/tool')).filter(n => n.endsWith('.test.cjs')).sort()
  .map(n => path.join(repo, 'hmos/tool', n));
const started = new Date().toISOString();
const result = cp.spawnSync(process.execPath, ['--test', ...tests], { cwd: repo, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
const log = (result.stdout || '') + (result.stderr || '');
fs.writeFileSync(path.join(report, 'models-final-tests.log'), log, { flag: 'wx' });
const after = inventory(); write('models-inputs-after.json', after); assert.deepEqual(after, before, 'actual test input drift');
const proof = { startedUtc: started, finishedUtc: new Date().toISOString(), exitCode: result.status,
  signal: result.signal, suites: tests.length, inputs: names.length, sourceIdentityVerified: true,
  log: 'models-final-tests.log', sha256: sha(Buffer.from(log)), scope: 'actual-source host tool/models; native replies and SDK lifecycle seams are not device acceptance' };
write('models-result.json', proof);
console.log(log.slice(-1400)); console.log(JSON.stringify(proof));
assert.equal(result.status, 0, 'model test failure');
