const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { test } = require('node:test'), assert = require('node:assert/strict');
const ts = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const directory = path.join(__dirname, 'image-gesture-tester');
function load(name, dependencies = {}) {
  const source = fs.readFileSync(path.join(directory, name + '.ets'), 'utf8');
  const compiled = ts.transpileModule(source, { reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.deepEqual((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), []);
  const exports = {}; vm.runInNewContext(compiled.outputText, { exports, Error, require: id => dependencies[id] || {} }); return exports;
}
const plans = load('ImagePointerPlan');
const bounds = { left: 138, top: 667, right: 1182, bottom: 1531, displayId: 0 };
const config = () => ({ operation: 'pinch-out', runLabel: 'mock-owned-image', expectVersionCode: 1000018,
  expectDraftTitle: 'own-fixture', expectImageName: 'own-image.png', ...bounds });
const plain = value => JSON.parse(JSON.stringify(value));
function fixture(overrides = {}) {
  const state = { parameters: config(), zoom: 100, version: 1000018, bounds: { ...bounds }, injectionResult: true,
    calls: [], journal: new Map(), directories: new Set(), finish: undefined, focused: true, active: true, ...overrides };
  const queries = { id(value) { return { id: value, inWindow(bundle) { this.bundle = bundle; return this; } }; },
    text(value) { return { text: value, inWindow(bundle) { this.bundle = bundle; return this; }, withinComponent(value) { this.modal = value; return this; } }; } };
  const component = (id) => ({ id, async getOriginalText() { return id === 'draft-title' ? 'own-fixture' : state.zoom + '%'; },
    async getDescription() { return 'own-image.png'; }, async getBounds() { return { ...state.bounds }; } });
  const driver = {
    async findWindow(filter) {
      assert.equal(filter.bundleName, 'dev.morrow.hmos');
      return { async getBundleName() { return 'dev.morrow.hmos'; }, async isFocused() { return state.focused; }, async isActive() { return state.active; } };
    },
    async findComponents(query) {
      assert.equal(query.bundle, 'dev.morrow.hmos');
      if (query.text) return query.text === 'own-image.png' ? [component('modal-name')] : [];
      return [component(query.id)];
    },
    async screenCap(file) { state.calls.push(['screenCap', file]); return true; },
    async dumpLayout(file) { state.calls.push(['dumpLayout', file]); return true; },
    async delayMs(delay) { state.calls.push(['delayMs', delay]); },
    async doubleClick(x, y) { state.calls.push(['doubleClick', x, y]); state.zoom = 100; },
    async injectMultiPointerAction(matrix, speed) {
      state.calls.push(['multiPointer', matrix, speed]);
      if (state.parameters.operation === 'pinch-out' || state.parameters.operation === 'pinch-moving-focal') state.zoom = 220;
      if (state.parameters.operation === 'pinch-in') state.zoom = 100;
      return state.injectionResult;
    }
  };
  const delegator = {
    getAppContext() { return { filesDir: '/tester/files' }; },
    async executeShellCommand(command) { assert.equal(command, 'bm dump -n dev.morrow.hmos'); return { exitCode: 0, stdResult: JSON.stringify({ versionCode: state.version }) }; },
    async print(message) { state.printed = JSON.parse(message); },
    async finishTest(message, code) { state.finish = { report: JSON.parse(message), code }; }
  };
  const io = { OpenMode: { WRITE_ONLY: 1, CREATE: 64, TRUNC: 512 },
    mkdirSync(target, recursive) { if (!recursive && state.directories.has(target)) throw new Error('Existing run label'); state.directories.add(target); },
    openSync(target) { return { fd: target }; }, writeSync(fd, value) { state.journal.set(fd, JSON.parse(value)); }, fsyncSync() {}, closeSync() {} };
  const testkit = { Driver: { create: () => driver }, ON: queries,
    PointerMatrix: { create(fingers, steps) { return { fingers, steps, points: Array.from({ length: fingers }, () => []), setPoint(finger, step, point) { this.points[finger][step] = plain(point); } }; } },
    abilityDelegatorRegistry: { getAbilityDelegator: () => delegator, getArguments: () => ({ parameters: Object.fromEntries(Object.entries(state.parameters).map(([k, v]) => [k, String(v)])) }) } };
  const api = load('ImageGestureRunner', { '@kit.TestKit': testkit, '@kit.CoreFileKit': { fileIo: io }, './ImagePointerPlan': plans });
  const runner = new api.default(); return { state, runner, async run() { await runner.onRun(); return state.finish; } };
}

test('actual trace generator supplies complete independent 2x9 paths inside observed px canvas', () => {
  for (const operation of ['pinch-out', 'pinch-in', 'pinch-moving-focal', 'pan-two', 'boundary-pan-two']) {
    const plan = plans.imagePointerPlan(operation, bounds); assert.equal(plan.fingers, 2); assert.equal(plan.steps, 9);
    for (const finger of plan.points) {
      assert.equal(finger.length, 9); for (const point of finger) {
        assert.ok(Number.isInteger(point.x) && Number.isInteger(point.y));
        assert.ok(point.x > bounds.left && point.x < bounds.right && point.y > bounds.top && point.y < bounds.bottom);
      }
    }
    assert.notDeepEqual(plain(plan.points[0]), plain(plan.points[1]));
  }
});
test('actual two-finger pure pan preserves separation while moving the focal center', () => {
  const plan = plans.imagePointerPlan('pan-two', bounds), startGap = plan.points[1][0].x - plan.points[0][0].x;
  for (let step = 1; step < plan.steps; step++) assert.ok(Math.abs(plan.points[1][step].x - plan.points[0][step].x - startGap) <= 1);
  assert.ok(plan.points[0].at(-1).x > plan.points[0][0].x); assert.ok(plan.points[0].at(-1).y > plan.points[0][0].y);
});
test('invalid bounds, unknown operations and nonpercentage accessibility labels fail closed', () => {
  for (const value of [NaN, Infinity, -1]) assert.equal(plans.validImageBounds({ ...bounds, left: value }), false);
  assert.throws(() => plans.imagePointerPlan('click-as-pinch', bounds));
  for (const value of ['Reset zoom', '80%', '251%', '', '100% other']) assert.throws(() => plans.imageZoomPercent(value));
  assert.equal(plans.imageZoomPercent('100%'), 100); assert.equal(plans.imageZoomPercent('250%'), 250);
});
test('actual runner submits one true multipointer matrix and records screenshot-only visual scope', async () => {
  const f = fixture(), result = await f.run(), actions = f.state.calls.filter(call => call[0] === 'multiPointer');
  assert.equal(actions.length, 1); assert.equal(actions[0][1].fingers, 2); assert.equal(actions[0][1].steps, 9);
  assert.equal(result.code, 0); assert.equal(result.report.status, 'ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW');
  assert.equal(result.report.visualEffect, 'SCREENSHOTS_CAPTURED_NOT_AUTOMATICALLY_QUALIFIED');
  assert.equal(result.report.tokenStatus, 'NOT_EXPOSED_BY_UI'); assert.equal(result.report.after.zoomPercent, 220);
  assert.equal(f.state.calls.filter(call => call[0] === 'screenCap').length, 2);
  assert.equal(f.state.journal.get('/tester/files/image-gestures/mock-owned-image/input-intent.json').status, 'INPUT_INTENT');
});
test('actual readonly observe never injects a gesture or clicks reset', async () => {
  const f = fixture({ parameters: { ...config(), operation: 'observe' } }), result = await f.run();
  assert.equal(result.report.status, 'READONLY_OBSERVED'); assert.equal(result.code, 0);
  assert.equal(f.state.calls.filter(call => ['multiPointer', 'doubleClick'].includes(call[0])).length, 0);
});
test('actual fixture name, focused foreground window, product version and supplied bounds gate input before a matrix is sent', async () => {
  for (const override of [{ version: 1000019 }, { bounds: { ...bounds, left: 150 } }, { parameters: { ...config(), expectImageName: 'foreign.png' } },
    { parameters: { ...config(), expectDraftTitle: 'foreign-draft' } }, { focused: false }, { active: false }]) {
    const f = fixture(override), result = await f.run(); assert.equal(result.code, 1);
    assert.equal(f.state.calls.filter(call => call[0] === 'multiPointer').length, 0);
  }
});
test('actual failed native injection remains unknown, preserves input intent and never retries', async () => {
  const f = fixture({ injectionResult: false }), result = await f.run();
  assert.equal(result.code, 1); assert.equal(result.report.status, 'UNKNOWN_INPUT_EFFECT');
  assert.equal(f.state.calls.filter(call => call[0] === 'multiPointer').length, 1);
  assert.equal(f.state.journal.has('/tester/files/image-gestures/mock-owned-image/input-intent.json'), true);
});
test('actual repeated label, stopped runner and unprepared zoom cannot replay input', async () => {
  const repeat = fixture(); await repeat.run(); repeat.state.calls.length = 0;
  const firstResult = plain(repeat.state.journal.get('/tester/files/image-gestures/mock-owned-image/result.json'));
  const again = await repeat.run(); assert.equal(again.code, 1); assert.equal(repeat.state.calls.length, 0);
  assert.deepEqual(plain(repeat.state.journal.get('/tester/files/image-gestures/mock-owned-image/result.json')), firstResult);
  const stopped = fixture(); stopped.runner.onStop(); await stopped.run(); assert.equal(stopped.state.calls.length, 0);
  const zoomed = fixture({ zoom: 200 }); await zoomed.run(); assert.equal(zoomed.state.calls.filter(c => c[0] === 'multiPointer').length, 0);
});
test('command printer validates exact fixture/bounds/version and never runs device commands', () => {
  const { command } = require('./image-gesture-tester/command.cjs');
  const result = command(config()); assert.equal(result.device, 'NOT_RUN'); assert.equal(result.install, 'NOT_RUN');
  assert.ok(result.remoteShellCommand.includes('ImageGestureRunner')); assert.equal(result.targetBundle, 'dev.morrow.hmos');
  for (const override of [{ expectVersionCode: undefined }, { left: NaN }, { expectDraftTitle: '' }, { runLabel: '../old' }]) assert.throws(() => command({ ...config(), ...override }));
});
test('independent project preparation blocks paths outside its owned work directory', () => {
  const { prepare } = require('./image-gesture-tester/prepare.cjs');
  assert.throws(() => prepare(path.join(__dirname, '..', 'entry'))); assert.throws(() => prepare(path.join(directory, 'work')));
});
