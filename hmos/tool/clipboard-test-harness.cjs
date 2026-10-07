'use strict';
// Shared synthetic SDK/FS fixtures. Production ETS is transpiled and executed;
// no device operation or copied production conversion logic is used.
const hostFs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modelRoot = path.resolve(__dirname, '../entry/src/main/ets/model');
function compile(name) {
  const sourcePath = path.join(modelRoot, name + '.ets');
  const source = hostFs.readFileSync(sourcePath, 'utf8');
  const result = ts.transpileModule(source, { fileName: sourcePath, reportDiagnostics: true,
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS } });
  const errors = (result.diagnostics || []).filter(item => item.category === ts.DiagnosticCategory.Error);
  assert.equal(errors.length, 0, ts.formatDiagnosticsWithColorAndContext(errors, {
    getCanonicalFileName: name => name, getCurrentDirectory: () => process.cwd(), getNewLine: () => '\n' }));
  return result.outputText;
}
const inputCode = compile('ClipboardInput');
const filesCode = compile('AttachmentFiles');
const attachmentCode = compile('Attachments');
const encoder = { TextEncoder: class { encodeInto(text) { return new Uint8Array(Buffer.from(text)); } } };
function sha(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }
function stream(bytes) { return { ok: true, error: '', byte_length: String(bytes.length), sha256: sha(bytes) }; }
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
function createClipboardHarness(records = [], options = {}) {
  const logs = []; const owner = { current: true }; const state = { count: 1, records, options };
  const board = {
    getChangeCount() { logs.push(['changeCount', state.count]); return state.count; },
    async getData() {
      logs.push(['getData']); if (options.read) { await options.read(); }
      return {
        getRecordCount: () => options.recordCount ?? state.records.length,
        getProperty: () => ({ mimeTypes: options.offered ?? [...new Set(state.records.flatMap(record => Object.keys(record)))] }),
        getRecord(index) {
          const record = state.records[index]; if (!record) { throw new Error('missing record'); }
          return {
            getValidTypes(candidates) { return options.validTypes ? options.validTypes(index, candidates) : candidates.filter(type => Object.hasOwn(record, type)); },
            async getData(type) {
              logs.push(['recordData', index, type]); const result = record[type];
              return typeof result === 'function' ? await result() : result;
            }
          };
        }
      };
    }
  };
  const imageKit = { image: { createImagePacker() {
    logs.push(['createPacker']); return {
      async packToData(pixel, packing) { logs.push(['packToData', packing]); return pixel.pack ? await pixel.pack(packing) : Uint8Array.from([137, 80, 78, 71]).buffer; },
      async release() { logs.push(['releasePacker']); if (options.releasePacker) { await options.releasePacker(); } }
    };
  } } };
  const module = { exports: {} };
  const attachments = { exports: {} };
  vm.runInNewContext(attachmentCode, { module: attachments, exports: attachments.exports,
    require: () => ({ AssetSelection: class {} }) });
  vm.runInNewContext(inputCode, { module, exports: module.exports, ArrayBuffer, Uint8Array, console,
    require(id) {
      if (id === '@kit.BasicServicesKit') { return { pasteboard: { getSystemPasteboard: () => board } }; }
      if (id === '@kit.ImageKit') { return imageKit; }
      if (id === '@kit.ArkTS') { return { util: encoder }; }
      if (id === './Attachments') { return attachments.exports; }
      throw new Error('unexpected ClipboardInput dependency ' + id);
    }
  }, { filename: path.join(modelRoot, 'ClipboardInput.ets') });
  function pixel(config = {}) {
    return {
      released: 0,
      async getImageInfo() { logs.push(['imageInfo']); if (config.info) { return await config.info(); } return { size: { width: 1, height: 1 } }; },
      getPixelBytesNumber() { return config.byteCount ?? 4; },
      async release() { this.released++; logs.push(['releasePixel']); if (config.release) { await config.release(); } },
      pack: config.pack
    };
  }
  const input = new module.exports.ClipboardInput();
  return { input, inputModule: module.exports, logs, state, owner, board, pixel,
    read: () => input.readAuthorized(() => owner.current),
    async take(source, maximum = 64 * 1024 * 1024, callback = async value => ({ bytes: Buffer.from(value.bytes), uri: value.uri, name: value.name })) {
      return module.exports.consumeClipboardSource(source, maximum, callback);
    } };
}
function createFilesHarness(clipboard = createClipboardHarness()) {
  // Reuse the existing independent in-memory FS fixture, not its tests. Keeping
  // one provider fixture ensures old and new imports see identical quota rules.
  const original = hostFs.readFileSync(path.join(__dirname, 'attachment-files-model.test.cjs'), 'utf8');
  const start = original.indexOf('function harness() {');
  const end = original.indexOf('\nfunction command(', start);
  assert.ok(start >= 0 && end > start);
  let fixture = original.slice(start, end);
  fixture = fixture.replace("if (id === '@kit.CryptoArchitectureKit') { return {}; }", "if (id === '@kit.CryptoArchitectureKit') { return cryptoKit; }");
  fixture = fixture.replace("if (id === './ClipboardInput') { return { CLIPBOARD_SOURCE_LIMIT: 2 * 1024 * 1024 }; }", "if (id === './ClipboardInput') { return inputModule; }");
  const cryptoKit = { cryptoFramework: { createMd(algorithm) {
    assert.equal(algorithm, 'SHA256'); const hash = crypto.createHash('sha256');
    return { async update(blob) { hash.update(Buffer.from(blob.data)); }, async digest() { return { data: new Uint8Array(hash.digest()) }; } };
  } } };
  const context = { assert, hostFs, path, vm, crypto, console, Buffer, Uint8Array, ArrayBuffer,
    ROOT: '/app/files/hmos-attachment-spool', CACHE: '/app/cache', URI: 'content://test-owned/document/中文文件.txt',
    SAVE_URI: 'content://test-owned/save/附件.txt', RESERVE: 512 * 1024 + 4096, BUDGET: 64 * 1024 * 1024,
    sourcePath: path.join(modelRoot, 'AttachmentFiles.ets'), transpiled: { outputText: filesCode },
    error(code, message = 'synthetic provider failure') { const result = new Error(message); result.code = code; return result; },
    sha, reply: bytes => JSON.stringify(stream(bytes)), cryptoKit, inputModule: clipboard.inputModule,
    exported: undefined };
  vm.runInNewContext(fixture + '\nexported = harness();', context);
  const h = context.exported; h.clipboard = clipboard; h.files = h.create();
  const imageBytes = Buffer.from([137, 80, 78, 71, 13, 10]);
  h.native.clipboardConvert = async (request, sourceFd) => {
    h.logs.push(['clipboardConvert', request, sourceFd]);
    const args = JSON.parse(request), bytes = h.read(h.fdPath(sourceFd));
    const hash = sha(bytes);
    assert.equal(args.expected_sha256, hash);
    return JSON.stringify({ ok: true, error: '', paste_text: 'converted', warnings: [], images: [], source_sha256: hash, source_byte_length: String(bytes.length) });
  };
  h.native.clipboardImage = async (request, sourceFd, destFd, maxBytes) => {
    h.logs.push(['clipboardImage', request, sourceFd, destFd, maxBytes]);
    const args = JSON.parse(request); assert.equal(args.expected_sha256, sha(h.read(h.fdPath(sourceFd))));
    h.writeFd(destFd, imageBytes); return JSON.stringify(stream(imageBytes));
  };
  h.setConvert = fn => { h.native.clipboardConvert = fn; };
  h.setImage = fn => { h.native.clipboardImage = fn; };
  h.imageBytes = imageBytes; h.stream = stream;
  return h;
}
module.exports = { createClipboardHarness, createFilesHarness, sha, stream, deferred };
