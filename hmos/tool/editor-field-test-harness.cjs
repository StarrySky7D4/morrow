'use strict';
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modules = new Map();
function load(name) {
  if (modules.has(name)) return modules.get(name);
  const exports = {}; modules.set(name, exports);
  const source = path.resolve(__dirname, '../entry/src/main/ets/model/' + name + '.ets');
  const compiled = ts.transpileModule(fs.readFileSync(source, 'utf8'), { fileName: source, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.equal((compiled.diagnostics || []).filter(x => x.category === ts.DiagnosticCategory.Error).length, 0);
  vm.runInNewContext(compiled.outputText, { exports, require: name => load(name.replace(/^\.\//, '')),
    setTimeout, clearTimeout, encodeURIComponent });
  return exports;
}
const policyModule = load('EditorFieldPolicy');
const segmenter = new Intl.Segmenter('und', { granularity: 'grapheme' });
// Synthetic worker contract for actual ETS policy/race tests. Unicode16 native
// segmentation itself is qualified by separate Rust/real Flutter fixtures.
function response(field, text) {
  const limit = policyModule.editorFieldLimit(field), count = [...segmenter.segment(text)].length;
  return { ok: count <= limit, error: count <= limit ? '' : 'EditorFieldGraphemeLimit', field,
    grapheme_count: count, utf16_length: text.length, utf8_length: Buffer.byteLength(text), limit, unicode_version: '16.0.0' };
}
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
function createPolicy(options = {}) {
  const requests = [], owner = { current: true };
  const policy = new policyModule.EditorFieldPolicy(async encoded => {
    const request = JSON.parse(encoded); requests.push(request);
    if (options.wait) await options.wait(request, requests.length);
    if (options.send) return options.send(request, requests.length);
    const result = response(request.field, request.text);
    if (options.mutate) options.mutate(result, request);
    return JSON.stringify(result);
  });
  return { policy, requests, owner, owned: () => owner.current };
}
module.exports = { load, response, createPolicy, deferred, ...policyModule };
