'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, repo = path.resolve(report, '../../../..'), label = process.argv[2];
assert.match(label || '', /^[a-z0-9-]+$/);
const sha = b => crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
function files(directory) { return fs.readdirSync(path.join(repo, directory)).sort().flatMap(name => {
  const relative = directory + '/' + name, stat = fs.lstatSync(path.join(repo, relative));
  assert.ok(!stat.isSymbolicLink()); return stat.isDirectory() ? files(relative) : [relative];
}); }
const tracked = cp.execFileSync('git', ['-C', repo, 'ls-files', '--', 'hmos'], { encoding: 'utf8' }).trim().split('\n');
const names = [...new Set([...files('hmos/entry/src/main/ets').filter(n => n.endsWith('.ets')),
  ...files('hmos/tool').filter(n => n.endsWith('.cjs')), ...tracked.filter(n => /store-fixture(?:-[a-z0-9-]+)?\.json$/.test(n)),
  'hmos/reports/ui-source/v31/run-models.cjs'])].sort();
const inventory = () => names.map(name => { const b = fs.readFileSync(path.join(repo, name)); return { path: name, bytes: b.length, sha256: sha(b) }; });
const write = (suffix, value) => fs.writeFileSync(path.join(report, 'models-' + label + '-' + suffix + '.json'), JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const before = inventory(); write('inputs-before', before);
const tests = files('hmos/tool').filter(n => n.endsWith('.test.cjs')).map(n => path.join(repo, n));
const startedUtc = new Date().toISOString(), result = cp.spawnSync(process.execPath, ['--test', ...tests], { cwd: repo, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
const log = (result.stdout || '') + (result.stderr || ''), logName = 'models-' + label + '-tests.log';
fs.writeFileSync(path.join(report, logName), log, { flag: 'wx' }); const after = inventory(); write('inputs-after', after);
const count = name => Number(log.match(new RegExp('(?:#|ℹ) ' + name + ' ([0-9]+(?:\\.[0-9]+)?)'))?.[1]);
const proof = { startedUtc, finishedUtc: new Date().toISOString(), exitCode: result.status, signal: result.signal, testFiles: tests.length,
  tests: count('tests'), passed: count('pass'), failed: count('fail'), cancelled: count('cancelled'), skipped: count('skipped'), durationMs: count('duration_ms'),
  inputs: names.length, sourceIdentityVerified: JSON.stringify(before) === JSON.stringify(after), log: logName, sha256: sha(Buffer.from(log)),
  scope: 'Actual ETS/tools/Index/component methods and Store-produced DTOs; controlled provider/AVPlayer/lifecycle/render seams, not device playback or full Flutter/Windows parity.' };
write('result', proof); console.log(log.slice(-1800)); console.log(JSON.stringify(proof)); assert.ok(proof.sourceIdentityVerified); assert.equal(result.status, 0);
