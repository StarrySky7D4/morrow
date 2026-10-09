'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..'), repo = path.resolve(root, '..');
const output = path.resolve(root, 'reports/ui-source/v30'); fs.mkdirSync(output, { recursive: true });
const label = process.argv[2] || 'initial'; assert.match(label, /^[A-Za-z0-9_-]+$/);
const base = path.join(output, 'music-ui-components-' + label);
for (const suffix of ['-inputs-before.json', '-inputs-after.json', '-result.json', '-tests.log']) {
  assert.ok(!fs.existsSync(base + suffix), 'preserve earlier report: ' + base + suffix);
}
const inputs = [__filename, path.join(__dirname, 'music-ui-component-test-harness.cjs'), path.join(__dirname, 'music-ui-components.test.cjs'),
  ...['MusicPanel', 'MusicFooter'].map(name => path.join(root, 'entry/src/main/ets/pages', name + '.ets')),
  ...['MusicUi', 'MusicLibrary', 'MusicPlayback', 'UiStrings'].map(name => path.join(root, 'entry/src/main/ets/model', name + '.ets'))];
const designReferences = [path.join(repo, 'build/win-cloud-20261005/lib/music/music_panel.dart'), path.join(repo, 'build/win-cloud-20261005/lib/little_tips.dart'),
  path.join(repo, 'build/win-cloud-20261005/lib/hold_reorder.dart'), 'C:/flutter/packages/flutter/lib/src/material/icons.dart'];
const snapshot = filenames => filenames.map(filename => { const bytes = fs.readFileSync(filename); return { path: path.relative(repo, filename).replaceAll('\\', '/'),
  byte_length: bytes.length, sha256: crypto.createHash('sha256').update(bytes).digest('hex') }; });
const before = { observed_at: new Date().toISOString(), inputs: snapshot(inputs), design_references: snapshot(designReferences) };
fs.writeFileSync(base + '-inputs-before.json', JSON.stringify(before, null, 2) + '\n');
const result = cp.spawnSync(process.execPath, ['--test', path.join(__dirname, 'music-ui-components.test.cjs')], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
const log = (result.stdout || '') + (result.stderr || ''); fs.writeFileSync(base + '-tests.log', log);
const after = { observed_at: new Date().toISOString(), inputs: snapshot(inputs), design_references: snapshot(designReferences) };
fs.writeFileSync(base + '-inputs-after.json', JSON.stringify(after, null, 2) + '\n');
const exact = JSON.stringify(before.inputs) === JSON.stringify(after.inputs) && JSON.stringify(before.design_references) === JSON.stringify(after.design_references);
const count = regex => Number(log.match(regex)?.[1] || 0), tests = count(/(?:ℹ|#) tests (\d+)/), passed = count(/(?:ℹ|#) pass (\d+)/), failed = count(/(?:ℹ|#) fail (\d+)/);
const summary = { qualification: 'PASS_SCOPED_ACTUAL_COMPONENT_METHODS', sdk_build: 'NOT_RUN', device_rendering: 'NOT_RUN',
  actual_pointer_reorder: 'NOT_RUN', avplayer_audio: 'NOT_RUN', exit_code: result.status, tests, passed, failed,
  source_exact_before_after: exact, input_count: inputs.length, design_reference_count: designReferences.length,
  duration_ms: Number(log.match(/(?:ℹ|#) duration_ms ([\d.]+)/)?.[1] || 0),
  log_sha256: crypto.createHash('sha256').update(log).digest('hex'), inputs: after.inputs,
  controlled_seams: ['ArkUI field props', 'measured Area/GestureEvent', 'Scroller', 'timers', 'callback delivery'],
  assertions: ['selected stable identity and actual playing phase', 'selected idle explicit toggle and strict seek', 'owned pause survives library Unknown',
    'originalSaved spool inspect without fabricated identity', 'exact pending choice, no cancel/delete', 'complete uncropped unsaved lyrics',
    'revision/import/hash/order held reorder guards', 'collapse preserves List and Scroller', 'Footer default material independence'] };
if (result.status !== 0 || !exact || tests !== 25 || passed !== 25 || failed !== 0) summary.qualification = 'FAILED';
fs.writeFileSync(base + '-result.json', JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify(summary, null, 2)); if (summary.qualification === 'FAILED') process.exitCode = 1;
