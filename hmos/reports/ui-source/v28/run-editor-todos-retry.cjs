'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), cp = require('node:child_process');
const report = __dirname, repo = path.resolve(report, '../../../..');
const suites = ['editor-todos-model.test.cjs', 'editor-todos-component-methods.test.cjs',
  'editor-input-policy-model.test.cjs', 'index-editor-todos-integration.test.cjs'];
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const identity = name => { const bytes = fs.readFileSync(path.join(repo, name)); return { path: name, bytes: bytes.length, sha256: sha(bytes) }; };
const candidates = ['hmos/entry/src/main/ets/pages/EditorTodos.ets', 'hmos/entry/src/main/ets/pages/Index.ets',
  ...fs.readdirSync(path.join(repo, 'hmos/entry/src/main/ets/model')).filter(n => n.endsWith('.ets')).map(n => 'hmos/entry/src/main/ets/model/' + n),
  ...fs.readdirSync(path.join(repo, 'hmos/tool')).filter(n => n.endsWith('.cjs')).map(n => 'hmos/tool/' + n),
  'hmos/reports/ui-source/v28/run-editor-todos-retry.cjs', 'hmos/reports/ui-source/v28/editor-todos-retry-source-trace.cjs'];
const baseline = new Map(candidates.map(name => [name, identity(name)]));
const trace = path.join(report, 'editor-todos-retry-final-source-traces'); fs.mkdirSync(trace);
const started = new Date().toISOString();
const result = cp.spawnSync(process.execPath, ['--require', path.join(report, 'editor-todos-retry-source-trace.cjs'), '--test', ...suites.map(n => path.join(repo, 'hmos/tool', n))],
  { cwd: repo, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, env: { ...process.env, TODO_RETRY_REPO: repo, TODO_RETRY_TRACE: trace } });
const log = (result.stdout || '') + (result.stderr || '');
fs.writeFileSync(path.join(report, 'editor-todos-retry-final.log'), log, { flag: 'wx' });
const traces = fs.readdirSync(trace).map(name => JSON.parse(fs.readFileSync(path.join(trace, name))));
const observed = new Map(), compiled = new Set();
for (const item of traces) {
  for (const read of item.reads) {
    if (observed.has(read.path)) assert.deepEqual(observed.get(read.path), read, 'Consistent actual read identity');
    observed.set(read.path, read);
  }
  for (const name of item.compiled) compiled.add(name);
}
const required = new Set([...suites.map(n => 'hmos/tool/' + n),
  'hmos/tool/editor-field-test-harness.cjs', 'hmos/tool/editor-todos-retry-test-harness.cjs', 'hmos/tool/index-business-test-harness.cjs',
  'hmos/entry/src/main/ets/pages/EditorTodos.ets', 'hmos/entry/src/main/ets/pages/Index.ets',
  'hmos/reports/ui-source/v28/run-editor-todos-retry.cjs', 'hmos/reports/ui-source/v28/editor-todos-retry-source-trace.cjs',
  ...compiled, ...[...observed.keys()].filter(n => n.endsWith('.cjs'))]);
for (const name of required) assert.ok(baseline.has(name), 'Actual input exists in pre-run source baseline: ' + name);
const names = [...required].sort(), before = names.map(n => baseline.get(n)), after = names.map(identity);
assert.deepEqual(after, before, 'Actual source inputs unchanged during four-suite run');
for (const name of required) if (observed.has(name)) assert.deepEqual(observed.get(name), baseline.get(name), 'Runtime read matches before/after identity');
const write = (name, value) => fs.writeFileSync(path.join(report, name), JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
write('editor-todos-retry-final-inputs-before.json', before); write('editor-todos-retry-final-inputs-after.json', after);
const inspectedOnly = [...observed.values()].filter(item => !required.has(item.path)).sort((a, b) => a.path.localeCompare(b.path));
write('editor-todos-retry-final-observed-only.json', inspectedOnly);
const total = Number(log.match(/ℹ tests (\d+)/)?.[1]), passed = Number(log.match(/ℹ pass (\d+)/)?.[1]);
const proof = { startedUtc: started, finishedUtc: new Date().toISOString(), exitCode: result.status, signal: result.signal,
  runtime: process.execPath, suites, actualInputs: names.length, observedOnlyInputs: inspectedOnly.length,
  sourceIdentityVerified: true, tests: total, pass: passed, fail: Number(log.match(/ℹ fail (\d+)/)?.[1]),
  cancelled: Number(log.match(/ℹ cancelled (\d+)/)?.[1]), skipped: Number(log.match(/ℹ skipped (\d+)/)?.[1]),
  durationMs: Number(log.match(/ℹ duration_ms ([\d.]+)/)?.[1]), log: 'editor-todos-retry-final.log', logSha256: sha(Buffer.from(log)),
  scope: 'Actual ETS model/policy and component/Index methods with controlled pure-worker and lifecycle replies. Observed-only preloaded model sources are separately listed; no native Store, SDK build, ArkUI renderer or device acceptance.' };
write('editor-todos-retry-final-result.json', proof); console.log(log.slice(-700)); console.log(JSON.stringify(proof));
assert.equal(result.status, 0); assert.ok(total > 126); assert.equal(passed, total); assert.equal(proof.fail + proof.cancelled + proof.skipped, 0);
