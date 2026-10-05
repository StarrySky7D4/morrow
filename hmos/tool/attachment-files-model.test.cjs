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
    async select(options) { logs.push(['select', options.maxSelectNumber, options.selectMode]); return selectUris; }
    async save(options) { logs.push(['save', ...options.newFileNames]); return saveUris; }
  }
  const picker = { DocumentViewPicker, DocumentSelectOptions: class {}, DocumentSaveOptions: class {},
    DocumentSelectMode: { FILE: 1 } };
  const module = { exports: {} };
  vm.runInNewContext(transpiled.outputText, {
    module, exports: module.exports, Uint8Array, ArrayBuffer, console,
    require(id) {
      if (id === '@kit.CoreFileKit') { return { fileIo: fs, picker }; }
      if (id === '@kit.ArkTS') { return { util: { TextEncoder: class {
        encodeInto(text) { return new Uint8Array(Buffer.from(text)); }
      } } }; }
      if (id === 'libmorrow.so') { return { default: native }; }
      if (id === '@kit.AbilityKit' || id === '@kit.BasicServicesKit' || id === './Attachments') { return {}; }
      throw new Error('Unexpected actual-source import: ' + id);
    },
  }, { filename: sourcePath });
  const context = { filesDir: '/app/files', cacheDir: CACHE };
  const create = () => new module.exports.AttachmentFiles(context);
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
    setSelect: value => { selectUris = value; }, setSave: value => { saveUris = value; },
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
  assert.equal(h.read(value.path).toString(), 'GIF89a'); assert.equal(h.logs.some(item => item[0] === 'save' || item[0] === 'select'), false);
  await assert.rejects(files.previewFile('request', 'other', h.sendBytes('x')), /关闭/);
  await files.releasePreview('../forged'); assert.ok(h.nodes.has(value.path));
  await files.releasePreview(value.token); noFilesUnder(h, CACHE + '/morrow-preview-'); assert.equal(h.fds.size, 0);
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
