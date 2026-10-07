/* Synthetic provider checks of the actual AttachmentFiles.ets implementation.
 * This does not load a device, call hdc, or prove picker/provider runtime support.
 * Run: node --test hmos/tool/attachment-files-model.test.cjs
 * Optional: HMOS_TYPESCRIPT_PATH points to the installed SDK TypeScript module.
 */
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const hostFs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');

const sdkTs = process.env.HMOS_TYPESCRIPT_PATH ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript';
const ts = require(sdkTs);
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/model/AttachmentFiles.ets');
const source = hostFs.readFileSync(sourcePath, 'utf8');
const transpiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS },
  fileName: sourcePath,
  reportDiagnostics: true,
});
const syntaxErrors = (transpiled.diagnostics || []).filter(item => item.category === ts.DiagnosticCategory.Error);
assert.equal(syntaxErrors.length, 0, ts.formatDiagnosticsWithColorAndContext(syntaxErrors, {
  getCanonicalFileName: name => name, getCurrentDirectory: () => process.cwd(), getNewLine: () => '\n',
}));

const ROOT = '/app/files/hmos-attachment-spool';
const CACHE = '/app/cache';
const URI = 'content://test-owned/document/中文文件.txt';
const SAVE_URI = 'content://test-owned/save/附件.txt';
const RESERVE = 512 * 1024 + 4096;
const BUDGET = 64 * 1024 * 1024;

function error(code, message = 'synthetic provider failure') {
  const result = new Error(message); result.code = code; return result;
}
function sha(bytes) { return crypto.createHash('sha256').update(bytes).digest('hex'); }
function reply(bytes) { return JSON.stringify({ ok: true, error: '', byte_length: String(bytes.length), sha256: sha(bytes) }); }

function harness() {
  const nodes = new Map();
  const fds = new Map();
  const logs = [];
  const faults = new Map();
  let nextFd = 10;
  let nextDirectory = 1;
  let selectUris = [URI];
  let selectOverride;
  let saveUris = [SAVE_URI];
  let prepareOverride;
  const modes = { READ_ONLY: 0, WRITE_ONLY: 1, READ_WRITE: 2, CREATE: 64, TRUNC: 512 };
  const putDir = name => nodes.set(name, { kind: 'directory' });
  const putFile = (name, bytes = '', options = {}) => nodes.set(name,
    { kind: 'file', bytes: Buffer.from(bytes), ...options });
  const putLink = (name, target) => nodes.set(name, { kind: 'symlink', target });
  putDir('/app'); putDir('/app/files'); putDir(CACHE);
  putFile(URI, '中文\n😀', { name: '中文文件.txt' });
  putFile(SAVE_URI, 'old-long-file', { name: '附件.txt' });

  function check(operation, name) {
    const failure = faults.get(operation + ':' + name);
    if (failure) { throw failure; }
  }
  function node(name) {
    const value = nodes.get(name);
    if (!value) { throw error(13900002, 'synthetic ENOENT'); }
    return value;
  }
  function descriptor(fd) {
    const value = fds.get(fd);
    if (!value) { throw error(13900009, 'synthetic closed FD'); }
    return value;
  }
  function stats(value) {
    return { size: value.size ?? value.bytes?.length ?? 0,
      isFile: () => value.kind === 'file', isDirectory: () => value.kind === 'directory',
      isSymbolicLink: () => value.kind === 'symlink' };
  }
  const fs = {
    OpenMode: modes,
    async lstat(name) { logs.push(['lstat', name]); check('lstat', name); return stats(node(name)); },
    async stat(fd) {
      const file = descriptor(fd); logs.push(['stat', file.path]); check('stat', file.path); return stats(node(file.path));
    },
    async mkdir(name) { logs.push(['mkdir', name]); if (nodes.has(name)) { throw error(13900017); } putDir(name); },
    async mkdtemp(template) {
      const name = template.replace('XXXXXX', String(nextDirectory++).padStart(6, '0'));
      assert.equal(node(path.posix.dirname(name)).kind, 'directory');
      putDir(name); logs.push(['mkdtemp', name]); return name;
    },
    async listFile(name) {
      check('listFile', name); assert.equal(node(name).kind, 'directory');
      return [...nodes.keys()].filter(key => key.startsWith(name + '/') && !key.slice(name.length + 1).includes('/'))
        .map(key => key.slice(name.length + 1));
    },
    async access(name) { check('access', name); return nodes.has(name); },
    async open(name, mode) {
      check('open', name);
      if (!nodes.has(name)) {
        if (!(mode & modes.CREATE)) { throw error(13900002); }
        assert.equal(node(path.posix.dirname(name)).kind, 'directory'); putFile(name);
      }
      const value = node(name); assert.equal(value.kind, 'file');
      if (mode & modes.TRUNC) { value.bytes = Buffer.alloc(0); delete value.size; }
      const fd = nextFd++; const file = { fd, name: value.name || path.posix.basename(name), path: name, offset: 0 };
      fds.set(fd, file); logs.push(['open', name, mode, fd]); return file;
    },
    closeSync(file) {
      const fd = typeof file === 'number' ? file : file.fd;
      assert.ok(fds.has(fd), 'caller FD is closed exactly once'); logs.push(['close', fd]); fds.delete(fd);
    },
    async write(fd, buffer) {
      const file = descriptor(fd); check('write', file.path);
      const bytes = Buffer.from(buffer); const value = node(file.path);
      const count = value.writeChunk ? Math.min(value.writeChunk, bytes.length) : bytes.length;
      value.bytes = Buffer.concat([value.bytes.subarray(0, file.offset), bytes.subarray(0, count)]);
      file.offset += count; return count;
    },
    async fsync(fd) { const file = descriptor(fd); logs.push(['fsync', file.path]); check('fsync', file.path); },
    async copyFile(sourceFd, destinationFd) {
      const sourceFile = descriptor(sourceFd); const destinationFile = descriptor(destinationFd);
      logs.push(['copyFile', sourceFile.path, destinationFile.path]); check('copyFile', destinationFile.path);
      node(destinationFile.path).bytes = Buffer.from(node(sourceFile.path).bytes);
    },
    async readText(name, options) {
      check('readText', name); return node(name).bytes.subarray(0, options.length).toString('utf8');
    },
    async unlink(name) { logs.push(['unlink', name]); check('unlink', name); node(name); nodes.delete(name); },
    async rmdir(name) {
      // Match the SDK's recursive rmdir so helper preflight failures are visible.
      logs.push(['rmdir', name]); check('rmdir', name); node(name);
      for (const key of [...nodes.keys()]) { if (key === name || key.startsWith(name + '/')) { nodes.delete(key); } }
    },
  };
  const native = {
    async prepareFile(sourceFd, destinationFd, maxBytes) {
      logs.push(['prepareFile', sourceFd, destinationFd, maxBytes]);
      if (prepareOverride) { return prepareOverride(sourceFd, destinationFd, maxBytes); }
      const bytes = node(descriptor(sourceFd).path).bytes;
      const destination = node(descriptor(destinationFd).path);
      destination.bytes = Buffer.from(bytes.subarray(0, maxBytes));
      if (bytes.length > maxBytes) { return JSON.stringify({ ok: false, error: 'limit', byte_length: '', sha256: '' }); }
      return reply(bytes);
    },
  };
  class DocumentViewPicker {
    constructor(context) { assert.equal(context.filesDir, '/app/files'); }
    async select(options) {
      logs.push(['select', options.maxSelectNumber, options.selectMode]);
      return selectOverride ? selectOverride(options) : selectUris;
    }
    async save(options) { logs.push(['save', ...options.newFileNames]); return saveUris; }
  }
  const picker = { DocumentViewPicker, DocumentSelectOptions: class {}, DocumentSaveOptions: class {},
    DocumentSelectMode: { FILE: 1 } };
  const module = { exports: {} };
  vm.runInNewContext(transpiled.outputText, {
    module, exports: module.exports, Uint8Array, ArrayBuffer, console,
    require(id) {
      if (id === '@kit.CoreFileKit') { return { fileIo: fs, picker, fileUri: {
        getUriFromPath(name) { logs.push(['fileUri', name]); check('fileUri', name); return 'file://dev.morrow.hmos' + encodeURI(name); },
      } }; }
      if (id === '@kit.ArkTS') { return { util: { TextEncoder: class {
        encodeInto(text) { return new Uint8Array(Buffer.from(text)); }
      } } }; }
      if (id === '@kit.CryptoArchitectureKit') { return {}; }
      if (id === './ClipboardInput') { return { CLIPBOARD_SOURCE_LIMIT: 2 * 1024 * 1024 }; }
      if (id === 'libmorrow.so') { return { default: native }; }
      if (id === '@kit.AbilityKit' || id === '@kit.BasicServicesKit' || id === './Attachments') { return {}; }
      throw new Error('Unexpected actual-source import: ' + id);
    },
  }, { filename: sourcePath });
  const create = (cacheDir = CACHE) => new module.exports.AttachmentFiles({ filesDir: '/app/files', cacheDir });
  const directory = id => ROOT + '/' + id;
  function seed(id = 'import-ABC123', options = {}) {
    const { data = 'payload', request = undefined, dataSize = undefined, name = '附件.txt' } = options;
    const metadata = Object.prototype.hasOwnProperty.call(options, 'metadata') ? options.metadata : 'valid';
    putDir(ROOT); putDir(directory(id));
    const bytes = Buffer.from(data);
    putFile(directory(id) + '/data', bytes, dataSize === undefined ? {} : { size: dataSize });
    const value = { spool_id: id, name, byte_length: String(dataSize ?? bytes.length), sha256: sha(bytes), original_request: '' };
    if (metadata !== undefined) { putFile(directory(id) + '/metadata.json', metadata === 'valid' ? JSON.stringify(value) : metadata); }
    if (request !== undefined) { putFile(directory(id) + '/request.json', typeof request === 'function' ? request(value) : request); }
    return value;
  }
  function sendBytes(bytes, overrides = {}) {
    bytes = Buffer.from(bytes);
    return async (serialized, fd) => {
      logs.push(['send', serialized, fd]);
      node(descriptor(fd).path).bytes = Buffer.from(bytes);
      return JSON.stringify({ ok: true, error: '', byte_length: String(bytes.length), sha256: sha(bytes), ...overrides });
    };
  }
  return { nodes, fds, logs, faults, modes, fs, native, create, seed, putDir, putFile, putLink, directory, sendBytes,
    imagePreviewScale: module.exports.imagePreviewScale, imagePreviewOffset: module.exports.imagePreviewOffset,
    attachmentPreviewName: module.exports.attachmentPreviewName,
    setSelect: value => { selectUris = value; }, setSave: value => { saveUris = value; },
    setSelectAsync: value => { selectOverride = value; },
    setPrepare: value => { prepareOverride = value; },
    read: name => node(name).bytes, writeFd: (fd, bytes) => { node(descriptor(fd).path).bytes = Buffer.from(bytes); },
    fdPath: fd => descriptor(fd).path };
}

function command(value, changes = {}) {
  return JSON.stringify({ action: 'import_file', import_request: {
    card_id: 'card-test', draft_id: 'draft-test', operation_id: 'operation-test', expected_generation: '0',
    name: value.name, kind: 'file', byte_length: value.byte_length, sha256: value.sha256, ...changes,
  } });
}
function reviews(files) { return files.recoveryDiagnostics().filter(issue => issue.requires_review); }
function noFilesUnder(h, prefix) { assert.equal([...h.nodes.keys()].some(name => name.startsWith(prefix)), false); }
function noNative(h) { assert.equal(h.logs.some(item => item[0] === 'prepareFile' || item[0] === 'send'), false); }

test('picker opens exact authorized URI, native streams Unicode bytes, returned metadata has no path', async () => {
  const h = harness(); const files = h.create(); const value = await files.pick();
  assert.equal(value.name, '中文文件.txt'); assert.equal(value.byte_length, '11'); assert.equal(value.sha256, sha(Buffer.from('中文\n😀')));
  assert.equal(h.logs.find(item => item[0] === 'open' && item[1] === URI)[2], h.modes.READ_ONLY);
  assert.equal(JSON.stringify(value).includes('content://'), false); assert.equal(JSON.stringify(value).includes('/app/'), false);
  assert.equal(h.fds.size, 0); await files.release(value); noFilesUnder(h, ROOT + '/import-');
});
test('picker cancellation does not prepare or create spool', async () => {
  const h = harness(); h.setSelect([]); assert.equal(await h.create().pick(), undefined); noNative(h); noFilesUnder(h, ROOT + '/import-');
});
test('provider open denial closes caller files and removes unadmitted private directory', async () => {
  const h = harness(); h.faults.set('open:' + URI, error(13900012));
  await assert.rejects(h.create().pick()); noNative(h); assert.equal(h.fds.size, 0); noFilesUnder(h, ROOT + '/import-');
});
test('native over-budget partial prepare is removed', async () => {
  const h = harness(); await assert.rejects(h.create().pick(3)); assert.equal(h.fds.size, 0); noFilesUnder(h, ROOT + '/import-');
});
test('unknown import retains exact original JSON and recovery supports only the same proposal', async () => {
  const h = harness(); const files = h.create(); const value = await files.pick();
  const original = command(value).replace('{', '{\n  '); let admissions = 0;
  const unknown = async (raw, fd) => { admissions++; assert.equal(raw, original); assert.ok(h.fds.has(fd)); throw error('NATIVE_OUTCOME_UNKNOWN'); };
  await assert.rejects(files.importPrepared(value, original, unknown));
  assert.equal(h.fds.size, 0); assert.equal(value.original_request, original);
  assert.equal(h.read(h.directory(value.spool_id) + '/request.json').toString(), original);
  await assert.rejects(files.importPrepared(value, command(value, { operation_id: 'different' }), unknown)); assert.equal(admissions, 1);
  const restarted = h.create(); const recovered = await restarted.recover(); assert.equal(recovered.length, 1);
  assert.equal(recovered[0].original_request, original);
  assert.equal(await restarted.importPrepared(recovered[0], original, async () => 'known'), 'known');
  await restarted.release(recovered[0]); noFilesUnder(h, h.directory(value.spool_id));
});
test('20 retained requests block picker before provider admission', async () => {
  const h = harness(); for (let i = 0; i < 20; i++) { h.seed('import-' + String(100000 + i), { request: command }); }
  await assert.rejects(h.create().pick(), /20/); assert.equal(h.logs.some(item => item[0] === 'select'), false); noNative(h);
});
test('remaining quota includes reserved sidecars of unknown imports', async () => {
  const h = harness(); h.seed('import-ABC123', { dataSize: BUDGET - 2 * RESERVE - 999, request: command });
  h.putFile(URI, 'tiny'); const value = await h.create().pick();
  assert.equal(h.logs.find(item => item[0] === 'prepareFile')[3], 999); assert.equal(value.byte_length, '4');
});
test('bad native whole hash never opens save picker', async () => {
  const h = harness(); await assert.rejects(h.create().exportToPicker('request', 'test', h.sendBytes('abc', { sha256: 'BAD' })));
  assert.equal(h.logs.some(item => item[0] === 'save'), false); assert.equal(h.fds.size, 0); noFilesUnder(h, CACHE + '/morrow-export-');
});
test('verified short export truncates longer destination, syncs and checks final size', async () => {
  const h = harness(); assert.equal(await h.create().exportToPicker('request', '附件.txt', h.sendBytes('abc')), true);
  assert.equal(h.read(SAVE_URI).toString(), 'abc');
  const opened = h.logs.find(item => item[0] === 'open' && item[1] === SAVE_URI);
  assert.ok(opened[2] & h.modes.TRUNC); assert.ok(h.logs.some(item => item[0] === 'fsync' && item[1] === SAVE_URI));
  assert.ok(h.logs.some(item => item[0] === 'stat' && item[1] === SAVE_URI)); assert.equal(h.fds.size, 0);
  noFilesUnder(h, CACHE + '/morrow-export-');
});
test('provider destination stat unsupported is explicitly unconfirmed', async () => {
  const h = harness(); h.faults.set('stat:' + SAVE_URI, error(13900038));
  await assert.rejects(h.create().exportToPicker('request', 'test', h.sendBytes('abc')), /尚未确认/);
  assert.equal(h.fds.size, 0); noFilesUnder(h, CACHE + '/morrow-export-');
});
test('private image preview is verified, one at a time, and token-scoped cleanup', async () => {
  const h = harness(); const files = h.create(); const value = await files.previewFile('request', '图.gif', h.sendBytes('GIF89a'));
  assert.equal(value.name, '图.gif'); assert.equal(value.byte_length, '6'); assert.equal(value.sha256, sha(Buffer.from('GIF89a')));
  assert.equal(value.uri, 'file://dev.morrow.hmos' + value.path); assert.equal(value.purpose, 'dialog'); assert.equal(files.isPreviewActive(value), true);
  assert.equal(h.read(value.path).toString(), 'GIF89a'); assert.equal(h.logs.some(item => item[0] === 'save' || item[0] === 'select'), false);
  await assert.rejects(files.previewFile('request', 'other', h.sendBytes('x')), /关闭/);
  await files.releasePreview('../forged'); assert.ok(h.nodes.has(value.path));
  await files.releasePreview(value.token); noFilesUnder(h, CACHE + '/morrow-preview-'); assert.equal(h.fds.size, 0);
  assert.equal(files.isPreviewActive(value), false);
});

test('dialog and inline previews coexist, and every original token is released independently', async () => {
  const h = harness(); const files = h.create();
  const inline = await files.previewFile('inline', 'same.png', h.sendBytes('inline'), 'inline', '6');
  const dialog = await files.previewFile('dialog', 'same.png', h.sendBytes('dialog'));
  assert.notEqual(inline.token, dialog.token); assert.equal(inline.purpose, 'inline');
  await files.releasePreview(dialog.token); assert.equal(files.isPreviewActive(inline), true); assert.equal(h.read(inline.path).toString(), 'inline');
  await files.releasePreview(inline.token); noFilesUnder(h, CACHE + '/morrow-preview-'); assert.equal(h.fds.size, 0);
});

test('inline admission permits eight files then blocks before native read, including another adapter', async () => {
  const h = harness(); const files = h.create(); const values = [];
  for (let index = 0; index < 8; index++) { values.push(await files.previewFile('inline-' + index, 'picture.png', h.sendBytes('x'), 'inline', '1')); }
  const before = h.logs.length; await assert.rejects(h.create().previewFile('ninth', 'picture.png', h.sendBytes('x'), 'inline', '1'), /8/);
  assert.equal(h.logs.length, before);
  const dialog = await h.create().previewFile('dialog', 'picture.png', h.sendBytes('y'));
  assert.equal(dialog.purpose, 'dialog');
  await files.releasePreview(values[0].token);
  assert.ok(await files.previewFile('replacement', 'picture.png', h.sendBytes('z'), 'inline', '1'));
});

test('single dialog admission is shared across adapters, while foreign release cannot delete it', async () => {
  const h = harness(); const owner = h.create(); const other = h.create();
  const value = await owner.previewFile('request', 'image.png', h.sendBytes('x')); const before = h.logs.length;
  await assert.rejects(other.previewFile('second', 'image.png', h.sendBytes('y')), /关闭/); assert.equal(h.logs.length, before);
  assert.equal(other.isPreviewActive(value), false); await other.releasePreview(value.token); assert.equal(owner.isPreviewActive(value), true);
  await owner.releasePreview(value.token); assert.ok(await other.previewFile('later', 'image.png', h.sendBytes('y')));
});

test('inline byte quota uses immutable native metadata and restores only after release', async () => {
  const h = harness(); const files = h.create();
  // Only admission is being exercised here. The synthetic provider represents
  // a large regular file by its stat size; this is not a real large-file/hash test.
  const large = async (_, fd) => { h.nodes.get(h.fdPath(fd)).size = BUDGET - 1;
    return JSON.stringify({ ok: true, error: '', byte_length: String(BUDGET - 1), sha256: 'a'.repeat(64) }); };
  const value = await files.previewFile('large', 'large.png', large, 'inline', String(BUDGET - 1));
  value.byte_length = '1'; assert.equal(files.isPreviewActive(value), false); const before = h.logs.length;
  await assert.rejects(h.create().previewFile('too-big', 'small.png', h.sendBytes('xx'), 'inline', '2'), /64 MiB/);
  assert.equal(h.logs.length, before);
  const last = await files.previewFile('fits', 'last.png', h.sendBytes('x'), 'inline', '1'); assert.ok(last);
  await files.releasePreview(value.token); assert.ok(await files.previewFile('again', 'small.png', h.sendBytes('xx'), 'inline', '2'));
});

for (const length of ['', '-1', '01', '1.0', '9007199254740992', String(BUDGET + 1)]) {
  test('inline refuses missing or invalid pre-admission length ' + JSON.stringify(length), async () => {
    const h = harness(); await assert.rejects(h.create().previewFile('request', 'image.png', h.sendBytes('x'), 'inline', length));
    assert.equal(h.logs.length, 0); assert.equal(h.fds.size, 0);
  });
}

test('unexpected inline result length is rejected and its private partial is removed', async () => {
  const h = harness(); const files = h.create();
  await assert.rejects(files.previewFile('request', 'image.png', h.sendBytes('xx'), 'inline', '1'), /长度/);
  assert.equal(h.fds.size, 0); noFilesUnder(h, CACHE + '/morrow-preview-');
  assert.ok(await files.previewFile('replacement', 'image.png', h.sendBytes('x'), 'inline', '1'));
});

test('invalid purpose does not read native bytes or create private paths', async () => {
  const h = harness(); await assert.rejects(h.create().previewFile('request', 'image.png', h.sendBytes('x'), 'unbounded', '1'));
  assert.equal(h.logs.length, 0);
});

test('preview identity rejects copied handles and every mutated display/path field', async () => {
  const h = harness(); const files = h.create(); const value = await files.previewFile('request', 'image.png', h.sendBytes('x'));
  assert.equal(files.isPreviewActive({ ...value }), false);
  for (const key of ['token', 'path', 'uri', 'name', 'byte_length', 'sha256', 'purpose']) {
    const before = value[key]; value[key] = '../forged'; assert.equal(files.isPreviewActive(value), false);
    value[key] = before; assert.equal(files.isPreviewActive(value), true);
  }
  await files.releasePreview(value.token);
});

test('failed preview URI conversion cleans its private copy and caller descriptor', async () => {
  const h = harness(); h.faults.set('fileUri:' + CACHE + '/morrow-preview-000001/data', error(13900005));
  await assert.rejects(h.create().previewFile('request', 'image.png', h.sendBytes('x')));
  noFilesUnder(h, CACHE + '/morrow-preview-'); assert.equal(h.fds.size, 0);
});

test('failed token cleanup retains admission and can be explicitly retried', async () => {
  const h = harness(); const files = h.create(); const value = await files.previewFile('request', 'image.png', h.sendBytes('x'));
  h.faults.set('unlink:' + value.path, error(13900012)); await assert.rejects(files.releasePreview(value.token));
  assert.equal(files.isPreviewActive(value), true); await assert.rejects(h.create().previewFile('other', 'image.png', h.sendBytes('x')), /关闭/);
  h.faults.delete('unlink:' + value.path); await files.releasePreview(value.token); assert.equal(files.isPreviewActive(value), false);
});

test('failed cleanup remains visible without a UI handle, explicit retry restores dialog admission', async () => {
  const h = harness(); const files = h.create(); const value = await files.previewFile('request', 'image.png', h.sendBytes('x'));
  assert.equal(files.previewCleanupPending(), false);
  h.faults.set('unlink:' + value.path, error(13900012)); await assert.rejects(files.releasePreview(value.token), /明确重试/);
  assert.equal(files.previewCleanupPending(), true);
  const before = h.logs.length; assert.equal(files.previewCleanupPending(), true); assert.equal(h.logs.length, before);
  await assert.rejects(files.previewFile('new', 'image.png', h.sendBytes('x')), /关闭/);
  h.faults.delete('unlink:' + value.path); await files.retryPreviewCleanup(); assert.equal(files.previewCleanupPending(), false);
  assert.equal(files.isPreviewActive(value), false); noFilesUnder(h, value.path.slice(0, -5));
  assert.ok(await files.previewFile('new', 'image.png', h.sendBytes('x')));
});

test('same-cache new helper retries old failed cleanup, old owner release does not touch disk twice', async () => {
  const h = harness(); const old = h.create(); const replacement = h.create();
  const value = await old.previewFile('request', 'image.png', h.sendBytes('x'));
  const originalToken = value.token;
  h.faults.set('unlink:' + value.path, error(13900012)); await assert.rejects(old.releasePreview(originalToken));
  assert.equal(replacement.previewCleanupPending(), true); h.faults.delete('unlink:' + value.path);
  value.token = '../caller-mutated';
  await replacement.retryPreviewCleanup(); assert.equal(old.previewCleanupPending(), false); assert.equal(old.isPreviewActive(value), false);
  const before = h.logs.length; await old.releasePreview(originalToken); assert.equal(h.logs.length, before);
  assert.ok(await replacement.previewFile('later', 'image.png', h.sendBytes('x')));
});

test('explicit cleanup skips healthy live tokens and preserves failed quota when retry also fails', async () => {
  const h = harness(); const files = h.create();
  const failed = await files.previewFile('failed', 'image.png', h.sendBytes('a'), 'inline', '1');
  const healthy = await files.previewFile('healthy', 'image.png', h.sendBytes('b'), 'inline', '1');
  h.faults.set('unlink:' + failed.path, error(13900012)); await assert.rejects(files.releasePreview(failed.token));
  await assert.rejects(files.retryPreviewCleanup(), /仍未清理/); assert.equal(files.previewCleanupPending(), true);
  assert.equal(files.isPreviewActive(failed), true); assert.equal(files.isPreviewActive(healthy), true);
  assert.equal(h.logs.some(item => item[0] === 'unlink' && item[1] === healthy.path), false);
  h.faults.delete('unlink:' + failed.path); await files.retryPreviewCleanup();
  assert.equal(files.isPreviewActive(failed), false); assert.equal(files.isPreviewActive(healthy), true); assert.equal(files.previewCleanupPending(), false);
});

test('explicit retry keeps immutable inline bytes until actual removal then restores byte budget', async () => {
  const h = harness(); const files = h.create();
  const large = async (_, fd) => { h.nodes.get(h.fdPath(fd)).size = BUDGET;
    return JSON.stringify({ ok: true, error: '', byte_length: String(BUDGET), sha256: 'a'.repeat(64) }); };
  const value = await files.previewFile('large', 'large.png', large, 'inline', String(BUDGET));
  h.faults.set('unlink:' + value.path, error(13900012)); await assert.rejects(files.releasePreview(value.token));
  value.byte_length = '0'; await assert.rejects(files.previewFile('new', 'new.png', h.sendBytes('x'), 'inline', '1'), /64 MiB/);
  h.faults.delete('unlink:' + value.path); await h.create().retryPreviewCleanup();
  assert.ok(await files.previewFile('new', 'new.png', h.sendBytes('x'), 'inline', '1'));
});

test('cache-root identity isolates cleanup and quotas even for a nested cache directory', async () => {
  const h = harness(); const nestedRoot = CACHE + '/nested'; h.putDir(nestedRoot);
  const owner = h.create(); const nested = h.create(nestedRoot);
  const parentValue = await owner.previewFile('parent', 'image.png', h.sendBytes('a'));
  const nestedValue = await nested.previewFile('nested', 'image.png', h.sendBytes('b'));
  h.faults.set('unlink:' + parentValue.path, error(13900012)); h.faults.set('unlink:' + nestedValue.path, error(13900012));
  await assert.rejects(owner.releasePreview(parentValue.token)); await assert.rejects(nested.releasePreview(nestedValue.token));
  h.faults.delete('unlink:' + parentValue.path); await h.create().retryPreviewCleanup();
  assert.equal(owner.previewCleanupPending(), false); assert.equal(nested.previewCleanupPending(), true); assert.ok(h.nodes.has(nestedValue.path));
  assert.equal(nested.isPreviewActive(nestedValue), true); h.faults.delete('unlink:' + nestedValue.path);
  await h.create(nestedRoot).retryPreviewCleanup(); assert.equal(nested.previewCleanupPending(), false);
});

test('Flutter image scale range is finite and panning stays inside contained image geometry', () => {
  const h = harness(); assert.equal(h.imagePreviewScale(-50), 0.8); assert.equal(h.imagePreviewScale(99), 2.5);
  assert.equal(h.imagePreviewScale(NaN), 1); assert.equal(h.imagePreviewScale(Infinity), 1);
  assert.equal(h.imagePreviewOffset(999, 200, 100, 2), 150);
  assert.equal(h.imagePreviewOffset(-999, 200, 100, 2), -150);
  assert.equal(h.imagePreviewOffset(25, 100, 200, 1), 0);
  assert.equal(h.imagePreviewOffset(25, 100, 100, 0.8), 0);
  assert.equal(h.imagePreviewOffset(NaN, 200, 100, 2), 0);
  assert.equal(h.imagePreviewOffset(10, 0, 100, 2), 0);
});
test('preview length mismatch removes private copy and closes FD', async () => {
  const h = harness(); await assert.rejects(h.create().previewFile('request', 'test', h.sendBytes('abc', { byte_length: '99' })), /长度/);
  assert.equal(h.fds.size, 0); noFilesUnder(h, CACHE + '/morrow-preview-');
});
test('unknown preview copy result removes private partial bytes without retry', async () => {
  const h = harness(); let admissions = 0;
  await assert.rejects(h.create().previewFile('request', 'test', async (_, fd) => {
    admissions++; h.writeFd(fd, 'partial'); throw error('NATIVE_OUTCOME_UNKNOWN');
  })); assert.equal(admissions, 1); assert.equal(h.fds.size, 0); noFilesUnder(h, CACHE + '/morrow-preview-');
});
test('provider copy failure is unconfirmed and private export is cleaned', async () => {
  const h = harness(); h.faults.set('copyFile:' + SAVE_URI, error(13900005));
  await assert.rejects(h.create().exportToPicker('request', 'test', h.sendBytes('abc')), /尚未确认/);
  assert.equal(h.fds.size, 0); noFilesUnder(h, CACHE + '/morrow-export-');
});

test('crash after mkdtemp before data creation cleans only strict unadmitted directory', async () => {
  const h = harness(); h.putDir(ROOT); h.putDir(h.directory('import-ABC123')); const files = h.create();
  assert.equal((await files.recover()).length, 0); noFilesUnder(h, h.directory('import-ABC123'));
  assert.equal(files.recoveryDiagnostics()[0].code, 'PREPARE_INTERRUPTED_CLEANED'); assert.equal(reviews(files).length, 0); noNative(h);
});
for (const metadata of [undefined, '', '{"spool_id":']) {
  test('crash with partial data and ' + String(metadata) + ' metadata, no request, frees quota', async () => {
    const h = harness(); h.seed('import-ABC123', { data: 'partial', metadata }); const files = h.create();
    assert.equal((await files.recover()).length, 0); noFilesUnder(h, h.directory('import-ABC123'));
    assert.equal(files.recoveryDiagnostics()[0].preserved, false); noNative(h);
  });
}
test('metadata write failure during live pick leaves no admitted request or caller FD', async () => {
  const h = harness(); h.faults.set('write:' + ROOT + '/import-000001/metadata.json', error(13900005));
  await assert.rejects(h.create().pick()); assert.equal(h.fds.size, 0); noFilesUnder(h, ROOT + '/import-');
});
test('20 crashed preparations are cleaned before a new picker request', async () => {
  const h = harness(); for (let i = 0; i < 20; i++) { h.seed('import-' + String(100000 + i), { metadata: undefined }); }
  const files = h.create(); const value = await files.pick(); assert.ok(value);
  assert.equal(files.recoveryDiagnostics().filter(issue => !issue.preserved).length, 20);
  assert.equal([...h.nodes.keys()].filter(name => /^\/app\/files\/hmos-attachment-spool\/import-[A-Za-z0-9]{6}$/.test(name)).length, 1);
});
for (const request of ['', '{"action":', 'null', '{}', '{"action":"import_file","import_request":[]}']) {
  test('existing corrupt request ' + JSON.stringify(request) + ' is retained with diagnostic and no handle', async () => {
    const h = harness(); h.seed('import-ABC123', { request }); const files = h.create();
    assert.equal((await files.recover()).length, 0); assert.ok(h.nodes.has(h.directory('import-ABC123') + '/data'));
    const issue = reviews(files)[0]; assert.equal(issue.code, 'REQUEST_INVALID'); assert.equal(issue.request_state, 'present');
    assert.equal(issue.preserved, true); noNative(h); assert.equal(h.logs.some(item => item[0] === 'unlink' || item[0] === 'rmdir'), false);
  });
}
test('partial metadata and partial request remain visible, neither becomes an import handle', async () => {
  const h = harness(); h.seed('import-ABC123', { metadata: '{', request: '{' }); const files = h.create();
  assert.equal((await files.recover()).length, 0); const issue = reviews(files)[0];
  assert.equal(issue.code, 'METADATA_INVALID'); assert.equal(issue.request_state, 'present'); assert.ok(h.nodes.has(h.directory('import-ABC123')));
});
test('missing metadata with a valid request is preserved', async () => {
  const h = harness(); h.seed('import-ABC123', { metadata: undefined, request: command }); const files = h.create();
  assert.equal((await files.recover()).length, 0); assert.equal(reviews(files)[0].code, 'METADATA_INVALID');
  assert.equal(reviews(files)[0].request_state, 'present'); assert.ok(h.nodes.has(h.directory('import-ABC123') + '/request.json'));
});
for (const changes of [{ sha256: '0'.repeat(64) }, { kind: 'unknown' }, { expected_generation: '18446744073709551616' }, { operation_id: '' }]) {
  test('schema-invalid or mismatched retained request ' + JSON.stringify(changes) + ' is not importable', async () => {
    const h = harness(); h.seed('import-ABC123', { request: value => command(value, changes) }); const files = h.create();
    assert.equal((await files.recover()).length, 0); assert.equal(reviews(files)[0].code, 'REQUEST_INVALID');
    assert.ok(h.nodes.has(h.directory('import-ABC123'))); noNative(h);
  });
}
for (const child of ['directory', 'data', 'metadata.json', 'request.json']) {
  test('symlink ' + child + ' is never recursively removed or exposed', async () => {
    const h = harness(); h.seed();
    const target = child === 'directory' ? h.directory('import-ABC123') : h.directory('import-ABC123') + '/' + child;
    h.putLink(target, '/unrelated'); const files = h.create();
    assert.equal((await files.recover()).length, 0); assert.ok(h.nodes.has(target)); assert.equal(reviews(files).length, 1);
    assert.equal(h.logs.some(item => item[0] === 'unlink' || item[0] === 'rmdir'), false); noNative(h);
  });
}
test('permission failure does not imply request absence and prevents quota admission', async () => {
  const h = harness(); h.seed('import-ABC123', { metadata: undefined });
  h.faults.set('lstat:' + h.directory('import-ABC123') + '/request.json', error(13900012)); const files = h.create();
  assert.equal((await files.recover()).length, 0); assert.equal(reviews(files)[0].request_state, 'unknown');
  assert.ok(h.nodes.has(h.directory('import-ABC123') + '/data')); await assert.rejects(files.pick());
  assert.equal(h.logs.some(item => item[0] === 'select' || item[0] === 'rmdir'), false); noNative(h);
});
test('unexpected child directory survives recursive SDK rmdir and blocks new admission', async () => {
  const h = harness(); h.seed('import-ABC123', { metadata: undefined }); h.putDir(h.directory('import-ABC123') + '/unexpected');
  h.putFile(h.directory('import-ABC123') + '/unexpected/owned-by-other-code', 'keep'); const files = h.create();
  assert.equal((await files.recover()).length, 0); assert.equal(reviews(files)[0].code, 'UNEXPECTED_CONTENTS');
  await assert.rejects(files.pick(), /核对/); assert.equal(h.read(h.directory('import-ABC123') + '/unexpected/owned-by-other-code').toString(), 'keep');
  assert.equal(h.logs.some(item => item[0] === 'rmdir'), false);
});
test('unknown namespace is retained, diagnosed, and cannot be quota-evicted', async () => {
  const h = harness(); h.putDir(ROOT); h.putDir(ROOT + '/other-files'); const files = h.create();
  assert.equal((await files.recover()).length, 0); assert.equal(reviews(files)[0].code, 'UNRECOGNIZED_NAMESPACE');
  await assert.rejects(files.pick(), /核对/); assert.ok(h.nodes.has(ROOT + '/other-files')); noNative(h);
});
test('complete preparation without request recovers an opaque handle for explicit use', async () => {
  const h = harness(); h.seed(); const files = h.create(); const values = await files.recover(); assert.equal(values.length, 1);
  assert.equal(values[0].original_request, ''); assert.equal(reviews(files).length, 0); noNative(h);
  await files.release(values[0]); noFilesUnder(h, h.directory('import-ABC123'));
});
test('corrupt cached metadata invalidates the old in-memory handle', async () => {
  const h = harness(); const files = h.create(); const value = await files.pick(); const directory = h.directory(value.spool_id);
  await assert.rejects(files.importPrepared(value, command(value), async () => { throw error('NATIVE_OUTCOME_UNKNOWN'); }));
  h.putFile(directory + '/metadata.json', '{'); assert.equal((await files.recover()).length, 0);
  await assert.rejects(files.importPrepared(value, value.original_request, async () => 'should-not-run'), /变化/);
  assert.ok(h.nodes.has(directory + '/request.json')); assert.equal(reviews(files).length, 1);
});
test('cleanup failure remains visible and blocks the unsafe cached item', async () => {
  const h = harness(); h.seed('import-ABC123', { metadata: undefined });
  h.faults.set('unlink:' + h.directory('import-ABC123') + '/data', error(13900012)); const files = h.create();
  assert.equal((await files.recover()).length, 0); const issue = reviews(files)[0];
  assert.equal(issue.code, 'PREPARE_CLEANUP_FAILED'); assert.equal(issue.request_state, 'absent'); assert.equal(issue.preserved, true);
  assert.ok(h.nodes.has(h.directory('import-ABC123') + '/data'));
});
test('oversized corrupt journals count actual bytes and block a new picker', async () => {
  const h = harness(); h.seed('import-ABC123', { request: '{' });
  h.nodes.get(h.directory('import-ABC123') + '/request.json').size = BUDGET;
  const files = h.create(); await assert.rejects(files.pick(), /64 MiB/);
  assert.equal(reviews(files)[0].code, 'REQUEST_INVALID'); assert.equal(h.logs.some(item => item[0] === 'select'), false);
  assert.ok(h.nodes.has(h.directory('import-ABC123') + '/request.json'));
});
test('diagnostics are copied so caller mutation cannot hide retained review state', async () => {
  const h = harness(); h.seed('import-ABC123', { request: '{' }); const files = h.create(); await files.recover();
  const issue = files.recoveryDiagnostics()[0]; issue.requires_review = false; issue.code = 'forged';
  assert.equal(files.recoveryDiagnostics()[0].requires_review, true); assert.equal(files.recoveryDiagnostics()[0].code, 'REQUEST_INVALID');
});

test('system open lease uses bounded safe original suffix and exact read metadata', async () => {
  const h = harness(), files = h.create();
  const lease = await files.previewFile('exact-request', '../中文:report.PDF', h.sendBytes('pdf'), 'open', '3');
  assert.equal(lease.name, '.._中文_report.PDF'); assert.equal(path.posix.basename(lease.path), lease.name);
  assert.ok(lease.path.startsWith(CACHE + '/morrow-preview-')); assert.equal(lease.uri, 'file://dev.morrow.hmos' + encodeURI(lease.path));
  assert.equal(lease.byte_length, '3'); assert.equal(lease.sha256, sha(Buffer.from('pdf')));
  assert.ok(files.isPreviewActive(lease)); assert.equal(h.fds.size, 0);
  await files.releasePreview(lease.token); assert.equal(files.isPreviewActive(lease), false); noFilesUnder(h, path.posix.dirname(lease.path) + '/');
});
test('system open requires preadmitted canonical length within 200 MiB', async () => {
  for (const length of ['', '-1', '01', '1.5', '209715201', '9007199254740992']) {
    const h = harness(); await assert.rejects(h.create().previewFile('exact', 'file.pdf', h.sendBytes('x'), 'open', length));
    noNative(h); assert.equal(h.logs.some(x => x[0] === 'mkdtemp'), false);
  }
});
test('system open actual byte length mismatch removes unregistered candidate', async () => {
  const h = harness(); await assert.rejects(h.create().previewFile('exact', 'file.pdf', h.sendBytes('xx'), 'open', '1'), /长度/);
  assert.equal(h.fds.size, 0); noFilesUnder(h, CACHE + '/morrow-preview-');
});
test('safe system preview filename preserves suffix under Unicode truncation', () => {
  const h = harness(); const value = h.attachmentPreviewName('😀'.repeat(100) + '.pdf');
  assert.ok(value.length <= 128); assert.ok(value.endsWith('.pdf')); assert.ok(!/[\uD800-\uDBFF]\.pdf$/.test(value));
  assert.equal(h.attachmentPreviewName('..'), '附件'); assert.equal(h.attachmentPreviewName('a/\u0000b.pdf'), 'a__b.pdf');
});
test('system open admission is shared but independent of image dialog and inline leases', async () => {
  const h = harness(), owner = h.create(), next = h.create();
  const open = await owner.previewFile('open', 'doc.pdf', h.sendBytes('a'), 'open', '1');
  const dialog = await next.previewFile('audio', 'sound.wav', h.sendBytes('b'), 'dialog', '1');
  const inline = await next.previewFile('inline', 'image.png', h.sendBytes('c'), 'inline', '1');
  const readCount = h.logs.filter(x => x[0] === 'send').length;
  await assert.rejects(next.previewFile('second-open', 'other.pdf', h.sendBytes('d'), 'open', '1'), /结束查看/);
  assert.equal(h.logs.filter(x => x[0] === 'send').length, readCount);
  await owner.releasePreview(open.token); await next.releasePreview(dialog.token); await next.releasePreview(inline.token);
  assert.ok(await next.previewFile('new-open', 'other.pdf', h.sendBytes('d'), 'open', '1'));
});
test('failed system open cleanup retains immutable admission until explicit retry', async () => {
  const h = harness(), owner = h.create(), next = h.create();
  const open = await owner.previewFile('open', 'doc.pdf', h.sendBytes('a'), 'open', '1');
  const dir = path.posix.dirname(open.path); h.faults.set('rmdir:' + dir, error(13900012));
  await assert.rejects(owner.releasePreview(open.token)); assert.equal(next.previewCleanupPending(), true);
  open.purpose = 'inline'; await assert.rejects(next.previewFile('second-open', 'other.pdf', h.sendBytes('a'), 'open', '1'));
  h.faults.delete('rmdir:' + dir); await next.retryPreviewCleanup(); assert.equal(next.previewCleanupPending(), false);
  assert.ok(await next.previewFile('replacement', 'other.pdf', h.sendBytes('a'), 'open', '1'));
});

test('named system preview cleanup refuses foreign sidecars before deleting its verified file', async () => {
  const h = harness(), files = h.create(); const open = await files.previewFile('open', 'doc.pdf', h.sendBytes('known'), 'open', '5');
  const dir = path.posix.dirname(open.path); h.putFile(dir + '/metadata.json', 'foreign');
  await assert.rejects(files.releasePreview(open.token)); assert.equal(h.read(open.path).toString(), 'known');
  assert.equal(h.read(dir + '/metadata.json').toString(), 'foreign');
  assert.equal(h.logs.some(x => x[0] === 'unlink' && (x[1] === open.path || x[1] === dir + '/metadata.json')), false);
});
test('named system preview cleanup validates its exact file is not a symlink', async () => {
  const h = harness(), files = h.create(); const open = await files.previewFile('open', 'doc.pdf', h.sendBytes('known'), 'open', '5');
  h.putFile('/foreign.pdf', 'keep'); h.putLink(open.path, '/foreign.pdf');
  await assert.rejects(files.releasePreview(open.token)); assert.equal(h.read('/foreign.pdf').toString(), 'keep');
  assert.equal(h.logs.some(x => x[0] === 'unlink' && x[1] === open.path), false);
});

test('batch selection returns every URI in provider order and passes the exact ceiling without copying bytes', async () => {
  const h = harness(), files = h.create();
  const batch = [URI + '?second', URI, URI + '?third']; h.setSelect(batch);
  const selected = await files.selectUris(3);
  assert.deepEqual(Array.from(selected), batch);
  assert.deepEqual(h.logs.filter(x => x[0] === 'select'), [['select', 3, 1]]);
  assert.equal(h.logs.some(x => x[0] === 'open' || x[0] === 'mkdtemp'), false);
  assert.equal(h.fds.size, 0); noNative(h); noFilesUnder(h, ROOT + '/import-');
  assert.notEqual(selected, batch, 'the provider cannot mutate the returned batch through the same array');
  batch[0] = 'content://unselected/provider-mutation'; selected[1] = 'content://unselected/caller-mutation';
  await assert.rejects(files.prepareUri(batch[0]), /系统选择/);
  await assert.rejects(files.prepareUri(selected[1]), /系统选择/); noNative(h);
  assert.equal((await files.prepareUri(URI)).name, '中文文件.txt', 'selection ownership survives caller array mutation');
  files.discardSelection();
});

test('invalid batch ceilings fail before picker admission or file copying', async () => {
  for (const maximum of [0, -1, 1.5, 21, NaN, Infinity, '2']) {
    const h = harness(); await assert.rejects(h.create().selectUris(maximum), /选择额度/);
    assert.equal(h.logs.some(x => x[0] === 'select' || x[0] === 'open' || x[0] === 'mkdtemp'), false);
    noNative(h);
  }
});

test('empty batch selection grants no URI and permits another explicit picker request', async () => {
  const h = harness(), files = h.create(); h.setSelect([]);
  assert.deepEqual(Array.from(await files.selectUris(20)), []);
  await assert.rejects(files.prepareUri(URI), /系统选择/); noNative(h); noFilesUnder(h, ROOT + '/import-');
  h.setSelect([URI]); assert.deepEqual(Array.from(await files.selectUris(1)), [URI]);
  assert.equal(h.logs.filter(x => x[0] === 'select').length, 2); files.discardSelection();
});

test('an unselected URI or another helper cannot consume the selecting helper grant', async () => {
  const h = harness(), owner = h.create(), stranger = h.create();
  await assert.rejects(owner.prepareUri(URI), /系统选择/); await owner.selectUris(2);
  await assert.rejects(owner.prepareUri(URI + '?unselected'), /系统选择/);
  await assert.rejects(stranger.prepareUri(URI), /系统选择/);
  noNative(h); assert.equal(h.logs.some(x => x[0] === 'open' || x[0] === 'mkdtemp'), false);
  const value = await owner.prepareUri(URI); assert.equal(value.name, '中文文件.txt');
  await assert.rejects(owner.prepareUri(URI), /系统选择/);
  assert.equal(h.logs.filter(x => x[0] === 'prepareFile').length, 1);
  await owner.release(value);
});

test('discarding an unread selection revokes its URI without opening or copying it', async () => {
  const h = harness(), files = h.create(); await files.selectUris(1); files.discardSelection();
  await assert.rejects(files.prepareUri(URI), /系统选择/); noNative(h);
  assert.equal(h.logs.some(x => x[0] === 'open' || x[0] === 'mkdtemp' || x[0] === 'unlink' || x[0] === 'rmdir'), false);
  assert.equal(h.read(URI).toString(), '中文\n😀');
  assert.deepEqual(Array.from(await files.selectUris(1)), [URI]); files.discardSelection();
});

const malformedSelections = [
  ['over the caller ceiling', [URI, URI + '?2', URI + '?3']],
  ['duplicate URIs', [URI, URI]],
  ['empty URI', [URI, '']],
  ['non-string URI', [URI, 42]],
  ['undefined URI', [URI, undefined]],
  ['control character in URI', [URI, URI + '\u0000']],
  ['DEL in URI', [URI, URI + '\u007f']],
  ['overlong URI', [URI, 'x'.repeat(16385)]],
  ['sparse array', [URI, ,]],
  ['null provider result', null],
  ['undefined provider result', undefined],
  ['object provider result', { 0: URI, length: 1 }],
  ['string provider result', URI],
];
for (const [label, returned] of malformedSelections) {
  test('batch rejects the whole ' + label + ' result without granting a valid prefix or admitting native work', async () => {
    const h = harness(), files = h.create(); h.setSelect(returned);
    await assert.rejects(files.selectUris(2), /系统返回/);
    await assert.rejects(files.prepareUri(URI), /系统选择/); noNative(h);
    assert.equal(h.logs.some(x => x[0] === 'open' || x[0] === 'mkdtemp'), false);
    noFilesUnder(h, ROOT + '/import-');
    h.setSelect([URI]); assert.deepEqual(Array.from(await files.selectUris(1)), [URI], 'invalid result did not leave a blocked partial batch');
    files.discardSelection();
  });
}

test('unread batch members block another picker until each is consumed or explicitly discarded', async () => {
  const h = harness(), files = h.create(), second = URI + '?second';
  h.putFile(second, 'second', { name: 'second.txt' }); h.setSelect([URI, second]);
  await files.selectUris(2); await assert.rejects(files.selectUris(1), /尚未处理/);
  const first = await files.prepareUri(URI); await assert.rejects(files.selectUris(1), /尚未处理/);
  assert.equal(h.logs.filter(x => x[0] === 'select').length, 1, 'no second provider request while unread grants exist');
  const next = await files.prepareUri(second); h.setSelect([]);
  assert.deepEqual(Array.from(await files.selectUris(1)), [], 'consuming the last grant permits a new explicit selection');
  assert.equal(h.logs.filter(x => x[0] === 'select').length, 2);
  await files.release(first); await files.release(next);
});

test('a pending provider reply serializes a second request and blocks it once the first grants arrive', async () => {
  const h = harness(), files = h.create(); let accept;
  const provider = new Promise(resolve => { accept = resolve; });
  let entered; const start = new Promise(resolve => { entered = resolve; });
  h.setSelectAsync(() => { entered(); return provider; });
  const first = files.selectUris(2); await start;
  const second = files.selectUris(1); const rejection = assert.rejects(second, /尚未处理/);
  assert.equal(h.logs.filter(x => x[0] === 'select').length, 1); noNative(h);
  accept([URI]); assert.deepEqual(Array.from(await first), [URI]); await rejection;
  assert.equal(h.logs.filter(x => x[0] === 'select').length, 1); files.discardSelection();
});

test('provider rejection grants no URI and requires a new explicit selection before preparation', async () => {
  const h = harness(), files = h.create(); h.setSelectAsync(async () => { throw error('PICKER_REJECTED'); });
  await assert.rejects(files.selectUris(3), /synthetic provider failure/);
  await assert.rejects(files.prepareUri(URI), /系统选择/); noNative(h);
  h.setSelectAsync(undefined); await files.selectUris(1);
  assert.ok(await files.prepareUri(URI)); assert.equal(h.logs.filter(x => x[0] === 'select').length, 2);
});

test('each selected URI recalculates the shared spool byte budget including other helper preparations', async () => {
  const h = harness(), files = h.create(), second = URI + '?second', third = URI + '?third';
  h.putFile(second, 'second', { name: 'second.txt' }); h.putFile(third, 'third', { name: 'third.txt' });
  h.setSelect([URI, second, third]); await files.selectUris(3);
  const first = await files.prepareUri(URI); const next = await files.prepareUri(second);
  const firstLength = Number(first.byte_length), nextLength = Number(next.byte_length);
  assert.deepEqual(h.logs.filter(x => x[0] === 'prepareFile').map(x => x[3]),
    [BUDGET - RESERVE, BUDGET - 2 * RESERVE - firstLength]);
  // A second helper prepares retained work after the batch was selected. Its
  // physical spool is part of the same root; stale initial quota cannot apply.
  const concurrent = h.seed('import-ABC123', { dataSize: BUDGET - 4 * RESERVE - 1024, request: command });
  assert.equal((await h.create().recover()).some(x => x.spool_id === concurrent.spool_id), true);
  const last = await files.prepareUri(third);
  assert.equal(h.logs.filter(x => x[0] === 'prepareFile').at(-1)[3], 1024 - firstLength - nextLength);
  assert.equal(last.byte_length, '5'); assert.equal(h.fds.size, 0);
  await files.release(first); await files.release(next); await files.release(last);
});

test('a selected URI whose shared spool count fills before preparation is denied without a native read or retry', async () => {
  const h = harness(), files = h.create(), second = URI + '?second';
  h.putFile(second, 'second'); h.setSelect([URI, second]); await files.selectUris(2);
  const first = await files.prepareUri(URI);
  for (let i = 0; i < 19; i++) { h.seed('import-' + String(100000 + i), { request: command }); }
  await assert.rejects(files.prepareUri(second), /20/);
  await assert.rejects(files.prepareUri(second), /系统选择/);
  assert.equal(h.logs.filter(x => x[0] === 'prepareFile').length, 1);
  assert.equal(h.logs.some(x => x[0] === 'open' && x[1] === second), false);
  assert.equal(h.read(h.directory(first.spool_id) + '/data').toString(), '中文\n😀');
  files.discardSelection(); await files.release(first);
});

test('a failed selected URI is consumed once while a previously prepared spool remains usable', async () => {
  const h = harness(), files = h.create(), second = URI + '?second';
  h.putFile(second, 'second'); h.setSelect([URI, second]); await files.selectUris(2);
  const first = await files.prepareUri(URI); h.faults.set('open:' + second, error(13900012));
  await assert.rejects(files.prepareUri(second)); h.faults.delete('open:' + second);
  await assert.rejects(files.prepareUri(second), /系统选择/);
  assert.equal(h.logs.filter(x => x[0] === 'prepareFile').length, 1); assert.equal(h.fds.size, 0);
  const raw = command(first); let calls = 0;
  assert.equal(await files.importPrepared(first, raw, async (sent, fd) => {
    calls++; assert.equal(sent, raw); assert.equal(h.read(h.fdPath(fd)).toString(), '中文\n😀'); return 'accepted';
  }), 'accepted'); assert.equal(calls, 1); await files.release(first);
});

test('discarding remaining grants does not delete or invalidate an already prepared spool', async () => {
  const h = harness(), files = h.create(), second = URI + '?second';
  h.putFile(second, 'second', { name: 'second.txt' }); h.setSelect([URI, second]); await files.selectUris(2);
  const prepared = await files.prepareUri(URI), savedPath = h.directory(prepared.spool_id) + '/data';
  const mutationCount = h.logs.filter(x => x[0] === 'unlink' || x[0] === 'rmdir').length;
  files.discardSelection(); files.discardSelection();
  await assert.rejects(files.prepareUri(second), /系统选择/);
  assert.equal(h.logs.filter(x => x[0] === 'unlink' || x[0] === 'rmdir').length, mutationCount);
  assert.equal(h.read(savedPath).toString(), '中文\n😀'); assert.equal(h.read(second).toString(), 'second');
  const recovered = await files.recover(); assert.equal(recovered.length, 1); assert.equal(recovered[0], prepared);
  assert.equal(await files.importPrepared(prepared, command(prepared), async () => 'accepted'), 'accepted');
  await files.release(prepared); noFilesUnder(h, h.directory(prepared.spool_id));
});
