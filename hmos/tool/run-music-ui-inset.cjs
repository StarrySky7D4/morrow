'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..'), repo = path.resolve(root, '..');
const output = path.resolve(root, 'reports/ui-source/v31'); fs.mkdirSync(output, { recursive: true });
const label = process.argv[2] || 'initial'; assert.match(label, /^[A-Za-z0-9_-]+$/);
const base = path.join(output, 'music-ui-inset-' + label);
for (const suffix of ['-inputs-before.json', '-inputs-after.json', '-result.json', '-tests.log']) {
  assert.ok(!fs.existsSync(base + suffix), 'preserve earlier report: ' + base + suffix);
}
const inputs = [__filename, path.join(__dirname, 'music-ui-component-test-harness.cjs'), path.join(__dirname, 'music-ui-inset.test.cjs'),
  path.join(root, 'entry/src/main/ets/pages/MusicPanel.ets'), path.join(root, 'entry/src/main/ets/model/Appearance.ets')];
const references = ['music/music_panel.dart', 'neumorphic_controls.dart', 'experimental_controls.dart']
  .map(name => path.join(repo, 'build/win-cloud-20261005/lib', name));
references.push('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/component/canvas.d.ts');
const snapshot = filenames => filenames.map(filename => { const bytes = fs.readFileSync(filename); return {
  path: path.relative(repo, filename).replaceAll('\\', '/'), byte_length: bytes.length,
  sha256: crypto.createHash('sha256').update(bytes).digest('hex') }; });
const before = { observed_at: new Date().toISOString(), inputs: snapshot(inputs), references: snapshot(references) };
fs.writeFileSync(base + '-inputs-before.json', JSON.stringify(before, null, 2) + '\n');
const result = cp.spawnSync(process.execPath, ['--test', path.join(__dirname, 'music-ui-inset.test.cjs')], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
const log = (result.stdout || '') + (result.stderr || ''); fs.writeFileSync(base + '-tests.log', log);
const after = { observed_at: new Date().toISOString(), inputs: snapshot(inputs), references: snapshot(references) };
fs.writeFileSync(base + '-inputs-after.json', JSON.stringify(after, null, 2) + '\n');
const exact = JSON.stringify(before.inputs) === JSON.stringify(after.inputs) && JSON.stringify(before.references) === JSON.stringify(after.references);
const count = regex => Number(log.match(regex)?.[1] || 0), tests = count(/(?:ℹ|#) tests (\d+)/), passed = count(/(?:ℹ|#) pass (\d+)/), failed = count(/(?:ℹ|#) fail (\d+)/);
const summary = { qualification: 'PASS_SCOPED_ACTUAL_INSET_PAINT_CALLS', sdk_build: 'NOT_RUN', device_rendering: 'NOT_RUN',
  exact_pixel_match: 'NOT_RUN', exit_code: result.status, tests, passed, failed,
  source_exact_before_after: exact, input_count: inputs.length, reference_count: references.length,
  duration_ms: Number(log.match(/(?:ℹ|#) duration_ms ([\d.]+)/)?.[1] || 0), node_version: process.version,
  log_sha256: crypto.createHash('sha256').update(log).digest('hex'), inputs: after.inputs,
  controlled_seams: ['SDK CanvasRenderingContext2D call recorder', 'SDK Path2D call recorder', 'CanvasGradient call recorder', 'timers'],
  assertions: ['seven styles and light/dark paint branches', 'zero depth and flat clear without new frame',
    'paper/fluent/brutalist/industrial exact alpha, deflate and width', 'clay/neumorphism complement walls, opposite offsets and unshifted clip',
    'Gaussian filter in VP and state restoration', 'neumorphism casts use actual palette surface',
    'separate depth strength and alpha saturation', 'bounded rounded geometry', 'fill=false current and selected rows',
    'coalesced resize and cancelled pending paint'],
  limitations: ['Controlled Canvas records paint calls; it does not execute SDK blur or rasterize pixels.',
    'Live SDK compilation, high contrast, style transitions and device pixel comparison are not covered.'] };
if (result.status !== 0 || !exact || tests !== 13 || passed !== 13 || failed !== 0) summary.qualification = 'FAILED';
fs.writeFileSync(base + '-result.json', JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify(summary, null, 2)); if (summary.qualification === 'FAILED') process.exitCode = 1;
