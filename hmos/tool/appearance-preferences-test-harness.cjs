'use strict';
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const cache = new Map();
function load(name) {
  if (cache.has(name)) return cache.get(name);
  assert.ok(['Appearance', 'AppearancePreferences'].includes(name));
  const file = path.resolve(__dirname, '../entry/src/main/ets/model/' + name + '.ets'), exports = {};
  const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), { fileName: file, reportDiagnostics: true,
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS } });
  assert.equal((compiled.diagnostics || []).filter(x => x.category === ts.DiagnosticCategory.Error).length, 0);
  cache.set(name, exports);
  vm.runInNewContext(compiled.outputText, { exports, require: dependency => load(dependency.replace('./', '')) });
  return exports;
}
const appearance = load('Appearance'), preferences = load('AppearancePreferences');
function snapshot(patch = {}) { return Object.assign(new appearance.AppearanceSnapshot(), patch); }
function material(id, patch = {}) { return Object.assign(new appearance.MaterialChoice(), { id }, patch); }
const plain = value => JSON.parse(JSON.stringify(value));
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
const tick = () => new Promise(resolve => setImmediate(resolve));
let namespaceId = 0;
function fixture(options = {}) {
  const initial = Object.hasOwn(options, 'raw') ? options.raw : JSON.stringify(snapshot());
  const backend = { cache: initial, disk: initial }, calls = [], notices = [], owner = { epoch: 'page-A:foreground-1', current: true };
  let reads = 0, puts = 0, flushes = 0, model;
  const port = {
    namespace: options.namespace || 'controlled-appearance-' + (++namespaceId),
    async read() { const n = ++reads; calls.push({ kind: 'read', n }); return options.read ? options.read(n, backend) : backend.cache; },
    async put(raw) { const n = ++puts; calls.push({ kind: 'put', n, raw }); if (options.put) await options.put(raw, n, backend); else backend.cache = raw; },
    async flush() { const n = ++flushes; calls.push({ kind: 'flush', n }); if (options.flush) await options.flush(n, backend); else backend.disk = backend.cache; }
  };
  model = new preferences.AppearancePreferences(port, { owned: () => owner.current, owner: () => owner.epoch,
    changed() { notices.push({ owner: owner.epoch, view: plain(model.view()) }); if (options.changed) options.changed(model); } });
  return { model, port, backend, calls, notices, owner, counts: () => ({ reads, puts, flushes }) };
}
module.exports = { ...appearance, ...preferences, snapshot, material, plain, deferred, tick, fixture };
