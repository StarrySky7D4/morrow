const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), crypto = require('node:crypto');
const { test } = require('node:test'), assert = require('node:assert/strict');
const ts = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modelPath = path.resolve(__dirname, '../entry/src/main/ets/model/AttachmentImageGestures.ets');
const previewPath = path.resolve(__dirname, '../entry/src/main/ets/pages/AttachmentImagePreview.ets');
const modelSource = fs.readFileSync(modelPath, 'utf8');
const previewSource = fs.readFileSync(previewPath, 'utf8').replace(/\r\n/g, '\n');
function compile(source, dependencies = {}) {
  const compiled = ts.transpileModule(source, { reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.deepEqual((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), []);
  const exports = {};
  vm.runInNewContext(compiled.outputText, { exports, Error, require: name => dependencies[name] || {} });
  return exports;
}
const api = compile(modelSource);
// Execute the component's actual fields and methods verbatim. Only ArkUI field
// decorators/struct syntax and the declarative builders are removed; no gesture,
// owner, decode, lifecycle or visibility implementation is reproduced here.
const methods = previewSource.slice(0, previewSource.indexOf('\n  @Builder\n'))
  .replace('@Component\n', '').replace('export struct ', 'export class ')
  .replace(/@(?:Prop|State)(?:\s+@Watch\('[^']+'\))?\s*/g, '');
const component = compile(methods + '\n}\n' + previewSource.slice(previewSource.indexOf('\nclass ImagePreviewSession')),
  { '../model/AttachmentImageGestures': api, '../model/UiStrings': { uiText: value => value } });
const plain = value => JSON.parse(JSON.stringify(value));
const expected = (scale = 1, x = 0, y = 0) => ({ scale, x, y });
const event = (values = {}) => ({ timestamp: 10, offsetX: 0, offsetY: 0, scale: 1,
  pinchCenterX: 310, pinchCenterY: 160, ...values });
function fixture(token = 'A', width = 620, height = 320) {
  const model = new api.AttachmentImageGestures(), scope = model.bind(token, width, height);
  model.setReady(token, true); return { model, scope };
}
function componentFixture() {
  const view = new component.AttachmentImagePreview();
  view.preview = { token: 'A', uri: 'file://verified/A.png', name: 'original-A.png' };
  view.aboutToAppear();
  const decode = view.completeFor(view.preview.token, view.preview.uri, view.epoch);
  decode({ loadingStatus: 1, width: 2000, height: 1000 });
  return { view, decode, get session() { return view.imageSession; } };
}
function transform(view) { return expected(view.zoom, view.offsetX, view.offsetY); }

test('actual ETS model and component-method harness expose their fresh source identity', t => {
  t.diagnostic('AttachmentImageGestures.ets SHA256 ' + crypto.createHash('sha256').update(modelSource).digest('hex'));
  t.diagnostic('AttachmentImagePreview.ets normalized-source SHA256 ' + crypto.createHash('sha256').update(previewSource).digest('hex'));
  assert.equal(typeof api.AttachmentImageGestures, 'function');
  assert.equal(typeof component.AttachmentImagePreview, 'function');
});

test('tight Flutter layout box has effective scale 1 through 2.5 with clipped pan at identity', () => {
  const f = fixture(), p = f.model.beginPinch(f.scope, 1, 310, 160);
  f.model.movePinch(p, .8, 400, 240); assert.deepEqual(plain(f.model.view()), expected());
  f.model.movePinch(p, 9, 310, 160); assert.equal(f.model.view().scale, 2.5);
  f.model.endPinch(p, .1, 310, 160); assert.deepEqual(plain(f.model.view()), expected());
});

test('off-center pinch preserves the scene point underneath the fingers', () => {
  const f = fixture(), p = f.model.beginPinch(f.scope, 1, 450, 160);
  f.model.movePinch(p, 2, 450, 160); assert.deepEqual(plain(f.model.view()), expected(2, -140, 0));
});

test('moving two-finger focal point translates the already scaled image', () => {
  const f = fixture(), p = f.model.beginPinch(f.scope, 1, 450, 160);
  f.model.movePinch(p, 2, 450, 160); f.model.movePinch(p, 2, 500, 200);
  assert.deepEqual(plain(f.model.view()), expected(2, -90, 40));
});

test('pinch registration scale is normalized without a recognition-threshold jump', () => {
  const f = fixture(), p = f.model.beginPinch(f.scope, 1.1, 310, 160);
  assert.deepEqual(plain(f.model.view()), expected());
  f.model.movePinch(p, 2.2, 310, 160); assert.deepEqual(plain(f.model.view()), expected(2));
});

test('single-finger pan uses the contain Image layout box, including letterboxing', () => {
  const f = fixture(); f.model.zoomBy(f.scope, 1);
  const p = f.model.beginPan(f.scope, 1, 0, 0); f.model.movePan(p, 1000, -1000);
  assert.deepEqual(plain(f.model.view()), expected(2, 310, -160));
});

test('both directions reverse immediately after hitting a pan boundary', () => {
  const f = fixture(); f.model.zoomBy(f.scope, 1);
  const p = f.model.beginPan(f.scope, 1, 0, 0);
  f.model.movePan(p, 500, -500); f.model.movePan(p, 490, -490);
  assert.deepEqual(plain(f.model.view()), expected(2, 300, -150));
});

test('moving pinch focal point reverses immediately after either boundary clips it', () => {
  const f = fixture(); f.model.zoomBy(f.scope, 1);
  const p = f.model.beginPinch(f.scope, 1, 310, 160);
  f.model.movePinch(p, 1, 1000, -1000); f.model.movePinch(p, 1, 990, -990);
  assert.deepEqual(plain(f.model.view()), expected(2, 300, -150));
});

test('two-finger translation works without requiring a pinch distance change', () => {
  const f = fixture(); f.model.zoomBy(f.scope, 1);
  const p = f.model.beginPan(f.scope, 2, 12, -12); f.model.movePan(p, 62, 28);
  assert.deepEqual(plain(f.model.view()), expected(2, 50, 40));
});

test('two-finger pan supersedes one-finger pan and its late cancellation cannot cancel the new owner', () => {
  const f = fixture(); f.model.zoomBy(f.scope, 1);
  const one = f.model.beginPan(f.scope, 1, 0, 0), two = f.model.beginPan(f.scope, 2, 0, 0);
  assert.equal(f.model.movePan(one, 100, 100), false); f.model.cancel(one);
  assert.equal(f.model.movePan(two, 50, 40), true); assert.deepEqual(plain(f.model.view()), expected(2, 50, 40));
});

test('pinch takes over both pan recognizers and parallel updates cannot double-apply translation', () => {
  const f = fixture(); f.model.zoomBy(f.scope, .5);
  const one = f.model.beginPan(f.scope, 1, 0, 0), two = f.model.beginPan(f.scope, 2, 0, 0);
  const p = f.model.beginPinch(f.scope, 1, 310, 160);
  f.model.movePinch(p, 1.2, 340, 180);
  const before = plain(f.model.view());
  assert.equal(f.model.movePan(one, 200, 200), false); assert.equal(f.model.endPan(two, 200, 200), false);
  assert.equal(f.model.beginPan(f.scope, 1, 0, 0).id, 0); assert.equal(f.model.beginPan(f.scope, 2, 0, 0).id, 0);
  assert.deepEqual(plain(f.model.view()), before);
});

test('finished and cancelled tickets cannot update again; fresh gestures can proceed', () => {
  const f = fixture(), p = f.model.beginPinch(f.scope, 1, 310, 160);
  assert.equal(f.model.endPinch(p, 2, 310, 160), true); assert.equal(f.model.movePinch(p, 1, 310, 160), false);
  const pan = f.model.beginPan(f.scope, 1, 0, 0); f.model.movePan(pan, 50, 0); f.model.cancel(pan);
  assert.equal(f.model.endPan(pan, 100, 0), false); assert.deepEqual(plain(f.model.view()), expected(2, 50, 0));
  const next = f.model.beginPan(f.scope, 1, 0, 0); assert.equal(f.model.endPan(next, 20, 0), true);
});

test('old end and cancel tickets never control a newer gesture in the same preview', () => {
  const f = fixture(); f.model.zoomBy(f.scope, 1);
  const old = f.model.beginPan(f.scope, 1, 0, 0), next = f.model.beginPan(f.scope, 1, 0, 0);
  assert.equal(f.model.endPan(old, 300, 160), false); f.model.cancel(old);
  assert.equal(f.model.movePan(next, 20, 30), true); assert.deepEqual(plain(f.model.view()), expected(2, 20, 30));
});

test('replacement, including reuse of the same token, revokes old starts, updates, ends and reset', () => {
  for (const token of ['B', 'A']) {
    const f = fixture(), p = f.model.beginPinch(f.scope, 1, 310, 160);
    const next = f.model.bind(token, 620, 320); f.model.setReady(token, true);
    assert.equal(f.model.beginPan(f.scope, 1, 0, 0).id, 0);
    assert.equal(f.model.endPinch(p, 2, 310, 160), false); assert.equal(f.model.reset(f.scope), false);
    assert.equal(f.model.zoomBy(next, 1), true); assert.deepEqual(plain(f.model.view()), expected(2));
  }
});

test('geometry changes reset the transform and reject all former geometry tickets', () => {
  const f = fixture(), p = f.model.beginPinch(f.scope, 1, 310, 160);
  f.model.movePinch(p, 2, 310, 160); const next = f.model.resize('A', 320, 620);
  assert.equal(f.model.endPinch(p, 2.5, 310, 160), false); assert.equal(f.model.zoomBy(f.scope, .5), false);
  f.model.zoomBy(next, 1); const pan = f.model.beginPan(next, 2, 0, 0); f.model.movePan(pan, 999, 999);
  assert.deepEqual(plain(f.model.view()), expected(2, 160, 310));
});

test('loading/background invalidation and disposal revoke tickets without autonomous replay', () => {
  for (const stop of ['not-ready', 'invalidate', 'dispose']) {
    const f = fixture(), p = f.model.beginPinch(f.scope, 1, 310, 160); f.model.movePinch(p, 2, 310, 160);
    if (stop === 'not-ready') { f.model.setReady('A', false); f.model.setReady('A', true); }
    if (stop === 'invalidate') { f.model.invalidate(); }
    if (stop === 'dispose') { f.model.dispose(); }
    const before = plain(f.model.view()); assert.equal(f.model.endPinch(p, 1, 310, 160), false);
    assert.equal(f.model.beginPan(f.scope, 1, 0, 0).id, 0); assert.deepEqual(plain(f.model.view()), before);
  }
});

test('reset and button zoom invalidate any currently admitted gesture tickets', () => {
  const f = fixture(), p = f.model.beginPinch(f.scope, 1, 310, 160); f.model.movePinch(p, 2, 310, 160);
  f.model.reset(f.scope); assert.equal(f.model.endPinch(p, 2.5, 500, 250), false);
  assert.deepEqual(plain(f.model.view()), expected());
  const next = f.model.beginPinch(f.scope, 1, 310, 160); f.model.zoomBy(f.scope, 1);
  assert.equal(f.model.movePinch(next, .8, 310, 160), false); assert.deepEqual(plain(f.model.view()), expected(2));
});

test('invalid gesture numbers and unusable viewport sizes cannot publish a corrupt transform', () => {
  for (const value of [NaN, Infinity, -Infinity, 0, -1]) {
    const f = fixture(), p = f.model.beginPinch(f.scope, 1, 310, 160);
    assert.equal(f.model.movePinch(p, value, 310, 160), false); assert.deepEqual(plain(f.model.view()), expected());
  }
  for (const size of [NaN, Infinity, 0, -1]) {
    const f = fixture('A', size, 320); assert.equal(f.model.beginPan(f.scope, 1, 0, 0).id, 0);
  }
  const f = fixture(); assert.equal(f.model.beginPan(f.scope, 3, 0, 0).id, 0);
  assert.equal(f.model.beginPan(f.scope, 1, NaN, 0).id, 0); assert.equal(f.model.zoomBy(f.scope, NaN), false);
});

test('returned view and scope snapshots cannot mutate the owned transform', () => {
  const f = fixture(), view = f.model.view(), scope = f.model.scope(); view.scale = 99; view.x = 99; scope.token = 'B';
  assert.deepEqual(plain(f.model.view()), expected()); assert.equal(f.model.scope().token, 'A');
});

test('actual component only admits status-1 decoded, finite, current exact token and URI', () => {
  const f = componentFixture(); assert.equal(f.view.canInteract(), true);
  f.view.preview = { token: 'B', uri: 'file://verified/B.png', name: 'B' }; f.view.previewChanged();
  f.decode({ loadingStatus: 1, width: 1, height: 1 }); assert.equal(f.view.decoded, false);
  const decode = f.view.completeFor('B', f.view.preview.uri, f.view.epoch);
  decode({ loadingStatus: 0, width: 1, height: 1 }); decode({ loadingStatus: 1, width: NaN, height: 1 });
  assert.equal(f.view.canInteract(), false); decode({ loadingStatus: 1, width: 1, height: 1 });
  assert.equal(f.view.canInteract(), true);
  f.view.preview.uri = 'file://verified/other.png'; decode({ loadingStatus: 1, width: 1, height: 1 });
  assert.equal(f.view.admitted(f.session), false);
});

test('actual component pinch uses moving focal point and actual pan binds a separate ticket', () => {
  const f = componentFixture(), s = f.session;
  f.view.pinchStart(s, event({ pinchCenterX: 450 }));
  f.view.pinchUpdate(s, event({ timestamp: 11, scale: 2, pinchCenterX: 500, pinchCenterY: 200 }));
  assert.deepEqual(transform(f.view), expected(2, -90, 40));
  f.view.panStart(s, 2, event({ timestamp: 12 })); f.view.panUpdate(s, 2, event({ timestamp: 13, offsetX: 200 }));
  assert.deepEqual(transform(f.view), expected(2, -90, 40));
  f.view.pinchEnd(s, event({ timestamp: 14, scale: 2, pinchCenterX: 500, pinchCenterY: 200 }));
  f.view.panStart(s, 1, event({ timestamp: 15 })); f.view.panEnd(s, 1, event({ timestamp: 16, offsetX: 20 }));
  assert.deepEqual(transform(f.view), expected(2, -70, 40));
});

test('actual component former preview callbacks cannot alter or fail the next preview', () => {
  const f = componentFixture(), old = f.session;
  const failed = f.view.failureFor(old.token, old.uri, old.epoch);
  f.view.pinchStart(old, event());
  f.view.preview = { token: 'B', uri: 'file://verified/B.png', name: 'B' }; f.view.previewChanged();
  f.view.completeFor('B', f.view.preview.uri, f.view.epoch)({ loadingStatus: 1, width: 40, height: 80 });
  const current = f.session; f.view.zoomFor(current, .5);
  f.view.pinchEnd(old, event({ timestamp: 20, scale: 2.5 })); f.view.zoomFor(old, 1); f.view.resetFor(old); failed();
  f.decode({ loadingStatus: 1, width: 40, height: 80 });
  assert.deepEqual(transform(f.view), expected(1.5)); assert.equal(f.view.decodeFailed, false);
});

test('actual component geometry replacement revokes old gesture closures while retaining decode identity', () => {
  const f = componentFixture(), old = f.session; f.view.pinchStart(old, event());
  f.view.contentWidth = 320; f.view.contentHeight = 620; f.view.geometryChanged();
  f.view.zoomFor(f.session, 1); f.view.pinchEnd(old, event({ timestamp: 20, scale: 2.5 })); f.view.resetFor(old);
  assert.deepEqual(transform(f.view), expected(2)); assert.equal(f.view.decoded, true);
});

test('actual component owner replacement changes its real ForEach key even if geometry and availability are unchanged', () => {
  const f = componentFixture(), original = f.session; f.view.geometryChanged();
  const geometry = f.session; assert.notEqual(geometry.key(), original.key());
  f.view.availabilityChanged(); assert.notEqual(f.session.key(), geometry.key());
  assert.equal(f.view.admitted(geometry), false); assert.equal(f.view.admitted(f.session), true);
  assert.match(previewSource, /\(session: ImagePreviewSession\): string => session\.key\(\)/);
});

test('actual component foreground resume replaces callback owner and retains the current transform', () => {
  const f = componentFixture(), old = f.session; f.view.zoomFor(old, 1); f.view.panStart(old, 1, event());
  f.view.visibilityFor(old, false); f.view.panUpdate(old, 1, event({ timestamp: 11, offsetX: 100 }));
  assert.equal(f.view.canInteract(), false); f.view.visibilityFor(old, true);
  assert.notEqual(f.session, old); f.view.panStart(old, 1, event({ timestamp: 12 }));
  f.view.panEnd(old, 1, event({ timestamp: 13, offsetX: 100 }));
  f.view.panStart(f.session, 1, event({ timestamp: 14 }));
  f.view.panEnd(f.session, 1, event({ timestamp: 15, offsetX: 20 }));
  assert.deepEqual(transform(f.view), expected(2, 20, 0));
});

test('actual component read loading/failure revokes former owner without resetting to another resource', () => {
  for (const blocker of ['loading', 'failure']) {
    const f = componentFixture(), old = f.session; f.view.zoomFor(old, 1); f.view.pinchStart(old, event());
    f.view[blocker] = blocker === 'loading' ? true : 'original source failed'; f.view.availabilityChanged();
    f.view.pinchEnd(old, event({ timestamp: 12, scale: 2.5 })); assert.equal(f.view.canInteract(), false);
    f.view[blocker] = blocker === 'loading' ? false : ''; f.view.availabilityChanged();
    f.view.resetFor(old); assert.deepEqual(transform(f.view), expected(2));
    f.view.resetFor(f.session); assert.deepEqual(transform(f.view), expected());
  }
});

test('actual component current decode failure stops gestures and preserves the original verified export', () => {
  const f = componentFixture(), s = f.session, original = plain(f.view.preview);
  f.view.zoomFor(s, 1); f.view.pinchStart(s, event()); f.view.failureFor(s.token, s.uri, s.epoch)();
  f.view.pinchEnd(s, event({ timestamp: 20, scale: 2.5 })); f.decode({ loadingStatus: 1, width: 10, height: 10 });
  assert.equal(f.view.decodeFailed, true); assert.equal(f.view.canInteract(), false);
  assert.deepEqual(transform(f.view), expected()); assert.deepEqual(plain(f.view.preview), original);
});

test('actual component disappearance rejects source, gesture, reset and visibility late callbacks', () => {
  const f = componentFixture(), s = f.session, fail = f.view.failureFor(s.token, s.uri, s.epoch);
  f.view.pinchStart(s, event()); f.view.aboutToDisappear();
  f.view.pinchEnd(s, event({ timestamp: 20, scale: 2.5 })); f.view.resetFor(s); f.view.visibilityFor(s, true); fail();
  f.decode({ loadingStatus: 1, width: 10, height: 10 }); assert.equal(f.view.canInteract(), false);
  assert.deepEqual(transform(f.view), expected());
});

test('actual component timestamp fences prevent queued old pan end/cancel from borrowing a new ticket', () => {
  const f = componentFixture(), s = f.session; f.view.zoomFor(s, 1);
  f.view.panStart(s, 1, event({ timestamp: 100 })); f.view.panStart(s, 1, event({ timestamp: 200 }));
  f.view.panEnd(s, 1, event({ timestamp: 150, offsetX: 310 }));
  f.view.panCancel(s, 1, event({ timestamp: 160 }));
  f.view.panEnd(s, 1, event({ timestamp: 210, offsetX: 20 })); assert.deepEqual(transform(f.view), expected(2, 20, 0));
});

test('actual component timestamp fences prevent queued old pinch end/cancel from borrowing a new ticket', () => {
  const f = componentFixture(), s = f.session;
  f.view.pinchStart(s, event({ timestamp: 100 })); f.view.pinchStart(s, event({ timestamp: 200 }));
  f.view.pinchEnd(s, event({ timestamp: 150, scale: 2.5 })); f.view.pinchCancel(s, event({ timestamp: 160 }));
  f.view.pinchEnd(s, event({ timestamp: 210, scale: 2 })); assert.deepEqual(transform(f.view), expected(2));
});
