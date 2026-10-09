'use strict';
const { test } = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), crypto = require('node:crypto'), path = require('node:path');
const { load, createPolicy, deferred } = require('./editor-field-test-harness.cjs');
const { EditorTodosDraft } = load('EditorTodos'), { TextValue, copyText } = load('EditorDraft');
const { editorTodoFilteredOld } = load('EditorInputPolicy');
const { createInputWorker } = require('./editor-todos-retry-test-harness.cjs');
const plain = value => JSON.parse(JSON.stringify(value));
const tick = () => new Promise(resolve => setImmediate(resolve));
function text(value, patch = {}) { return Object.assign(new TextValue(), { text: value }, patch); }
function fixture(initial = '', options = {}) {
  const field = createPolicy(), captures = [], formats = [], failures = [], statuses = [];
  let owner = 'A', revision = 0, current = text(initial), serial = 0, model;
  model = new EditorTodosDraft({
    capture(value, capturedOwner) {
      if (options.rejectCapture || owner !== capturedOwner) return undefined;
      captures.push(copyText(value)); current = copyText(value); return ++revision;
    },
    isCurrent(capturedOwner, capturedRevision, value) {
      return owner === capturedOwner && revision === capturedRevision && JSON.stringify(value) === JSON.stringify(current);
    },
    async count(value, guard) {
      if (options.countWait) await options.countWait(value);
      if (options.countError) throw new Error('CountUnknown');
      const result = await field.policy.checkField('todos', value, guard); return result.grapheme_count;
    },
    async format(oldValue, newValue, remaining, guard) {
      formats.push({ oldValue: copyText(oldValue), newValue: copyText(newValue), remaining, guard });
      if (options.format) return options.format(oldValue, newValue, remaining, guard, formats.length);
      return { action: 'accepted', value: copyText(newValue) };
    },
    changed() { if (model) statuses.push(model.status()); }, failed(capturedOwner, message) { failures.push({ owner: capturedOwner, message }); },
    id() { return options.id ? options.id() : 'row_' + (++serial); }
  });
  model.bind(owner, revision, current);
  return { model, captures, formats, failures, statuses, get raw() { return current; }, get owner() { return owner; }, get revision() { return revision; },
    ticket(index = 0) { return model.ticket(model.view().rows[index].id); },
    bind(value = current, newOwner = owner) { owner = newOwner; current = copyText(value); revision++; model.bind(owner, revision, current); },
    echo() { model.bind(owner, revision, current); },
    external(value = current) { current = copyText(value); revision++; },
    switchOwner(newOwner = 'B', value = text('B')) { owner = newOwner; current = copyText(value); revision++; model.bind(owner, revision, current); }
  };
}

test('actual ETS model exposes its fresh source identity and uses injected field count/formatter only', t => {
  const source = fs.readFileSync(path.resolve(__dirname, '../entry/src/main/ets/model/EditorTodos.ets'));
  t.diagnostic('EditorTodos.ets SHA256 ' + crypto.createHash('sha256').update(source).digest('hex'));
  assert.equal(typeof EditorTodosDraft, 'function');
  assert.doesNotMatch(source.toString(), /Intl\.Segmenter|\.slice\(0,\s*1000\)|tasksAdd|TaskId\s*=/);
});

test('LF split preserves interior/trailing blanks and CR original without trimming or normalization', () => {
  const f = fixture('first\n\nlast\r\n');
  assert.deepEqual(plain(f.model.view().rows.map(r => r.value.text)), ['first', '', 'last\r', '']);
  assert.equal(f.model.view().value.text, 'first\n\nlast\r\n');
});

test('empty source has zero rows but explicit Add keeps an empty local row across its same-value echo', async () => {
  const f = fixture(); assert.equal(f.model.view().rows.length, 0);
  const id = await f.model.add(); assert.ok(id); f.echo();
  assert.equal(f.raw.text, ''); assert.equal(f.model.view().rows.length, 1);
  assert.equal(fixture(f.raw.text).model.view().rows.length, 0);
});

test('external source sync reuses IDs by index and invalidates previous row callbacks', async () => {
  const f = fixture('a\nb'), ticket = f.ticket(), before = f.model.view().rows.map(r => r.id);
  f.bind(text('x\ny\nz'));
  assert.deepEqual(f.model.view().rows.slice(0, 2).map(r => r.id), before);
  await f.model.input(ticket, 'old'); assert.equal(f.raw.text, 'x\ny\nz');
});

test('row and view copies cannot mutate private raw state', () => {
  const f = fixture('a'), view = f.model.view(), row = f.model.row(view.rows[0].id);
  view.rows[0].value.text = 'tampered'; view.value.text = 'tampered'; row.value.text = 'tampered';
  assert.equal(f.model.view().value.text, 'a'); assert.equal(f.model.row(view.rows[0].id).value.text, 'a');
});

test('duplicate and malformed local IDs reject instead of producing ambiguous rows', () => {
  assert.throws(() => fixture('a\nb', { id: () => 'same' }), /标识/);
  assert.throws(() => fixture('a', { id: () => 'bad/id' }), /标识/);
});

test('selection mapping uses UTF16 offsets across rows including a supplementary emoji', async () => {
  const f = fixture('😀\nnext'); await f.model.selection(f.ticket(1), 3, 1);
  assert.equal(f.raw.selection_base, 6); assert.equal(f.raw.selection_extent, 4);
  assert.equal(f.raw.text, '😀\nnext');
});

test('external aggregate selection maps within one row but preserves a cross-row selection only in raw', () => {
  const f = fixture('😀\nnext'); f.bind(text('😀\nnext', { selection_base: 4, selection_extent: 6, affinity: 1, directional: true }));
  assert.equal(f.model.view().rows[1].value.selection_base, 1);
  f.bind(text('😀\nnext', { selection_base: 1, selection_extent: 5 }));
  assert.equal(f.model.view().value.selection_extent, 5);
  assert.equal(f.model.view().rows[1].value.selection_extent, -1);
});

test('external metadata clearing does not leave stale composition on a row', () => {
  const f = fixture('abc'); f.bind(text('abc', { composing_start: 0, composing_end: 2 }));
  assert.equal(f.model.view().rows[0].value.composing_end, 2);
  f.bind(text('abc')); assert.equal(f.model.view().rows[0].value.composing_end, -1);
  assert.equal(f.model.canChangeRows(), true);
});

test('remaining budget subtracts other graphemes and one LF per gap, not their UTF16 lengths', async () => {
  const f = fixture('😀\nx\n'); await f.model.input(f.ticket(1), 'updated');
  assert.equal(f.formats[0].remaining, 997); assert.equal(f.raw.text, '😀\nupdated\n');
});

test('zero remaining is forwarded intact to native row no-growth policy', async () => {
  const f = fixture('a'.repeat(999) + '\nx', { format: (oldValue) => ({ action: 'retained', value: editorTodoFilteredOld(oldValue) }) });
  await f.model.input(f.ticket(1), 'growth');
  assert.equal(f.formats[0].remaining, 0); assert.equal(f.raw.text, 'a'.repeat(999) + '\nx');
  assert.match(f.model.view().notice, /原内容/);
});

test('zero retained adopts complete filtered-old invalid-selection finalize instead of demanding exact-old', async () => {
  const f = fixture('\n' + 'x'.repeat(999), { format: oldValue => ({ action: 'retained', value: editorTodoFilteredOld(oldValue) }) });
  await f.model.input(f.ticket(), 'growth');
  assert.equal(f.formats[0].remaining, 0); assert.equal(f.raw.text, '\n' + 'x'.repeat(999));
  assert.equal(f.raw.selection_base, -1); assert.equal(f.raw.selection_extent, -1); assert.equal(f.raw.affinity, 1);
  assert.equal(f.raw.directional, false); assert.equal(f.model.view().business_ready, true);
});

test('zero retained keeps legal reverse UTF16 positions/affinity/direction and replaces raw CR with one space', async () => {
  const f = fixture('ab\r\n' + 'x'.repeat(999), { format: oldValue => ({ action: 'retained', value: editorTodoFilteredOld(oldValue) }) });
  f.bind(text(f.raw.text, { selection_base: 2, selection_extent: 0, affinity: 0, directional: true }));
  await f.model.input(f.ticket(), 'ab\rGrowth');
  assert.equal(f.formats[0].remaining, 0); assert.equal(f.raw.text, 'ab \n' + 'x'.repeat(999));
  assert.equal(f.raw.selection_base, 2); assert.equal(f.raw.selection_extent, 0);
  assert.equal(f.raw.affinity, 0); assert.equal(f.raw.directional, true); assert.equal(f.model.view().business_ready, true);
});

test('zero retained finalizes collapsed composition but rejects any unrelated receipt position change', async () => {
  const f = fixture('\n' + 'x'.repeat(999), { format: oldValue => ({ action: 'retained', value: editorTodoFilteredOld(oldValue) }) });
  f.bind(text(f.raw.text, { selection_base: 0, selection_extent: 0, affinity: 0, composing_start: 0, composing_end: 0 }));
  await f.model.input(f.ticket(), 'growth');
  assert.equal(f.raw.composing_start, -1); assert.equal(f.raw.composing_end, -1); assert.equal(f.raw.affinity, 0);
  assert.equal(f.model.view().business_ready, true);
  const bad = fixture('\n' + 'x'.repeat(999), { format: oldValue => {
    const value = editorTodoFilteredOld(oldValue); value.affinity = 0; return { action: 'retained', value };
  } });
  await bad.model.input(bad.ticket(), 'full candidate');
  assert.equal(bad.raw.text, 'full candidate\n' + 'x'.repeat(999)); assert.equal(bad.model.view().business_ready, false);
});

test('positive remaining retained continues to require exact old without zero-rule metadata relaxation', async () => {
  const f = fixture('old', { format: oldValue => ({ action: 'retained', value: editorTodoFilteredOld(oldValue) }) });
  await f.model.input(f.ticket(), 'whole future candidate');
  assert.equal(f.formats[0].remaining, 1000); assert.equal(f.raw.text, 'whole future candidate');
  assert.equal(f.model.view().business_ready, false);
});

test('native CRLF-filter full result is adopted without an independent ETS replacement', async () => {
  const f = fixture('a', { format: () => ({ action: 'accepted', value: text('a  b', { selection_base: 4, selection_extent: 4, affinity: 1 }) }) });
  await f.model.input(f.ticket(), 'a\r\nb');
  assert.equal(f.formats[0].newValue.text, 'a\r\nb'); assert.equal(f.raw.text, 'a  b');
  assert.equal(f.raw.selection_base, 4); assert.equal(f.raw.affinity, 1);
});

test('complete raw future candidate is captured before any asynchronous formatter; result is visible', async () => {
  const wait = deferred(), f = fixture('old', { format: async () => wait.promise });
  const running = f.model.input(f.ticket(), '完整candidate'); await tick();
  assert.equal(f.raw.text, '完整candidate'); assert.equal(f.model.view().business_ready, false);
  assert.equal(f.model.view().raw_capture_complete, true); assert.equal(f.model.view().format_pending, true);
  wait.resolve({ action: 'truncated', value: text('完整') }); await running;
  assert.equal(f.raw.text, '完整'); assert.equal(f.model.view().business_ready, true); assert.match(f.model.view().notice, /核对/);
});

test('malformed native receipt preserves complete candidate and blocks confirmed publication', async () => {
  const f = fixture('old', { format: () => ({ action: 'retained', value: text('different') }) });
  await f.model.input(f.ticket(), 'candidate'); assert.equal(f.raw.text, 'candidate');
  assert.equal(f.model.view().business_ready, false); assert.equal(f.failures.length, 1);
  assert.equal(f.model.view().raw_capture_complete, true); assert.equal(f.model.view().capture_error, '');
  assert.match(f.model.view().format_error, /回执/);
});

test('pure worker Unknown never removes or truncates the captured raw candidate', async () => {
  const f = fixture('old', { format: () => { throw new Error('Unknown'); } });
  await f.model.input(f.ticket(), 'whole raw'); assert.equal(f.raw.text, 'whole raw'); assert.match(f.model.view().error, /Unknown/);
  assert.equal(f.model.status().raw_capture_complete, true); assert.equal(f.model.status().business_ready, false);
  assert.equal(f.model.status().format_pending, false); assert.equal(f.model.status().capture_error, '');
});

test('preview keeps complete text and composition in aggregate, with no native formatting until commit', async () => {
  const f = fixture('first\nab'); await f.model.input(f.ticket(1), 'ab', '候😀', 1);
  assert.equal(f.raw.text, 'first\na候😀b'); assert.equal(f.raw.composing_start, 7); assert.equal(f.raw.composing_end, 10);
  assert.equal(f.model.view().rows[1].display_text, 'ab'); assert.equal(f.formats.length, 0);
  assert.equal(f.model.canChangeRows(), false);
  assert.equal(f.model.status().raw_capture_complete, true); assert.equal(f.model.status().business_ready, false);
  assert.equal(f.model.status().format_pending, false); assert.equal(f.model.status().format_error, '');
  await f.model.input(f.ticket(1), 'a候😀b'); assert.equal(f.formats.length, 1); assert.equal(f.raw.composing_start, -1);
});

test('unknown preview offset preserves old raw and reports capture incomplete without guessing', async () => {
  const f = fixture('old'); await f.model.input(f.ticket(), 'new', '候选', 9);
  assert.equal(f.raw.text, 'old'); assert.equal(f.model.view().business_ready, false); assert.equal(f.formats.length, 0);
  assert.equal(f.model.status().raw_capture_complete, false); assert.match(f.model.status().capture_error, /候选位置/);
  assert.equal(f.model.status().format_error, '');
});

test('row selection during pending formatting invalidates old reply and rechecks exact new UTF16 selection', async () => {
  const a = deferred(), b = deferred(), f = fixture('old', { format: (_, __, ___, ____, n) => n === 1 ? a.promise : b.promise });
  const first = f.model.input(f.ticket(), 'candidate'); await tick();
  const second = f.model.selection(f.ticket(), 2, 2); await tick();
  a.resolve({ action: 'accepted', value: text('stale') }); await first; assert.equal(f.raw.text, 'candidate');
  b.resolve({ action: 'accepted', value: text('candidate', { selection_base: 2, selection_extent: 2 }) }); await second;
  assert.equal(f.raw.selection_base, 2); assert.equal(f.formats[1].oldValue.text, 'old');
});

test('rapid same-row typing retains frozen last confirmed old value and rejects late candidate reply', async () => {
  const a = deferred(), b = deferred(), f = fixture('old', { format: (_, __, ___, ____, n) => n === 1 ? a.promise : b.promise });
  const first = f.model.input(f.ticket(), 'first'); await tick();
  const second = f.model.input(f.ticket(), 'second'); await tick();
  assert.equal(f.formats[1].oldValue.text, 'old');
  b.resolve({ action: 'accepted', value: text('second') }); await second;
  a.resolve({ action: 'accepted', value: text('first') }); await first; assert.equal(f.raw.text, 'second');
});

test('unexpected second-row input preserves complete raw candidates but cannot publish pending formatting', async () => {
  const wait = deferred(), f = fixture('a\nb', { format: () => wait.promise });
  const first = f.model.input(f.ticket(), 'A'); await tick(); assert.equal(f.model.canEdit(f.ticket(1).id), false);
  await f.model.input(f.ticket(1), 'B'); assert.equal(f.raw.text, 'A\nB'); assert.equal(f.model.view().business_ready, false);
  wait.resolve({ action: 'accepted', value: text('A') }); await first; assert.equal(f.raw.text, 'A\nB');
});

test('owner switch blocks late row formatter and old controller events', async () => {
  const wait = deferred(), f = fixture('a', { format: () => wait.promise }), ticket = f.ticket();
  const running = f.model.input(ticket, 'A'); await tick(); f.switchOwner();
  wait.resolve({ action: 'accepted', value: text('stale') }); await running; await f.model.input(ticket, 'old controller');
  assert.equal(f.raw.text, 'B'); assert.equal(f.model.view().owner, 'B');
});

test('same text edit-away-and-back at parent epoch blocks a late formatter', async () => {
  const wait = deferred(), f = fixture('a', { format: () => wait.promise });
  const running = f.model.input(f.ticket(), 'A'); await tick(); f.external(text('other')); f.external(text('A'));
  wait.resolve({ action: 'accepted', value: text('stale') }); await running; assert.equal(f.raw.text, 'A');
});

test('stop revokes callbacks and preserves complete raw, while fresh bind cannot qualify canceled formatting', async () => {
  const wait = deferred(), f = fixture('a', { format: () => wait.promise }), ticket = f.ticket();
  const running = f.model.input(ticket, 'A'); await tick(); f.model.stop();
  wait.resolve({ action: 'accepted', value: text('stale') }); await running; await f.model.input(ticket, 'late');
  assert.equal(f.raw.text, 'A'); f.echo(); assert.equal(f.model.canChangeRows(), false); assert.equal(f.model.view().raw_capture_complete, true);
});

test('capture rejection restores source-consistent local rows instead of creating a second raw draft', async () => {
  const f = fixture('old', { rejectCapture: true }); await f.model.input(f.ticket(), 'new');
  assert.equal(f.raw.text, 'old'); assert.equal(f.model.view().value.text, 'old');
  assert.equal(f.model.view().rows[0].value.text, 'old'); assert.equal(f.model.view().business_ready, false);
  assert.equal(f.model.status().raw_capture_complete, false); assert.match(f.model.status().capture_error, /未确认/);
});

test('Add includes new separator in full future count and refuses total1000 without slicing', async () => {
  const f = fixture('a'.repeat(999)); assert.ok(await f.model.add()); assert.equal(f.raw.text.length, 1000);
  assert.equal(await f.model.add(), undefined); assert.equal(f.model.view().rows.length, 2);
});

test('Add caps local rows100 but restore preserves over100 existing raw rows', async () => {
  const f = fixture(Array(100).fill('').join('\n')); assert.equal(await f.model.add(), undefined);
  const large = fixture(Array(101).fill('x').join('\n')); assert.equal(large.model.view().rows.length, 101);
  assert.equal(large.raw.text.split('\n').length, 101);
});

test('late Add count after changed owner cannot append a row to a new card', async () => {
  const wait = deferred(), f = fixture('', { countWait: () => wait.promise });
  const running = f.model.add(); f.switchOwner('B', text('b')); wait.resolve();
  await assert.rejects(running, /变化|终止/); assert.equal(f.raw.text, 'b');
});

test('remove accepts blank row, joins all remaining rows, and resets aggregate selection only', () => {
  const f = fixture('a\n\nb'); f.model.remove(f.ticket(1)); assert.equal(f.raw.text, 'a\nb'); assert.equal(f.raw.selection_base, -1);
  f.model.remove(f.ticket()); f.model.remove(f.ticket()); assert.equal(f.raw.text, ''); assert.equal(f.model.view().rows.length, 0);
});

test('up/down preserve row IDs and row UTF16 cursor metadata while changing aggregate order', async () => {
  const f = fixture('a\n😀'); await f.model.selection(f.ticket(1), 2, 2); const id = f.ticket(1).id;
  f.model.move(f.ticket(1), -1); assert.equal(f.raw.text, '😀\na'); assert.equal(f.model.view().rows[0].id, id);
  assert.equal(f.model.view().rows[0].value.selection_base, 2); assert.equal(f.raw.selection_base, -1);
  f.model.move(f.ticket(), 1); assert.equal(f.raw.text, 'a\n😀');
});

test('private drag can insert before and after a real target with stable row identity', () => {
  const f = fixture('a\nb\nc'), id = f.ticket().id, drag = f.model.beginDrag(f.ticket());
  f.model.drop(drag, f.ticket(2), true); assert.equal(f.raw.text, 'b\nc\na'); assert.equal(f.model.view().rows[2].id, id);
  f.model.drop(f.model.beginDrag(f.ticket(2)), f.ticket(), false); assert.equal(f.raw.text, 'a\nb\nc');
});

test('stale drag revision and old owner never reorder raw', async () => {
  const f = fixture('a\nb'), drag = f.model.beginDrag(f.ticket()); await f.model.selection(f.ticket(), 1, 1);
  f.model.drop(drag, f.ticket(1), true); assert.equal(f.raw.text, 'a\nb');
  const old = f.model.beginDrag(f.ticket()); f.switchOwner('B', text('x\ny'));
  f.model.drop(old, f.ticket(1), true); assert.equal(f.raw.text, 'x\ny');
});

test('pending/IME prevents remove, move, Add and beginDrag without losing raw candidate', async () => {
  const f = fixture('a\nb'); await f.model.input(f.ticket(), 'a', '候', 1); const before = f.raw.text;
  f.model.remove(f.ticket()); f.model.move(f.ticket(), 1);
  assert.equal(await f.model.add(), undefined); assert.equal(f.model.beginDrag(f.ticket()), undefined); assert.equal(f.raw.text, before);
});

test('status owns an independent full TextValue and cannot mutate row capture authority', () => {
  const f = fixture('😀\nraw'); f.bind(text(f.raw.text, { selection_base: 5, selection_extent: 3, affinity: 0, directional: true }));
  const status = f.model.status(); assert.equal(status.owner, f.owner); assert.equal(status.revision, f.revision);
  assert.deepEqual(plain(status.value), plain(f.raw)); status.value.text = 'changed'; status.value.selection_base = 0;
  assert.equal(f.model.status().value.text, '😀\nraw'); assert.equal(f.model.status().value.selection_base, 5);
});

test('pending count and formatter never transiently publish a captured candidate as business-ready', async () => {
  const wait = deferred(), f = fixture('old\nother', { countWait: () => wait.promise });
  const running = f.model.input(f.ticket(), 'whole pending raw'); await tick();
  const captured = f.statuses.filter(status => status.value.text === 'whole pending raw\nother');
  assert.ok(captured.length > 0); assert.ok(captured.every(status => status.raw_capture_complete && !status.business_ready && status.format_pending));
  assert.deepEqual(plain(f.model.status().value), plain(f.raw)); wait.resolve(); await running;
  assert.equal(f.model.status().business_ready, true);
});

test('same exact parent echo retains pending status and full raw; epoch change cancels only formatting qualification', async () => {
  const wait = deferred(), f = fixture('old', { format: () => wait.promise });
  const running = f.model.input(f.ticket(), 'whole raw'); await tick(); f.echo();
  assert.equal(f.model.status().format_pending, true); assert.equal(f.model.status().raw_capture_complete, true);
  f.bind(); assert.equal(f.model.status().format_pending, false); assert.equal(f.model.status().raw_capture_complete, true);
  assert.equal(f.model.status().business_ready, false); assert.match(f.model.status().format_error, /中断/);
  wait.resolve({ action: 'accepted', value: text('stale') }); await running; assert.equal(f.raw.text, 'whole raw');
});

test('Unknown and incomplete capture remain separate across same raw rebind and recover after a confirmed new input', async () => {
  let unknown = true;
  const f = fixture('old', { format: (_, value) => { if (unknown) throw new Error('Unknown'); return { action: 'accepted', value }; } });
  await f.model.input(f.ticket(), 'full candidate'); f.bind();
  assert.equal(f.model.status().raw_capture_complete, true); assert.equal(f.model.status().business_ready, false);
  unknown = false; await f.model.input(f.ticket(), 'confirmed candidate'); assert.equal(f.model.status().business_ready, true);
  await f.model.input(f.ticket(), 'unlocated', '候', 99); f.bind();
  assert.equal(f.model.status().raw_capture_complete, false); assert.equal(f.model.status().business_ready, false);
  await f.model.input(f.ticket(), 'recovered'); assert.equal(f.model.status().raw_capture_complete, true);
  assert.equal(f.model.status().business_ready, true); assert.equal(f.model.status().capture_error, '');
});

test('disabling a fully confirmed raw value does not make its existing formatting result incomplete', async () => {
  const f = fixture('old'); await f.model.input(f.ticket(), 'confirmed'); f.model.stop();
  assert.equal(f.model.status().raw_capture_complete, true); assert.equal(f.model.status().business_ready, true);
  assert.equal(f.model.canChangeRows(), false); f.echo(); assert.equal(f.model.canChangeRows(), true);
});

test('raw sentinel and cross-row composition are retained without guessing a restorable row range', async () => {
  const f = fixture('abc\ndef'); f.bind(text(f.raw.text, { composing_start: 1, composing_end: -1 }));
  assert.deepEqual(plain(f.model.status().value), plain(f.raw)); assert.equal(f.model.status().raw_capture_complete, true);
  assert.equal(f.model.status().business_ready, false); assert.equal(f.model.canEdit(f.ticket().id), false);
  await f.model.selection(f.ticket(), 1, 1); assert.equal(f.raw.composing_start, 1); assert.equal(f.raw.composing_end, -1);
  f.bind(text(f.raw.text, { composing_start: 2, composing_end: 5 }));
  await f.model.input(f.ticket(1), 'late'); assert.equal(f.raw.text, 'abc\ndef'); assert.equal(f.raw.composing_end, 5);
  assert.equal(f.model.status().raw_capture_complete, true); assert.equal(f.formats.length, 0);
});

test('other row controller callbacks cannot clear the captured active preview composition', async () => {
  const f = fixture('a\nb'); await f.model.input(f.ticket(), 'a', '候', 1); const raw = plain(f.raw);
  assert.equal(f.model.canEdit(f.ticket(1).id), false);
  await f.model.input(f.ticket(1), 'other'); await f.model.selection(f.ticket(1), 0, 0);
  assert.deepEqual(plain(f.raw), raw); assert.equal(f.model.status().raw_capture_complete, true);
});

test('remaining-count failure preserves complete future raw and blocks only formatter business qualification', async () => {
  const f = fixture('old\nother', { countError: true }); await f.model.input(f.ticket(), 'whole future raw');
  const status = f.model.status(); assert.equal(status.value.text, 'whole future raw\nother');
  assert.equal(status.raw_capture_complete, true); assert.equal(status.business_ready, false); assert.equal(status.format_pending, false);
  assert.equal(status.capture_error, ''); assert.match(status.format_error, /CountUnknown/); assert.equal(f.formats.length, 0);
});

test('explicit formatter retry reuses the complete original wire and fixed remaining without recapturing raw', async () => {
  const worker = createInputWorker({ fail: (_, number) => number === 1 });
  const f = fixture('old\n😀', { format: worker.format });
  f.bind(text(f.raw.text, { selection_base: 2, selection_extent: 0, affinity: 0, directional: true }));
  await f.model.input(f.ticket(), 'new\rtext'); const raw = plain(f.raw), revision = f.revision, captures = f.captures.length;
  assert.equal(f.model.canRetry(f.ticket()), true); assert.equal(f.model.view().retry_available, true);
  assert.equal(f.model.view().retry_row_id, f.ticket().id); assert.equal(f.model.canChangeRows(), false);
  f.echo(); assert.equal(f.model.canRetry(f.ticket()), true); assert.equal(worker.requests.length, 1);
  await f.model.retryFormat(f.ticket());
  assert.equal(worker.requests.length, 2); assert.equal(worker.requests[1], worker.requests[0]);
  assert.deepEqual(worker.hashes, worker.requests); assert.equal(JSON.parse(worker.requests[1]).limit, 998);
  assert.deepEqual(JSON.parse(worker.requests[1]).old_value, plain(text('old', { selection_base: 2, selection_extent: 0, affinity: 0, directional: true })));
  assert.equal(raw.text, 'new\rtext\n😀'); assert.equal(f.raw.text, 'new text\n😀');
  assert.equal(f.revision, revision + 1); assert.equal(f.captures.length, captures + 1);
  assert.equal(f.model.status().business_ready, true); assert.equal(f.model.canRetry(f.ticket()), false);
});

test('remaining-count Unknown retries only pure counts for the same complete raw before its first formatter request', async () => {
  const options = { countError: true }, worker = createInputWorker(); options.format = worker.format;
  const f = fixture('old\n😀\n', options); await f.model.input(f.ticket(), 'full candidate');
  const raw = plain(f.raw), revision = f.revision, captures = f.captures.length;
  assert.equal(f.model.canRetry(f.ticket()), true); assert.equal(worker.requests.length, 0);
  options.countError = false; await f.model.retryFormat(f.ticket());
  assert.equal(worker.requests.length, 1); assert.equal(JSON.parse(worker.requests[0]).limit, 997);
  assert.deepEqual(JSON.parse(worker.requests[0]).new_value, plain(text('full candidate')));
  assert.deepEqual(plain(f.raw), { ...raw, affinity: 1 }); assert.equal(f.revision, revision + 1); assert.equal(f.captures.length, captures + 1);
  assert.equal(f.model.status().business_ready, true);
});

test('a malformed receipt stays unqualified and another explicit attempt still uses the same failed wire', async () => {
  const worker = createInputWorker({ mutate: (reply, _, number) => { if (number < 3) reply.request_sha256 = '0'.repeat(64); } });
  const f = fixture('old', { format: worker.format }); await f.model.input(f.ticket(), 'full future');
  await f.model.retryFormat(f.ticket()); assert.equal(f.model.canRetry(f.ticket()), true);
  assert.equal(f.raw.text, 'full future'); assert.equal(f.model.status().business_ready, false);
  await f.model.retryFormat(f.ticket()); assert.equal(f.model.status().business_ready, true);
  assert.equal(worker.requests.length, 3); assert.ok(worker.requests.every(encoded => encoded === worker.requests[0]));
});

test('duplicate explicit retry cannot dispatch twice while its real pure worker is pending', async () => {
  const wait = deferred(), worker = createInputWorker({ fail: (_, number) => number === 1, wait: (_, number) => number === 2 ? wait.promise : undefined });
  const f = fixture('old', { format: worker.format }); await f.model.input(f.ticket(), 'future');
  const retrying = f.model.retryFormat(f.ticket()); await tick(); const captures = f.captures.length;
  assert.equal(f.model.status().format_pending, true); assert.equal(f.model.canRetry(f.ticket()), false);
  await f.model.retryFormat(f.ticket()); assert.equal(worker.requests.length, 2); assert.equal(f.captures.length, captures);
  wait.resolve(); await retrying; assert.equal(f.model.status().business_ready, true);
});

test('zero remaining retry preserves filtered-old no-growth with exact reverse UTF16 metadata', async () => {
  const worker = createInputWorker({ fail: (_, number) => number === 1, action: () => 'retained', value: request => editorTodoFilteredOld(request.old_value) });
  const f = fixture('x'.repeat(999) + '\na\r😀', { format: worker.format });
  f.bind(text(f.raw.text, { selection_base: 1004, selection_extent: 1000, affinity: 0, directional: true }));
  await f.model.input(f.ticket(1), 'growth\r😀'); assert.equal(f.raw.text.endsWith('growth\r😀'), true);
  await f.model.retryFormat(f.ticket(1)); assert.equal(worker.requests[1], worker.requests[0]);
  const request = JSON.parse(worker.requests[1]); assert.equal(request.limit, 0);
  assert.deepEqual(request.old_value, plain(text('a\r😀', { selection_base: 4, selection_extent: 0, affinity: 0, directional: true })));
  assert.equal(f.raw.text, 'x'.repeat(999) + '\na 😀'); assert.equal(f.raw.selection_base, 1004); assert.equal(f.raw.selection_extent, 1000);
  assert.equal(f.model.status().business_ready, true);
});

test('successful other-row input cannot clear a failed row or borrow its formatting qualification', async () => {
  const worker = createInputWorker({ fail: (_, number) => number === 1 }), f = fixture('old\nother', { format: worker.format });
  await f.model.input(f.ticket(), 'unconfirmed'); assert.equal(f.model.canEdit(f.ticket(1).id), false);
  await f.model.input(f.ticket(1), 'confirmed other');
  assert.equal(f.raw.text, 'unconfirmed\nconfirmed other'); assert.equal(f.model.status().business_ready, false);
  assert.match(f.model.status().format_error, /Unknown/); assert.equal(f.model.canChangeRows(), false);
  assert.equal(f.model.canRetry(f.ticket()), false); assert.equal(f.model.canEdit(f.ticket().id), true);
  await f.model.input(f.ticket(), 'checked first');
  assert.equal(JSON.parse(worker.requests[2]).old_value.text, 'old'); assert.equal(f.model.status().business_ready, true);
});

test('same-row input after Unknown preserves original confirmed old instead of trusting the unchecked current raw', async () => {
  const worker = createInputWorker({ fail: (_, number) => number === 1 }), f = fixture('old', { format: worker.format });
  await f.model.input(f.ticket(), 'full unchecked\rvalue'); await f.model.input(f.ticket(), 'full unchecked\rvalue');
  assert.equal(worker.requests[1], worker.requests[0]); assert.equal(f.raw.text, 'full unchecked value');
  assert.equal(f.model.status().business_ready, true);
});

test('selection-only changes invalidate exact retry but do not grant failed raw business readiness', async () => {
  const worker = createInputWorker({ fail: (_, number) => number === 1 }), f = fixture('old', { format: worker.format });
  await f.model.input(f.ticket(), 'candidate'); await f.model.selection(f.ticket(), 3, 1);
  assert.equal(f.model.canRetry(f.ticket()), false); await f.model.retryFormat(f.ticket());
  assert.equal(worker.requests.length, 1); assert.equal(f.model.status().business_ready, false);
  assert.equal(f.raw.selection_base, 3); assert.equal(f.raw.selection_extent, 1);
  await f.model.input(f.ticket(), 'candidate'); assert.equal(JSON.parse(worker.requests[1]).old_value.text, 'old');
  assert.equal(f.model.status().business_ready, true);
});

test('identical complete selection echo preserves the failed fixed-wire retry without recapturing or confirming', async () => {
  const worker = createInputWorker({ fail: (_, number) => number === 1 }), f = fixture('old', { format: worker.format });
  f.bind(text('old', { selection_base: 2, selection_extent: 1, affinity: 0, directional: true }));
  await f.model.input(f.ticket(), 'candidate'); const raw = plain(f.raw), revision = f.revision, captures = f.captures.length;
  await f.model.selection(f.ticket(), 2, 1); await f.model.selection(f.ticket(), 2, 1);
  assert.deepEqual(plain(f.raw), raw); assert.equal(f.revision, revision); assert.equal(f.captures.length, captures);
  assert.equal(f.model.canRetry(f.ticket()), true); assert.equal(f.model.status().business_ready, false);
  await f.model.retryFormat(f.ticket()); assert.equal(worker.requests[1], worker.requests[0]); assert.equal(f.model.status().business_ready, true);
});

test('revision rebind, stopped owner, foreign ticket and replaced source cannot reissue a failed formatter snapshot', async () => {
  for (const change of ['revision', 'stop', 'foreign', 'source', 'owner']) {
    const worker = createInputWorker({ fail: () => true }), f = fixture('old', { format: worker.format });
    await f.model.input(f.ticket(), 'candidate'); const old = f.ticket();
    if (change === 'revision') f.bind();
    else if (change === 'stop') f.model.stop();
    else if (change === 'foreign') old.owner = 'foreign';
    else if (change === 'source') f.external();
    else f.switchOwner();
    assert.equal(f.model.canRetry(old), false, change); await f.model.retryFormat(old);
    assert.equal(worker.requests.length, 1, change);
  }
});

test('failed-row preview invalidates pure retry and waits for commit without losing its original old baseline', async () => {
  const worker = createInputWorker({ fail: (_, number) => number === 1 }), f = fixture('old\nother', { format: worker.format });
  await f.model.input(f.ticket(), 'candidate'); await f.model.input(f.ticket(), 'candidate', '候', 1);
  assert.equal(f.raw.composing_start, 1); assert.equal(f.model.canRetry(f.ticket()), false);
  await f.model.retryFormat(f.ticket()); assert.equal(worker.requests.length, 1); assert.equal(f.model.status().business_ready, false);
  await f.model.input(f.ticket(), 'c候andidate'); assert.equal(JSON.parse(worker.requests[1]).old_value.text, 'old');
  assert.equal(f.model.status().business_ready, true);
});

test('late explicit-retry receipt cannot affect a changed owner or newer same-row input', async () => {
  for (const change of ['owner', 'new-input']) {
    const wait = deferred(), worker = createInputWorker({ fail: (_, number) => number === 1, wait: (_, number) => number === 2 ? wait.promise : undefined });
    const f = fixture('old', { format: worker.format }); await f.model.input(f.ticket(), 'candidate');
    const retrying = f.model.retryFormat(f.ticket()); await tick();
    if (change === 'owner') f.switchOwner('B', text('new owner'));
    else await f.model.input(f.ticket(), 'newer candidate');
    const raw = plain(f.raw); wait.resolve(); await retrying; assert.deepEqual(plain(f.raw), raw, change);
    assert.equal(f.model.status().business_ready, true, change);
  }
});
