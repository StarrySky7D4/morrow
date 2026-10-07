'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createClipboardHarness, createFilesHarness, sha, stream, deferred } = require('./clipboard-test-harness.cjs');
const raw = value => Uint8Array.from(Buffer.from(value)).buffer;
const root = '/app/files/hmos-attachment-spool';
const budget = 64 * 1024 * 1024;
const reserve = 512 * 1024 + 4096;
function privateFiles(h) { return [...h.nodes.keys()].filter(name => name.startsWith(root + '/')); }
function response(value, changes = {}) {
  return { ok: true, error: '', paste_text: 'converted', warnings: [], images: [],
    source_sha256: value.sha256.toLowerCase(), source_byte_length: value.byte_length, ...changes };
}
async function prepared(records = [{ 'text/html': '<p>own</p>' }]) {
  const c = createClipboardHarness(records), h = createFilesHarness(c), snapshot = await c.read();
  const value = await h.files.prepareClipboard(snapshot.records[0].html || snapshot.records[0].plain || snapshot.records[0].image || snapshot.records[0].file);
  return { ...h, c, snapshot, value };
}
function imageResponse(h, changes = {}) {
  return response(h.value, { paste_text: '![image](attachment:clipboard-' + h.value.sha256 + '-image-1.png)', images: [{
    local_id: 'image-1', name: 'clipboard-' + h.value.sha256 + '-image-1.png', byte_length: String(h.imageBytes.length), sha256: sha(h.imageBytes), ...changes
  }] });
}

test('bounded original HTML bytes become a normal recoverable spool without raw provider data', async () => {
  const h = await prepared([{ 'text/html': '<p>中文\u0000😀</p>' }]);
  const expected = Buffer.from('<p>中文\u0000😀</p>');
  assert.deepEqual(h.read(h.directory(h.value.spool_id) + '/data'), expected);
  assert.equal(h.value.byte_length, String(expected.length)); assert.equal(h.value.sha256, sha(expected));
  assert.equal(h.fds.size, 0); assert.equal(h.logs.some(item => item[0] === 'prepareFile'), false);
  await h.c.input.dispose(h.snapshot);
  const recovered = await h.files.recover(); assert.equal(recovered[0], h.value);
  await h.files.release(h.value); assert.equal(privateFiles(h).length, 0);
});
test('a minted system URI can be opened once but never becomes a picker grant or persistent URI', async () => {
  const uri = 'file://docs/storage/Own.wav'; const c = createClipboardHarness([{ 'text/uri': uri }]); const h = createFilesHarness(c);
  h.putFile(uri, 'own audio', { name: 'Own.wav' }); const s = await c.read();
  await assert.rejects(h.files.prepareUri(uri), /不属于/);
  const value = await h.files.prepareClipboard(s.records[0].file);
  assert.equal(value.name, 'Own.wav'); assert.equal(JSON.stringify(value).includes(uri), false);
  assert.equal(h.read(h.directory(value.spool_id) + '/metadata.json').includes(Buffer.from(uri)), false);
  await assert.rejects(h.files.prepareClipboard(s.records[0].file), /已使用/);
  assert.equal(h.logs.filter(item => item[0] === 'open' && item[1] === uri).length, 1);
  await c.input.dispose(s);
});
test('plain URI text and forged capabilities cannot open a caller-selected path or invoke native', async () => {
  const c = createClipboardHarness([{ 'text/plain': 'file:///private/secret' }]); const h = createFilesHarness(c), s = await c.read();
  const fake = new c.inputModule.ClipboardSource(); fake.name = 'file:///private/secret'; fake.mime = 'text/uri';
  await assert.rejects(h.files.prepareClipboard(fake), /不是本次/);
  assert.equal(h.logs.some(item => item[0] === 'open' || item[0] === 'prepareFile'), false);
  const value = await h.files.prepareClipboard(s.records[0].plain);
  assert.deepEqual(h.read(h.directory(value.spool_id) + '/data'), Buffer.from('file:///private/secret'));
  await c.input.dispose(s);
});
test('unreadable actual system URI gives explicit failure, consumes once and cleans private partial', async () => {
  const c = createClipboardHarness([{ 'text/uri': 'file://docs/denied.pdf' }]); const h = createFilesHarness(c), s = await c.read();
  await assert.rejects(h.files.prepareClipboard(s.records[0].file), /系统剪贴板文件无法打开/);
  await assert.rejects(h.files.prepareClipboard(s.records[0].file), /已使用/);
  assert.equal(privateFiles(h).length, 0); assert.equal(h.fds.size, 0); await c.input.dispose(s);
});
test('changing owner during issued native URI read removes only unadmitted partial and never replays', async () => {
  const uri = 'file://docs/own.pdf'; const c = createClipboardHarness([{ 'text/uri': uri }]), h = createFilesHarness(c), s = await c.read();
  h.putFile(uri, 'original', { name: 'own.pdf' }); const done = deferred(), entered = deferred();
  h.setPrepare(async (sourceFd, destFd) => { entered.resolve(); await done.promise; h.writeFd(destFd, 'original'); return JSON.stringify(stream(Buffer.from('original'))); });
  const pending = h.files.prepareClipboard(s.records[0].file); await entered.promise;
  c.owner.current = false; assert.equal(h.fds.size, 2); done.resolve(); await assert.rejects(pending, /已变化/);
  assert.equal(h.fds.size, 0); assert.equal(privateFiles(h).length, 0); await c.input.dispose(s);
});
test('owner change during bounded-byte write invalidates the resulting spool before admission', async () => {
  const c = createClipboardHarness([{ 'text/html': '<p>x</p>' }]), h = createFilesHarness(c), s = await c.read();
  const originalWrite = h.fs.write; h.fs.write = async (...args) => { const result = await originalWrite(...args); c.owner.current = false; return result; };
  await assert.rejects(h.files.prepareClipboard(s.records[0].html), /已变化/);
  assert.equal(privateFiles(h).length, 0); assert.equal(h.fds.size, 0); await c.input.dispose(s);
});
test('snapshot disposal after preparation revokes only sources and does not discard a PreparedAttachment', async () => {
  const h = await prepared(); await h.c.input.dispose(h.snapshot);
  const result = await h.files.convertPreparedClipboard(h.value, 'html', 'description');
  assert.equal(result.source_sha256, h.value.sha256); assert.equal(result.paste_text, 'converted');
  assert.equal(h.files.recoveryDiagnostics().length, 0); assert.ok(h.read(h.directory(h.value.spool_id) + '/data'));
});
test('each source uses the shared 20-slot spool cap across adapter instances', async () => {
  const c = createClipboardHarness([{ 'text/html': 'new' }]), h = createFilesHarness(c), s = await c.read();
  for (let i = 0; i < 20; i++) { h.seed('import-' + String(i).padStart(6, '0')); }
  await assert.rejects(h.files.prepareClipboard(s.records[0].html), /20/);
  assert.equal(h.logs.some(item => item[0] === 'open' || item[0] === 'prepareFile'), false);
  assert.equal([...h.nodes.keys()].filter(name => /\/data$/.test(name)).length, 20); await c.input.dispose(s);
});
test('shared sidecar reserve reduces the actual binary write budget before admission', async () => {
  const c = createClipboardHarness([{ 'image/png': new ArrayBuffer(1025) }]), h = createFilesHarness(c), s = await c.read();
  h.seed('import-BUDGET', { data: 'old', dataSize: budget - reserve * 2 - 1024 });
  await assert.rejects(h.files.prepareClipboard(s.records[0].image), /剩余附件预算/);
  assert.equal([...h.nodes.keys()].filter(name => /\/data$/.test(name)).length, 1); await c.input.dispose(s);
});
test('PixelMap bufferSize uses actual remaining global quota and no packToFile bypass', async () => {
  const c = createClipboardHarness(), pixel = c.pixel(); c.state.records = [{ pixelMap: pixel }]; const h = createFilesHarness(c), s = await c.read();
  h.seed('import-BUDGET', { data: 'old', dataSize: budget - reserve * 2 - 1000 });
  const value = await h.files.prepareClipboard(s.records[0].image);
  assert.equal(c.logs.find(item => item[0] === 'packToData')[1].bufferSize, 1000); assert.equal(value.byte_length, '4');
  assert.equal(pixel.released, 1); assert.equal(h.fds.size, 0); await c.input.dispose(s);
});
for (const invalid of [0, -1, Number.NaN, 1.5, budget + 1]) {
  test('invalid byte budget ' + invalid + ' does not consume a valid source', async () => {
    const c = createClipboardHarness([{ 'text/html': 'x' }]), h = createFilesHarness(c), s = await c.read();
    await assert.rejects(h.files.prepareClipboard(s.records[0].html, invalid), /额度/);
    assert.equal(h.logs.some(item => item[0] === 'open'), false); assert.ok(await h.files.prepareClipboard(s.records[0].html)); await c.input.dispose(s);
  });
}
test('HTML over converter 2MiB limit keeps exact original but cannot enter native conversion', async () => {
  const h = await prepared([{ 'text/html': 'x'.repeat(2 * 1024 * 1024 + 1) }]);
  assert.equal(h.value.byte_length, String(2 * 1024 * 1024 + 1));
  await assert.rejects(h.files.convertPreparedClipboard(h.value, 'html', 'description'), /额度/);
  assert.equal(h.logs.some(item => item[0] === 'clipboardConvert'), false); assert.ok(h.read(h.directory(h.value.spool_id) + '/data'));
  await h.c.input.dispose(h.snapshot);
});
for (const format of ['plain', 'html', 'rtf', 'xml']) {
  test('native conversion request is exact source SHA and one of the five fields: ' + format, async () => {
    const h = await prepared(); const reply = await h.files.convertPreparedClipboard(h.value, format, 'description');
    const request = JSON.parse(h.logs.find(item => item[0] === 'clipboardConvert')[1]);
    assert.deepEqual(request, { format, section: 'description', expected_sha256: h.value.sha256 }); assert.equal(reply.source_byte_length, h.value.byte_length);
    assert.equal(h.fds.size, 0); await h.c.input.dispose(h.snapshot);
  });
}
for (const [label, changes] of [
  ['foreign source SHA', { source_sha256: 'f'.repeat(64) }], ['foreign source length', { source_byte_length: '01' }],
  ['oversized text', { paste_text: 'x'.repeat(20001) }], ['wrong warnings type', { warnings: {} }],
  ['malformed warning', { warnings: [null] }], ['images object', { images: {} }],
  ['sparse images', { images: [null] }], ['success with error', { error: 'unexpected' }],
  ['failure with content', { ok: false, error: 'fail', source_sha256: '', source_byte_length: '', paste_text: 'late' }]
]) {
  test('conversion rejects ' + label + ' while preserving original spool', async () => {
    const h = await prepared(); h.setConvert(async () => JSON.stringify(response(h.value, changes)));
    await assert.rejects(h.files.convertPreparedClipboard(h.value, 'html', 'description'));
    assert.ok(h.read(h.directory(h.value.spool_id) + '/data')); assert.equal(h.fds.size, 0); await h.c.input.dispose(h.snapshot);
  });
}
test('native explicit conversion error keeps original bytes and exposes no forged result', async () => {
  const h = await prepared(); h.setConvert(async () => JSON.stringify({ ok: false, error: 'Malformed HTML', paste_text: '', warnings: [], images: [], source_sha256: '', source_byte_length: '' }));
  await assert.rejects(h.files.convertPreparedClipboard(h.value, 'html', 'description'), /Malformed HTML/);
  assert.ok(h.read(h.directory(h.value.spool_id) + '/data')); await h.c.input.dispose(h.snapshot);
});
test('issued conversion FD and original remain alive until completion even when release is queued', async () => {
  const h = await prepared(), done = deferred(), entered = deferred();
  h.setConvert(async (request, fd) => { entered.resolve(); await done.promise; assert.equal(h.fdPath(fd), h.directory(h.value.spool_id) + '/data'); return JSON.stringify(response(h.value)); });
  const convert = h.files.convertPreparedClipboard(h.value, 'html', 'description'); await entered.promise;
  const release = h.files.release(h.value); assert.equal(h.fds.size, 1); assert.ok(h.read(h.directory(h.value.spool_id) + '/data'));
  done.resolve(); await convert; await release; assert.equal(h.fds.size, 0); assert.equal(privateFiles(h).length, 0); await h.c.input.dispose(h.snapshot);
});
test('embedded image is extracted by exact validated converter identity and full source hash', async () => {
  const h = await prepared(); h.setConvert(async () => JSON.stringify(imageResponse(h)));
  const conversion = await h.files.convertPreparedClipboard(h.value, 'html', 'description');
  const image = await h.files.prepareEmbeddedClipboardImage(h.value, conversion, 'image-1');
  assert.equal(image.name, conversion.images[0].name); assert.equal(image.sha256, sha(h.imageBytes));
  assert.deepEqual(h.read(h.directory(image.spool_id) + '/data'), h.imageBytes);
  assert.deepEqual(JSON.parse(h.logs.find(item => item[0] === 'clipboardImage')[1]), { format: 'html', section: 'description', expected_sha256: h.value.sha256, local_id: 'image-1' });
  assert.equal(h.fds.size, 0); await h.c.input.dispose(h.snapshot);
});
test('copied or changed converter object and foreign source handles cannot extract image bytes', async () => {
  const h = await prepared(); h.setConvert(async () => JSON.stringify(imageResponse(h)));
  const conversion = await h.files.convertPreparedClipboard(h.value, 'html', 'description');
  await assert.rejects(h.files.prepareEmbeddedClipboardImage(h.value, { ...conversion }, 'image-1'), /身份/);
  await assert.rejects(h.files.prepareEmbeddedClipboardImage({ ...h.value }, conversion, 'image-1'), /身份/);
  conversion.images[0].sha256 = 'f'.repeat(64); await assert.rejects(h.files.prepareEmbeddedClipboardImage(h.value, conversion, 'image-1'), /身份/);
  assert.equal(h.logs.some(item => item[0] === 'clipboardImage'), false); await h.c.input.dispose(h.snapshot);
});
for (const [label, changes] of [
  ['duplicate ID', { local_id: 'image-0' }], ['foreign name', { name: 'clipboard-foreign-image-1.png' }],
  ['path name', { name: '/private/image.png' }], ['zero length', { byte_length: '0' }],
  ['noncanonical decimal', { byte_length: '01' }], ['unsafe length', { byte_length: '9007199254740992' }],
  ['uppercase hash', { sha256: 'A'.repeat(64) }], ['too many image ID', { local_id: 'image-11' }]
]) {
  test('conversion refuses embedded ' + label, async () => {
    const h = await prepared(); h.setConvert(async () => JSON.stringify(imageResponse(h, changes)));
    await assert.rejects(h.files.convertPreparedClipboard(h.value, 'html', 'description')); assert.equal(h.fds.size, 0); await h.c.input.dispose(h.snapshot);
  });
}
test('embedded output hash or size mismatch cleans image partial and keeps original', async () => {
  for (const change of [{ sha256: 'f'.repeat(64) }, { byte_length: '5' }]) {
    const h = await prepared(); h.setConvert(async () => JSON.stringify(imageResponse(h)));
    const conversion = await h.files.convertPreparedClipboard(h.value, 'html', 'description');
    h.setImage(async (request, sourceFd, destFd) => { h.writeFd(destFd, h.imageBytes); return JSON.stringify({ ...stream(h.imageBytes), ...change }); });
    await assert.rejects(h.files.prepareEmbeddedClipboardImage(h.value, conversion, 'image-1'), /不一致/);
    assert.equal([...h.nodes.keys()].filter(name => /\/data$/.test(name)).length, 1); assert.equal(h.fds.size, 0); await h.c.input.dispose(h.snapshot);
  }
});
test('embedded size pre-admission uses remaining source-plus-sidecar quota', async () => {
  const h = await prepared(); h.setConvert(async () => JSON.stringify(imageResponse(h)));
  const conversion = await h.files.convertPreparedClipboard(h.value, 'html', 'description');
  h.seed('import-BUDGET', { data: 'old', dataSize: budget - reserve * 3 - Number(h.value.byte_length) - 5 });
  await assert.rejects(h.files.prepareEmbeddedClipboardImage(h.value, conversion, 'image-1'), /剩余附件预算/);
  assert.equal(h.logs.some(item => item[0] === 'clipboardImage'), false); await h.c.input.dispose(h.snapshot);
});
test('releasing a converter source revokes old extraction authority', async () => {
  const h = await prepared(); h.setConvert(async () => JSON.stringify(imageResponse(h)));
  const conversion = await h.files.convertPreparedClipboard(h.value, 'html', 'description'); await h.files.release(h.value);
  await assert.rejects(h.files.prepareEmbeddedClipboardImage(h.value, conversion, 'image-1'), /身份/);
  assert.equal(h.logs.some(item => item[0] === 'clipboardImage'), false); await h.c.input.dispose(h.snapshot);
});
test('owner change during encoder cleanup removes its previously complete but unadmitted spool', async () => {
  const c = createClipboardHarness([], { releasePacker: async () => { c.owner.current = false; } }), pixel = c.pixel();
  c.state.records = [{ pixelMap: pixel }]; const h = createFilesHarness(c), s = await c.read();
  await assert.rejects(h.files.prepareClipboard(s.records[0].image), /已变化/);
  assert.equal(privateFiles(h).length, 0); assert.equal(h.fds.size, 0); assert.equal(pixel.released, 1); await c.input.dispose(s);
});
test('encoder release failure prevents prepared image admission and cleans only its own spool', async () => {
  const c = createClipboardHarness([], { releasePacker: async () => { throw new Error('release failed'); } }), pixel = c.pixel();
  c.state.records = [{ pixelMap: pixel }]; const h = createFilesHarness(c), s = await c.read();
  await assert.rejects(h.files.prepareClipboard(s.records[0].image), /释放未确认/);
  assert.equal(privateFiles(h).length, 0); assert.equal(h.fds.size, 0); assert.equal(pixel.released, 1); await c.input.dispose(s);
});
