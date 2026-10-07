// Root-owned actual UI qualification. No clipboard flavors or app files are injected.
// Loading this module performs no device operation; every mutation is explicit and journaled.
'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process'), assert = require('node:assert/strict');
process.env.HMOS_DEVICE ||= '127.0.0.1:5555';
process.env.HMOS_REPORT_DIR ||= path.resolve(__dirname, '../reports/ui-source/v17/device-clipboard');
process.env.HMOS_EXPECTED_VERSION ||= '0.1.0-hmos-dev.17';
process.env.HMOS_EXPECTED_HAP_SHA256 ||= 'D9DECC46BB0E953BB56A4CDC5DBBB71D0B863E6380C273CC677B73C36689C03F';
assert.equal(process.env.HMOS_DEVICE, '127.0.0.1:5555');
const m = require('./media-controls-device-check.cjs'), binding = require('./multiselect-device-check.cjs');
const out = process.env.HMOS_REPORT_DIR, hdc = 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const fixture = 'HMOS-clipboard-20261007-C', journal = path.join(out, 'progress-clipboard.json');
let active, serial = 0;
const wait = ms => { assert.ok(ms >= 0 && ms <= 3000); Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms); };
const flat = tree => m.f.d.flatten(tree);
const byId = (tree, id) => flat(tree).find(x => x.n.id === id)?.n;
const byText = (tree, text) => flat(tree).filter(x => x.n.text === text).at(-1)?.n;
function read() { return fs.existsSync(journal) ? JSON.parse(fs.readFileSync(journal, 'utf8')) : { fixture, device: process.env.HMOS_DEVICE, stages: [] }; }
function persist(value) {
  assert.equal(value.fixture, fixture); assert.equal(value.device, process.env.HMOS_DEVICE);
  const pending = journal + '.' + process.pid + '-' + (++serial) + '.pending';
  fs.writeFileSync(pending, JSON.stringify(value, null, 2) + '\n'); fs.renameSync(pending, journal);
}
function update() { if (active) { const state = read(), index = state.stages.findIndex(x => x.label === active.label); assert.ok(index >= 0); state.stages[index] = active; persist(state); } }
function command(...args) {
  const action = { arguments: args, phase: 'INTENT', at: new Date().toISOString() }; assert.ok(active); active.actions.push(action); update();
  const reply = cp.execFileSync(hdc, ['-t', process.env.HMOS_DEVICE, ...args.map(String)], { encoding: 'utf8', timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
  assert.ok(!/illegal|incorrect|failed|\berror\b/i.test(reply) || /no error/i.test(reply), reply);
  action.phase = 'ACKNOWLEDGED'; action.reply = reply; update(); return reply;
}
function key(...values) { return command('shell', 'uitest', 'uiInput', 'keyEvent', ...values.map(String)); }
function click(node) {
  assert.ok(node && node.bounds && node.bounds.every(Number.isFinite) && node.bounds[3] - node.bounds[1] > 12 && node.enabled !== false, 'fresh observed enabled control');
  return command('shell', 'uitest', 'uiInput', 'click', Math.round((node.bounds[0] + node.bounds[2]) / 2), Math.round((node.bounds[1] + node.bounds[3]) / 2));
}
function capture(label) { const tree = m.capture(label); active.captures.push({ label, json: path.join(out, label + '.json'), png: path.join(out, label + '.png') }); update(); return tree; }
function stage(label, body) {
  assert.match(label, /^[A-Za-z0-9-]{1,70}$/); assert.ok(!active);
  const state = read(); assert.ok(!state.stages.some(x => x.label === label), 'issued stage must be inspected, never replayed');
  const install = binding.installBinding(); active = { label, phase: 'RUNNING', started: new Date().toISOString(), installation: install, actions: [], captures: [] };
  state.stages.push(active); persist(state);
  try {
    const bundle = binding.bundleIdentity(command('shell', 'bm', 'dump', '-n', 'dev.morrow.hmos')); assert.equal(bundle.versionName, install.versionName);
    const result = body(label); active.result = result; active.phase = 'PASS'; active.completed = new Date().toISOString(); update(); console.log(JSON.stringify({ label, result, phase: active.phase })); return result;
  } catch (error) { active.phase = 'FAILED_OR_UNKNOWN'; active.error = error.message; update(); throw error; } finally { active = undefined; }
}
function input(tree, id, value) {
  click(byId(tree, id)); key(2072, 2017);
  const own = '/data/local/tmp/hmos-clipboard-input-' + process.pid + '-' + (++serial) + '.sh', local = path.join(out, active.label + '-input-' + process.pid + '-' + serial + '.sh');
  const encoded = Buffer.from(value, 'utf8').toString('base64');
  fs.writeFileSync(local, "#!/bin/sh\nvalue=$(printf '%s' '" + encoded + "' | base64 -d; printf '.')\nexec uitest uiInput text \"${value%.}\"\n", { flag: 'wx' });
  command('file', 'send', local, own); command('shell', 'sh', own); command('shell', 'rm', '-f', own); key('Back'); wait(350);
  const after = capture(active.label + '-input-' + serial); assert.equal(byId(after, id)?.text, value, 'exact visible input'); return after;
}
function editor(tree) { assert.equal(byId(tree, 'draft-title')?.text, fixture, 'exact own editor'); assert.ok(!byId(tree, 'attachment-preview-dialog')); }
function previewToggle(tree) {
  const heading = flat(tree).find(x => x.n.text === '正文 · Markdown'); assert.ok(heading, 'observed body heading');
  const row = [...heading.parents].reverse().find(p => p.type === 'Row'); assert.ok(row);
  const buttons = row.children.filter(n => n.type === 'Button'); assert.equal(buttons.length, 1); return buttons[0];
}
function seek(tree, predicate, label, up = false) {
  for (let i = 0; i < 10; i++) {
    const item = flat(tree).find(x => predicate(x.n) && x.n.bounds && x.n.bounds[3] - x.n.bounds[1] > 18);
    if (item) return { tree, node: item.n };
    const anchor = flat(tree).find(x => x.n.id === 'markdown-preview' || x.n.id === 'draft-description' || x.n.id === 'draft-title' || x.n.id?.startsWith('draft-asset:'));
    const scroll = anchor?.parents.filter(p => p.type === 'Scroll').at(-1); assert.ok(scroll, 'observed editor scroll');
    const [l, t, r, b] = scroll.bounds; command('shell', 'uitest', 'uiInput', 'swipe', r - 12, up ? t + 100 : b - 100, r - 12, up ? b - 100 : t + 100, 650);
    tree = capture(active.label + '-seek-' + (++serial));
  }
  throw new Error('observed editor control missing: ' + label);
}
function pins(tree) { return flat(tree).filter(x => x.n.id?.startsWith('draft-asset:')).map(x => ({ id: x.n.id.slice('draft-asset:'.length), text: flat([x.n]).map(y => y.n.text).filter(Boolean) })); }
function settled(label) { wait(1000); return capture(label); }
if (require.main === module) {
  const label = process.argv[2]; assert.ok(label, 'explicit fresh read-only label required');
  stage(label, key => { const tree = settled(key + '-observed'); return { editor: byId(tree, 'draft-title')?.text, nodes: flat(tree).filter(x => x.n.text || x.n.id).map(x => ({ type: x.n.type, id: x.n.id, text: x.n.text, bounds: x.n.bounds })) }; });
}
module.exports = { m, out, fixture, stage, command, key, click, capture, input, editor, previewToggle, seek, pins, settled, wait, flat, byId, byText };
