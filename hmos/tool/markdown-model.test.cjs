// Executes the actual ArkTS coordinator with controlled transport and time.
// These cases prove preview coordination, not ArkUI rendering or parser output.
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
