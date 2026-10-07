'use strict';
const assert = require('node:assert/strict'), { test } = require('node:test');
const { load, createPolicy, deferred, editorFieldLimit } = require('./editor-field-test-harness.cjs');
const { Values } = load('EditorDraft');
test('exact Flutter field limits use grapheme policy, including todos 1000', () => {
  assert.deepEqual(['title', 'todos', 'hypothesis', 'conclusion', 'description'].map(editorFieldLimit), [60, 1000, 5000, 10000, 20000]);
  assert.throws(() => editorFieldLimit('other'), /字段/);
});
test('emoji, combining marks, family, flags and CRLF count complete graphemes', async () => {
  for (const text of ['😀'.repeat(60), 'e\u0301'.repeat(60), '👨‍👩‍👧‍👦'.repeat(60), '🇨🇳'.repeat(60), '\r\n'.repeat(60)]) {
    const h = createPolicy(), result = await h.policy.requireField('title', text, h.owned);
    assert.equal(result.grapheme_count, 60); assert.equal(result.utf16_length, text.length);
    assert.equal(h.requests[0].text, text);
  }
});
test('each exact boundary is accepted and one additional grapheme is fully rejected', async () => {
  for (const field of ['title', 'todos', 'hypothesis', 'conclusion', 'description']) {
    const h = createPolicy(), limit = editorFieldLimit(field), text = '😀'.repeat(limit);
    assert.equal((await h.policy.requireField(field, text, h.owned)).grapheme_count, limit);
    const over = await h.policy.checkField(field, text + 'x', h.owned);
    assert.equal(over.ok, false); assert.equal(over.grapheme_count, limit + 1);
    await assert.rejects(h.policy.requireField(field, text + 'x', h.owned), /完整内容超限/);
    assert.equal(h.requests.at(-1).text, text + 'x');
  }
});
test('empty, whitespace and intentional replacement characters are never normalized', async () => {
  const h = createPolicy(); for (const text of ['', ' \t\n', '\uFFFD']) {
    const result = await h.policy.requireField('title', text, h.owned);
    assert.equal(result.utf16_length, text.length); assert.equal(h.requests.at(-1).text, text);
  }
});
test('lone high or low surrogate is rejected before encoding and paired emoji survives', async () => {
  const h = createPolicy(); for (const text of ['\uD800', 'x\uDC00', '\uD800x']) await assert.rejects(h.policy.requireField('title', text, h.owned), /Unicode/);
  assert.equal(h.requests.length, 0); assert.equal((await h.policy.requireField('title', '😀', h.owned)).grapheme_count, 1);
});
test('owner loss or stop before the worker completes rejects a late count', async () => {
  for (const action of [h => h.owner.current = false, h => h.policy.stop()]) {
    const wait = deferred(), h = createPolicy({ wait: () => wait.promise });
    const pending = h.policy.requireField('title', 'text', h.owned); await Promise.resolve(); action(h); wait.resolve();
    await assert.rejects(pending, /变化/); assert.equal(h.requests.length, 1);
  }
});
test('an already closed owner is rejected without issuing a worker request', async () => {
  const h = createPolicy(); h.owner.current = false; await assert.rejects(h.policy.checkField('title', 'text', h.owned), /变化/); assert.equal(h.requests.length, 0);
});
test('field, limit, Unicode identity and UTF16/UTF8 lengths must match the original text', async () => {
  for (const mutation of [r => r.field = 'todos', r => r.limit = 59, r => r.unicode_version = '15.0.0', r => r.utf16_length++, r => r.utf8_length++]) {
    const h = createPolicy({ mutate: mutation }); await assert.rejects(h.policy.requireField('title', '汉😀', h.owned), /回执/);
  }
});
test('invalid counts or contradictory success cannot authorize a field', async () => {
  for (const mutation of [r => r.grapheme_count = -1, r => r.grapheme_count = .5, r => r.grapheme_count = 0,
    r => r.grapheme_count = 100, r => r.ok = false, r => r.error = 'unexpected', r => r.ok = 1]) {
    const h = createPolicy({ mutate: mutation }); await assert.rejects(h.policy.requireField('title', 'text', h.owned), /回执/);
  }
});
test('a bounded native failure remains explicit and cannot become a truncated success', async () => {
  const h = createPolicy({ send: request => JSON.stringify({ ok: false, error: 'EditorFieldRequestBudget', field: request.field,
    grapheme_count: -1, utf16_length: -1, utf8_length: -1, limit: editorFieldLimit(request.field), unicode_version: '16.0.0' }) });
  const text = 'e' + '\u0301'.repeat(300000); const result = await h.policy.checkField('title', text, h.owned);
  assert.equal(result.ok, false); assert.equal(result.grapheme_count, -1);
  await assert.rejects(h.policy.requireField('title', text, h.owned), /EditorFieldRequestBudget/); assert.equal(h.requests.at(-1).text.length, 300001);
});
test('malformed JSON or worker rejection fails closed without a replay', async () => {
  for (const send of [() => '{bad', () => JSON.stringify(null), () => { throw new Error('worker failed'); }]) {
    const h = createPolicy({ send }); await assert.rejects(h.policy.requireField('title', 'x', h.owned)); assert.equal(h.requests.length, 1);
  }
});
test('full Values are frozen before awaits and all five fields are checked', async () => {
  const first = deferred(), h = createPolicy({ wait: (_request, index) => index === 1 ? first.promise : undefined });
  const values = new Values(); values.title.text = 'original'; values.todos.text = '😀'.repeat(1000);
  const pending = h.policy.requireValues(values, h.owned); values.todos.text = 'mutated'; first.resolve();
  const result = await pending; assert.equal(result.length, 5); assert.deepEqual(h.requests.map(r => r.field), ['title', 'todos', 'hypothesis', 'conclusion', 'description']);
  assert.equal(h.requests[1].text, '😀'.repeat(1000));
});
test('raw inspect and explicitly composing raw validation preserve IME metadata', async () => {
  const h = createPolicy(), values = new Values(); values.title.text = '候选'; values.title.composing_start = 0; values.title.composing_end = 2;
  await assert.rejects(h.policy.requireValues(values, h.owned), /输入法/); assert.equal(h.requests.length, 0);
  assert.equal((await h.policy.checkValues(values, h.owned)).length, 5);
  assert.equal((await h.policy.requireValues(values, h.owned, true)).length, 5);
  assert.equal(values.title.composing_end, 2);
});
test('full Values validates every field and reports oversized fields without saving', async () => {
  const h = createPolicy(), values = new Values(); values.title.text = 'x'.repeat(61); values.description.text = 'x'.repeat(20001);
  const result = await h.policy.checkValues(values, h.owned); assert.equal(result[0].ok, false); assert.equal(result[4].ok, false); assert.equal(h.requests.length, 5);
  await assert.rejects(h.policy.requireValues(values, h.owned), /超限/);
});
test('owner generation drift during full Values stops subsequent workers', async () => {
  const first = deferred(), h = createPolicy({ wait: () => first.promise });
  const pending = h.policy.requireValues(new Values(), h.owned); h.owner.current = false; first.resolve();
  await assert.rejects(pending, /变化/); assert.equal(h.requests.length, 1);
});
test('native request-byte failure before field parsing remains an explicit independent budget error', async () => {
  const h = createPolicy({ send: () => JSON.stringify({ ok: false, error: 'EditorFieldRequestBytesLimit', field: '',
    grapheme_count: -1, utf16_length: -1, utf8_length: -1, limit: 0, unicode_version: '16.0.0' }) });
  await assert.rejects(h.policy.requireField('title', 'e' + '\u0301'.repeat(300000), h.owned), /独立字节预算.*EditorFieldRequestBytesLimit/);
  assert.equal(h.requests.length, 1);
});
