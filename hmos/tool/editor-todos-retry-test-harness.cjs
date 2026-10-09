'use strict';
const assert = require('node:assert/strict'), crypto = require('node:crypto');
const { load } = require('./editor-field-test-harness.cjs');
const { EditorInputPolicy, editorTodoFilteredOld } = load('EditorInputPolicy');
const { copyText } = load('EditorDraft');
const segmenter = new Intl.Segmenter('und', { granularity: 'grapheme' });
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
// Controlled pure worker receipts exercise the real ETS validator. This is
// neither native Unicode qualification nor a draft/business Store simulation.
function createInputWorker(options = {}) {
  const requests = [], hashes = [];
  const policy = new EditorInputPolicy(async encoded => {
    requests.push(encoded); const request = JSON.parse(encoded), number = requests.length;
    assert.deepEqual(Object.keys(request), ['field', 'mode', 'old_value', 'new_value', 'limit']);
    assert.equal(request.field, 'todos'); assert.equal(request.mode, 'todo_row');
    if (options.wait) await options.wait(request, number);
    if (options.fail && options.fail(request, number)) throw new Error('PureFormatterUnknown');
    const action = options.action ? options.action(request, number) : 'accepted';
    const value = options.value ? options.value(request, number) : editorTodoFilteredOld(request.new_value);
    const metrics = (prefix, item) => ({ [prefix + 'grapheme_count']: [...segmenter.segment(item.text)].length,
      [prefix + 'utf16_length']: item.text.length, [prefix + 'utf8_length']: Buffer.byteLength(item.text) });
    const reply = { ok: true, error: '', field: request.field, mode: request.mode, limit: request.limit,
      action, format_applied: true, old_value: request.old_value, new_value: request.new_value, value: copyText(value),
      ...metrics('old_', request.old_value), ...metrics('new_', request.new_value), ...metrics('', value),
      unicode_version: '16.0.0', request_sha256: sha(encoded) };
    if (options.mutate) options.mutate(reply, request, number);
    return JSON.stringify(reply);
  }, async encoded => { hashes.push(encoded); return sha(encoded); });
  return { policy, requests, hashes,
    format: (oldValue, newValue, remaining, guard) => policy.format('todos', oldValue, newValue, guard, remaining) };
}
module.exports = { createInputWorker };
