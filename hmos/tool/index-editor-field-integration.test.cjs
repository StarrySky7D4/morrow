'use strict';
// Fresh actual Index Save/Session/Business/Draft/FieldPolicy/Workbench queue.
// Native replies, field worker and framework lease delivery are controlled;
// these assertions are not Store, ArkUI IME, SDK semantic or device acceptance.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const assert = require('node:assert/strict'), { test, after } = require('node:test');
const { editorHarness, plain, settle, deferred, actualMethod, source, sourceSHA, assertSourceUnchanged } = require('./index-business-test-harness.cjs');
const harness = options => editorHarness(options, 'field');
const methods = actualMethod('submit') + actualMethod('flushDraft');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
after(assertSourceUnchanged);
test('fresh actual Index methods use installed SDK compiler; emit source hash', () => {
  assert.ok(methods.includes('JSON.stringify(cmd) === original') && methods.includes('draft.flush()'));
  console.log('Index.ets SHA256=' + crypto.createHash('sha256').update(source).digest('hex') + '; SDK TypeScript=' + ts.version);
});
test('all five fields retain original graphemes and UTF16 range in actual raw journal', async () => {
  const h = harness(); try {
    const text = '  e\u0301👩‍👩‍👧‍👦🇨🇳\r\n  ';
    for (const name of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
      h.page.editorInputChanged(name, text); h.page.draftSelectionChanged(name, text.length, 3);
    }
    assert.equal(await h.page.flushDraft(), true);
    for (const name of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
      const value = h.raw().at(-1).request.values[name]; assert.equal(value.text, text);
      assert.equal(value.selection_base, text.length); assert.equal(value.selection_extent, 3);
    }
    assert.equal(h.raw().at(-1).request.assets[0].asset_id, 'confirmed-pin-original');
  } finally { await h.close(); }
});
test('IME candidate is journaled losslessly in all fields while business admission is blocked', async () => {
  for (const name of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
    const h = harness(); try {
      h.page.editorInputChanged(name, 'A😀B', { value: '候选e\u0301', offset: 3 });
      assert.equal(await h.page.flushDraft(), true); const raw = h.raw().at(-1).request.values[name];
      assert.equal(raw.text, 'A😀候选e\u0301B'); assert.equal(raw.composing_start, 3); assert.equal(raw.composing_end, 7);
      if (name === 'todos') h.page.mutate('task_add'); else await h.page.save();
      await settle(); assert.equal(h.business().length, 0); assert.equal(h.draft.current[name].text, raw.text);
    } finally { await h.close(); }
  }
});
test('invalid IME offset blocks retention and leaves previous confirmed raw values intact', async () => {
  const h = harness(); try {
    h.page.editorInputChanged('description', 'new', { value: '候选', offset: 9 });
    assert.equal(h.page.draftCaptureIncomplete, true); assert.equal(await h.page.flushDraft(), false);
    await h.page.save(); assert.equal(h.business().length, 0);
    assert.equal(h.draft.current.description.text, 'Body'); assert.equal(h.draft.confirmed.assets[0].selection.asset_id, 'confirmed-pin-original');
  } finally { await h.close(); }
});
test('older count cannot replace a newer edit or a newer UTF16 selection count', async () => {
  const gates = [], h = harness({ fieldEffect: () => { const gate = deferred(); gates.push(gate); return gate.promise; } });
  try {
    h.page.editorInputChanged('title', 'old'); h.page.editorInputChanged('title', 'e\u0301😀');
    await settle(); assert.equal(gates.length, 2); gates[1].resolve(); await settle(); assert.equal(h.page.fieldCountLabel('title'), '2 / 60');
    gates[0].resolve(); await settle(); assert.equal(h.page.fieldCountLabel('title'), '2 / 60');
    h.page.draftSelectionChanged('title', 4, 2); await settle(); gates[2].resolve(); await settle();
    assert.equal(h.draft.current.title.selection_base, 4); assert.equal(h.draft.current.title.selection_extent, 2);
  } finally { await h.close(); }
});
test('complete Save validation revokes edits, selection, category, owner, assets and foreground changes', async () => {
  for (const change of ['text', 'selection', 'category', 'owner', 'assets', 'foreground', 'closed', 'disposed']) {
    const gate = deferred(), h = harness({ fieldEffect: ({ request }) => request.field === 'title' ? gate.promise : undefined });
    try {
      const run = h.page.save(); for (let n = 0; n < 8; n++) await settle();
      assert.equal(h.page.fieldValidationWorking, true);
      if (change === 'text') h.page.editorInputChanged('description', 'later');
      if (change === 'selection') h.page.draftSelectionChanged('title', 1, 4);
      if (change === 'category') { const values = h.draft.current; values.category = '灵感'; h.draft.update(values); }
      if (change === 'owner') h.page.attachmentEditorIdentity = 'other-owner';
      if (change === 'assets') { const values = h.draft.current; values.assets = []; h.draft.update(values); }
      if (change === 'foreground') { h.page.foreground = false; h.page.foregroundChanged(); }
      if (change === 'closed') { h.page.editorOpen = false; }
      if (change === 'disposed') h.draft.dispose();
      gate.resolve(); await run; assert.equal(h.business().length, 0, change); assert.equal(h.page.pending, '', change);
      assert.equal(h.page.fieldValidationWorking, false, change);
    } finally { await h.close(); }
  }
});
test('later input during final raw flush prevents issuing a frozen business command', async () => {
  const gate = deferred(), h = harness({ draftEffect: () => gate.promise }); try {
    h.page.editorInputChanged('description', 'first'); const run = h.page.save();
    await settle(); assert.equal(h.raw().length, 1); h.page.editorInputChanged('description', 'later');
    gate.resolve(); await run; assert.equal(h.business().length, 0); assert.equal(h.draft.current.description.text, 'later');
    assert.equal(h.draft.current.assets[0].asset_id, 'confirmed-pin-original');
  } finally { await h.close(); }
});
test('over-limit title keeps every character and confirmed pins without business mutation', async () => {
  const h = harness(); try {
    const text = '👩‍👩‍👧‍👦'.repeat(61); h.page.editorInputChanged('title', text);
    await h.page.save(); assert.equal(h.business().length, 0);
    assert.equal(h.draft.current.title.text, text); assert.equal(h.draft.confirmed.assets.length, 1);
    assert.equal(await h.page.flushDraft(), true); assert.equal(h.raw().at(-1).request.values.title.text, text);
  } finally { await h.close(); }
});
test('count worker failure never authorizes business or normalizes original input', async () => {
  const h = harness({ fieldUnknown: true }); try {
    h.page.editorInputChanged('description', '  e\u0301\n'); await h.page.save();
    assert.equal(h.business().length, 0); assert.equal(h.draft.current.description.text, '  e\u0301\n');
    assert.equal(await h.page.flushDraft(), true); assert.equal(h.raw().at(-1).request.values.description.text, '  e\u0301\n');
  } finally { await h.close(); }
});
test('unknown raw journal preserves exact pending request and confirmed pins, no business issue', async () => {
  const h = harness({ draftUnknown: true }); try {
    h.page.editorInputChanged('description', 'raw unconfirmed'); await h.page.save();
    assert.equal(h.business().length, 0); assert.equal(h.draft.unknown, true);
    assert.equal(JSON.parse(h.draft.pending).draft.values.description.text, 'raw unconfirmed');
    assert.equal(h.draft.confirmed.values.description.text, 'Body'); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { await h.close(); }
});
test('Unknown complete Save retains registered exact wire and immutable publication with confirmed pins', async () => {
  const h = harness({ businessUnknown: true }); try {
    h.page.editorInputChanged('title', ' e\u0301 '); await h.page.save();
    assert.equal(h.business().length, 1); const session = h.page.businessSession;
    assert.equal(session.originalSave, h.business()[0].serialized); assert.equal(session.businessUnknown, true);
    assert.equal(JSON.parse(session.originalRequest).business.title, ' e\u0301 ');
    assert.equal(session.publication.values.assets[0].asset_id, 'confirmed-pin-original');
    assert.equal(h.draft.confirmed.values.title.text, ' e\u0301 '); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { await h.close(); }
});
test('create/edit business send preserves exact source strings and binds confirmed generation', async () => {
  for (const create of [false, true]) { const h = harness({ create, todos: create ? 'pending todo remains' : '' }); try {
    h.page.editorInputChanged('title', ' e\u0301😀 '); h.page.editorInputChanged('description', '👩‍👩‍👧‍👦\r\n');
    await h.page.save(); assert.equal(h.business().length, 1);
    const sent = h.business()[0], cmd = sent.command; assert.equal(cmd.title, ' e\u0301😀 '); assert.equal(cmd.description, '👩‍👩‍👧‍👦\r\n');
    assert.equal(sent.publication.draft_id, h.draft.scope.draft_id); assert.equal(sent.publication.generation, h.draft.confirmed.generation);
    assert.equal(sent.publication.save_operation, h.draft.confirmed.operation_id); assert.equal(h.draft.current.todos.text, create ? 'pending todo remains' : '');
    assert.equal(cmd.action, create ? 'create' : 'edit'); assert.equal(cmd.source, create ? '' : '0a02aabb');
    assert.equal(h.draft.confirmed.assets.length, 1); assert.equal(h.events.some(e => e.kind === 'retire-attempt'), false);
  } finally { await h.close(); } }
});
test('task_add validates full raw text and does not consume later text or IME', async () => {
  const gate = deferred(), h = harness({ todos: 'Task😀', businessEffect: () => gate.promise }); try {
    h.page.mutate('task_add'); await settle(); assert.equal(h.business().length, 1);
    h.page.editorInputChanged('todos', 'Task😀', { value: '候选', offset: 6 }); gate.resolve(); await settle();
    assert.equal(h.business()[0].command.text, 'Task😀'); assert.equal(h.draft.current.todos.text, 'Task😀候选');
    assert.equal(h.draft.current.todos.composing_start, 6); assert.equal(h.page.taskText, 'Task😀');
  } finally { await h.close(); }
});
test('task rename validates its untrimmed original while command retains established trimming', async () => {
  const h = harness({ todos: 'independent pending todo' }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged(' e\u0301😀 '); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); assert.equal(h.business()[0].command.text, 'e\u0301😀');
    assert.ok(h.events.some(e => e.kind === 'field-check' && e.request.field === 'todos' && e.request.text === ' e\u0301😀 '));
    assert.equal(h.draft.current.todos.text, 'independent pending todo');
    assert.equal(h.page.taskEditId, ''); assert.equal(h.page.taskRenameText, ''); assert.equal(h.page.submittedRename, undefined);
  } finally { await h.close(); }
});
test('same-value rename roundtrip after issue does not consume newer input epoch', async () => {
  const gate = deferred(), h = harness({ todos: 'independent todo', businessEffect: () => gate.promise }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Task'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1);
    h.page.taskRenameChanged('Later'); h.page.taskRenameChanged('Task'); gate.resolve(); await settle();
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, 'Task');
    assert.equal(h.page.taskRenameText, 'Task'); assert.equal(h.page.pending, '');
  } finally { await h.close(); }
});
test('actual cancel callback invalidates pending validation and later reopen is not consumed', async () => {
  const gate = deferred(), h = harness({ todos: 'independent todo', fieldEffect: ({ request }) => request.field === 'todos' ? gate.promise : undefined }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('New name'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.page.fieldValidationWorking, true);
    h.page.cancelRenameFromActualBuilder(); h.page.renameTask(h.page.current().tasks[0]);
    assert.equal(h.page.taskEditId, '', 'busy validation rejects reopen until the prior check is revoked');
    gate.resolve(); await settle(); assert.equal(h.business().length, 0);
    h.page.renameTask(h.page.current().tasks[0]); await settle();
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameText, 'Task');
  } finally { await h.close(); }
});
test('unknown rename preserves original request and frozen raw identity through explicit reconciliation', async () => {
  const options = { todos: 'independent todo', businessUnknown: true }, h = harness(options); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged(' Task '); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); const original = h.page.pending, snapshot = plain(h.page.submittedRename);
    const epoch = h.page.submittedRenameEpoch; assert.equal(snapshot.text, ' Task '); assert.equal(JSON.parse(original).text, 'Task');
    h.page.taskRenameChanged(' Task ', { value: '候选', offset: 6 });
    assert.equal(h.page.pending, original); assert.deepEqual(plain(h.page.submittedRename), snapshot); assert.equal(h.page.submittedRenameEpoch, epoch);
    options.businessUnknown = false; await h.page.retry();
    assert.equal(h.business().length, 2); assert.equal(h.business()[1].serialized, original);
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, ' Task 候选');
    assert.equal(h.draft.current.assets[0].asset_id, 'confirmed-pin-original');
  } finally { await h.close(); }
});
test('confirmed not-committed rename clears only frozen request, preserving user text', async () => {
  const h = harness({ todos: 'independent todo', businessResult: { ok: false, effect: 'not_committed', error: 'synthetic rejection' } }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Original rename'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); assert.equal(h.page.pending, ''); assert.equal(h.page.submittedRename, undefined);
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, 'Original rename');
    assert.equal(h.page.taskRenameText, 'Original rename'); assert.equal(h.draft.current.assets[0].asset_id, 'confirmed-pin-original');
  } finally { await h.close(); }
});
test('foreground loss after issued rename revokes successful input consumption', async () => {
  const gate = deferred(), h = harness({ todos: 'independent todo', businessEffect: () => gate.promise }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Task'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); h.page.foreground = false; h.page.foregroundChanged();
    gate.resolve(); await settle(); assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameText, 'Task');
  } finally { await h.close(); }
});
test('late IME rename callback during issued business request retains newer candidate', async () => {
  const gate = deferred(), h = harness({ todos: 'independent pending todo', businessEffect: () => gate.promise }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Task'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); assert.equal(h.page.busy, true);
    h.page.taskRenameChanged('Task', { value: '候选', offset: 4 }); gate.resolve(); await settle();
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, 'Task候选');
    assert.equal(h.page.taskRenameValue.composing_start, 4); assert.equal(h.page.taskRenameText, 'Task');
  } finally { await h.close(); }
});

test('actual ordinary-field construction selections do not change raw or epoch before focus, including during restore', async () => {
  for (const restoring of [false, true]) {
    const h = harness(); try {
      h.page.draftFocusedFields.clear(); h.page.draftRestoreInput = restoring;
      const original = plain(h.draft.current), epoch = h.page.editorInputEpoch, owner = h.page.editorViewOwner;
      for (const name of ['title', 'description', 'hypothesis', 'conclusion']) {
        h.page.leaseSelectionChanged(owner, name, h.page.draftField(name).text.length + 1, -2);
        h.page.leaseSelectionChanged(owner, name, 0.5, 0);
        h.page.leaseSelectionChanged(owner, name, 0, 0);
      }
      assert.equal(h.page.editorInputEpoch, epoch); assert.equal(h.page.retirementInput.size, 0);
      assert.equal(h.page.draftCaptureIncomplete, false); assert.equal(h.page.draftCaptureBlocked.size, 0);
      assert.deepEqual(plain(h.draft.current), original); assert.deepEqual(plain(h.page.editorValues), original);
      assert.equal(h.page.message, ''); assert.equal(h.raw().length, 0); assert.equal(h.business().length, 0);
      h.page.draftRestoreInput = false; assert.equal(await h.page.flushDraft(), true);
      assert.equal(h.raw().length, 0, 'unchanged restore must not consume another raw journal generation');
    } finally { await h.close(); }
  }
});

test('actual unfocused initialization cannot clear or overwrite an existing unknown event', async () => {
  const h = harness(); try {
    const owner = h.page.editorViewOwner, unknown = JSON.stringify({ start: 77, end: -4 });
    h.page.leaseUncaptured(owner, 'title', unknown); h.page.draftFocusedFields.clear(); h.page.draftRestoreInput = true;
    const epoch = h.page.editorInputEpoch, original = plain(h.draft.current);
    h.page.leaseSelectionChanged(owner, 'title', 0, 0); h.page.leaseSelectionChanged(owner, 'title', 999, -2);
    assert.equal(h.page.retirementInput.get('title:text'), unknown); assert.equal(h.page.editorInputEpoch, epoch);
    assert.equal(h.page.draftCaptureIncomplete, true); assert.equal(h.page.draftCaptureBlocked.has('title'), true);
    assert.deepEqual(plain(h.draft.current), original); assert.equal(await h.page.flushDraft(), false);
    assert.equal(h.raw().length, 0); assert.equal(h.business().length, 0);
  } finally { await h.close(); }
});

test('actual focused invalid ranges remain exact uncaptured events in active and disabled ordinary fields', async () => {
  for (const disabledFlag of ['', 'draftRestoreInput', 'attachmentWorking', 'draftRetiring', 'draftRetirementUnknown']) {
    for (const name of ['title', 'description', 'hypothesis', 'conclusion']) {
      const h = harness(); try {
        const owner = h.page.editorViewOwner; h.page.draftFocusedFields.clear(); h.page.leaseFocusChanged(owner, name, true);
        assert.deepEqual([...h.page.draftFocusedFields], [name]); if (disabledFlag) h.page[disabledFlag] = true;
        const original = plain(h.draft.current), live = plain(h.page.editorValues), epoch = h.page.editorInputEpoch;
        const start = h.page.draftField(name).text.length + 3, end = -2;
        h.page.leaseSelectionChanged(owner, name, start, end);
        assert.deepEqual(JSON.parse(h.page.retirementInput.get(name + ':text')), { start, end });
        assert.equal(h.page.editorInputEpoch, epoch + 1); assert.equal(h.page.draftCaptureBlocked.has(name), true);
        assert.equal(h.page.draftCaptureIncomplete, true); assert.deepEqual(plain(h.draft.current), original);
        assert.deepEqual(plain(h.page.editorValues), live); assert.equal(await h.page.flushDraft(), false);
        assert.equal(h.raw().length, 0); assert.equal(h.business().length, 0);
      } finally { await h.close(); }
    }
  }
});

test('actual focused valid reverse selection preserves complete composition, UTF16 metadata and confirmed pins', async () => {
  const h = harness(); try {
    const owner = h.page.editorViewOwner; h.page.draftFocusedFields.clear(); h.page.leaseFocusChanged(owner, 'description', true);
    h.page.editorValues.description.affinity = 1; h.page.editorValues.description.directional = true;
    h.page.leaseTextChanged(owner, 'description', 'A😀B', { value: 'e\u0301', offset: 3 });
    const original = plain(h.draft.current), epoch = h.page.editorInputEpoch;
    h.page.leaseSelectionChanged(owner, 'description', 5, 3);
    assert.equal(h.page.editorInputEpoch, epoch + 1); assert.equal(h.page.retirementInput.size, 0);
    assert.equal(await h.page.flushDraft(), true); const written = h.raw().at(-1).request.values;
    assert.deepEqual(written.description, { ...original.description, selection_base: 5, selection_extent: 3 });
    assert.equal(written.description.text, 'A😀e\u0301B'); assert.equal(written.description.composing_start, 3);
    assert.equal(written.description.composing_end, 5); assert.equal(written.description.affinity, 1);
    assert.equal(written.description.directional, true);
    for (const name of ['title', 'hypothesis', 'conclusion', 'todos']) assert.deepEqual(written[name], original[name]);
    assert.equal(h.raw().at(-1).request.assets[0].asset_id, 'confirmed-pin-original'); assert.equal(h.business().length, 0);
  } finally { await h.close(); }
});

test('actual blur excludes subsequent unfocused ranges without erasing the invalid selection already delivered', async () => {
  const h = harness(); try {
    const owner = h.page.editorViewOwner; h.page.draftFocusedFields.clear(); h.page.leaseFocusChanged(owner, 'title', true);
    h.page.leaseSelectionChanged(owner, 'title', 88, -2);
    const originalEvent = h.page.retirementInput.get('title:text'), epoch = h.page.editorInputEpoch;
    h.page.leaseFocusChanged(owner, 'title', false); assert.equal(h.page.draftFocusedFields.has('title'), false);
    h.page.leaseSelectionChanged(owner, 'title', 999, -3); h.page.leaseSelectionChanged(owner, 'title', 0, 0);
    assert.equal(h.page.retirementInput.get('title:text'), originalEvent); assert.equal(h.page.editorInputEpoch, epoch);
    assert.equal(h.page.draftCaptureIncomplete, true); assert.equal(await h.page.flushDraft(), false);
    assert.equal(h.raw().length, 0); assert.equal(h.draft.current.title.text, 'Title');
  } finally { await h.close(); }
});

test('actual old or revoked leases cannot submit valid, invalid, focus or text events to a replacement editor', async () => {
  for (const revoke of [false, true]) {
    const h = harness(); try {
      const oldOwner = h.page.editorViewOwner, original = plain(h.draft.current), epoch = h.page.editorInputEpoch;
      if (revoke) h.page.editorViewRevoked = true;
      else { h.page.editorViewOwner = 'replacement-owner'; h.page.attachmentEditorIdentity = 'replacement-owner'; }
      h.page.leaseSelectionChanged(oldOwner, 'title', 3, 1); h.page.leaseSelectionChanged(oldOwner, 'title', 999, -2);
      h.page.leaseTextChanged(oldOwner, 'description', 'old text', { value: '候', offset: 99 });
      h.page.leaseFocusChanged(oldOwner, 'description', true);
      assert.equal(h.page.editorInputEpoch, epoch); assert.equal(h.page.retirementInput.size, 0);
      assert.equal(h.page.draftCaptureIncomplete, false); assert.deepEqual(plain(h.draft.current), original);
      assert.deepEqual(plain(h.page.editorValues), original); assert.equal(h.raw().length, 0); assert.equal(h.business().length, 0);
    } finally { await h.close(); }
  }
});

test('actual disabled text and preview still retain exact events regardless of focused selection admission', async () => {
  for (const offset of [3, 99]) {
    const h = harness(); try {
      const owner = h.page.editorViewOwner; h.page.draftFocusedFields.clear(); h.page.draftRetiring = true;
      const original = plain(h.draft.current), epoch = h.page.editorInputEpoch, preview = { value: '候e\u0301', offset };
      h.page.leaseTextChanged(owner, 'description', 'A😀B', preview);
      assert.deepEqual(JSON.parse(h.page.retirementInput.get('description:text')), { value: 'A😀B', preview });
      assert.equal(h.page.editorInputEpoch, epoch + 1); assert.equal(h.page.draftCaptureIncomplete, true);
      assert.deepEqual(plain(h.draft.current), original); assert.equal(h.draft.confirmed.assets[0].selection.asset_id, 'confirmed-pin-original');
      if (offset === 3) {
        assert.equal(h.page.editorValues.description.text, 'A😀候e\u0301B');
        assert.equal(h.page.editorValues.description.composing_start, 3); assert.equal(h.page.editorValues.description.composing_end, 6);
      } else assert.deepEqual(plain(h.page.editorValues.description), original.description);
      assert.equal(await h.page.flushDraft(), false); assert.equal(h.raw().length, 0); assert.equal(h.business().length, 0);
    } finally { await h.close(); }
  }
});

test('actual focused valid retirement selection retains original event and local range without writing a retired raw scope', async () => {
  const h = harness(); try {
    const owner = h.page.editorViewOwner; h.page.draftFocusedFields.clear(); h.page.leaseFocusChanged(owner, 'title', true);
    h.page.draftRetiring = true; const original = plain(h.draft.current), epoch = h.page.editorInputEpoch;
    h.page.leaseSelectionChanged(owner, 'title', 4, 1);
    assert.deepEqual(JSON.parse(h.page.retirementInput.get('title:selection')), { value: 'Title', start: 4, end: 1 });
    assert.equal(h.page.editorInputEpoch, epoch + 1); assert.equal(h.page.editorValues.title.selection_base, 4);
    assert.equal(h.page.editorValues.title.selection_extent, 1); assert.deepEqual(plain(h.draft.current), original);
    assert.equal(h.page.draftCaptureIncomplete, true); assert.equal(await h.page.flushDraft(), false); assert.equal(h.raw().length, 0);
  } finally { await h.close(); }
});
