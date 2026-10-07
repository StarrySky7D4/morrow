'use strict';
const { test } = require('node:test'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const { load, deferred } = require('./editor-field-test-harness.cjs');
const { EditorDirectInput } = load('EditorDirectInput'), { EditorInputPolicy } = load('EditorInputPolicy');
const { editorFieldLimit } = load('EditorFieldPolicy'), { TextValue } = load('EditorDraft');
const copy = value => JSON.parse(JSON.stringify(value));
const text = (value, patch = {}) => Object.assign(new TextValue(), { text: value,
  selection_base: value.length, selection_extent: value.length, affinity: 1 }, patch);
const tick = () => new Promise(resolve => setImmediate(resolve));
const segmenter = new Intl.Segmenter('und', { granularity: 'grapheme' });
// Actual ETS coordinator and receipt validator; synthetic workers provide
// controlled ordering/decisions. Unicode/Flutter algorithms are separately
// covered by the native full-value fixtures, not by this host Segmenter.
function fixture(options = {}) {
  const root = new Map(), calls = [], requests = [], applications = [], failures = [], events = [];
  let coordinator;
  const policy = new EditorInputPolicy(async encoded => {
    const request = JSON.parse(encoded), index = requests.length; requests.push(copy(request));
    if (options.waits?.[index]) await options.waits[index].promise;
    if (options.rejects?.[index]) throw new Error(options.rejects[index]);
    const decision = options.decide?.(request, index) || {}, value = decision.value || request.new_value;
    const metric = (prefix, v) => ({ [prefix + 'grapheme_count']: [...segmenter.segment(v.text)].length,
      [prefix + 'utf16_length']: v.text.length, [prefix + 'utf8_length']: Buffer.byteLength(v.text) });
    const reply = { ok: true, error: '', field: request.field, mode: request.mode, limit: editorFieldLimit(request.field),
      action: decision.action || 'accepted', format_applied: decision.applied ??
        (request.old_value.text !== request.new_value.text || request.old_value.composing_start !== request.old_value.composing_end &&
          request.new_value.composing_start === request.new_value.composing_end),
      old_value: request.old_value, new_value: request.new_value, value,
      ...metric('old_', request.old_value), ...metric('new_', request.new_value), ...metric('', value),
      unicode_version: '16.0.0', request_sha256: crypto.createHash('sha256').update(encoded).digest('hex') };
    if (options.malformed) options.malformed(reply);
    return JSON.stringify(reply);
  }, async encoded => crypto.createHash('sha256').update(encoded).digest('hex'));
  const isCurrent = (owner, key, revision, value) => {
    const current = root.get(key);
    return !!current && current.owner === owner && current.revision === revision && JSON.stringify(current.value) === JSON.stringify(value);
  };
  coordinator = new EditorDirectInput({
    isCurrent,
    format: (field, oldValue, newValue, owned) => {
      calls.push({ field, old: copy(oldValue), next: copy(newValue), owned });
      return policy.format(field, oldValue, newValue, owned);
    },
    apply: (proposal, owned) => {
      assert.equal(owned(), true); assert.equal(isCurrent(proposal.owner, proposal.key, proposal.revision, proposal.expected), true);
      applications.push(copy(proposal));
      if (options.apply) return options.apply(proposal, owned, root, coordinator);
      const current = root.get(proposal.key); current.revision++; current.value = copy(proposal.value);
      return current.revision;
    },
    changed: () => { events.push('changed'); }
  });
  function bind(key, value = text(''), field = key, owner = 'editor-A', revision = 0, preview = false) {
    root.set(key, { owner, revision, value: copy(value) }); coordinator.bind(key, field, owner, revision, value, preview);
  }
  function capture(key, value, preview = false) {
    const current = root.get(key); current.revision++; current.value = copy(value);
    return coordinator.capture(key, current.owner, current.revision, value, preview);
  }
  return { coordinator, root, calls, requests, applications, failures, events, policy, bind, capture };
}

test('raw capture is already complete while a formatter is pending and confirmation alone is blocked', async () => {
  const wait = deferred(), h = fixture({ waits: [wait] }); h.bind('title', text('old'));
  const raw = text('汉😀e\u0301', { selection_base: 5, selection_extent: 1, affinity: 0, directional: true });
  assert.equal(h.capture('title', raw), true); assert.equal(h.coordinator.view('title').phase, 'queued');
  assert.deepEqual(h.root.get('title').value, copy(raw)); assert.equal(h.coordinator.canConfirm('title'), false);
  await tick(); assert.equal(h.coordinator.view('title').phase, 'pending'); assert.equal(h.calls.length, 1);
  assert.deepEqual(h.requests[0].new_value, copy(raw)); wait.resolve(); await tick();
  assert.equal(h.coordinator.canConfirm('title'), true); assert.equal(h.applications.length, 0);
});
test('same-turn onChange and onSelection coalesce and preserve the original old value', async () => {
  const h = fixture(); h.bind('title', text('old', { selection_base: 1, selection_extent: 2 }));
  h.capture('title', text('new')); h.capture('title', text('new', { selection_base: 1, selection_extent: 2, affinity: 0, directional: true }));
  await tick(); assert.equal(h.requests.length, 1);
  assert.deepEqual(h.requests[0].old_value, copy(text('old', { selection_base: 1, selection_extent: 2 })));
  assert.equal(h.requests[0].new_value.affinity, 0); assert.equal(h.requests[0].new_value.directional, true);
});
test('later selection revokes old worker and reissues against original edit, preventing selection-only bypass', async () => {
  const waits = [deferred(), deferred()], h = fixture({ waits,
    decide: req => ({ action: 'truncated', value: text('x'.repeat(60), { selection_base: Math.min(req.new_value.selection_base, 60),
      selection_extent: Math.min(req.new_value.selection_extent, 60) }) }) });
  h.bind('title', text('a')); h.capture('title', text('x'.repeat(61))); await tick();
  h.capture('title', text('x'.repeat(61), { selection_base: 2, selection_extent: 5 })); await tick();
  assert.equal(h.requests.length, 2); assert.equal(h.calls[0].owned(), false);
  assert.equal(h.requests[1].old_value.text, 'a'); assert.equal(h.requests[1].new_value.selection_base, 2);
  waits[0].resolve(); await tick(); assert.equal(h.applications.length, 0); assert.equal(h.coordinator.canConfirm(), false);
  waits[1].resolve(); await tick(); assert.equal(h.applications.length, 1);
  assert.deepEqual(h.root.get('title').value, copy(text('x'.repeat(60), { selection_base: 2, selection_extent: 5 })));
  assert.equal(h.coordinator.canConfirm(), true); assert.match(h.coordinator.view('title').notice, /字数/);
});
test('a new text edit uses the immediately preceding raw value and rejects previous reply', async () => {
  const waits = [deferred(), deferred()], h = fixture({ waits }); h.bind('title', text('a'));
  h.capture('title', text('ab')); await tick(); h.capture('title', text('abc')); await tick();
  assert.equal(h.requests[1].old_value.text, 'ab'); waits[1].resolve(); await tick(); waits[0].resolve(); await tick();
  assert.equal(h.root.get('title').value.text, 'abc'); assert.equal(h.coordinator.view('title').value.text, 'abc');
  assert.equal(h.coordinator.canConfirm(), true);
});
test('independent fields do not invalidate one another and task rename uses separate key with todos policy', async () => {
  const waits = [deferred(), deferred()], h = fixture({ waits }); h.bind('title'); h.bind('taskRename', text(''), 'todos');
  h.capture('title', text('new title')); await tick(); h.capture('taskRename', text('新步骤')); await tick();
  assert.equal(h.requests[1].field, 'todos'); assert.equal(h.calls[0].owned(), true);
  waits[1].resolve(); await tick(); assert.equal(h.coordinator.canConfirm('taskRename'), true); assert.equal(h.coordinator.canConfirm(), false);
  waits[0].resolve(); await tick(); assert.equal(h.coordinator.canConfirm(), true);
});
test('owner replacement and same-identity unbind/rebind prevent ABA adoption', async () => {
  for (const replace of [h => h.bind('title', text('replacement'), 'title', 'editor-B'),
    h => { h.coordinator.unbind('title'); h.bind('title', text('new'), 'title', 'editor-A', 1); }]) {
    const wait = deferred(), h = fixture({ waits: [wait] }); h.bind('title'); h.capture('title', text('new')); await tick();
    replace(h); assert.equal(h.calls[0].owned(), false); wait.resolve(); await tick();
    assert.equal(h.applications.length, 0); assert.equal(h.coordinator.view('title').phase, 'ready');
  }
});
test('stop cancels queued and running workers and stopped fields cannot confirm or capture', async () => {
  for (const running of [false, true]) {
    const wait = deferred(), h = fixture({ waits: [wait] }); h.bind('title'); h.capture('title', text('new'));
    if (running) await tick(); h.coordinator.stop(); wait.resolve(); await tick();
    assert.equal(h.coordinator.view('title').phase, 'stopped'); assert.equal(h.coordinator.canConfirm(), false);
    assert.equal(h.capture('title', text('later')), false); assert.equal(h.applications.length, 0);
    assert.equal(h.requests.length, running ? 1 : 0);
  }
});
test('an uncaptured external field revision or metadata change revokes an in-flight result', async () => {
  for (const mutate of [s => s.revision++, s => s.value.affinity = 0, s => s.value.directional = true, s => s.owner = 'other']) {
    const wait = deferred(), h = fixture({ waits: [wait] }); h.bind('title'); h.capture('title', text('new')); await tick();
    mutate(h.root.get('title')); wait.resolve(); await tick(); assert.equal(h.applications.length, 0); assert.equal(h.coordinator.canConfirm(), false);
  }
});
test('active preview is fully captured and checked without adoption; commit preserves composing oldValue gate', async () => {
  const h = fixture(); h.bind('title', text('a'));
  const candidate = text('a候选', { composing_start: 1, composing_end: 3, selection_base: 2, selection_extent: 2 });
  h.capture('title', candidate, true); await tick();
  assert.equal(h.coordinator.view('title').phase, 'deferred'); assert.equal(h.coordinator.canConfirm(), false);
  assert.deepEqual(h.root.get('title').value, copy(candidate)); assert.equal(h.applications.length, 0);
  h.capture('title', text('a候选'), false); await tick();
  assert.equal(h.requests[1].old_value.composing_start, 1); assert.equal(h.requests[1].old_value.composing_end, 3);
  assert.equal(h.requests[1].new_value.composing_start, -1); assert.equal(h.coordinator.canConfirm(), true);
});
test('preview flag alone blocks adoption even when SDK composing metadata is collapsed', async () => {
  const h = fixture(); h.bind('title'); h.capture('title', text('candidate'), true); await tick();
  assert.equal(h.coordinator.view('title').preview_active, true); assert.equal(h.coordinator.canConfirm(), false);
  h.capture('title', text('candidate'), false); await tick(); assert.equal(h.coordinator.canConfirm(), true);
});
test('a destructive preview proposal is counted separately and never replaces complete candidate raw', async () => {
  const h = fixture({ decide: () => ({ action: 'truncated', value: text('x'.repeat(60)) }) }); h.bind('title');
  h.capture('title', text('x'.repeat(61), { composing_start: 60, composing_end: 61 }), true); await tick();
  assert.equal(h.coordinator.view('title').phase, 'deferred'); assert.equal(h.coordinator.view('title').grapheme_count, -1);
  assert.equal(h.root.get('title').value.text.length, 61); assert.equal(h.applications.length, 0);
  h.capture('title', text('x'.repeat(61)), false); await tick();
  assert.equal(h.requests[1].old_value.composing_start, 60); assert.equal(h.root.get('title').value.text.length, 60);
  assert.equal(h.coordinator.view('title').grapheme_count, 60); assert.equal(h.applications.length, 1);
});
test('positive retained adoption preserves the complete old selection and receives a new field revision', async () => {
  const old = text('x'.repeat(60), { affinity: 0, directional: true });
  const h = fixture({ decide: req => ({ action: 'retained', value: req.old_value }) }); h.bind('title', old);
  h.capture('title', text('x'.repeat(60) + 'y')); await tick();
  assert.deepEqual(h.root.get('title').value, copy(old)); assert.equal(h.root.get('title').revision, 2);
  assert.equal(h.coordinator.view('title').revision, 2); assert.equal(h.coordinator.canConfirm(), true);
  assert.match(h.coordinator.view('title').notice, /上限/);
});
test('retained old active composition after commit is explicit failure, never silently cleared', async () => {
  const old = text('x'.repeat(60), { composing_start: 58, composing_end: 60 });
  const h = fixture({ decide: req => ({ action: 'retained', value: req.old_value }) }); h.bind('title', old);
  h.capture('title', text('x'.repeat(60) + 'y')); await tick();
  assert.equal(h.applications.length, 0); assert.equal(h.coordinator.view('title').phase, 'failed');
  assert.match(h.coordinator.view('title').error, /无法恢复/); assert.equal(h.root.get('title').value.text.length, 61);
  assert.equal(h.root.get('title').value.composing_start, -1); assert.equal(h.coordinator.canConfirm(), false);
});
test('worker failure retains raw, blocks confirmation and only explicit retry replays the same intent', async () => {
  const h = fixture({ rejects: ['Native unavailable'] }); h.bind('title', text('old')); h.capture('title', text('new')); await tick();
  assert.equal(h.coordinator.view('title').phase, 'failed'); assert.equal(h.coordinator.canConfirm(), false);
  assert.equal(h.root.get('title').value.text, 'new'); await tick(); assert.equal(h.requests.length, 1);
  assert.equal(h.coordinator.retry('title'), true); await tick(); assert.equal(h.requests.length, 2);
  assert.deepEqual(h.requests[1], h.requests[0]); assert.equal(h.coordinator.canConfirm(), true);
  assert.equal(h.coordinator.retry('title'), false);
});
test('selection after failed truncation retains original edit identity, not a same-text bypass', async () => {
  const h = fixture({ rejects: ['first failure'], decide: req => ({ action: 'truncated', value: text('x'.repeat(60),
    { selection_base: req.new_value.selection_base, selection_extent: req.new_value.selection_extent }) }) });
  h.bind('title', text('a')); h.capture('title', text('x'.repeat(61), { selection_base: 1, selection_extent: 1 })); await tick();
  h.capture('title', text('x'.repeat(61), { selection_base: 2, selection_extent: 2 })); await tick();
  assert.equal(h.requests[1].old_value.text, 'a'); assert.equal(h.root.get('title').value.text.length, 60); assert.equal(h.coordinator.canConfirm(), true);
});
test('late failure cannot poison a newer successful field revision', async () => {
  const wait = deferred(), h = fixture({ waits: [wait], rejects: ['late failure'] }); h.bind('title');
  h.capture('title', text('first')); await tick(); h.capture('title', text('second')); await tick();
  wait.resolve(); await tick(); assert.equal(h.coordinator.view('title').phase, 'ready'); assert.equal(h.coordinator.view('title').error, '');
  assert.equal(h.root.get('title').value.text, 'second');
});
test('apply refusal, stale adoption receipt or throwing controller cannot authorize confirmation', async () => {
  for (const apply of [() => undefined, () => 1, () => 99, () => { throw new Error('controller failed'); }]) {
    const h = fixture({ apply, decide: () => ({ action: 'truncated', value: text('x'.repeat(60)) }) });
    h.bind('title'); h.capture('title', text('x'.repeat(61))); await tick();
    assert.equal(h.coordinator.view('title').phase, 'failed'); assert.equal(h.coordinator.canConfirm(), false);
    assert.equal(h.root.get('title').value.text.length, 61);
  }
});
test('reentrant new raw during controller adoption cannot be overwritten by the previous result', async () => {
  const h = fixture({ decide: req => req.new_value.text.length > 60 ? { action: 'truncated', value: text('x'.repeat(60)) } : {},
    apply: (_proposal, _owned, root) => { const current = root.get('title'); current.revision++;
      current.value = copy(text('typed during apply')); h.coordinator.capture('title', current.owner, current.revision, current.value); return current.revision; } });
  h.bind('title'); h.capture('title', text('x'.repeat(61))); await tick();
  assert.equal(h.root.get('title').value.text, 'typed during apply'); assert.equal(h.coordinator.view('title').value.text, 'typed during apply');
  assert.equal(h.coordinator.canConfirm(), true);
});
test('callers cannot mutate bound values, captured snapshots, views or applied proposals into later adoption', async () => {
  const wait = deferred(), h = fixture({ waits: [wait] }); const bound = text('old'); h.bind('title', bound); bound.text = 'mutated bind';
  const raw = text('new'); h.capture('title', raw); raw.text = 'mutated capture';
  const view = h.coordinator.view('title'); view.value.text = 'mutated view'; await tick(); wait.resolve(); await tick();
  assert.equal(h.requests[0].old_value.text, 'old'); assert.equal(h.requests[0].new_value.text, 'new');
  assert.equal(h.coordinator.view('title').value.text, 'new'); assert.equal(h.root.get('title').value.text, 'new');
});
test('strict Unicode and independent request budget failures preserve all original raw text', async () => {
  for (const value of ['\uD800', 'x'.repeat(512 * 1024)]) {
    const h = fixture(); h.bind('title'); h.capture('title', text(value)); await tick();
    assert.equal(h.coordinator.view('title').phase, 'failed'); assert.equal(h.root.get('title').value.text, value);
    assert.equal(h.requests.length, 0); assert.equal(h.applications.length, 0); assert.equal(h.coordinator.canConfirm(), false);
  }
  const h = fixture(); h.bind('title'); h.capture('title', text('\uFFFD')); await tick(); assert.equal(h.coordinator.canConfirm(), true);
});
test('malformed receipt or invalid full metadata fails without discarding raw or automatic replay', async () => {
  const h = fixture({ malformed: reply => reply.request_sha256 = '0'.repeat(64) }); h.bind('title'); h.capture('title', text('raw')); await tick();
  assert.equal(h.coordinator.view('title').phase, 'failed'); assert.equal(h.requests.length, 1); assert.equal(h.root.get('title').value.text, 'raw');
  const invalid = fixture(); invalid.bind('title'); invalid.capture('title', text('a', { selection_base: 2 })); await tick();
  assert.equal(invalid.coordinator.view('title').phase, 'failed'); assert.equal(invalid.requests.length, 0);
});
test('selection-only after settled input follows the actual same-text gate and does not imply save eligibility', async () => {
  const h = fixture(); h.bind('title', text('x'.repeat(61))); h.capture('title', text('x'.repeat(61), { selection_base: 1, selection_extent: 3 })); await tick();
  assert.equal(h.requests[0].old_value.text, h.requests[0].new_value.text); assert.equal(h.coordinator.canConfirm(), true);
  assert.equal(h.coordinator.view('title').grapheme_count, 61); assert.equal(h.applications.length, 0);
});
test('revision reuse, stale owners, unknown fields and inactive slots cannot issue or adopt new work', async () => {
  const h = fixture(); h.bind('title');
  assert.equal(h.coordinator.capture('missing', 'editor-A', 1, text('x')), false);
  assert.equal(h.coordinator.capture('title', 'editor-B', 1, text('x')), false);
  assert.equal(h.coordinator.capture('title', 'editor-A', -1, text('x')), false);
  h.root.set('title', { owner: 'editor-A', revision: 0, value: copy(text('changed without revision')) });
  assert.equal(h.coordinator.capture('title', 'editor-A', 0, text('changed without revision')), false);
  assert.equal(h.coordinator.canConfirm('title'), false); assert.throws(() => h.bind('other', text(''), 'unknown'));
  h.coordinator.unbind('title'); assert.equal(h.coordinator.view('title'), undefined); assert.equal(h.coordinator.canConfirm(), false);
  await tick(); assert.equal(h.requests.length, 0);
});

test('foreground stop and same-owner rebind restore every unchanged field slot', async () => {
  const h = fixture(), keys = ['title', 'description', 'hypothesis', 'conclusion', 'todos'];
  for (const key of keys) { h.bind(key, text('raw-' + key)); }
  h.coordinator.stop();
  for (const key of keys) { assert.equal(h.coordinator.view(key).phase, 'stopped'); }
  for (const key of keys) {
    const current = h.root.get(key);
    h.coordinator.bind(key, key, current.owner, current.revision, current.value);
  }
  for (const key of keys) { assert.equal(h.coordinator.canConfirm(key), true, key); }
  assert.equal(h.coordinator.canConfirm(), true);
  await tick(); assert.equal(h.requests.length, 0); assert.equal(h.applications.length, 0);
});
