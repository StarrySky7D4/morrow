'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { createClipboardHarness, deferred } = require('./clipboard-test-harness.cjs');
const bytes = text => Uint8Array.from(Buffer.from(text)).buffer;

test('actual SDK snapshot reads each requested flavor and exposes opaque capabilities', async () => {
  const h = createClipboardHarness([{ 'text/plain': 'A\tB', 'text/html': '<b>A</b>', 'text/rtf': bytes('{\\rtf1 A}'),
    'application/xml': bytes('<Workbook/>'), 'application/x-provider-object': bytes('object') }]);
  const snapshot = await h.read(), record = snapshot.records[0];
  assert.equal(record.plain_text, 'A\tB'); assert.equal(record.plain.format, 'plain');
  assert.equal(record.html.format, 'html'); assert.equal(record.rtf.format, 'rtf'); assert.equal(record.xml.format, 'xml');
  assert.equal(record.binary[0].name, 'clipboard-record-1.bin');
  assert.deepEqual((await h.take(record.rtf)).bytes, Buffer.from('{\\rtf1 A}'));
  assert.equal(JSON.stringify(snapshot).includes('Workbook'), false);
  assert.equal(h.input.isCurrent(snapshot), true); await h.input.dispose(snapshot);
});
test('system file URI excludes text flavors and is never obtained from plain text', async () => {
  const uri = 'file://docs/storage/Own.pdf';
  const h = createClipboardHarness([{ 'text/uri': uri, 'text/plain': 'duplicate', 'text/html': 'duplicate' }]);
  const snapshot = await h.read(), record = snapshot.records[0];
  assert.ok(record.file); assert.equal(record.plain, undefined); assert.equal(record.html, undefined);
  assert.equal(h.logs.filter(item => item[0] === 'recordData').length, 1);
  assert.equal(JSON.stringify(record).includes(uri), false); assert.equal((await h.take(record.file)).uri, uri);
  await h.input.dispose(snapshot);
  const h2 = createClipboardHarness([{ 'text/plain': uri }]); const s2 = await h2.read();
  assert.equal(s2.records[0].file, undefined); await h2.input.dispose(s2);
});
test('non-file URI is literal content without file authority', async () => {
  const h = createClipboardHarness([{ 'text/uri': 'https://example.invalid/' }]); const s = await h.read();
  assert.equal(s.records[0].uri_text, 'https://example.invalid/'); assert.equal(s.records[0].file, undefined); await h.input.dispose(s);
});
test('binary image wins PixelMap independent of provider order and has no duplicate binary entry', async () => {
  const h = createClipboardHarness(); const pixel = h.pixel();
  h.state.records = [{ pixelMap: pixel, 'image/png': bytes('png'), 'image/gif': bytes('gif') }];
  const s = await h.read(); assert.equal(s.records[0].image.mime, 'image/gif'); assert.equal(s.records[0].binary.length, 0);
  assert.deepEqual((await h.take(s.records[0].image)).bytes, Buffer.from('gif'));
  assert.equal(h.logs.some(item => item[0] === 'packToData'), false);
  await h.input.dispose(s); assert.equal(pixel.released, 1);
});
test('PixelMap encodes bounded PNG with explicit bufferSize and releases encoder and image', async () => {
  const h = createClipboardHarness(); const pixel = h.pixel(); h.state.records = [{ pixelMap: pixel }];
  const s = await h.read(); assert.equal((await h.take(s.records[0].image, 1024)).bytes.length, 4);
  assert.deepEqual(JSON.parse(JSON.stringify(h.logs.find(item => item[0] === 'packToData')[1])), { format: 'image/png', quality: 100, bufferSize: 1024 });
  assert.equal(pixel.released, 1); assert.equal(h.logs.filter(item => item[0] === 'releasePacker').length, 1);
  await h.input.dispose(s); assert.equal(pixel.released, 1);
});
test('dispose during encoding invalidates late bytes without releasing the live encoder early', async () => {
  const done = deferred(), entered = deferred(); const h = createClipboardHarness();
  const pixel = h.pixel({ pack: async () => { entered.resolve(); return await done.promise; } }); h.state.records = [{ pixelMap: pixel }];
  const s = await h.read(); let callback = 0;
  const take = h.take(s.records[0].image, 1024, async () => { callback++; }); await entered.promise;
  const disposal = h.input.dispose(s); assert.equal(pixel.released, 0); assert.equal(h.logs.some(item => item[0] === 'releasePacker'), false);
  done.resolve(bytes('png')); await assert.rejects(take, /已变化/); await disposal;
  assert.equal(callback, 0); assert.equal(pixel.released, 1); assert.equal(h.logs.filter(item => item[0] === 'releasePacker').length, 1);
});
test('owner change while reading a flavor invalidates the entire snapshot and does not re-read', async () => {
  const h = createClipboardHarness([{ 'text/html': async () => { h.owner.current = false; return '<p>x</p>'; } }]);
  await assert.rejects(h.read(), /已变化/); assert.equal(h.logs.filter(item => item[0] === 'getData').length, 1);
});
test('changeCount after system read rejects instead of adopting new content', async () => {
  const h = createClipboardHarness([], { read: async () => { h.state.count++; } }); await assert.rejects(h.read(), /已变化/);
});
test('changeCount before consume stops old source and a fresh read requires explicit disposal', async () => {
  const h = createClipboardHarness([{ 'text/plain': 'first' }]); const s = await h.read(); h.state.count++;
  await assert.rejects(h.take(s.records[0].plain), /已变化/); await assert.rejects(h.read(), /已有/);
  await h.input.dispose(s); assert.equal((await h.read()).change_count, 2);
});
test('source is one-shot while sibling flavors remain usable until snapshot disposal', async () => {
  const h = createClipboardHarness([{ 'text/plain': 'plain', 'text/html': '<p>rich</p>' }]); const s = await h.read();
  await h.take(s.records[0].plain); await assert.rejects(h.take(s.records[0].plain), /已使用/);
  assert.deepEqual((await h.take(s.records[0].html)).bytes, Buffer.from('<p>rich</p>')); await h.input.dispose(s);
});
test('copied or fabricated source metadata cannot become authority', async () => {
  const h = createClipboardHarness([{ 'text/plain': 'plain' }]); const s = await h.read();
  await assert.rejects(h.take({ ...s.records[0].plain }), /不是本次/);
  const fabricated = new h.inputModule.ClipboardSource(); fabricated.mime = 'text/uri'; fabricated.name = '/etc/passwd';
  await assert.rejects(h.take(fabricated), /不是本次/); await h.input.dispose(s);
});
for (const field of ['name', 'mime', 'kind', 'format']) {
  test('mutated source ' + field + ' is refused', async () => {
    const h = createClipboardHarness([{ 'text/plain': 'plain' }]); const s = await h.read(); s.records[0].plain[field] = 'changed';
    await assert.rejects(h.take(s.records[0].plain), /变化/); await h.input.dispose(s);
  });
}
for (const count of [-1, 21, 1.5, Number.NaN]) {
  test('invalid or over-limit record count ' + count + ' does not silently truncate', async () => {
    const h = createClipboardHarness([], { recordCount: count }); await assert.rejects(h.read(), /20/);
    assert.equal(h.logs.some(item => item[0] === 'recordData'), false);
  });
}
for (const types of [['text/plain', , 'text/html'], ['bad\0type'], ['x'.repeat(257)], Array.from({ length: 65 }, (_, i) => 'x/' + i)]) {
  test('invalid provider MIME list is refused before flavor reads', async () => {
    const h = createClipboardHarness([{ 'text/plain': 'x' }], { offered: types }); await assert.rejects(h.read(), /格式/);
    assert.equal(h.logs.some(item => item[0] === 'recordData'), false);
  });
}
test('unrequested MIME and sparse valid-type reply do not mint capabilities', async () => {
  const h = createClipboardHarness([{ 'text/plain': 'x' }], { validTypes: () => ['unknown/not-requested'] }); await assert.rejects(h.read(), /未请求/);
  const h2 = createClipboardHarness([{ 'text/plain': 'x' }], { validTypes: () => ['text/plain', ,] }); await assert.rejects(h2.read(), /格式/);
});
test('wrong value types warn without pretending the format was imported', async () => {
  const h = createClipboardHarness([{ 'text/plain': { uri: 'file:///secret' }, pixelMap: {}, 'application/x-object': 'not binary' }]);
  const s = await h.read(); assert.equal(s.records[0].plain, undefined); assert.equal(s.records[0].image, undefined);
  assert.equal(s.records[0].binary.length, 0); assert.equal(s.warnings.length, 3); await h.input.dispose(s);
});
test('UTF8 byte limit is enforced before exposing plain_text, while valid HTML sibling survives', async () => {
  const h = createClipboardHarness([{ 'text/plain': '中'.repeat(800000), 'text/html': '<p>valid</p>' }]); const s = await h.read();
  assert.equal(s.records[0].plain_text, ''); assert.equal(s.records[0].plain, undefined); assert.ok(s.records[0].html); assert.equal(s.warnings.length, 1);
  await h.input.dispose(s);
});
test('snapshot buffers are copied before a provider can mutate its ArrayBuffer', async () => {
  const original = bytes('original'); const h = createClipboardHarness([{ 'image/png': original }]); const s = await h.read();
  new Uint8Array(original).fill(0); assert.deepEqual((await h.take(s.records[0].image)).bytes, Buffer.from('original')); await h.input.dispose(s);
});
test('PixelMap oversized dimensions stop before encoder admission and still release', async () => {
  const h = createClipboardHarness(); const pixel = h.pixel({ info: async () => ({ size: { width: 1000, height: 1000 } }) }); h.state.records = [{ pixelMap: pixel }];
  const s = await h.read(); await assert.rejects(h.take(s.records[0].image, 1000), /编码预算/);
  assert.equal(h.logs.some(item => item[0] === 'createPacker'), false); assert.equal(pixel.released, 1); await h.input.dispose(s);
});
test('PixelMap encoded overflow and rejected encoder never become output and never replay', async () => {
  for (const pack of [async () => new ArrayBuffer(1025), async () => { throw new Error('encode failed'); }]) {
    const h = createClipboardHarness(); const pixel = h.pixel({ pack }); h.state.records = [{ pixelMap: pixel }]; const s = await h.read();
    await assert.rejects(h.take(s.records[0].image, 1024)); await assert.rejects(h.take(s.records[0].image, 1024), /已使用/);
    assert.equal(pixel.released, 1); assert.equal(h.logs.filter(item => item[0] === 'packToData').length, 1); await h.input.dispose(s);
  }
});
test('snapshot disposal revokes unread sources and does not read their providers again', async () => {
  const h = createClipboardHarness([{ 'text/plain': 'plain' }]); const s = await h.read(); await h.input.dispose(s);
  await assert.rejects(h.take(s.records[0].plain), /不是本次/); assert.equal(h.input.isCurrent(s), false);
});
test('late SDK PixelMap reply is released when the owner changed before flavor admission', async () => {
  const h = createClipboardHarness(); const pixel = h.pixel();
  h.state.records = [{ pixelMap: async () => { h.owner.current = false; return pixel; } }];
  await assert.rejects(h.read(), /已变化/); assert.equal(pixel.released, 1);
});
test('HTML and RTF larger than converter limit preserve original capability within attachment budget', async () => {
  const h = createClipboardHarness([{ 'text/html': 'x'.repeat(2 * 1024 * 1024 + 1), 'text/rtf': new ArrayBuffer(2 * 1024 * 1024 + 1) }]);
  const s = await h.read(); assert.ok(s.records[0].html); assert.ok(s.records[0].rtf); assert.equal(s.warnings.length, 0);
  assert.equal((await h.take(s.records[0].html)).bytes.length, 2 * 1024 * 1024 + 1); await h.input.dispose(s);
});
test('owner change during awaited encoder cleanup invalidates successful late bytes', async () => {
  const h = createClipboardHarness([], { releasePacker: async () => { h.owner.current = false; } }); const pixel = h.pixel();
  h.state.records = [{ pixelMap: pixel }]; const s = await h.read();
  await assert.rejects(h.take(s.records[0].image, 1024), /已变化/); assert.equal(pixel.released, 1); await h.input.dispose(s);
});
test('source kinds reuse the actual attachment suffix classifier', async () => {
  const h = createClipboardHarness([{ 'audio/wav': bytes('audio'), 'video/mp4': bytes('video'), 'image/tiff': bytes('tiff'), 'image/png': bytes('png') }]);
  const s = await h.read(), record = s.records[0]; assert.equal(record.image.kind, 'image');
  assert.deepEqual(Array.from(record.binary, source => [source.name.split('.').pop(), source.kind]), [['tiff', 'file'], ['mp4', 'video'], ['wav', 'audio']]);
  await h.input.dispose(s);
});
test('encoder release failure is explicit after pixel release and does not allow replay', async () => {
  const h = createClipboardHarness([], { releasePacker: async () => { throw new Error('release failed'); } }), pixel = h.pixel();
  h.state.records = [{ pixelMap: pixel }]; const s = await h.read();
  await assert.rejects(h.take(s.records[0].image, 1024), /释放未确认/); assert.equal(pixel.released, 1);
  await assert.rejects(h.take(s.records[0].image, 1024), /已使用/); assert.ok(s.warnings.some(value => value.includes('编码器'))); await h.input.dispose(s);
});
test('disposal attempts every unused PixelMap before reporting a release failure', async () => {
  const h = createClipboardHarness(), first = h.pixel({ release: async () => { throw new Error('release failed'); } }), second = h.pixel();
  h.state.records = [{ pixelMap: first }, { pixelMap: second }]; const s = await h.read();
  await assert.rejects(h.input.dispose(s), /释放未确认/); assert.equal(first.released, 1); assert.equal(second.released, 1);
  assert.equal(h.input.isCurrent(s), false); await assert.rejects(h.take(s.records[1].image), /不是本次/);
});
