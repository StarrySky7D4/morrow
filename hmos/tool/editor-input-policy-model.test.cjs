'use strict';
const { test } = require('node:test'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const { load, deferred } = require('./editor-field-test-harness.cjs');
const { EditorInputPolicy } = load('EditorInputPolicy'), { TextValue } = load('EditorDraft');
const plain = value => JSON.parse(JSON.stringify(value));
const text = (value, patch = {}) => Object.assign(new TextValue(), { text: value,
  selection_base: 0, selection_extent: 0, affinity: 1 }, patch);
const sha = encoded => crypto.createHash('sha256').update(encoded).digest('hex');
const count = value => [...new Intl.Segmenter('und', { granularity: 'grapheme' }).segment(value)].length;
// Synthetic receipts exercise the actual ETS validator and asynchronous owner
// barriers. The formatter algorithm is tested against real Flutter separately.
function fixture(options = {}) {
  const requests = [], owner = { current: true };
  const policy = new EditorInputPolicy(async encoded => {
    requests.push(encoded); if (options.wait) await options.wait.promise;
    const request = JSON.parse(encoded), value = options.value || request.new_value;
    const metrics = (prefix, v) => ({ [prefix + 'grapheme_count']: count(v.text),
      [prefix + 'utf16_length']: v.text.length, [prefix + 'utf8_length']: Buffer.byteLength(v.text) });
    const reply = { ok: true, error: '', field: request.field, mode: request.mode,
      limit: request.limit ?? 60, action: options.action || 'accepted', format_applied: options.applied ?? true,
      old_value: request.old_value, new_value: request.new_value, value,
      ...metrics('old_', request.old_value), ...metrics('new_', request.new_value), ...metrics('', value),
      unicode_version: '16.0.0', request_sha256: sha(encoded) };
    if (options.mutate) options.mutate(reply);
    return options.raw ?? JSON.stringify(reply);
  }, async encoded => { if (options.hashWait) await options.hashWait.promise; return options.hash || sha(encoded); });
  return { policy, requests, owner, owned: () => owner.current };
}
const tick = () => new Promise(resolve => setImmediate(resolve));

test('actual ETS keeps full Unicode text and editing metadata in the hashed request', async () => {
  const old = text('old'), next = text('汉😀e\u0301', { selection_base: 5, selection_extent: 1, affinity: 0,
    directional: true, composing_start: 1, composing_end: 3 }), h = fixture();
  const result = await h.policy.format('title', old, next, h.owned);
  assert.deepEqual(plain(result.value), plain(next)); assert.equal(result.grapheme_count, 3);
  assert.deepEqual(JSON.parse(h.requests[0]).new_value, plain(next));
});
test('selection-only overlimit receipt is accepted without authorizing a save', async () => {
  const old = text('x'.repeat(61)), next = text(old.text, { selection_base: 1, selection_extent: 2 }), h = fixture({ applied: false });
  const result = await h.policy.format('title', old, next, h.owned);
  assert.equal(result.format_applied, false); assert.equal(result.grapheme_count, 61);
});
test('truncated native prefix requires a full boundary count and retains full request', async () => {
  const next = text('😀'.repeat(61)), value = text('😀'.repeat(60), { selection_base: 120, selection_extent: 120 }), h = fixture({ value, action: 'truncated' });
  assert.deepEqual(plain((await h.policy.format('title', text(''), next, h.owned)).value), plain(value));
  assert.equal(JSON.parse(h.requests[0]).new_value.text.length, 122);
});
test('positive-limit retained must preserve the entire old editing value', async () => {
  const old = text('x'.repeat(60), { selection_base: 60, selection_extent: 60, composing_start: 3, composing_end: 3 });
  const h = fixture({ value: old, action: 'retained' });
  assert.deepEqual(plain((await h.policy.format('title', old, text(old.text + 'y'), h.owned)).value), plain(old));
  const bad = fixture({ value: text(old.text, { selection_base: 60, selection_extent: 60 }), action: 'retained' });
  await assert.rejects(bad.policy.format('title', old, text(old.text + 'y'), bad.owned), /处理结果/);
});
for (const [name, old, expected] of [
  ['collapsed composition', text('a', { composing_start: 1, composing_end: 1 }), text('a')],
  ['invalid selection', text('a', { selection_base: -1, selection_extent: 1, affinity: 0, directional: true }),
    text('a', { selection_base: -1, selection_extent: -1, affinity: 1, directional: false })],
  ['CRLF replacement', text('a\r\nb', { selection_base: 4, selection_extent: 1, affinity: 0, directional: true }),
    text('a  b', { selection_base: 4, selection_extent: 1, affinity: 0, directional: true })]
]) test('zero-budget retained validates filtered old: ' + name, async () => {
  const h = fixture({ value: expected, action: 'retained' });
  assert.deepEqual(plain((await h.policy.format('todos', old, text(old.text + 'x'), h.owned, 0)).value), plain(expected));
  const bad = fixture({ value: text('unrelated'), action: 'retained' });
  await assert.rejects(bad.policy.format('todos', old, text(old.text + 'x'), bad.owned, 0), /处理结果/);
});
test('accepted row validates filtering metadata as well as its text', async () => {
  const next = text('a\nb', { composing_start: 1, composing_end: 1 }), h = fixture({ value: text('a b') });
  assert.equal((await h.policy.format('todos', text(''), next, h.owned, 10)).value.composing_start, -1);
  const bad = fixture({ value: text('a b', { composing_start: 1, composing_end: 1 }) });
  await assert.rejects(bad.policy.format('todos', text(''), next, bad.owned, 10), /处理结果/);
});
test('accepted row cannot contradict a positive quota or zero-budget no-growth guard', async () => {
  for (const [old, next, remaining] of [[text(''), text('ab'), 1], [text('a'), text('ab'), 0]]) {
    const h = fixture(); await assert.rejects(h.policy.format('todos', old, next, h.owned, remaining), /处理结果/);
    assert.equal(h.requests.length, 1);
  }
});
test('wrong request hash, full echoes, Unicode identity and all lengths reject once', async () => {
  for (const mutate of [r => r.request_sha256 = '0'.repeat(64), r => r.old_value.affinity = 0,
    r => r.new_value.selection_base = 1, r => r.unicode_version = '17.0.0', r => r.field = 'todos',
    r => r.mode = 'todo_row', r => r.limit = 59, r => r.new_utf8_length++, r => r.utf16_length++,
    r => r.grapheme_count = 0]) {
    const h = fixture({ mutate }); await assert.rejects(h.policy.format('title', text('a'), text('b'), h.owned), /回执/);
    assert.equal(h.requests.length, 1);
  }
});
test('stop or owner change during hashing prevents any worker request', async () => {
  for (const change of [h => h.policy.stop(), h => h.owner.current = false]) {
    const hashWait = deferred(), h = fixture({ hashWait }), pending = h.policy.format('title', text(''), text('a'), h.owned);
    change(h); hashWait.resolve(); await assert.rejects(pending, /身份/); assert.equal(h.requests.length, 0);
  }
});
test('late worker reply cannot survive stop or a same-text owner epoch change', async () => {
  for (const change of [h => h.policy.stop(), h => h.owner.current = false]) {
    const wait = deferred(), h = fixture({ wait }), pending = h.policy.format('title', text(''), text('a'), h.owned);
    await tick(); change(h); wait.resolve(); await assert.rejects(pending, /变化/); assert.equal(h.requests.length, 1);
  }
});
test('original full editing values are frozen and caller mutations invalidate pending results', async () => {
  const wait = deferred(), h = fixture({ wait }), next = text('a'), pending = h.policy.format('title', text(''), next, h.owned);
  await tick(); next.selection_base = 1; wait.resolve(); await assert.rejects(pending, /变化/);
  assert.equal(JSON.parse(h.requests[0]).new_value.selection_base, 0);
});
test('invalid Unicode, row mode and wire budgets fail before issuing a worker', async () => {
  for (const [field, value, remaining] of [['title', '\uD800', undefined], ['title', 'a', 0], ['todos', 'a', -1],
    ['todos', 'a', 1001], ['title', 'x'.repeat(512 * 1024), undefined]]) {
    const h = fixture(); await assert.rejects(h.policy.format(field, text(''), text(value), h.owned, remaining)); assert.equal(h.requests.length, 0);
  }
});
test('explicit failure and malformed replies fail without an automatic replay', async () => {
  for (const options of [{ raw: '{bad' }, { raw: 'null' }, { mutate: r => { r.ok = false; r.error = 'NativeBudget'; } },
    { hash: 'invalid' }, { raw: ' '.repeat(512 * 1024 + 1) }]) {
    const h = fixture(options); await assert.rejects(h.policy.format('title', text(''), text('a'), h.owned));
    assert.equal(h.requests.length, options.hash ? 0 : 1);
  }
});
