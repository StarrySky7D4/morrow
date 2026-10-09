'use strict';
// Current complete Save with actual Index/Session/Business/Draft/Handoff and
// the one real Workbench tail. Native DTOs, fields and lease callbacks remain
// controlled; this is not Store, ArkUI rendering or device acceptance.
const crypto = require('node:crypto'), assert = require('node:assert/strict'), { test, after } = require('node:test');
const { editorHarness, plain, settle, deferred, source, assertSourceUnchanged } = require('./index-business-test-harness.cjs');
const harness = options => editorHarness(options, 'todos');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
after(assertSourceUnchanged);
test('actual Index create sends one immutable operation with complete raw todos and confirmed publication', async () => {
  const gate = deferred(), raw = '  e\u0301😀  \n\nsecond\r\n  second  \n', h = harness({ todos: raw, businessEffect: () => gate.promise });
  try {
    h.edit('  Exact title  ', 'title'); await h.save(); assert.equal(h.business().length, 1);
    const sent = h.business()[0], record = h.draft.confirmed;
    assert.equal(sent.command.action, 'create'); assert.equal(sent.command.todos, raw); assert.equal(sent.command.title, '  Exact title  ');
    assert.equal(sent.command.id, h.draft.scope.card_id); assert.equal(sent.publication.draft_id, record.scope.draft_id);
    assert.equal(sent.publication.generation, record.generation); assert.equal(sent.publication.save_operation, record.operation_id);
    assert.equal(h.raw().at(-1).request.values.todos.text, raw); assert.equal(h.raw().at(-1).request.assets[0].asset_id, 'confirmed-create-pin');
    assert.equal(h.page.businessSession.originalSave, sent.serialized); assert.equal(h.page.businessSession.publication.values.todos.text, raw);
    gate.resolve(); await h.finish(); assert.deepEqual(h.business().map(e => e.command.action), ['create']);
  } finally { gate.resolve(); await h.close(); }
});
test('new-card todos composition is preserved in confirmed raw but never admitted as create', async () => {
  const h = harness(); try {
    h.edit({ text: 'first\n候选😀', selection_base: 10, selection_extent: 6, affinity: 1, directional: true, composing_start: 6, composing_end: 10 });
    await h.save(); assert.equal(h.business().length, 0); assert.equal(h.retirements().length, 0); assert.equal(h.page.editorOpen, true);
    assert.deepEqual(plain(h.draft.confirmed.values.todos), plain(h.draft.current.todos));
    assert.equal(h.raw().at(-1).request.values.todos.composing_start, 6); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { await h.close(); }
});
test('complete grapheme overflow or 101 original blank rows stay raw without a partial create', async () => {
  for (const raw of ['😀'.repeat(1001), '\n'.repeat(100)]) {
    const h = harness({ todos: 'before' }); try {
      h.edit(raw); await h.save(); assert.equal(h.business().length, 0); assert.equal(h.retirements().length, 0);
      assert.equal(h.draft.current.todos.text, raw); assert.equal(h.draft.confirmed.values.todos.text, raw);
      assert.equal(h.raw().at(-1).request.values.todos.text, raw); assert.equal(h.page.pending, '');
    } finally { await h.close(); }
  }
});
test('complete Save admits exactly 100 original LF rows with full publication and no legacy create transport', async () => {
  const raw = Array.from({ length: 100 }, (_, n) => 'row-' + n).join('\n'), h = harness({ todos: 'before' }); try {
    h.edit(raw); await h.save(); await h.finish(); assert.equal(h.business().length, 1);
    const session = h.page.businessSession; assert.equal(session.qualified, true);
    assert.equal(JSON.parse(session.originalRequest).business.todos, raw); assert.equal(session.publication.values.todos.text, raw);
    assert.equal(h.calls.filter(w => JSON.parse(w).action === 'editor_save').length, 1);
    assert.equal(h.calls.filter(w => ['create', 'edit'].includes(JSON.parse(w).action)).length, 0);
    assert.equal(h.page.editorOpen, false);
  } finally { await h.close(); }
});
test('continued_todos uses own root context and admits 100 rows but preserves 101 blank rows before issuing another intent', async () => {
  for (const rows of [100, 101]) {
    let h, first = true; h = harness({ todos: 'root', businessEffect: () => {
      if (first) { first = false; h.edit('first successor'); } } });
    try {
      await h.save(); await h.finish(); const root = h.page.ownedTodosBaseline, child = h.page.editorDraft;
      assert.ok(root?.qualified); assert.notEqual(child, h.draft); const originalRoot = root.originalRequest;
      const raw = rows === 100 ? Array.from({ length: 100 }, (_, n) => 'next-' + n).join('\n') : '\n'.repeat(100);
      h.edit(raw); h.page.todoBusinessReady = true; const before = h.calls.length; await h.save(); await h.finish();
      assert.equal(child.confirmed.values.todos.text, raw); assert.equal(child.confirmed.assets.length, 1);
      const newActions = h.calls.slice(before).map(w => JSON.parse(w).action);
      if (rows === 101) {
        assert.equal(newActions.includes('editor_intent_prepare'), false); assert.equal(newActions.includes('editor_save'), false);
        assert.equal(h.page.ownedTodosBaseline, root); assert.equal(child.disposed, false); assert.equal(h.page.editorOpen, true);
      } else {
        assert.equal(newActions.includes('editor_save'), true); const submission = JSON.parse(h.page.businessSession.originalRequest);
        assert.equal(submission.mode, 'continued_todos'); assert.equal(submission.continuation.root_request_json, originalRoot);
        assert.equal(submission.continuation.baseline.revision, child.scope.source_revision); assert.equal(submission.business.todos, raw);
      }
    } finally { await h.close(); }
  }
});
test('unready direct-input or row formatting blocks business while exact raw remains retainable', async () => {
  for (const dependency of ['direct', 'rows']) {
    const options = { directReady: dependency !== 'direct', todosReady: dependency !== 'rows' }, h = harness(options);
    try {
      h.edit('pending\nfull raw'); await h.save(); assert.equal(h.business().length, 0, dependency);
      assert.equal(h.page.pending, '', dependency); assert.equal(h.retirements().length, 0, dependency);
      assert.equal(await h.page.flushDraft(), true, 'raw retention is a separate explicit action despite unready business input');
      assert.equal(h.draft.confirmed.values.todos.text, 'pending\nfull raw', dependency);
      assert.equal(h.draft.confirmed.assets.length, 1, dependency); assert.equal(h.raw().length, 1, dependency);
      options.directReady = true; h.page.todoBusinessReady = true; await h.save(); await h.finish();
      assert.equal(h.business().length, 1, dependency); assert.equal(h.business()[0].command.todos, 'pending\nfull raw', dependency);
      assert.equal(h.raw().length, 1, dependency);
    } finally { await h.close(); }
  }
});
test('issued backend rejection preserves full todos/pins and requires explicit original-request retry', async () => {
  const raw = '  first  \n\nsecond  ', h = harness({ todos: raw,
    businessResult: () => ({ ok: false, error: 'TaskTextByteLimit', effect: 'not_committed', cards: [], drafts: [] }) });
  try {
    await h.save(); assert.equal(h.business().length, 1); assert.equal(h.page.pending, ''); assert.equal(h.page.submittedRaw, undefined);
    assert.equal(h.page.editorOpen, true); assert.equal(h.draft.current.todos.text, raw); assert.equal(h.draft.confirmed.assets.length, 1);
    assert.equal(h.retirements().length, 0); await settle(); assert.equal(h.business().length, 1);
    const original = h.page.businessSession.originalSave; h.edit('corrected'); await h.save(); assert.equal(h.business().length, 1);
    await h.page.reconcileBusiness('save'); assert.equal(h.business().length, 2);
    assert.equal(h.business()[1].serialized, original); assert.equal(h.business()[1].command.todos, raw);
    assert.equal(h.draft.current.todos.text, 'corrected'); assert.equal(h.page.businessPending(), true);
  } finally { await h.close(); }
});
test('Unknown create retains exact wire and explicit retry does not borrow later todos', async () => {
  let h; h = harness({ todos: '  original\nraw  ', businessResult: ({ command, count }) => {
    if (count === 1) throw Error('synthetic lost reply'); return h.success(command);
  } });
  try {
    await h.save(); const original = h.business()[0].serialized;
    const session = h.page.businessSession; assert.equal(session.originalSave, original); assert.equal(session.businessUnknown, true);
    assert.equal(session.publication.values.todos.text, '  original\nraw  ');
    h.edit('later\nnew row'); await settle(); assert.equal(h.business().length, 1);
    await h.page.reconcileBusiness('save'); await h.finish(); assert.equal(h.business().length, 2);
    assert.equal(h.business()[1].serialized, original); assert.equal(session.businessUnknown, false);
    assert.equal(h.page.editorDraft.current.todos.text, 'later\nnew row'); assert.equal(h.page.editorOpen, true);
    assert.notEqual(h.page.editorDraft, h.draft); assert.equal(h.retirements().length, 1); assert.equal(h.draft.disposed, true);
  } finally { await h.close(); }
});
test('raw journal Unknown reconciles only its original todos; later rows need a fresh confirmed publication', async () => {
  const options = { rawUnknown: true }, h = harness(options); try {
    h.edit('first\nuncertain'); await h.save(); assert.equal(h.business().length, 0); assert.equal(h.draft.unknown, true);
    const original = h.raw()[0].serialized; assert.equal(h.draft.pending, original);
    h.edit('later\nraw'); options.rawUnknown = false; await h.draft.retry();
    assert.equal(h.raw().length, 2); assert.equal(h.raw()[1].serialized, original); assert.equal(h.draft.unknown, false);
    assert.equal(h.draft.confirmed.values.todos.text, 'first\nuncertain'); assert.equal(h.draft.current.todos.text, 'later\nraw');
    assert.equal(h.draft.dirty, true); assert.equal(h.business().length, 0); assert.equal(h.draft.confirmed.assets.length, 1);
    await h.save(); await h.finish(); assert.equal(h.raw().length, 3); assert.equal(h.raw()[2].request.values.todos.text, 'later\nraw');
    assert.equal(h.business().length, 1); assert.equal(h.business()[0].command.todos, 'later\nraw');
    assert.equal(h.business()[0].publication.generation, h.draft.confirmed.generation);
    assert.equal(h.business()[0].publication.save_operation, h.raw()[2].request.operation_id);
  } finally { await h.close(); }
});
test('structured unconfirmed or historical committed error preserves the original create wire', async () => {
  for (const effect of ['unknown', 'committed']) {
    const h = harness({ todos: 'first\nsecond', businessResult: () => ({ ok: false, error: 'OriginalOutcomeNeedsReconciliation', effect, cards: [], drafts: [] }) });
    try {
      await h.save(); const original = h.business()[0].serialized, session = h.page.businessSession;
      assert.equal(session.originalSave, original, effect); assert.equal(session.publication.values.todos.text, 'first\nsecond', effect);
      assert.equal(h.retirements().length, 0, effect); await settle(); assert.equal(h.business().length, 1, effect);
      await h.page.reconcileBusiness('save');
      if (effect === 'committed') { assert.equal(h.business().length, 1); assert.equal(session.committed, true); }
      else { assert.equal(h.business().length, 2); assert.equal(h.business()[1].serialized, original); assert.equal(session.businessUnknown, true); }
      assert.equal(h.page.businessSession, session, effect); assert.equal(session.originalSave, original, effect);
    } finally { await h.close(); }
  }
});
test('late todos text/range/composition/edit-away-back revokes create input consumption', async () => {
  for (const mutation of ['text', 'selection', 'composition', 'away-back']) {
    const gate = deferred(), h = harness({ todos: 'original\nraw', businessEffect: () => gate.promise });
    try {
      await h.save(); assert.equal(h.business().length, 1, mutation);
      if (mutation === 'text') h.edit('later\nraw');
      if (mutation === 'selection') h.edit({ ...plain(h.draft.current.todos), selection_base: 8, selection_extent: 2, affinity: 1, directional: true });
      if (mutation === 'composition') h.edit({ ...plain(h.draft.current.todos), composing_start: 0, composing_end: 8 });
      if (mutation === 'away-back') { h.edit('away'); h.edit('original\nraw'); }
      const later = plain(h.draft.current.todos); gate.resolve(); await h.finish();
      assert.deepEqual(plain(h.page.editorDraft.current.todos), later, mutation); assert.equal(h.retirements().length, 1, mutation);
      assert.equal(h.page.editorOpen, true, mutation); assert.equal(h.draft.disposed, true, mutation);
      assert.notEqual(h.page.editorDraft, h.draft, mutation); assert.equal(h.page.editorDraft.dirty, false, mutation);
      assert.equal(h.business()[0].command.todos, 'original\nraw', mutation);
    } finally { gate.resolve(); await h.close(); }
  }
});
test('a different editor owner with identical raw values cannot be consumed by an old create receipt', async () => {
  const gate = deferred(), h = harness({ todos: 'same\nraw', businessEffect: () => gate.promise });
  try {
    await h.save(); const replacement = h.replaceEditor();
    gate.resolve(); await h.finish(); assert.equal(h.retirements().length, 0);
    assert.equal(h.page.editorOpen, true); assert.equal(h.page.editorDraft, replacement); assert.equal(replacement.disposed, false);
    assert.equal(replacement.current.todos.text, 'same\nraw'); assert.equal(h.page.selected, replacement.scope.card_id);
  } finally { gate.resolve(); await h.close(); }
});
test('successful exact create retires its full raw generation without emptying/normalizing it first', async () => {
  const raw = '  first\n\nfirst  \n second  ', h = harness({ todos: 'before' }); try {
    h.edit(raw); await h.save(); assert.equal(h.business().length, 1); assert.equal(h.retirements().length, 1);
    const sent = h.business()[0], close = h.retirements()[0].command;
    assert.equal(close.disposition, 'saved_exact'); assert.equal(close.parent.proof.draft_id, sent.publication.draft_id);
    assert.equal(close.parent.proof.generation, sent.publication.generation); assert.equal(close.parent.proof.save_operation, sent.publication.save_operation);
    assert.deepEqual(h.raw().map(e => e.request.values.todos.text), [raw]);
    assert.equal(h.draft.confirmed.values.todos.text, raw); assert.equal(h.draft.disposed, true);
    assert.equal(h.page.editorDraft, undefined); assert.equal(h.page.editorOpen, false); assert.equal(h.page.pending, '');
  } finally { await h.close(); }
});
test('late input/range/roundtrip or owner change during exact close preserves live input; revoked SDK lease cannot submit', async () => {
  for (const mutation of ['input', 'selection', 'away-back', 'callback', 'owner']) {
    const gate = deferred(), h = harness({ todos: 'published\nraw', retireEffect: () => gate.promise });
    try {
      await h.save(); assert.equal(h.retirements().length, 1, mutation);
      const replacement = mutation === 'owner' ? h.replaceEditor('replacement\nretained') : undefined;
      if (mutation === 'input') h.edit('late\nretained');
      if (mutation === 'selection') h.edit({ ...plain(h.draft.current.todos), selection_base: 9, selection_extent: 2, affinity: 1, directional: true });
      if (mutation === 'away-back') { h.edit('away'); h.edit('published\nraw'); }
      if (mutation === 'callback') {
        const epoch = h.page.editorInputEpoch, before = plain(h.draft.current), preview = { value: '候选😀', offset: 2 };
        const owner = h.page.editorViewOwner; assert.equal(h.page.editorViewRevoked, true);
        h.page.leaseTextChanged(owner, 'title', '晚到SDK原文', preview);
        assert.equal(h.page.editorInputEpoch, epoch); assert.equal(h.page.draftCaptureIncomplete, false);
        assert.equal(h.page.retirementInput.size, 0); assert.deepEqual(plain(h.draft.current), before, 'revoked callback cannot update journal');
      }
      const expected = plain(h.page.editorDraft.current.todos);
      gate.resolve(); await h.finish();
      if (mutation === 'callback') { assert.equal(h.page.editorOpen, false); assert.equal(h.draft.disposed, true); continue; }
      assert.equal(h.page.editorOpen, true, mutation);
      assert.ok(h.page.editorDraft && !h.page.editorDraft.disposed, mutation);
      if (replacement) assert.equal(h.page.editorDraft, replacement, mutation);
      assert.equal(h.retirements().length, 1, mutation); assert.deepEqual(plain(h.page.editorDraft.current.todos), expected, mutation);
      if (mutation !== 'owner') {
        assert.equal(h.page.draftRetiredIdentity, h.draft.scope.draft_id, mutation); const rawCount = h.raw().length;
        assert.equal(await h.page.flushDraft(), false, mutation); assert.equal(await h.page.retireDraft(), false, mutation);
        assert.equal(h.raw().length, rawCount, mutation); assert.equal(h.retirements().length, 1, mutation);
        assert.equal(h.page.inputReadyFor('create'), false, mutation); assert.equal(h.page.editorOpen, true, mutation);
      }
    } finally { gate.resolve(); await h.close(); }
  }
});
test('invalid exact-close receipt retains its original fixed close and raw editor', async () => {
  const h = harness({ todos: 'published\nraw', retireResult: ({ command }) => ({ ok: true, drafts: [
    { scope: { card_id: command.id, draft_id: 'different-draft' }, generation: '99', current_generation: '99', active: false, current_active: false }
  ] }) });
  try {
    await h.save(); assert.equal(h.retirements().length, 1); const handoff = h.page.businessHandoff;
    assert.equal(handoff.originalClose, h.retirements()[0].serialized); assert.equal(handoff.pendingNative, 'close');
    assert.equal(h.page.editorOpen, true); assert.equal(h.draft.disposed, false);
    assert.equal(h.draft.current.todos.text, 'published\nraw'); const original = plain(h.draft.current), epoch = h.page.editorInputEpoch;
    h.page.leaseTextChanged(h.page.editorViewOwner, 'description', '清理Unknown完整SDK输入', { value: '候选', offset: 2 });
    assert.equal(h.page.editorInputEpoch, epoch); assert.equal(h.page.retirementInput.size, 0);
    assert.deepEqual(plain(h.draft.current), original); assert.equal(handoff.originalClose, h.retirements()[0].serialized);
    assert.equal(h.page.businessPending(), true); await settle(); assert.equal(h.retirements().length, 1);
  } finally { await h.close(); }
});
test('existing V2 pending task blocks body Save and preserves task identity; separate raw remains retainable', async () => {
  const raw = 'pending single task', h = harness({ existing: true, todos: raw }); try {
    h.edit('edited existing body', 'description'); await h.save(); assert.equal(h.business().length, 0);
    assert.equal(h.draft.current.todos.text, raw); assert.match(h.page.message, /先添加或取消/);
    assert.equal(await h.page.flushDraft(), true); assert.equal(h.draft.confirmed.values.description.text, 'edited existing body');
    assert.equal(h.page.cards[0].tasks[0].id, 'original-v2-task-id'); assert.equal(h.retirements().length, 0);
    assert.equal(h.page.editorOpen, true); assert.equal(h.draft.disposed, false);
  } finally { await h.close(); }
});
test('legacy create command cannot borrow raw todos or bypass complete Save', async () => {
  const h = harness({ todos: 'authoritative\nraw' }); try {
    const command = h.page.command('create'), raw = h.draft.current;
    Object.assign(command, { title: raw.title.text, description: raw.description.text, hypothesis: raw.hypothesis.text,
      conclusion: raw.conclusion.text, category: raw.category, stage: raw.stage, todos: 'different\nraw' });
    const actualSubmit = Object.getPrototypeOf(h.page).submit.bind(h.page);
    await assert.rejects(actualSubmit(command, raw), /完整原请求日志/); await settle();
    assert.equal(h.business().length, 0); assert.equal(h.page.pending, '');
    assert.equal(h.draft.current.todos.text, 'authoritative\nraw'); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { await h.close(); }
});
test('emit fresh source identity for the actual Index business boundary', () => {
  console.log('Index.ets SHA256=' + crypto.createHash('sha256').update(source).digest('hex') + '; SDK TypeScript=' + ts.version);
});
