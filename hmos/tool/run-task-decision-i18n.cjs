'use strict';
// Hash-bound source/runtime localization checks only. No SDK build or device.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const os = require('node:os');
const cp = require('node:child_process'), assert = require('node:assert/strict');
const contract = require('./task-decision-i18n.cjs');
const root = path.resolve(__dirname, '..');
const label = process.argv[2] || 'a1';
assert.match(label, /^[a-z0-9-]+$/);
const options = process.argv.slice(3);
for (const option of options) assert.ok(['--clean-checkout', '--live-source'].includes(option), 'known explicit check option');
const cleanCheckout = options.includes('--clean-checkout'), liveSource = options.includes('--live-source');
assert.ok(!(cleanCheckout && liveSource), 'clean checkout does not access live worktree sources');
const folder = path.join(root, 'reports/ui-source/v33');
fs.mkdirSync(folder, { recursive: true });
const base = path.join(folder, `task-decision-i18n-${label}`);
for (const suffix of ['-result.json', '-inputs-before.json', '-inputs-after.json', '-tests.log', '-clean-checkout.json']) {
  assert.ok(!fs.existsSync(base + suffix), 'preserve earlier evidence: ' + base + suffix);
}
contract.catalog();
if (liveSource) contract.check('live');
const tests = path.join(__dirname, 'task-decision-i18n.test.cjs');
for (const filename of [__filename, tests, path.join(__dirname, 'generate-ui-strings.cjs'),
  path.join(root, 'entry/src/main/ets/pages/Index.ets')]) contract.usedInputs.add(filename);
const filenames = [...contract.usedInputs].sort();
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function snapshot() {
  return filenames.map(filename => {
    const bytes = fs.readFileSync(filename);
    return { path: path.relative(path.resolve(root, '..'), filename).replace(/\\/g, '/'),
      bytes: bytes.length, sha256: sha(bytes) };
  });
}
function write(suffix, value) { fs.writeFileSync(base + suffix, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' }); }
const before = snapshot(); write('-inputs-before.json', before);
const liveCheck = liveSource ? contract.check('live') : undefined;
let executedTests = tests, executionOptions = {}, portable;
if (cleanCheckout) {
  const cleanRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'morrow-task-i18n-'));
  const repository = path.resolve(root, '..');
  const copied = filenames.map(filename => {
    const relative = path.relative(repository, filename);
    assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative), 'only repository inputs are copied');
    const target = path.join(cleanRoot, relative); fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(filename, target); return { source: filename, target };
  });
  assert.equal(fs.existsSync(path.join(cleanRoot, 'build')), false, 'fresh checkout tree has no build directory');
  const guard = path.join(cleanRoot, 'guard-file-reads.cjs');
  const guardSource = `'use strict';
const fs = require('node:fs'), path = require('node:path');
const root = __dirname, attempts = [];
function check(file, operation) {
  if (typeof file === 'number') return;
  const filename = path.resolve(String(file));
  const allowed = filename === root || filename.startsWith(root + path.sep);
  attempts.push({ operation, path: filename, allowed });
  if (!allowed) throw new Error('Isolated task i18n check denied external file read: ' + filename);
}
for (const name of ['readFileSync', 'readFile']) {
  const original = fs[name]; fs[name] = function(file, ...args) { check(file, name); return original.call(this, file, ...args); };
}
const open = fs.openSync;
fs.openSync = function(file, flags, ...args) {
  if (typeof flags === 'number' || String(flags).includes('r') || String(flags).includes('+')) check(file, 'openSync');
  return open.call(this, file, flags, ...args);
};
for (const name of ['readFile', 'open']) {
  const original = fs.promises[name];
  fs.promises[name] = function(file, ...args) { check(file, 'promises.' + name); return original.call(this, file, ...args); };
}
process.on('exit', () => fs.writeFileSync(path.join(root, 'file-read-audit-' + process.pid + '.json'), JSON.stringify(attempts)));
`;
  fs.writeFileSync(guard, guardSource, { flag: 'wx' });
  executedTests = path.join(cleanRoot, path.relative(repository, tests));
  executionOptions = { cwd: cleanRoot, env: { ...process.env,
    NODE_OPTIONS: '--require ' + JSON.stringify(guard.replace(/\\/g, '/')) } };
  portable = { root: cleanRoot, buildDirectoryAbsent: true, copiedRepositoryInputCount: copied.length,
    guardSha256: sha(Buffer.from(guardSource)), copiedInputsUnchanged: false, externalReads: -1,
    copied, guard };
}
const started = Date.now();
const result = cp.spawnSync(process.execPath, ['--test', '--test-reporter=tap', executedTests], {
  encoding: 'utf8', timeout: 90000, maxBuffer: 8 * 1024 * 1024, ...executionOptions });
const durationMs = Date.now() - started;
const log = result.stdout + result.stderr; fs.writeFileSync(base + '-tests.log', log, { flag: 'wx' });
const after = snapshot(); write('-inputs-after.json', after);
if (portable) {
  portable.copiedInputsUnchanged = portable.copied.every(input =>
    sha(fs.readFileSync(input.target)) === sha(fs.readFileSync(input.source)));
  const auditFiles = fs.readdirSync(portable.root).filter(file => /^file-read-audit-\d+\.json$/.test(file));
  const reads = auditFiles.flatMap(file => JSON.parse(fs.readFileSync(path.join(portable.root, file), 'utf8')));
  portable.auditProcessCount = auditFiles.length; portable.fileReadCount = reads.length;
  portable.externalReads = reads.filter(input => !input.allowed).length;
  portable.readPaths = [...new Set(reads.map(input => path.relative(portable.root, input.path).replace(/\\/g, '/')))].sort();
  portable.guardUnchanged = sha(fs.readFileSync(portable.guard)) === portable.guardSha256;
  portable.buildDirectoryAbsent = !fs.existsSync(path.join(portable.root, 'build'));
  delete portable.copied; delete portable.guard;
  write('-clean-checkout.json', portable);
}
const match = key => Number(log.match(new RegExp('^# ' + key + ' (\\d+)$', 'm'))?.[1] ?? -1);
const summary = { tests: match('tests'), pass: match('pass'), fail: match('fail'),
  cancelled: match('cancelled'), skipped: match('skipped') };
const stable = JSON.stringify(before) === JSON.stringify(after);
const passed = result.status === 0 && !result.error && stable && summary.tests === 12 &&
  summary.pass === 12 && summary.fail === 0 && summary.cancelled === 0 && summary.skipped === 0 &&
  (!portable || portable.copiedInputsUnchanged && portable.guardUnchanged && portable.buildDirectoryAbsent &&
    portable.auditProcessCount >= 2 && portable.fileReadCount > 0 && portable.externalReads === 0);
const report = { status: passed ? 'PASS' : 'FAIL', scope: 'actual uiText and task UI source/reference wiring; no layout, SDK or device proof',
  sourceInputCount: filenames.length, frozenInputsUnchanged: stable, summary, durationMs,
  exitCode: result.status, signal: result.signal, processError: result.error?.message || '',
  locales: contract.locales, sourceKeys: contract.keys, actualLookups: 27,
  node: { path: process.execPath, version: process.version, sha256: sha(fs.readFileSync(process.execPath)) },
  logSha256: sha(Buffer.from(log)), sdk: 'NOT_RUN', device: 'NOT_RUN',
  uiStringsChanged: false, referenceMode: liveSource ? 'pinned-plus-explicit-live-check' : 'pinned',
  pinnedFixture: path.relative(path.resolve(root, '..'), contract.fixturePath).replace(/\\/g, '/'),
  pinnedSourceFileCount: 18, liveSourceFileCount: liveSource ? 18 : 0, liveCheck, cleanCheckout: portable,
  provenance: 'repository-owned fixture extracted from actual Flutter main parts and assembled ARB sources; existing HMOS entries retained; live source access requires --live-source' };
write('-result.json', report); console.log(JSON.stringify(report));
if (!passed) process.exitCode = 1;
