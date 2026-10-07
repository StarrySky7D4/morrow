/* Execute the actual AttachmentOpen.ets with synthetic SDK providers.
 * This verifies ownership/lifetime decisions, not a device viewer or format.
 */
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/model/AttachmentOpen.ets');
const source = fs.readFileSync(sourcePath, 'utf8');
const result = ts.transpileModule(source, { compilerOptions: {
  module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020,
}, fileName: sourcePath, reportDiagnostics: true });
assert.equal(result.diagnostics.filter(x => x.category === ts.DiagnosticCategory.Error).length, 0);
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const settle = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };

function harness() {
  const exports = {}, logs = [], states = [], known = new Map(), faults = {};
  const ctx = { cacheDir: '/app/cache' };
  const uri = value => 'file://dev.morrow.hmos' + value.split('/').map(encodeURIComponent).join('/');
  let displayed = false, supported = true;
  const preview = {
    async hasDisplayed(context) {
      assert.equal(context.cacheDir, ctx.cacheDir); logs.push(['hasDisplayed']);
      if (faults.observe) throw new Error('synthetic observation failure');
      if (faults.observeOnce) { delete faults.observeOnce; throw new Error('synthetic observation failure'); }
      return displayed;
    },
    async canPreview(context, value) {
      logs.push(['canPreview', value]);
      if (faults.checkGate) return await faults.checkGate.promise;
      if (faults.canPreview) throw new Error('synthetic support failure');
      return supported;
    },
    async openPreview(context, info) {
      logs.push(['openPreview', { ...info }]);
      if (faults.openGate) await faults.openGate.promise;
      if (faults.openError) { if (faults.shownOnError) displayed = true; throw new Error('synthetic unknown dispatch result'); }
      if (!faults.notDisplayed) displayed = true;
      if (faults.observeAfterOpen) faults.observe = true;
    },
    async closePreview(context) {
      logs.push(['closePreview']);
      if (faults.close) throw new Error('synthetic close failure');
      if (!faults.stillDisplayed) displayed = false;
    },
  };
  vm.runInNewContext(result.outputText, { exports, Map, Number, Promise, Error,
    require(name) {
      if (name === '@kit.CoreFileKit') return { fileUri: { getUriFromPath: uri } };
      if (name === '@kit.PreviewKit') return { filePreview: preview };
      if (name === '@kit.AbilityKit' || name === './AttachmentFiles') return {};
      throw new Error('Unexpected source dependency: ' + name);
    },
  }, { filename: sourcePath });
  const active = value => {
    const entry = known.get(value.token);
    return entry?.handle === value && Object.keys(entry.copy).every(key => entry.copy[key] === value[key]);
  };
  const release = async token => {
    logs.push(['release', token]);
    if (faults.release) throw new Error('synthetic cleanup failure');
    assert.ok(known.has(token), 'release only a known original capability'); known.delete(token);
  };
  const create = (changed = state => states.push(state)) => new exports.AttachmentOpen(ctx, active, release, changed);
  const lease = (name = '中文 附件.txt', token = 'morrow-preview-ABC123') => {
    const value = { token, path: ctx.cacheDir + '/' + token + '/' + name, name,
      byte_length: '3', sha256: 'a'.repeat(64), purpose: 'open' };
    value.uri = uri(value.path); known.set(token, { handle: value, copy: { ...value } }); return value;
  };
  return { api: exports, model: create(), create, lease, logs, states, known, faults,
    show(value) { displayed = value; }, supported(value) { supported = value; }, active };
}
const calls = (f, name) => f.logs.filter(x => x[0] === name);

test('verified private URI and SDK MIME reach the system viewer, launch never releases bytes', async () => {
  const f = harness(), value = f.lease(); assert.equal(await f.model.open(value), true);
  assert.deepEqual(calls(f, 'openPreview')[0][1], { title: value.name, uri: value.uri, mimeType: 'text/plain' });
  assert.equal(f.model.snapshot().phase, 'displayed'); assert.equal(f.model.snapshot().retained, true);
  assert.equal(calls(f, 'release').length, 0); assert.equal(f.model.owns(value), true);
});

test('forged, wrong-purpose and arbitrary provider URI candidates are rejected without adopting or opening', async () => {
  const f = harness(), value = f.lease();
  assert.equal(await f.model.open({ ...value }), false);
  for (const change of [{ purpose: 'inline' }, { uri: 'content://other/document' }, { path: '/other/cache/a.txt' },
    { byte_length: '01' }, { byte_length: String(200 * 1024 * 1024 + 1) }, { sha256: 'A'.repeat(64) }]) {
    const candidate = Object.assign(f.lease('a.txt'), change);
    // Even a synthetic ledger admitting it cannot bypass structural checks.
    f.known.set(candidate.token, { handle: candidate, copy: { ...candidate } });
    assert.equal(await f.model.open(candidate), false);
  }
  assert.equal(f.model.snapshot().retained, false); assert.equal(calls(f, 'openPreview').length, 0);
  assert.equal(calls(f, 'release').length, 0);
});

test('all guarded executable families avoid system dispatch and safely release unopened lease', async () => {
  for (const ext of ['exe', 'com', 'msi', 'bat', 'cmd', 'ps1', 'vbs', 'js', 'lnk', 'url', 'scr', 'reg']) {
    const f = harness(), value = f.lease('附件.' + ext.toUpperCase());
    assert.equal(f.api.attachmentOpenBlocked(value.name), true); assert.equal(await f.model.open(value), true);
    assert.equal(f.model.snapshot().phase, 'unsupported'); assert.equal(f.model.snapshot().retained, false);
    assert.equal(calls(f, 'hasDisplayed').length, 0); assert.equal(calls(f, 'openPreview').length, 0);
    assert.deepEqual(calls(f, 'release'), [['release', value.token]]);
  }
});

test('MIME hints follow PreviewKit values and unknown names remain unspecified', () => {
  const f = harness(), expected = { txt:'text/plain', cpp:'text/x-c++src', c:'text/x-csrc', h:'text/x-chdr', java:'text/x-java',
    xhtml:'application/xhtml+xml', xml:'text/xml', html:'text/html', htm:'text/html', jpg:'image/jpeg', jpeg:'image/jpeg',
    png:'image/png', gif:'image/gif', webp:'image/webp', bmp:'image/bmp', svg:'image/svg+xml', m4a:'audio/mp4a-latm',
    aac:'audio/aac', mp3:'audio/mpeg', ogg:'audio/ogg', wav:'audio/x-wav', mp4:'video/mp4', mkv:'video/x-matroska',
    ts:'video/mp2ts', pdf:'application/pdf', doc:'application/msword', docx:'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    xls:'application/vnd.ms-excel', xlsx:'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet', ppt:'application/vnd.ms-powerpoint',
    pptx:'application/vnd.openxmlformats-officedocument.presentationml.presentation', csv:'text/csv', ofd:'general.ofd' };
  for (const [ext, mime] of Object.entries(expected)) assert.equal(f.api.attachmentPreviewMime('中文 name.' + ext.toUpperCase()), mime);
  for (const name of ['README', 'archive.zip', 'name.application/pdf', 'run.exe', 'data.json']) assert.equal(f.api.attachmentPreviewMime(name), '');
});

test('support failure or unsupported file before dispatch removes own file and does not launch', async () => {
  for (const mode of ['unsupported', 'check-failure', 'initial-observation-failure']) {
    const f = harness(), value = f.lease('unknown.bin');
    if (mode === 'unsupported') f.supported(false);
    if (mode === 'check-failure') f.faults.canPreview = true;
    if (mode === 'initial-observation-failure') f.faults.observe = true;
    await f.model.open(value);
    assert.equal(f.model.snapshot().retained, false); assert.equal(calls(f, 'openPreview').length, 0);
    assert.equal(calls(f, 'release').length, 1);
    assert.equal(f.model.snapshot().phase, mode === 'unsupported' ? 'unsupported' : 'error');
  }
});

test('a pre-existing singleton window is never replaced or closed by candidate opening', async () => {
  const f = harness(); f.show(true); await f.model.open(f.lease()); await f.model.close();
  assert.equal(f.model.snapshot().phase, 'error'); assert.equal(calls(f, 'canPreview').length, 0);
  assert.equal(calls(f, 'openPreview').length, 0); assert.equal(calls(f, 'closePreview').length, 0);
  assert.equal(f.model.snapshot().retained, false);
});

test('UI navigation, foreground observation and window disappearance retain bytes for possible external readers', async () => {
  const f = harness(), value = f.lease(); await f.model.open(value); f.show(false); await f.model.reconcile();
  assert.equal(f.model.snapshot().phase, 'retained'); assert.equal(f.model.snapshot().can_finish, true);
  assert.equal(calls(f, 'release').length, 0); assert.equal(f.known.has(value.token), true);
  const replacement = f.create(); assert.equal(replacement.owns(value), true);
  await replacement.reconcile(); assert.equal(calls(f, 'release').length, 0);
});

test('close and successful closure observation preserve bytes until explicit all-readers-finished action', async () => {
  const f = harness(), value = f.lease(); await f.model.open(value); await f.model.close();
  assert.equal(calls(f, 'closePreview').length, 1); assert.equal(f.model.snapshot().phase, 'retained');
  assert.equal(calls(f, 'release').length, 0); await f.model.finishReadersClosed();
  assert.equal(f.model.snapshot().phase, 'idle'); assert.equal(f.model.snapshot().retained, false);
  assert.deepEqual(calls(f, 'release'), [['release', value.token]]);
});

test('finish refuses while the system preview is still displayed', async () => {
  const f = harness(); await f.model.open(f.lease()); await f.model.finishReadersClosed();
  assert.equal(f.model.snapshot().phase, 'displayed'); assert.equal(f.model.snapshot().can_finish, false);
  assert.equal(calls(f, 'release').length, 0); assert.equal(calls(f, 'closePreview').length, 0);
});

test('open rejection after dispatch is unknown and never automatically replayed or removed', async () => {
  const f = harness(), value = f.lease(); f.faults.openError = true; f.faults.shownOnError = true;
  await f.model.open(value); assert.equal(f.model.snapshot().phase, 'unknown');
  assert.equal(await f.model.open(value), false); await f.model.reconcile();
  assert.equal(f.model.snapshot().phase, 'displayed'); assert.equal(calls(f, 'openPreview').length, 1);
  assert.equal(calls(f, 'release').length, 0);
});

test('silent launch without observed window remains retained rather than being treated as reader completion', async () => {
  const f = harness(); f.faults.notDisplayed = true; await f.model.open(f.lease());
  assert.equal(f.model.snapshot().phase, 'retained'); assert.equal(calls(f, 'release').length, 0);
});

test('observation and closure failures retain the known capability and permit only explicit retry', async () => {
  const f = harness(); await f.model.open(f.lease()); f.faults.close = true; await f.model.close();
  assert.equal(f.model.snapshot().phase, 'unknown'); assert.equal(calls(f, 'release').length, 0);
  delete f.faults.close; f.faults.observe = true; await f.model.finishReadersClosed();
  assert.equal(f.model.snapshot().phase, 'unknown'); assert.equal(calls(f, 'release').length, 0);
  delete f.faults.observe; await f.model.close(); await f.model.finishReadersClosed();
  assert.equal(f.model.snapshot().retained, false); assert.equal(calls(f, 'openPreview').length, 1);
});

test('close resolution without observed closure cannot authorize removal', async () => {
  const f = harness(); await f.model.open(f.lease()); f.faults.stillDisplayed = true; await f.model.close();
  assert.equal(f.model.snapshot().phase, 'displayed'); await f.model.finishReadersClosed();
  assert.equal(calls(f, 'release').length, 0);
});

test('failed file removal keeps original capability across adapter recreation, retry does not reopen', async () => {
  const f = harness(), value = f.lease(); await f.model.open(value); await f.model.close();
  f.faults.release = true; await f.model.finishReadersClosed(); assert.equal(f.model.snapshot().phase, 'cleanup_failed');
  const replacement = f.create(); assert.equal(replacement.owns(value), true);
  assert.equal(await replacement.open(f.lease('other.txt', 'morrow-preview-DEF456')), false);
  delete f.faults.release; await replacement.finishReadersClosed();
  assert.equal(replacement.snapshot().retained, false); assert.equal(calls(f, 'openPreview').length, 1);
  assert.deepEqual(calls(f, 'release').map(x => x[1]), [value.token, value.token]);
});

test('failed pre-dispatch cleanup does not claim ownership of unrelated singleton on later retries', async () => {
  const f = harness(); f.faults.release = true; f.supported(false); await f.model.open(f.lease());
  assert.equal(f.model.snapshot().phase, 'cleanup_failed'); f.show(true); const count = calls(f, 'hasDisplayed').length;
  await f.model.reconcile(); await f.model.close(); assert.equal(calls(f, 'hasDisplayed').length, count);
  assert.equal(calls(f, 'closePreview').length, 0); delete f.faults.release; await f.model.finishReadersClosed();
  assert.equal(f.model.snapshot().retained, false); assert.equal(calls(f, 'hasDisplayed').length, count);
});

test('mutation during support await cannot inject another file URI into dispatch', async () => {
  const f = harness(), value = f.lease(), gate = deferred(); f.faults.checkGate = gate;
  const opening = f.model.open(value); await settle(); value.uri = 'file://docs/storage/Users/currentUser/private.txt';
  value.token = 'morrow-preview-DEF456'; gate.resolve(true); await opening;
  assert.equal(calls(f, 'openPreview').length, 0);
  assert.deepEqual(calls(f, 'release'), [['release', 'morrow-preview-ABC123']]);
});

test('queued finish waits for dispatch settlement and never removes in-flight bytes', async () => {
  const f = harness(), value = f.lease(), gate = deferred(); f.faults.openGate = gate;
  const opening = f.model.open(value); await settle(); const finishing = f.model.finishReadersClosed(); await settle();
  assert.equal(f.model.snapshot().busy, true); assert.equal(calls(f, 'release').length, 0);
  gate.resolve(); await opening; await finishing;
  assert.equal(f.model.snapshot().phase, 'displayed'); assert.equal(calls(f, 'release').length, 0);
});

test('returned UI snapshots cannot alter retained session or expose private URI', async () => {
  const f = harness(); await f.model.open(f.lease()); const view = f.model.snapshot(); view.retained = false; view.phase = 'idle';
  assert.equal(f.model.snapshot().retained, true); assert.equal(f.model.snapshot().phase, 'displayed');
  assert.equal(Object.hasOwn(view, 'uri'), false); assert.equal(Object.hasOwn(view, 'path'), false);
});

test('cleanup uses original immutable token after a post-dispatch caller mutation', async () => {
  const f = harness(), value = f.lease(); await f.model.open(value); value.token = 'other-handle'; value.path = '/other';
  f.show(false); await f.model.finishReadersClosed();
  assert.deepEqual(calls(f, 'release'), [['release', 'morrow-preview-ABC123']]);
});
