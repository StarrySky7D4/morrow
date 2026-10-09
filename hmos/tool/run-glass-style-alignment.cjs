'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '..'), repo = path.resolve(root, '..');
const output = path.resolve(root, 'reports/ui-source/v32'); fs.mkdirSync(output, { recursive: true });
const label = process.argv[2] || 'initial'; assert.match(label, /^[A-Za-z0-9_-]+$/);
const base = path.join(output, 'glass-style-alignment-' + label);
for (const suffix of ['-inputs-before.json', '-inputs-after.json', '-result.json', '-tests.log']) {
  assert.ok(!fs.existsSync(base + suffix), 'preserve earlier report: ' + base + suffix);
}
const inputs = [__filename, ...['index-glass-style.test.cjs', 'music-ui-component-test-harness.cjs', 'music-ui-inset.test.cjs'].map(name => path.join(__dirname, name)),
  ...['MusicPanel', 'RecessedGlassRelief', 'Index'].map(name => path.join(root, 'entry/src/main/ets/pages', name + '.ets')),
  path.join(root, 'entry/src/main/ets/model/Appearance.ets')];
const references = ['appearance.dart', 'music/music_panel.dart', 'neumorphic_controls.dart', 'experimental_controls.dart',
  'liquid_glass.dart', 'surface_paint_boundary.dart', 'main.dart'].map(name => path.join(repo, 'build/win-cloud-20261005/lib', name));
const sdk = 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets';
references.push(path.join(sdk, 'component/canvas.d.ts'), path.join(sdk, 'component/common.d.ts'), path.join(sdk, 'api/@ohos.arkui.UIContext.d.ts'));
const snapshot = filenames => filenames.map(filename => { const bytes = fs.readFileSync(filename); return {
  path: path.relative(repo, filename).replaceAll('\\', '/'), byte_length: bytes.length,
  sha256: crypto.createHash('sha256').update(bytes).digest('hex') }; });
const before = { observed_at: new Date().toISOString(), inputs: snapshot(inputs), references: snapshot(references) };
fs.writeFileSync(base + '-inputs-before.json', JSON.stringify(before, null, 2) + '\n');
const result = cp.spawnSync(process.execPath, ['--test', path.join(__dirname, 'index-glass-style.test.cjs'), path.join(__dirname, 'music-ui-inset.test.cjs')],
  { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
const log = (result.stdout || '') + (result.stderr || ''); fs.writeFileSync(base + '-tests.log', log);
const after = { observed_at: new Date().toISOString(), inputs: snapshot(inputs), references: snapshot(references) };
fs.writeFileSync(base + '-inputs-after.json', JSON.stringify(after, null, 2) + '\n');
const exact = JSON.stringify(before.inputs) === JSON.stringify(after.inputs) && JSON.stringify(before.references) === JSON.stringify(after.references);
const count = regex => Number(log.match(regex)?.[1] || 0), tests = count(/(?:ℹ|#) tests (\d+)/), passed = count(/(?:ℹ|#) pass (\d+)/), failed = count(/(?:ℹ|#) fail (\d+)/);
const summary = { qualification: 'PASS_SCOPED_ACTUAL_GLASS_STYLE_CALLS', sdk_build: 'NOT_RUN', device_rendering: 'NOT_RUN',
  exact_pixel_match: 'NOT_RUN', secondary_outer_cast: 'OPEN_SDK_SCALAR_SHADOW_AND_PARENT_CLIP',
  exit_code: result.status, tests, passed, failed, source_exact_before_after: exact, input_count: inputs.length,
  reference_count: references.length, duration_ms: Number(log.match(/(?:ℹ|#) duration_ms ([\d.]+)/)?.[1] || 0), node_version: process.version,
  log_sha256: crypto.createHash('sha256').update(log).digest('hex'), inputs: after.inputs,
  controlled_seams: ['UIContext.vp2px density conversion', 'ArkUI Builder child/attribute call recorder',
    'SDK CanvasRenderingContext2D, Path2D and CanvasGradient call recorder', 'timers'],
  assertions: ['seven-style light/dark shared inset paint', 'search -1 versus music -.8 signed widget depth',
    'global music inner palette versus local outer material', 'ordinary Glass and candidate preview exclude extra raised contours',
    'search keeps real recessed paint and nonflat outer cast suppression', 'primary shadow physical px, fill=false and distinct flat material modes',
    'candidate material preview binds its own mode',
    'Footer/Liquid separate material guards', 'complement walls, blur VP, bounded radius, resize cancellation'],
  limitations: ['ArkUI and Canvas APIs remain controlled seams; there is no SDK rasterization or device pixel acceptance.',
    'SDK exposes one general shadow. A correct second outer cast needs a bounded outer wrapper that survives parent clipping.',
    'High-contrast rendering, animation/optical refraction and exact native shadow blur equivalence remain unqualified.'] };
if (result.status !== 0 || !exact || tests !== 23 || passed !== 23 || failed !== 0) summary.qualification = 'FAILED';
fs.writeFileSync(base + '-result.json', JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify(summary, null, 2)); if (summary.qualification === 'FAILED') process.exitCode = 1;
