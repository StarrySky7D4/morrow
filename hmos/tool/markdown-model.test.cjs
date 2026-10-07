// Executes the actual ArkTS model with controlled transport and time.
// URI golden cases come from installed Dart Uri, independently of this model.
// These cases prove coordination/resolution, not ArkUI rendering or byte reads.
const fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../entry/src/main/ets/model/Markdown.ets'), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } }).outputText;
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
function harness(debounce = 150) {
  let now = 0, sequence = 0;
  const timers = new Map(), api = {}, calls = [], states = [];
  vm.runInNewContext(compiled, { exports: api, Error, setTimeout(fn, delay) {
    const id = ++sequence; timers.set(id, { fn, at: now + delay }); return id;
  }, clearTimeout(id) { timers.delete(id); } });
  const tick = async milliseconds => {
    const end = now + milliseconds;
    while (true) {
      const next = [...timers].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!next) break;
      now = next[1].at; timers.delete(next[0]); next[1].fn(); await settle();
    }
    now = end; await settle();
  };
  let coordinator;
  coordinator = new api.MarkdownPreviewCoordinator(
    text => new Promise((resolve, reject) => calls.push({ text, resolve, reject })),
    () => states.push({ scope: coordinator.scope, text: coordinator.text, enabled: coordinator.enabled,
      phase: coordinator.phase, loading: coordinator.loading, doc: plain(coordinator.doc), error: coordinator.error }), debounce);
  const doc = text => {
    const value = new api.MarkdownDoc(), block = new api.MarkdownBlock(), run = new api.MarkdownRun();
    run.text = text; block.runs = [run]; value.blocks = [block]; return value;
  };
  return { api, coordinator, calls, states, timers, tick, doc };
}

test('enabled preview debounces and sends only latest input', async () => {
  const h = harness(); h.coordinator.update('e1', 'off', false); await h.tick(500);
  assert.equal(h.calls.length, 0); assert.equal(h.coordinator.phase, 'idle');
  h.coordinator.update('e1', 'a', true); await h.tick(100); h.coordinator.update('e1', 'ab', true);
  await h.tick(149); assert.equal(h.calls.length, 0); await h.tick(1); assert.equal(h.calls[0].text, 'ab');
  h.calls[0].resolve(h.doc('ab')); await settle(); assert.equal(h.coordinator.phase, 'ready');
  h.coordinator.update('e1', 'ab', true); await h.tick(500); assert.equal(h.calls.length, 1); h.coordinator.dispose();
});

test('pending input removes old preview; serial work coalesces later input', async () => {
  const h = harness(); h.coordinator.update('e1', 'initial', true); await h.tick(150);
  h.calls[0].resolve(h.doc('initial')); await settle(); assert.equal(h.coordinator.doc.blocks[0].runs[0].text, 'initial');
  h.coordinator.update('e1', 'A', true); assert.equal(h.coordinator.doc, undefined); await h.tick(150);
  h.coordinator.update('e1', 'B', true); await h.tick(150); h.coordinator.update('e1', 'C', true); await h.tick(150);
  assert.equal(h.calls.length, 2); h.calls[1].resolve(h.doc('A')); await settle();
  assert.equal(h.calls.length, 3); assert.equal(h.calls[2].text, 'C'); assert.equal(h.coordinator.doc, undefined);
  h.calls[2].resolve(h.doc('C')); await settle(); assert.equal(h.coordinator.doc.blocks[0].runs[0].text, 'C');
  assert.equal(h.states.some(s => s.doc?.blocks[0]?.runs[0].text === 'A'), false); h.coordinator.dispose();
});

test('switching editor with identical text rejects stale success and failure', async () => {
  for (const fail of [false, true]) {
    const h = harness(); h.coordinator.update('A', 'same', true); await h.tick(150);
    h.coordinator.update('B', 'same', true); await h.tick(150); const states = h.states.length;
    fail ? h.calls[0].reject(new Error('obsolete')) : h.calls[0].resolve(h.doc('obsolete'));
    await settle(); assert.equal(h.states.length, states); assert.equal(h.coordinator.error, '');
    assert.equal(h.coordinator.doc, undefined); assert.equal(h.calls[1].text, 'same');
    h.calls[1].resolve(h.doc('current')); await settle(); assert.equal(h.coordinator.doc.blocks[0].runs[0].text, 'current');
    h.coordinator.dispose();
  }
});

test('A to B to A cannot revive an earlier receipt', async () => {
  const h = harness(); h.coordinator.update('A', 'A', true); await h.tick(150);
  h.coordinator.update('B', 'B', true); h.coordinator.update('A', 'A', true); await h.tick(150);
  h.calls[0].resolve(h.doc('first A')); await settle(); assert.equal(h.coordinator.doc, undefined);
  assert.equal(h.calls[1].text, 'A'); h.calls[1].resolve(h.doc('latest A')); await settle();
  assert.equal(h.coordinator.doc.blocks[0].runs[0].text, 'latest A'); h.coordinator.dispose();
});

test('stale failure cannot show an error or block the latest request', async () => {
  const h = harness(); h.coordinator.update('e1', 'before', true); await h.tick(150);
  h.coordinator.update('e1', 'after', true); await h.tick(150); h.calls[0].reject(new Error('old failure'));
  await settle(); assert.equal(h.coordinator.phase, 'loading'); assert.equal(h.coordinator.error, '');
  h.calls[1].resolve(h.doc('after')); await settle(); assert.equal(h.coordinator.phase, 'ready'); h.coordinator.dispose();
});

test('failure has a nonempty error, no automatic replay, and explicit retry', async () => {
  const h = harness(); h.coordinator.update('e1', 'current', true); await h.tick(150);
  h.calls[0].reject(new Error('')); await settle(); assert.equal(h.coordinator.phase, 'failed');
  assert.equal(h.coordinator.error, 'Markdown preview failed'); assert.equal(h.coordinator.doc, undefined);
  h.coordinator.update('e1', 'current', true); await h.tick(5000); assert.equal(h.calls.length, 1);
  h.coordinator.retry(); await h.tick(0); assert.equal(h.calls.length, 2); assert.equal(h.calls[1].text, 'current');
  h.calls[1].resolve(h.doc('current')); await settle(); assert.equal(h.coordinator.phase, 'ready');
  h.coordinator.retry(); await h.tick(0); assert.equal(h.calls.length, 2); h.coordinator.dispose();
});

test('corrected input schedules only the corrected text after a failure', async () => {
  const h = harness(); h.coordinator.update('e1', 'bad', true); await h.tick(150);
  h.calls[0].reject(new Error('failure')); await settle(); h.coordinator.update('e1', 'corrected', true);
  assert.equal(h.coordinator.error, ''); await h.tick(150); assert.equal(h.calls[1].text, 'corrected');
  h.calls[1].resolve(h.doc('corrected')); await settle(); h.coordinator.dispose();
});

test('disable cancels debounce and ignores issued work while preserving original input', async () => {
  const h = harness(); const text = 'raw 😀\n text ';
  h.coordinator.update('e1', text, true); h.coordinator.update('e1', text, false); await h.tick(500);
  assert.equal(h.calls.length, 0); h.coordinator.update('e1', text, true); await h.tick(150);
  h.coordinator.update('e1', text, false); const states = h.states.length;
  h.calls[0].reject(new Error('closed')); await settle(); assert.equal(h.states.length, states);
  assert.equal(h.coordinator.text, text); assert.equal(h.coordinator.doc, undefined);
  assert.equal(h.coordinator.phase, 'idle'); assert.equal(h.coordinator.error, ''); h.coordinator.dispose();
});

test('closing and reopening the same editor rejects the previous request', async () => {
  const h = harness(); h.coordinator.update('e1', 'same', true); await h.tick(150);
  h.coordinator.update('e1', 'same', false); h.coordinator.update('e1', 'same', true); await h.tick(150);
  h.calls[0].resolve(h.doc('previous')); await settle(); assert.equal(h.coordinator.doc, undefined);
  h.calls[1].resolve(h.doc('reopened')); await settle(); assert.equal(h.coordinator.doc.blocks[0].runs[0].text, 'reopened');
  h.coordinator.dispose();
});

test('dispose cancels debounce and late work cannot publish or schedule', async () => {
  const scheduled = harness(); scheduled.coordinator.update('e1', 'scheduled', true);
  scheduled.coordinator.dispose(); await scheduled.tick(500); assert.equal(scheduled.calls.length, 0);
  for (const fail of [false, true]) {
    const h = harness(); h.coordinator.update('e1', 'issued', true); await h.tick(150); const states = h.states.length;
    h.coordinator.dispose(); fail ? h.calls[0].reject(new Error('late')) : h.calls[0].resolve(h.doc('late')); await settle();
    h.coordinator.update('e2', 'ignored', true); h.coordinator.retry(); await h.tick(500);
    assert.equal(h.states.length, states); assert.equal(h.calls.length, 1); assert.equal(h.coordinator.disposed, true);
    assert.equal(h.coordinator.doc, undefined); assert.equal(h.coordinator.phase, 'idle');
  }
});

test('20k UTF-16 input is sent whole without trimming or clipping surrogate pairs', async () => {
  const h = harness(); const text = ' 😀\r\n' + '𠮷'.repeat(9997) + 'z';
  assert.equal(text.length, 20000); h.coordinator.update('e1', text, true); await h.tick(150);
  assert.equal(h.calls[0].text, text); h.calls[0].resolve(h.doc(text)); await settle();
  assert.equal(h.coordinator.doc.blocks[0].runs[0].text, text); assert.equal(h.coordinator.text, text); h.coordinator.dispose();
});

test('malformed JSON blocks, runs and tables are rejected rather than displayed', async () => {
  const h = harness(); const invalid = [null, {}, { blocks: null }, { blocks: [{}] }];
  const kind = h.doc('a'); kind.blocks[0].kind = 'html'; invalid.push(kind);
  const run = h.doc('a'); run.blocks[0].runs[0].image = 'true'; invalid.push(run);
  const level = h.doc('a'); level.blocks[0].level = Infinity; invalid.push(level);
  const table = h.doc('a'); table.blocks[0].rows = [{ header: true, cells: [{ runs: [{}] }] }]; invalid.push(table);
  const align = h.doc('a'); align.blocks[0].alignments = ['execute']; invalid.push(align);
  for (const [index, doc] of invalid.entries()) {
    h.coordinator.update(`editor-${index}`, 'text', true); await h.tick(150); h.calls[index].resolve(doc); await settle();
    assert.equal(h.coordinator.phase, 'failed'); assert.match(h.coordinator.error, /Invalid Markdown/);
    assert.equal(h.coordinator.doc, undefined);
  }
  h.coordinator.dispose();
});

test('complete DTO snapshots are owned independently of transport and getter mutations', async () => {
  const h = harness(); const doc = h.doc('styled'), block = doc.blocks[0], run = block.runs[0];
  Object.assign(run, { bold: true, italic: true, strike: true, code: true, href: 'https://example.com', image: true });
  Object.assign(block, { kind: 'table', indent: 2, quote: 1, marker: '☑', language: 'rust',
    rows: [{ header: true, cells: [{ runs: [{ ...run }] }] }], alignments: ['center'] });
  h.coordinator.update('e1', 'table', true); await h.tick(150); h.calls[0].resolve(doc); await settle();
  const accepted = plain(h.coordinator.doc); assert.deepEqual(accepted, plain(doc));
  doc.blocks[0].rows[0].cells[0].runs[0].text = 'transport mutated';
  const getter = h.coordinator.doc; getter.blocks[0].runs[0].text = 'getter mutated'; getter.blocks[0].alignments[0] = 'left';
  assert.deepEqual(plain(h.coordinator.doc), accepted); h.coordinator.dispose();
});

test('valid empty documents render without invented content', async () => {
  const h = harness(); h.coordinator.update('e1', '', true); await h.tick(150);
  assert.equal(h.calls[0].text, ''); h.calls[0].resolve(new h.api.MarkdownDoc()); await settle();
  assert.equal(h.coordinator.phase, 'ready'); assert.deepEqual(plain(h.coordinator.doc), { blocks: [] }); h.coordinator.dispose();
});

test('attachment paths match independent Dart URI golden results', () => {
  const { api } = harness();
  const golden = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures/flutter-markdown-uri.json'), 'utf8'));
  assert.equal(golden.dart, '3.12.0'); assert.equal(golden.cases.length, 26);
  for (const item of golden.cases) {
    const asset = Object.assign(new api.MarkdownAttachment(), { id: 'golden-asset', name: item.name ?? '', kind: 'image' });
    const result = api.resolveMarkdownImage(item.href, [asset]);
    assert.equal(result.error, item.error, item.href);
    if (item.error) { assert.equal(result.kind, 'invalid', item.href); assert.equal(result.attachment_id, ''); }
    else {
      assert.equal(result.name, item.name, item.href);
      assert.equal(result.kind, item.name ? 'attachment' : 'not_imported', item.href);
      assert.equal(result.attachment_id, item.name ? asset.id : '', item.href);
    }
  }
});

test('attachment lookup matches image and GIF name, explicit location, or asset ID', () => {
  const { api } = harness();
  const assets = [
    Object.assign(new api.MarkdownAttachment(), { id: 'asset-1', name: '中文 😀.png', location: 'C:/private/image.png', kind: 'image' }),
    Object.assign(new api.MarkdownAttachment(), { id: 'asset-gif', name: 'animation.gif', kind: 'gif' }),
  ];
  for (const href of ['attachment:%E4%B8%AD%E6%96%87%20%F0%9F%98%80.png',
    'attachment:C%3A%2Fprivate%2Fimage.png', 'attachment:asset-1']) {
    const result = api.resolveMarkdownImage(href, assets);
    assert.equal(result.kind, 'attachment'); assert.equal(result.attachment_id, 'asset-1');
    assert.equal(result.name, '中文 😀.png'); assert.equal(result.href, href);
  }
  assert.equal(api.resolveMarkdownImage('attachment:animation.gif', assets).attachment_id, 'asset-gif');
});

test('first eligible match follows inventory order without preferring ID over name', () => {
  const { api } = harness();
  const assets = [
    { id: 'file-asset', name: 'shared', location: '', kind: 'file' },
    { id: 'first-image', name: 'shared', location: '', kind: 'image' },
    { id: 'shared', name: 'last-image.png', location: '', kind: 'gif' },
  ];
  assert.equal(api.resolveMarkdownImage('attachment:shared', assets).attachment_id, 'first-image');
  assert.equal(api.resolveMarkdownImage('attachment:shared', assets.slice().reverse()).attachment_id, 'shared');
});

test('numeric tokens are literal names or IDs, never attachment indexes', () => {
  const { api } = harness();
  const assets = [{ id: 'first', name: 'one.png', location: '', kind: 'image' }];
  for (const name of ['0', '1', '-1']) { assert.equal(api.resolveMarkdownImage('attachment:' + name, assets).kind, 'not_imported'); }
  assets.push({ id: 'numbered', name: '0', location: '', kind: 'image' });
  assert.equal(api.resolveMarkdownImage('attachment:0', assets).attachment_id, 'numbered');
});

test('attachment decoding is exact once and case-sensitive without URL form decoding', () => {
  const { api } = harness();
  const assets = [{ id: 'literal', name: 'my%20file.png', location: '', kind: 'image' },
    { id: 'plus', name: 'my+file.png', location: '', kind: 'gif' }];
  assert.equal(api.resolveMarkdownImage('attachment:my%2520file.png', assets).attachment_id, 'literal');
  assert.equal(api.resolveMarkdownImage('attachment:my%20file.png', assets).kind, 'not_imported');
  assert.equal(api.resolveMarkdownImage('attachment:my+file.png?ignored#ignored', assets).attachment_id, 'plus');
  assert.equal(api.resolveMarkdownImage('attachment:MY+FILE.PNG', assets).kind, 'not_imported');
});

test('unsupported, missing or malformed attachments cannot become URI or path reads', () => {
  const { api } = harness();
  const assets = [{ id: 'file', name: 'image.png', location: '/private/image.png', kind: 'file' },
    { id: 'video', name: 'movie.mp4', location: '', kind: 'video' },
    { id: '', name: 'broken.png', location: '', kind: 'image' }];
  for (const href of ['attachment:image.png', 'attachment:movie.mp4', 'attachment:missing.png',
    'attachment:broken.png', 'file:///private/image.png', '/private/image.png', 'C:/private/image.png',
    'data:image/png;base64,AA==', 'mailto:image@example.com', 'javascript:alert(1)']) {
    const result = api.resolveMarkdownImage(href, assets);
    assert.equal(result.kind, 'not_imported', href); assert.equal(result.attachment_id, '', href);
  }
  for (const href of ['attachment:%FF.png', 'attachment:%ED%A0%80.png', 'attachment://host:bad/image.png']) {
    const result = api.resolveMarkdownImage(href, assets);
    assert.equal(result.kind, 'invalid', href); assert.equal(result.attachment_id, '', href);
  }
});

test('remote images remain explicit load candidates with a host and no credentials', () => {
  const { api } = harness();
  for (const href of ['https://example.com/image.png', 'http://example.com:8080/image.png', 'https://[::1]/image.png']) {
    const result = api.resolveMarkdownImage(href, []);
    assert.equal(result.kind, 'remote'); assert.equal(result.attachment_id, ''); assert.equal(result.href, href);
  }
  assert.equal(api.resolveMarkdownImage('HTTPS://example.com/image.png', []).href, 'https://example.com/image.png');
  for (const href of ['https://user:password@example.com/image.png', 'https:///image.png',
    'https:relative.png', 'http://example.com:bad/image.png', 'http://example.com%40evil.com/image.png',
    'https://example .com/image.png']) {
    assert.equal(api.resolveMarkdownImage(href, []).kind, 'invalid', href);
  }
});

test('image source collection covers tables and paragraphs without code or duplicate reads', () => {
  const { api } = harness();
  const image = (href, text = '') => Object.assign(new api.MarkdownRun(), { href, text, image: true });
  const paragraph = new api.MarkdownBlock(); paragraph.runs = [image('attachment:one.png', 'first alt'),
    Object.assign(new api.MarkdownRun(), { href: 'https://example.com/link', text: 'link' }), image('attachment:one.png', 'second alt')];
  const table = new api.MarkdownBlock(); table.kind = 'table'; table.rows = [{ header: true, cells: [
    { runs: [image('attachment:two.gif')] }, { runs: [image('https://example.com/remote.png')] }] }];
  const code = new api.MarkdownBlock(); code.kind = 'code'; code.runs = [image('attachment:code.png')];
  const doc = new api.MarkdownDoc(); doc.blocks = [paragraph, table, code];
  const original = plain(doc);
  assert.deepEqual(plain(api.collectMarkdownImages(doc)), ['attachment:one.png', 'attachment:two.gif', 'https://example.com/remote.png']);
  assert.deepEqual(plain(doc), original); assert.deepEqual(plain(api.collectMarkdownImages(new api.MarkdownDoc())), []);
});

test('attachment resolution does not mutate inventory and never exposes a location as an image source', () => {
  const { api } = harness();
  const asset = { id: 'verified-id', name: 'image.png', location: 'file://docs/private/image.png', kind: 'image' };
  const original = plain(asset);
  const result = api.resolveMarkdownImage('attachment:file%3A%2F%2Fdocs%2Fprivate%2Fimage.png', [asset]);
  assert.equal(result.kind, 'attachment'); assert.equal(result.attachment_id, 'verified-id');
  assert.equal(result.href, 'attachment:file%3A%2F%2Fdocs%2Fprivate%2Fimage.png');
  assert.equal(Object.values(result).includes(asset.location), false); assert.deepEqual(asset, original);
  result.attachment_id = 'mutated'; assert.equal(asset.id, 'verified-id');
});
