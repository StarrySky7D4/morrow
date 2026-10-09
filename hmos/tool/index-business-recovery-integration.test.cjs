'use strict';
// Actual Index methods + actual ETS models + one Workbench queue. Complete
// actual Store DTOs cover recovery reads; transport/SDK delivery are controlled.
// Passing this suite is not native mutation, ArkUI runtime or device evidence.
const fs = require('node:fs'), path = require('node:path');
const assert = require('node:assert/strict'), { test, after } = require('node:test');
const { harness, plain, readonly, deferred, settle, sha, source, actualMethod, identities, assertSourceUnchanged } =
  require('./index-business-recovery-test-harness.cjs');
function context(t, options) { const h = harness(options); t.after(() => h.dispose()); return h; }
function card(changes = {}) {
  return { id: 'fresh-current-card', revision: '7', source: '0a02aabb', content_kind: 'v2', title: 'current full title',
    description: 'current full body 🧪 é', hypothesis: 'current hypothesis', conclusion: 'current conclusion', category: '实验',
    stage: '待验证', favorite: true, deleted: false, deleted_at: '0', tasks: [
      { id: 'stable-task-1', text: 'duplicate label', completion: 1 }, { id: 'stable-task-2', text: 'duplicate label', completion: 0 }],
    assets: [], ...changes };
}
function routeFresh(h, current, drafts = []) {
  h.setRoute((_wire, p) => {
    if (p.action === 'list') return readonly({ cards: [current] });
    if (p.action === 'draft_list') return readonly({ drafts });
    throw new Error('Fresh open must only read: ' + p.action);
  });
}
function loadHistory() {
  const bytes = fs.readFileSync(path.resolve(__dirname, '../reports/ui-source/v26/editor-reopen-history-store-fixture.json'));
  assert.equal(sha(bytes), '4191624d7b2894abc17a3874ea3005249b450e6ee270b9c880ea2f04530415db');
  return JSON.parse(bytes);
}
function recoveryRoute(h, fixture, { closed = false } = {}) {
  const views = new Map((closed ? fixture.closed_parts : fixture.planned_parts).map(view => [view.part, view]));
  h.setRoute((wire, p) => {
    if (p.action === 'editor_intent_read') {
      assert.equal(p.editor_intent_ref.intent_id, fixture.proof.intent_id);
      assert.equal(p.editor_intent_ref.prepare_operation, fixture.proof.prepare_operation);
      return readonly({ editor_intents: [views.get(p.editor_intent_ref.part)] });
    }
    if (p.action === 'editor_commit_inspect') {
      assert.equal(wire, views.get('inspect').inspect_request_json); return plain(fixture.original_inspect_reply);
    }
    if (p.action === 'draft_read_history') {
      assert.equal(p.id, fixture.card_id); assert.equal(p.draft_id, fixture.fixed_parent_s2.scope.draft_id);
      assert.equal(p.draft_operation, fixture.fixed_parent_s2.operation_id); assert.equal(p.generation, fixture.fixed_parent_s2.generation);
      return plain(closed ? fixture.parent_history_retired_reply : fixture.parent_history_active_reply);
    }
    if (p.action === 'draft_read') {
      assert.equal(p.id, fixture.card_id); assert.equal(p.draft_id, fixture.child_current_read_reply.drafts[0].scope.draft_id);
      return plain(fixture.child_current_read_reply);
    }
    if (p.action === 'list') return readonly({ cards: h.page.cards });
    if (p.action === 'draft_list') return readonly({ drafts: h.page.draftRecords });
    if (p.action === 'editor_intent_list') return readonly({ editor_intents: [...views.values()].filter(v => v.part === 'summary') });
    throw new Error('Readonly source recovery must not mutate: ' + p.action);
  });
  return views;
}
function recoveryCard(fixture) { return { ...plain(fixture.original_inspect_reply.editor_commit.historical_card), content_kind: 'v2' }; }
function bindTodoComponent(h) {
  let todos, id = 0;
  todos = new h.m.EditorTodosDraft({ capture: (value, owner) => h.page.captureTodoRows(value, owner),
    isCurrent: (owner, revision, value) => h.page.ownsEditorView(owner) && h.page.todoRowsCurrent(owner, revision, value),
    count: async (text, owned) => { const result = await h.page.fieldPolicy.checkField('todos', text, owned);
      if (result.grapheme_count < 0) throw new Error(result.error); return result.grapheme_count; },
    format: async () => { throw new Error('Explicit input formatter required'); },
    changed: () => { if (todos) h.page.todoRowsStatus(todos.status()); }, failed: () => {}, id: () => 'actual-presentation-row-' + (++id) });
  todos.bind(h.page.editorViewOwner, h.page.todoInputRevision, h.page.todoInputValue); return todos;
}

test('actual fresh reopen installs latest full source, empty LF and a real writer without borrowing old root or task labels', async t => {
  const h = context(t), stale = card(), latest = card({ revision: '10', source: '0a04ccddee00', title: 'after metadata and TaskId', stage: '已完成',
    tasks: [{ id: 'stable-task-2', text: 'renamed duplicate', completion: 1 }, { id: 'stable-task-1', text: 'duplicate label', completion: 1 }] });
  h.page.cards = [stale]; h.page.detailId = stale.id; h.page.taskText = 'stale panel text'; routeFresh(h, latest);
  await h.page.openCardEditor(stale);
  assert.equal(h.page.editorOpen, true); assert.equal(h.page.editorBusinessMode, 'current_v2');
  assert.equal(h.page.editorDraft instanceof h.m.EditorDraftCoordinator, true);
  assert.equal(h.page.editorDraft.scope.source, latest.source); assert.equal(h.page.editorDraft.scope.source_revision, latest.revision);
  assert.equal(h.page.editorDraft.current.title.text, latest.title); assert.equal(h.page.editorDraft.current.todos.text, '');
  assert.equal(h.page.taskText, ''); assert.equal(h.page.ownedTodosBaseline, undefined); assert.equal(h.page.businessBinding, undefined);
  assert.deepEqual(plain(h.page.cards[0].tasks), latest.tasks); assert.equal(h.page.cards[0].favorite, true);
  assert.deepEqual(h.calls.map(w => JSON.parse(w).action), ['list', 'draft_list']);
  assert.equal(h.page.editorViewRevoked, true); assert.equal(h.page.ownsEditorView(h.page.editorViewOwner), false);
  h.mount(); assert.equal(h.page.ownsEditorView(h.page.editorViewOwner), true);
  assert.equal(h.page.inputReadyFor('edit'), true); assert.equal(h.page.editorMetadataReady(h.page.editorViewOwner), false);
  assert.equal(h.page.newCardTodos(), false); assert.equal(h.page.detailId, '');
  const full = h.page.editorDraft.current; h.page.actualEditorCategorySelect(h.page.editorViewOwner, 0, '灵感');
  assert.deepEqual(plain(h.page.editorDraft.current), plain(full)); assert.equal(h.calls.length, 2);
});

test('actual migrated legacy classification opens current_v2 with no raw LF reconstruction', async t => {
  const h = context(t), current = card({ content_kind: 'legacy' }); h.page.cards = [current]; routeFresh(h, current);
  await h.page.openCardEditor(current); h.mount();
  assert.equal(h.page.editorBusinessMode, 'current_v2'); assert.equal(h.page.editorDraft.current.todos.text, '');
  assert.equal(h.page.newCardTodos(), false); assert.equal(h.page.editorMetadataReady(h.page.editorViewOwner), false);
});

test('actual fresh read rejects missing or unknown content kind, regardless of task ids or editor markers', async t => {
  for (const kind of [undefined, '', 'unknown', 'editor-marker']) {
    const h = context(t), original = card(), bad = card({ content_kind: kind }); h.page.cards = [original]; routeFresh(h, bad);
    await h.page.openCardEditor(original);
    assert.equal(h.page.editorDraft, undefined); assert.equal(h.page.editorOpen, false); assert.equal(h.page.dirty, false);
    assert.equal(h.page.cards[0].source, original.source); assert.equal(h.calls.length, 1); assert.match(h.page.message, /来源尚未确认/);
  }
});

test('fresh card or draft list Unknown does not install, dispose or dispatch a write', async t => {
  for (const action of ['list', 'draft_list']) {
    const h = context(t), c = card(); h.page.cards = [c]; h.page.title = 'retained existing text'; routeFresh(h, c);
    h.setIntercept((_wire, p, route) => { if (p.action === action) throw new Error('controlled readonly Unknown'); return route(); });
    await h.page.openCardEditor(c);
    assert.equal(h.page.editorOpen, false); assert.equal(h.page.editorDraft, undefined); assert.equal(h.page.title, 'retained existing text');
    assert.equal(h.page.businessWorking, false); assert.match(h.page.message, /readonly Unknown/);
    assert.equal(h.calls.some(w => !['list', 'draft_list'].includes(JSON.parse(w).action)), false);
  }
});

test('late fresh read cannot overwrite a newly owned real editor or complete raw selection', async t => {
  const h = context(t), c = card(), wait = deferred(); h.page.cards = [c]; routeFresh(h, c);
  h.setIntercept(async (_wire, p, route) => { if (p.action === 'list') await wait.promise; return route(); });
  const opening = h.page.openCardEditor(c); await settle();
  h.page.selected = c.id; h.page.title = 'S3 retained'; h.page.description = 'S3 complete raw'; h.page.editorOpen = true;
  h.page.attachDraft(c); h.mount(); h.adopt('description', 'late S4 🧪');
  const writer = h.page.editorDraft, full = writer.current; wait.resolve(); await opening;
  assert.equal(h.page.editorDraft, writer); assert.equal(writer.disposed, false); assert.deepEqual(plain(writer.current), plain(full));
  assert.equal(h.page.description, 'late S4 🧪'); assert.match(h.page.message, /归属已变化/);
  assert.equal(h.calls.some(w => !['list', 'draft_list'].includes(JSON.parse(w).action)), false);
});

test('fresh read cutoff refuses dirty, background and replaced opening identity independently', async t => {
  for (const change of [page => { page.dirty = true; }, page => { page.foreground = false; }, page => { page.editorOpenIdentity = 'new-owner'; }]) {
    const h = context(t), c = card(), wait = deferred(); h.page.cards = [c]; h.page.description = 'retained raw'; routeFresh(h, c);
    h.setIntercept(async (_wire, p, route) => { if (p.action === 'draft_list') await wait.promise; return route(); });
    const opening = h.page.openCardEditor(c); await settle(); change(h.page); wait.resolve(); await opening;
    assert.equal(h.page.editorDraft, undefined); assert.equal(h.page.editorOpen, false); assert.equal(h.page.description, 'retained raw');
    assert.match(h.page.message, /归属已变化/);
  }
});

test('actual fresh open guards existing input, native pending and retirement before any read', async t => {
  for (const field of ['editorOpen', 'busy', 'draftWorking', 'draftRetiring', 'draftRetirementUnknown', 'dirty']) {
    const h = context(t), c = card(); h.page.cards = [c]; h.page[field] = true; await h.page.openCardEditor(c);
    assert.equal(h.calls.length, 0); assert.equal(h.page.editorDraft, undefined);
  }
  const h = context(t), c = card(); h.page.cards = [c]; h.page.pending = 'original pending literal'; await h.page.openCardEditor(c);
  assert.equal(h.calls.length, 0); assert.equal(h.page.pending, 'original pending literal');
});

test('actual metadata eligibility is restricted to mounted new-card scope, both enabled and callback use the guard', t => {
  const h = context(t); h.page.newCard();
  assert.equal(h.page.editorMetadataReady(h.page.editorViewOwner), false); h.mount();
  assert.equal(h.page.editorMetadataReady(h.page.editorViewOwner), true);
  h.page.actualEditorCategorySelect(h.page.editorViewOwner, 1, '实验');
  assert.equal(h.page.editorDraft.current.category, '实验'); assert.equal(h.page.editorDraft.current.stage, '待验证');
  h.page.draftRestoreInput = true;
  assert.equal(h.page.editorMetadataReady(h.page.editorViewOwner), false);
  h.page.actualEditorCategorySelect(h.page.editorViewOwner, 0, '灵感'); assert.equal(h.page.editorDraft.current.category, '实验');
  assert.ok(/\.enabled\(this\.editorMetadataReady\(owner\)/.test(source), 'actual UI enabled uses metadata guard');
  assert.ok(/if \(!this\.editorMetadataReady\(owner\)/.test(source), 'actual callback uses the same metadata guard');
});

test('actual Index restart reads fixed S2 history and advanced current child, installs the same real paused writer without any active parent', async t => {
  const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture); h.page.cards = [recoveryCard(fixture)];
  h.page.draftRecords = [plain(fixture.child_current_read_reply.drafts[0])];
  const install = h.page.installRecoveredBusinessChild.bind(h.page); let beforeInstall;
  h.page.installRecoveredBusinessChild = (...args) => {
    beforeInstall = args[2]; assert.equal(beforeInstall.writer.writesPaused, true); assert.equal(h.page.editorDraft, undefined);
    assert.equal(args[1].parentHistory.generation, '2'); assert.equal(args[1].parentHistory.operation_id, 'late-S2');
    return install(...args);
  };
  await h.page.recoverLinkedBusinessDraft(h.page.draftRecords[0]);
  assert.equal(h.page.editorOpen, true, h.page.message); assert.equal(h.page.editorDraft, beforeInstall.writer);
  assert.equal(h.page.editorDraft, h.page.businessRecovery.ready.writer); assert.equal(h.page.businessBinding.parent, undefined);
  assert.equal(h.page.businessBinding.liveDraft, h.page.editorDraft); assert.equal(h.page.editorDraft.writesPaused, false);
  assert.equal(h.page.editorDraft.confirmed.generation, '2'); assert.equal(h.page.businessSession.publication.generation, '1');
  assert.deepEqual(plain(h.page.editorDraft.current), fixture.child_current_read_reply.drafts[0].values);
  assert.equal(h.page.taskText, fixture.child_current_read_reply.drafts[0].values.todos.text);
  assert.equal(h.page.ownedTodosBaseline, h.page.businessSession); assert.equal(h.page.businessHandoff, beforeInstall.handoff);
  assert.equal(h.page.businessSession.qualified, true); assert.equal(h.page.businessRecovery.handoff.firstCommitted, true);
  assert.equal(h.page.businessRecovery.handoff.currentConfirmed.generation, '2');
  assert.equal(h.page.editorViewRevoked, true); assert.equal(h.page.inputReadyFor('edit'), false);
  h.mount(); await h.runTimers(); assert.equal(h.page.newCardTodos(), true); assert.equal(h.page.todoBusinessReady, false);
  const todos = bindTodoComponent(h); await settle(); assert.equal(todos.status().business_ready, true);
  assert.equal(h.page.inputReadyFor('edit'), true); assert.equal(h.page.editorMetadataReady(h.page.editorViewOwner), false);
  const full = h.page.editorDraft.current; h.page.actualEditorCategorySelect(h.page.editorViewOwner, 0, '实验');
  assert.deepEqual(plain(h.page.editorDraft.current), plain(full)); todos.stop();
  assert.equal(h.calls.every(w => ['editor_intent_read', 'editor_commit_inspect', 'list', 'draft_read_history', 'draft_read'].includes(JSON.parse(w).action)), true);
  assert.equal(h.calls.filter(w => JSON.parse(w).action === 'draft_read_history').length, 1);
  assert.ok(actualMethod('restoreDraft').includes('this.recoverLinkedBusinessDraft(record)'), 'ordinary raw restore uses exact linked recovery');
});

test('actual closed handoff restart uses retired parent history, keeps original close readonly and does not revive its Save', async t => {
  const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture, { closed: true }); h.page.cards = [recoveryCard(fixture)];
  await h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0]));
  assert.equal(h.page.editorOpen, true, h.page.message); assert.equal(h.page.businessSession.phase, 'closed');
  assert.equal(h.page.businessSession.qualified, true); assert.equal(h.page.businessPending(), false); assert.equal(h.page.businessHandoff, undefined);
  assert.equal(h.page.businessBinding.parent, undefined); assert.equal(h.page.businessRecovery.handoff.closed, true);
  assert.equal(h.page.businessRecovery.parentHistory.current_active, false); assert.equal(h.page.businessRecovery.parentHistory.generation, '2');
  assert.equal(h.page.businessRecovery.handoff.originalClose, JSON.stringify(fixture.close_request));
  assert.equal(h.page.ownedTodosBaseline, h.page.businessSession); assert.throws(() => h.page.businessSession.retrySave(), /historical inspection/);
  h.mount(); await h.runTimers(); h.adopt('description', 'S4 current writer input 🧪');
  assert.equal(h.page.editorDraft.current.description.text, 'S4 current writer input 🧪'); assert.equal(h.page.businessSession.phase, 'closed');
  assert.equal(h.calls.some(w => ['editor_save', 'editor_intent_prepare', 'draft_save', 'draft_continue_business', 'editor_intent_close'].includes(JSON.parse(w).action)), false);
});

test('actual Unknown current read is explicitly retried through the identical queue wire and never first-sends raw or business', async t => {
  const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture); h.page.cards = [recoveryCard(fixture)]; let lose = true;
  h.setIntercept((_wire, p, route) => { const reply = route(); if (p.action === 'draft_read' && lose) { lose = false; throw new Error('controlled current Unknown'); } return reply; });
  await h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0]));
  const recovery = h.page.businessRecovery, original = recovery.pendingRead;
  assert.equal(h.page.editorOpen, false); assert.equal(h.page.editorDraft, undefined); assert.equal(recovery.unknown, true);
  assert.equal(recovery.parentHistory.operation_id, 'late-S2'); assert.equal(h.page.businessSession.qualified, true);
  const before = h.calls.length; await h.page.recoverCurrentBusinessChild();
  assert.equal(h.page.businessRecovery, recovery); assert.equal(h.page.editorOpen, true, h.page.message);
  assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).action), ['list', 'draft_read']); assert.equal(h.calls.at(-1), original);
  assert.equal(recovery.pendingRead, ''); assert.equal(h.page.businessBinding.parent, undefined);
  assert.equal(h.calls.some(w => !['editor_intent_read', 'editor_commit_inspect', 'list', 'draft_read_history', 'draft_read'].includes(JSON.parse(w).action)), false);
});

test('actual Unknown fixed parent history uses original operation/generation before current child and preserves own Session facts', async t => {
  const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture); h.page.cards = [recoveryCard(fixture)]; let lose = true;
  h.setIntercept((_wire, p, route) => { const reply = route(); if (p.action === 'draft_read_history' && lose) { lose = false; throw new Error('controlled parent Unknown'); } return reply; });
  await h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0]));
  const original = h.page.businessRecovery.pendingRead; assert.equal(JSON.parse(original).draft_operation, 'late-S2');
  assert.equal(h.page.businessSession.publication.generation, '1'); assert.equal(h.page.editorDraft, undefined);
  const before = h.calls.length; await h.page.recoverCurrentBusinessChild();
  assert.equal(h.page.editorOpen, true, h.page.message); assert.equal(h.calls[before + 1], original);
  assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).action), ['list', 'draft_read_history', 'draft_read']);
  assert.equal(h.page.businessRecovery.parentHistory.generation, '2'); assert.equal(h.page.editorDraft.confirmed.generation, '2');
});

test('actual late complete editor input during current recovery response cannot be overwritten or disposed', async t => {
  const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture); const c = recoveryCard(fixture); h.page.cards = [c];
  const wait = deferred(), entered = deferred();
  h.setIntercept(async (_wire, p, route) => { const reply = route(); if (p.action === 'draft_read') { entered.resolve(); await wait.promise; } return reply; });
  const recovery = h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0])); await entered.promise;
  h.page.selected = c.id; h.page.title = 'separate complete editor'; h.page.description = 'late independent raw'; h.page.editorOpen = true;
  h.page.attachDraft(c); h.mount(); h.adopt('description', 'late S4 selection 🧪'); const writer = h.page.editorDraft, full = writer.current;
  wait.resolve(); await recovery;
  assert.equal(h.page.editorDraft, writer); assert.equal(writer.disposed, false); assert.deepEqual(plain(writer.current), plain(full));
  assert.equal(h.page.businessRecovery.currentRecord.generation, '2'); assert.equal(h.page.businessRecovery.ready, undefined);
  assert.equal(h.page.businessSession.qualified, true); assert.equal(h.page.businessRecovery.pendingRead, '');
  assert.match(h.page.message, /owner\/input cutoff/); assert.equal(h.page.description, 'late S4 selection 🧪');
});

test('actual foreign or inactive current child response cannot create a grant or fake writer from first ACK', async t => {
  for (const kind of ['foreign', 'inactive', 'partial']) {
    const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture); h.page.cards = [recoveryCard(fixture)];
    h.setIntercept((_wire, p, route) => { const reply = route(); if (p.action === 'draft_read') {
      if (kind === 'foreign') reply.drafts[0].scope.draft_id = 'different-child';
      if (kind === 'inactive') { reply.drafts[0].active = false; reply.drafts[0].current_active = false; }
      if (kind === 'partial') delete reply.drafts[0].business_link;
    } return reply; });
    await h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0]));
    assert.equal(h.page.editorDraft, undefined); assert.equal(h.page.editorOpen, false); assert.equal(h.page.businessRecovery.ready, undefined);
    assert.equal(h.page.businessSession.qualified, true); assert.equal(h.page.businessRecovery.handoff.confirmed, undefined);
    assert.equal(h.calls.some(w => ['draft_save', 'draft_continue_business', 'editor_save'].includes(JSON.parse(w).action)), false);
  }
});

test('actual recovery preserves full child LF with source conflict after independent metadata or TaskId advances current card', async t => {
  const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture); const current = recoveryCard(fixture);
  current.revision = '2'; current.source = '0a04ddeeff00'; current.tasks.reverse(); current.stage = '已完成'; h.page.cards = [current];
  await h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0]));
  assert.equal(h.page.editorOpen, true, h.page.message); assert.equal(h.page.draftConflict, true);
  assert.equal(h.page.editorDraft.scope.source, fixture.child_current_read_reply.drafts[0].scope.source);
  assert.deepEqual(plain(h.page.editorDraft.current), fixture.child_current_read_reply.drafts[0].values);
  assert.deepEqual(plain(h.page.current().tasks), current.tasks); assert.equal(h.page.current().source, current.source);
  h.mount(); await h.runTimers(); const before = h.calls.length; await h.page.save(); assert.equal(h.calls.length, before);
  assert.equal(h.page.businessSession.qualified, true); assert.equal(h.page.businessBinding.parent, undefined);
});

test('actual current card native DTOs after metadata and TaskId reopen feed current_v2 strict Save with exact full source and no continuation root', async t => {
  const bytes = fs.readFileSync(path.resolve(__dirname, '../reports/ui-source/v27/editor-card-source-store-fixture.json'));
  assert.equal(sha(bytes), '409c9adf3fa6297aa60f9ea7130b8d39c451b37516965da48d7c2548895449ff');
  const fixture = JSON.parse(bytes);
  for (const item of fixture.cases) {
    const h = context(t), current = plain(item.reopened_reply.cards[0]); h.page.cards = [plain(item.initial_reply.cards[0])]; routeFresh(h, current);
    await h.page.openCardEditor(h.page.cards[0]); h.mount(); h.adopt('description', 'next body edit after actual TaskId mutations');
    let proposal, raw;
    h.setRoute((_wire, p) => {
      if (p.action === 'draft_save') {
        raw = Object.assign(new h.m.DraftRecord(), { scope: plain(h.page.editorDraft.scope), values: { ...plain(p.draft.values), assets: plain(p.draft.assets) },
          generation: h.m.nextGeneration(p.draft.expected_generation), current_generation: h.m.nextGeneration(p.draft.expected_generation),
          active: true, current_active: true, operation_id: p.draft.operation_id, request_sha256: sha(JSON.stringify(p.draft)),
          repeated: false, assets: [], consumed_imports: [], fork_link: null, fork_retirement: null, business_link: null, business_retirement: null });
        return readonly({ effect: 'committed', receipt_revision: raw.generation, drafts: [raw] });
      }
      if (p.action === 'editor_intent_prepare') { proposal = JSON.parse(p.editor_intent.request_json);
        return { ...readonly(), ok: false, error: 'controlled definite preparation rejection' }; }
      if (p.action === 'editor_intent_list') return readonly();
      throw new Error('Expected only raw/prepare admission: ' + p.action);
    });
    await h.page.save(); assert.ok(proposal, h.page.message); assert.equal(proposal.mode, 'current_v2');
    assert.equal(proposal.business.source, current.source); assert.equal(proposal.publication.generation, raw.generation);
    assert.equal(proposal.business.todos, ''); assert.equal(proposal.continuation, null);
    assert.equal(proposal.business.category, current.category); assert.equal(proposal.business.stage, current.stage);
    assert.deepEqual(plain(h.page.current().tasks), current.tasks); assert.equal(h.page.current().favorite, current.favorite);
    assert.equal(h.page.editorDraft.disposed, false); assert.equal(h.page.ownedTodosBaseline, undefined);
    assert.equal(h.calls.some(w => ['editor_intent_issue', 'editor_save', 'draft_continue_business'].includes(JSON.parse(w).action)), false);
  }
});

test('actual fresh source accepts real AssetView names and rejects incomplete card DTOs before opening', async t => {
  const h = context(t), c = card({ assets: [{ id: 'actual-asset-shape', name: 'original.png', kind: 'image', media_type: 'image/png',
    byte_length: '8', sha256: 'a'.repeat(64) }] }); h.page.cards = [c]; routeFresh(h, c);
  await h.page.openCardEditor(c); assert.equal(h.page.editorOpen, true, h.page.message);
  assert.equal(h.page.editorDraft.current.assets[0].asset_id, c.assets[0].id); assert.equal(h.page.editorAssets[0].name, 'original.png');
  for (const change of [v => { delete v.title; }, v => { v.source = 'not-hex'; }, v => { v.revision = '0'; },
    v => { delete v.assets[0].name; }, v => { delete v.tasks[0].id; }]) {
    const next = context(t), original = card(), bad = plain(c); change(bad); next.page.cards = [original]; routeFresh(next, bad);
    await next.page.openCardEditor(original); assert.equal(next.page.editorOpen, false); assert.equal(next.page.editorDraft, undefined);
    assert.equal(next.calls.length, 1); assert.equal(next.page.cards[0].source, original.source);
  }
});

test('actual fresh and recovery reads reject active file, paste, raw fork and owned boundary before queue admission', async t => {
  for (const [key, value] of [['attachmentWorking', true], ['pasteWorking', true], ['rawFork', {}], ['rawForkWorking', true],
    ['editorBoundary', 'owned-boundary'], ['attachmentPending', 'fixed-file-request'], ['pendingSpools', [{}]],
    ['importRecords', [{ phase: 'unknown' }]]]) {
    const h = context(t), c = card(); h.page.cards = [c]; h.page[key] = value; await h.page.openCardEditor(c);
    assert.equal(h.calls.length, 0, 'fresh read respects ' + key); assert.equal(h.page.editorDraft, undefined);
    const fixture = loadHistory(); await h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0]));
    assert.equal(h.calls.length, 0, 'linked recovery respects ' + key); assert.equal(h.page.businessRecovery, undefined);
  }
});

test('actual intent-list restore then resume with no active parent reads current child and never creates a publication S1 writer', async t => {
  const h = context(t), fixture = loadHistory(), views = recoveryRoute(h, fixture); h.page.cards = [recoveryCard(fixture)];
  h.page.draftRecords = []; await h.page.restoreBusinessIntent(fixture.planned_parts[0]);
  assert.equal(h.page.businessSession.phase, 'handoff_planned'); assert.equal(h.page.editorDraft, undefined);
  assert.equal(h.page.businessBinding, undefined); assert.equal(h.calls.every(w => JSON.parse(w).action === 'editor_intent_read'), true);
  const originalSession = h.page.businessSession; await h.page.resumeBusinessEditor();
  assert.equal(h.page.editorOpen, true, h.page.message); assert.equal(h.page.businessSession, originalSession);
  assert.equal(h.page.businessBinding.parent, undefined); assert.equal(h.page.editorDraft.scope.draft_id, 'child');
  assert.equal(h.page.editorDraft.confirmed.generation, '2'); assert.equal(h.page.businessRecovery.parentHistory.generation, '2');
  assert.equal(h.calls.some(w => ['editor_save', 'draft_continue_business', 'draft_save'].includes(JSON.parse(w).action)), false);
});

test('actual recovered current child only retires fixed parent after explicit page action and a close rejection preserves current writer', async t => {
  const h = context(t), fixture = loadHistory(), views = recoveryRoute(h, fixture); h.page.cards = [recoveryCard(fixture)];
  await h.page.recoverLinkedBusinessDraft(plain(fixture.child_current_read_reply.drafts[0])); h.mount(); await h.runTimers();
  const writer = h.page.editorDraft, before = h.calls.length;
  h.setRoute((wire, p) => {
    if (p.action === 'editor_intent_read') return readonly({ editor_intents: [views.get(p.editor_intent_ref.part)] });
    if (p.action === 'draft_continue_business_retire') {
      assert.equal(p.business_retirement.request_json, fixture.retirement_request.business_retirement.request_json); return plain(fixture.retirement_reply);
    }
    if (p.action === 'editor_intent_close') return { ...readonly(), ok: false, error: 'controlled definite close rejection' };
    throw new Error('No other action is permitted at explicit fixed cleanup: ' + p.action);
  });
  await h.page.reconcileBusinessHandoff();
  assert.equal(h.page.editorDraft, writer); assert.equal(writer.disposed, false); assert.equal(h.page.businessBinding.parent, undefined);
  assert.equal(h.page.businessHandoff.retired?.active, false, h.page.message + ' / ' + h.calls.slice(before).map(w => JSON.parse(w).action));
  assert.equal(h.page.businessSession.qualified, true);
  assert.match(h.page.message, /close rejection/);
  assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).action),
    ['editor_intent_read', 'editor_intent_read', 'draft_continue_business_retire', 'editor_intent_close']);
  assert.deepEqual(plain(writer.current), fixture.child_current_read_reply.drafts[0].values);
});

async function preparedRecovery(h, fixture, identity) {
  recoveryRoute(h, fixture); h.page.cards = [recoveryCard(fixture)]; h.page.businessRecoveryIdentity = identity;
  const session = await h.m.EditorBusinessSessionCoordinator.restore(fixture.proof, h.page.businessHooks(undefined, identity));
  h.page.businessSession = session; await session.inspect();
  const recovery = new h.m.EditorBusinessRecoveryCoordinator(session, h.page.businessRecoveryHooks(identity), () => h.page.businessRecoveryCurrent(identity));
  h.page.businessRecovery = recovery; h.page.businessRecoverySession = session;
  const ready = await recovery.load(); h.track(ready.writer);
  return { session, recovery, ready };
}

test('actual install refuses a different real prepared Recovery writer instead of granting from matching detached raw values', async t => {
  const fixture = loadHistory(), first = context(t), other = context(t);
  const a = await preparedRecovery(first, fixture, 'exact-owner-a'), b = await preparedRecovery(other, fixture, 'exact-owner-b');
  assert.notEqual(a.ready.writer, b.ready.writer); assert.deepEqual(plain(a.ready.values), plain(b.ready.values));
  assert.throws(() => first.page.installRecoveredBusinessChild(a.session, a.recovery, b.ready, 'exact-owner-a'), /安装前|归属|writer|identity/);
  assert.equal(first.page.editorOpen, false); assert.equal(first.page.editorDraft, undefined);
  assert.equal(a.ready.writer.writesPaused, true); assert.equal(a.ready.writer.disposed, false); assert.equal(b.ready.writer.disposed, false);
  assert.equal(a.session.qualified, true); assert.equal(first.calls.some(w => ['draft_save', 'editor_save', 'draft_continue_business'].includes(JSON.parse(w).action)), false);
});

test('actual install refuses stale detached scope while preserving the same actual unclaimed writer and own known facts', async t => {
  const h = context(t), fixture = loadHistory(), prepared = await preparedRecovery(h, fixture, 'exact-record-owner');
  prepared.ready.record.scope.card_id = 'foreign-card';
  assert.throws(() => h.page.installRecoveredBusinessChild(prepared.session, prepared.recovery, prepared.ready, 'exact-record-owner'), /安装前|归属|record|identity/);
  assert.equal(h.page.editorOpen, false); assert.equal(h.page.editorDraft, undefined); assert.equal(prepared.ready.writer.disposed, false);
  assert.equal(prepared.ready.writer.writesPaused, true); assert.equal(prepared.recovery.currentRecord.scope.card_id, fixture.card_id);
  assert.equal(prepared.session.qualified, true);
});

test('actual fresh details open detects linked current draft and runs exact recovery before any editor attachment', async t => {
  const h = context(t), fixture = loadHistory(); recoveryRoute(h, fixture); const c = recoveryCard(fixture); h.page.cards = [c];
  h.page.draftRecords = [plain(fixture.child_current_read_reply.drafts[0])];
  const recover = h.page.recoverLinkedBusinessDraft.bind(h.page); let observed;
  h.page.recoverLinkedBusinessDraft = record => { observed = recover(record); return observed; };
  await h.page.openCardEditor(c); assert.ok(observed, 'actual fresh open selected exact linked restore'); await observed;
  assert.equal(h.page.editorOpen, true, h.page.message); assert.equal(h.page.businessBinding.parent, undefined);
  assert.equal(h.page.editorDraft.scope.draft_id, 'child'); assert.equal(h.page.editorDraft.confirmed.generation, '2');
  assert.deepEqual(plain(h.page.editorDraft.current), fixture.child_current_read_reply.drafts[0].values);
  assert.equal(h.calls.some(w => ['draft_continue_business', 'draft_save', 'editor_save'].includes(JSON.parse(w).action)), false);
});

after(() => { assertSourceUnchanged(); console.log(JSON.stringify({ qualification: 'actual-Index-source-only', inputs: identities() })); });
module.exports = { loadHistory, recoveryRoute };
