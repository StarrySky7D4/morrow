'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process');
const root = path.resolve(__dirname, '../..'), report = path.join(root, 'hmos/reports/ui-source/v29');
const tests = ['hmos/tool/music-library-model.test.cjs', 'hmos/tool/music-library-native-dto.test.cjs'];
const inputs = [
  'hmos/entry/src/main/ets/model/MusicLibrary.ets',
  'hmos/entry/src/main/ets/model/EditorFieldPolicy.ets',
  'hmos/entry/src/main/ets/model/EditorDraft.ets',
  'hmos/tool/editor-field-test-harness.cjs', 'hmos/tool/music-library-test-harness.cjs', ...tests,
  'hmos/tool/music-library-source-trace.cjs', 'hmos/tool/run-music-library.cjs',
  'hmos/reports/ui-source/v29/music-store-fixture-stage1.json', 'hmos/reports/ui-source/v29/music-store-fixture.json'
];
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const inventory = () => inputs.map(file => { const bytes = fs.readFileSync(path.join(root, file)); return { path: file, bytes: bytes.length, sha256: sha(bytes) }; });
const before = inventory(), tracePath = path.join(report, 'music-library-source-reads.jsonl');
fs.writeFileSync(tracePath, '');
const preload = path.join(root, 'hmos/tool/music-library-source-trace.cjs');
const command = ['--require', preload, '--test', ...tests];
const result = cp.spawnSync(process.execPath, command, { cwd: root, encoding: 'utf8', env: { ...process.env, MUSIC_LIBRARY_TRACE: tracePath } });
const log = (result.stdout || '') + (result.stderr || '');
fs.writeFileSync(path.join(report, 'music-library-final.log'), log);
const after = inventory(), drift = before.filter((value, index) => JSON.stringify(value) !== JSON.stringify(after[index]));
const reads = fs.readFileSync(tracePath, 'utf8').trim().split('\n').filter(Boolean).map(line => JSON.parse(line));
const actual = [...new Set(reads.flatMap(value => value.files))].sort();
const required = inputs.filter(file => file !== 'hmos/tool/run-music-library.cjs');
const missing = required.filter(file => !actual.includes(file)), unexpected = actual.filter(file => !inputs.includes(file));
const counts = {}; for (const name of ['tests', 'pass', 'fail', 'cancelled', 'skipped', 'todo', 'duration_ms']) {
  const match = log.match(new RegExp('(?:ℹ|#) ' + name + ' ([0-9.]+)')); counts[name] = match ? Number(match[1]) : null;
}
const output = { scope: 'Actual ETS coordinator with controlled host replies and unchanged complete native Store fixture DTOs; not SDK/device/decoded music proof',
  executable: process.execPath, command, exit_code: result.status, test_files: tests, counts, inputs_before: before, inputs_after: after,
  drift, actual_observed_reads: actual, missing_reads: missing, unexpected_reads: unexpected,
  tooling: { typescript: process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript',
    loader: 'SDK TypeScript transpile of actual ETS in a VM; not ArkTS SDK compilation' },
  log_sha256: sha(Buffer.from(log)), trace_sha256: sha(fs.readFileSync(tracePath)) };
fs.writeFileSync(path.join(report, 'music-library-result.json'), JSON.stringify(output, null, 2) + '\n');
console.log(JSON.stringify({ exit_code: output.exit_code, counts, inputs: before.length, actual_reads: actual.length, drift: drift.length, missing_reads: missing, unexpected_reads: unexpected }));
process.exitCode = result.status || drift.length || missing.length || unexpected.length || counts.fail || counts.skipped || (counts.tests === 28 && counts.pass === 28 ? 0 : 1);
