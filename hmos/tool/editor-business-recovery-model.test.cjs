'use strict';
// Actual ETS Recovery + Handoff/Session/Business/Draft models. Transport reads
// use complete actual Store-produced DTOs; Node providers are controlled and
// this does not prove page installation, SDK, native new writes, or device UI.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), { test } = require('node:test'), crypto = require('node:crypto');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const source = fs.readFileSync(path.join(__dirname, 'editor-business-session-model.test.cjs'), 'utf8');
const marker = source.indexOf('const action = wire =>'); assert.ok(marker > 0); const shared = {};
vm.runInNewContext(source.slice(0, marker)
  .replace("...load('EditorBusinessSession')", "...load('EditorBusinessSession'), ...load('EditorBusinessHandoff'), ...load('EditorBusinessRecovery')")
  .replace('{ exports, require: load, Uint8Array }', '{ exports, require: load, Uint8Array, setTimeout, clearTimeout }') +
  '\nObject.assign(shared, { model });', { require, __dirname, process, Buffer, TextEncoder, shared, setTimeout, clearTimeout });
const readonlyView = view => ({ ok: true, error: '', effect: 'not_committed', receipt_revision: '', editor_intents: [plain(view)], intent_next_after: '' });
function fixture() {
  const bytes = fs.readFileSync(path.resolve(__dirname, '../reports/ui-source/v26/editor-reopen-history-store-fixture.json'));
  assert.equal(sha(bytes), '4191624d7b2894abc17a3874ea3005249b450e6ee270b9c880ea2f04530415db'); return JSON.parse(bytes);
}
async function context(t, { closed = false, qualified = true } = {}) {
  const actual = fixture(), m = shared.model(), calls = [], draftCalls = [], owner = { current: true, guard: true, installed: null };
  const parts = new Map((closed ? actual.closed_parts : actual.planned_parts).map(view => [view.part, view]));
  let intercept, operations = 0, changes = 0;
  const route = wire => {
    const request = JSON.parse(wire);
    if (request.action === 'editor_intent_read') {
      assert.equal(request.editor_intent_ref.intent_id, actual.proof.intent_id);
      assert.equal(request.editor_intent_ref.prepare_operation, actual.proof.prepare_operation);
      return readonlyView(parts.get(request.editor_intent_ref.part));
    }
    if (request.action === 'editor_commit_inspect') {
      assert.equal(wire, parts.get('inspect').inspect_request_json); return plain(actual.original_inspect_reply);
    }
    if (request.action === 'draft_read_history') {
      assert.equal(request.id, actual.card_id); assert.equal(request.draft_id, actual.fixed_parent_s2.scope.draft_id);
      assert.equal(request.draft_operation, actual.fixed_parent_s2.operation_id); assert.equal(request.generation, actual.fixed_parent_s2.generation);
      return plain(closed ? actual.parent_history_retired_reply : actual.parent_history_active_reply);
    }
    if (request.action === 'draft_read') {
      assert.deepEqual(request, { action: 'draft_read', id: actual.card_id, draft_id: actual.child_current_read_reply.drafts[0].scope.draft_id });
      return plain(actual.child_current_read_reply);
    }
    throw new Error('Recovery must not dispatch native mutation ' + request.action);
  };
  const sender = async wire => {
    calls.push(wire); const request = JSON.parse(wire);
    return intercept ? intercept(wire, request, () => route(wire)) : route(wire);
  };
  const sessionHooks = { fields: new m.EditorFieldPolicy(async () => { throw new Error('Recovery must not regenerate field projections'); }),
    send: sender, changed: () => changes++, isCurrent: () => owner.current, isExact: () => false, parentReady: () => false };
  const session = await m.EditorBusinessSessionCoordinator.restore(actual.proof, sessionHooks); if (qualified) await session.inspect();
  const hooks = { send: sender, sendDraft: async wire => { draftCalls.push(wire); throw new Error('Recovery must not dispatch draft save'); },
    changed: () => changes++, operation: () => { operations++; return 'forbidden-recovery-new-operation'; },
    isCurrent: () => owner.current, isExact: () => false, importsReady: () => true, childCurrent: child => child === owner.installed };
  const controller = () => new m.EditorBusinessRecoveryCoordinator(session, hooks, () => owner.guard);
  const recovery = qualified ? controller() : undefined;
  t.after(() => { recovery?.ready?.writer.dispose(); recovery?.dispose(); owner.installed?.dispose(); });
  return { actual, m, calls, draftCalls, owner, session, recovery, controller,
    setIntercept(value) { intercept = value; }, get operations() { return operations; }, get changes() { return changes; } };
}

test('actual readonly recovery prepares the same paused current writer with full S3, fixed S2 parent and no business/raw mutation', async t => {
  const h = await context(t), before = h.calls.length, result = await h.recovery.load();
  assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).action), ['editor_intent_read', 'editor_intent_read', 'editor_intent_read', 'draft_read_history', 'draft_read']);
  assert.equal(result.writer, h.recovery.ready.writer); assert.equal(result.handoff, h.recovery.handoff);
  assert.ok(result.writer instanceof h.m.EditorDraftCoordinator);
  assert.equal(result.writer.writesPaused, true); assert.equal(result.writer.dirty, false); assert.equal(result.record.generation, '2');
  assert.deepEqual(plain(result.values), h.actual.child_current_read_reply.drafts[0].values);
  assert.equal(result.parentReference.generation, '2'); assert.equal(result.parentReference.save_operation, 'late-S2');
  assert.equal(h.session.publication.generation, '1'); assert.equal(h.recovery.parentHistory.current_active, true);
  assert.equal(result.handoff.pendingNative, ''); assert.equal(result.handoff.firstCommitted, true);
  assert.equal(h.operations, 0); assert.equal(h.draftCalls.length, 0); assert.equal(h.session.qualified, true);
  const nativeRaw = result.writer.current; result.values.description.text = 'caller mutation'; result.record.scope.source = 'foreign';
  result.parentReference.save_operation = 'foreign'; h.recovery.parentHistory.values.title.text = 'getter mutation';
  assert.deepEqual(plain(h.recovery.ready.values), plain(nativeRaw)); assert.equal(result.writer.scope.source, h.session.confirmed.historical_card.source);
  await assert.rejects(async () => result.writer.flush(), /paused/); assert.equal(h.draftCalls.length, 0);
  h.owner.installed = result.writer; const writer = h.recovery.claimWriter(); assert.equal(writer, result.writer);
  h.owner.guard = false; assert.throws(() => h.recovery.claimWriter(), /already claimed/); assert.equal(writer.disposed, false);
  assert.equal(writer.writesPaused, true); h.recovery.dispose(); assert.equal(writer.disposed, false); writer.resumeWrites();
  assert.equal(h.draftCalls.length, 0); assert.equal(h.session.qualified, true);
});

test('actual closed handoff reads retired parent history and active current child while keeping original closed session readonly', async t => {
  const h = await context(t, { closed: true }), before = h.calls.length, result = await h.recovery.load();
  assert.equal(h.session.phase, 'closed'); assert.equal(result.handoff.closed, true); assert.equal(result.record.active, true);
  assert.equal(h.recovery.parentHistory.active, true); assert.equal(h.recovery.parentHistory.current_active, false);
  assert.equal(h.recovery.parentHistory.current_generation, '3'); assert.equal(result.parentReference.generation, '2');
  assert.equal(result.handoff.originalClose, JSON.stringify(h.actual.close_request)); assert.equal(result.handoff.retirementCommitted, false);
  assert.equal(result.values.todos.text, h.actual.child_current_read_reply.drafts[0].values.todos.text);
  assert.throws(() => h.session.retrySave(), /historical inspection/); assert.equal(h.operations, 0);
  assert.equal(h.calls.slice(before).some(w => !['editor_intent_read', 'draft_read_history', 'draft_read'].includes(JSON.parse(w).action)), false);
});

test('Unknown current read retries original string after owner loss, keeps facts and never opens or installs into revoked cutoff', async t => {
  const h = await context(t); let lose = true;
  h.setIntercept((wire, request, route) => { const reply = route(); if (request.action === 'draft_read' && lose) { lose = false; throw new Error('lost current read'); } return reply; });
  await assert.rejects(h.recovery.load(), /lost current read/); const original = h.recovery.originalCurrentRead;
  assert.equal(h.recovery.pendingRead, original); assert.equal(h.recovery.unknown, true); assert.equal(h.recovery.ready, undefined);
  assert.equal(h.recovery.parentHistory.generation, '2'); assert.equal(h.recovery.handoff.originalHandoffLiteral, h.actual.handoff_request.business_handoff.request_json);
  await assert.rejects(h.recovery.load(), /fixed readonly/); h.owner.current = h.owner.guard = false;
  await assert.rejects(h.recovery.retry(), /owner\/input cutoff/); assert.equal(h.calls.at(-1), original);
  assert.equal(h.recovery.unknown, false); assert.equal(h.recovery.currentRecord.generation, '2'); assert.equal(h.recovery.ready, undefined);
  assert.equal(h.session.qualified, true); h.owner.current = h.owner.guard = true; const before = h.calls.length;
  const result = await h.recovery.load(); assert.deepEqual(h.calls.slice(before), [original]);
  assert.equal(result.writer.writesPaused, true); assert.equal(h.operations, 0); assert.equal(h.draftCalls.length, 0);
});

test('Unknown exact S2 parent read retries that fixed operation/generation before reading current child', async t => {
  const h = await context(t); let lose = true;
  h.setIntercept((wire, request, route) => { const reply = route(); if (request.action === 'draft_read_history' && lose) { lose = false; throw new Error('lost parent history'); } return reply; });
  await assert.rejects(h.recovery.load(), /lost parent history/); const original = h.recovery.pendingRead;
  assert.equal(JSON.parse(original).draft_operation, 'late-S2'); assert.equal(h.recovery.currentRecord, undefined);
  const before = h.calls.length, result = await h.recovery.retry(); assert.equal(h.calls[before], original);
  assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).action), ['draft_read_history', 'draft_read']);
  assert.equal(result.parentReference.generation, '2'); assert.equal(h.recovery.pendingRead, ''); assert.equal(h.session.qualified, true);
});

test('Unknown plan part only re-reads the original proof parts and never reconstructs first/retirement/close operations', async t => {
  const h = await context(t); let lose = true;
  h.setIntercept((wire, request, route) => { const reply = route(); if (request.action === 'editor_intent_read' && request.editor_intent_ref.part === 'close' && lose) {
    lose = false; throw new Error('lost close plan read'); } return reply; });
  await assert.rejects(h.recovery.load(), /lost close plan read/); const original = h.recovery.pendingRead;
  assert.equal(JSON.parse(original).editor_intent_ref.part, 'close'); assert.equal(h.recovery.handoff, undefined);
  const before = h.calls.length, result = await h.recovery.retry(); assert.equal(h.calls[before + 2], original);
  assert.equal(result.handoff.originalHandoffLiteral, h.actual.handoff_request.business_handoff.request_json);
  assert.equal(result.handoff.originalRetirement, JSON.stringify(h.actual.retirement_request));
  assert.equal(h.operations, 0); assert.equal(h.recovery.pendingRead, ''); assert.equal(h.session.qualified, true);
});

test('owner changes during actual current reply keep current history known and do not install or overwrite page input', async t => {
  const h = await context(t);
  h.setIntercept((wire, request, route) => { const reply = route(); if (request.action === 'draft_read') h.owner.guard = false; return reply; });
  await assert.rejects(h.recovery.load(), /owner\/input cutoff/);
  assert.equal(h.recovery.unknown, false); assert.equal(h.recovery.currentRecord.generation, '2'); assert.equal(h.recovery.ready, undefined);
  assert.equal(h.recovery.handoff.firstCommitted, false); assert.equal(h.session.committed, true); assert.equal(h.session.qualified, true);
  assert.equal(h.draftCalls.length, 0); assert.equal(h.operations, 0);
});

test('revocation after preparation disposes unclaimed actual writer but preserves original session and exact handoff/history facts', async t => {
  const h = await context(t), ready = await h.recovery.load(); h.owner.guard = false;
  assert.throws(() => h.recovery.claimWriter(), /owner\/input cutoff/); assert.equal(ready.writer.disposed, true);
  assert.equal(h.recovery.disposed, true); assert.equal(h.recovery.ready, undefined); assert.equal(h.recovery.currentRecord.generation, '2');
  assert.equal(h.recovery.parentHistory.operation_id, 'late-S2'); assert.equal(h.recovery.handoff.firstCommitted, true);
  assert.equal(h.session.qualified, true); assert.equal(h.draftCalls.length, 0);
});

test('dispose during readonly current request does not cancel native read or erase its successful late record', async t => {
  const h = await context(t); let release, signal; const entered = new Promise(resolve => signal = resolve);
  h.setIntercept((wire, request, route) => {
    const reply = route(); return request.action === 'draft_read' ? new Promise(resolve => { release = () => resolve(reply); signal(); }) : reply;
  });
  const work = h.recovery.load(); await entered; assert.equal(typeof release, 'function'); h.recovery.dispose(); release();
  await assert.rejects(work, /owner\/input cutoff/); assert.equal(h.recovery.currentRecord.generation, '2');
  assert.equal(h.recovery.ready, undefined); assert.equal(h.recovery.unknown, false); assert.equal(h.session.qualified, true);
});

test('malformed/foreign/inactive/advanced current replies retain fixed read for explicit retry and never return a fake active writer', async t => {
  const changes = [r => r.drafts = [], r => r.drafts.push(plain(r.drafts[0])), r => r.effect = 'committed', r => r.receipt_revision = '2',
    r => r.drafts[0].scope.card_id = 'foreign', r => r.drafts[0].scope.source = '0a02',
    r => r.drafts[0].business_link.plan_operation = 'foreign-plan', r => delete r.drafts[0].business_retirement,
    r => r.drafts[0].current_active = false, r => r.drafts[0].active = false, r => r.drafts[0].current_generation = '3',
    r => r.drafts[0].values.description.selection_extent = 99999, r => r.drafts[0].values.description.text = '\ud800',
    r => r.cards = ['汉'.repeat(180000)]];
  for (const change of changes) {
    const h = await context(t); h.setIntercept((wire, request, route) => { const reply = route(); if (request.action === 'draft_read') change(reply); return reply; });
    await assert.rejects(h.recovery.load()); const original = h.recovery.originalCurrentRead;
    assert.equal(h.recovery.pendingRead, original); assert.equal(h.recovery.ready, undefined); assert.equal(h.session.qualified, true);
    assert.equal(h.operations, 0); assert.equal(h.draftCalls.length, 0); h.setIntercept(undefined);
    const result = await h.recovery.retry(); assert.equal(h.calls.at(-1), original); assert.equal(result.writer.writesPaused, true);
    assert.equal(result.writer.confirmed.active, true);
  }
});

test('read preparation rejects a merely restored unqualified session and overlapping reads without sending a mutation', async t => {
  const unqualified = await context(t, { qualified: false }); const before = unqualified.calls.length;
  assert.throws(() => unqualified.controller(), /original qualified/); assert.equal(unqualified.calls.length, before);
  const h = await context(t); let release, signal; const entered = new Promise(resolve => signal = resolve);
  h.setIntercept((wire, request, route) => {
    const reply = route(); return request.action === 'draft_read' ? new Promise(resolve => { release = () => resolve(reply); signal(); }) : reply;
  });
  const first = h.recovery.load(); await entered; assert.throws(() => h.recovery.load(), /already in flight/);
  release(); const result = await first; assert.equal(result.writer.writesPaused, true); assert.equal(h.draftCalls.length, 0);
});

test('actual saved_exact closed session has no child plan and never invents a parent/writer or labels known absence as a read Unknown', async t => {
  const bytes = fs.readFileSync(path.resolve(__dirname, '../reports/ui-source/v25/editor-handoff-store-fixture.json'));
  assert.equal(sha(bytes), '9af7ccaabaf247bbb06e60cd7302da878f515baa3704495735542bdb783e486f');
  const actual = JSON.parse(bytes).saved_exact, m = shared.model(), calls = [];
  const sender = async wire => {
    calls.push(wire); const request = JSON.parse(wire);
    if (request.action === 'editor_intent_read') return readonlyView(actual.closed_parts[request.editor_intent_ref.part]);
    if (request.action === 'editor_commit_inspect') { assert.equal(wire, actual.original_inspect_transport); return plain(actual.original_inspect_reply); }
    throw new Error('saved_exact recovery cannot invent a draft read or mutation');
  };
  const hooks = { send: sender, fields: new m.EditorFieldPolicy(async () => { throw new Error('No projection reconstruction'); }),
    changed: () => {}, isCurrent: () => true, isExact: () => false, parentReady: () => false };
  const session = await m.EditorBusinessSessionCoordinator.restore(actual.intent_proof, hooks); await session.inspect();
  const handoffHooks = { ...hooks, sendDraft: async () => { throw new Error('No writer or save'); },
    importsReady: () => true, childCurrent: () => false, operation: () => { throw new Error('No new operation'); } };
  const recovery = new m.EditorBusinessRecoveryCoordinator(session, handoffHooks, () => true); t.after(() => recovery.dispose());
  const before = calls.length; await assert.rejects(recovery.load(), /no fixed current child plan/);
  assert.equal(recovery.unknown, false); assert.equal(recovery.pendingRead, ''); assert.equal(recovery.ready, undefined);
  assert.equal(recovery.handoff.closed, true); assert.equal(recovery.handoff.originalClose, JSON.stringify(actual.close_transport));
  assert.equal(recovery.currentRecord, undefined); assert.equal(session.qualified, true);
  assert.equal(calls.slice(before).every(wire => JSON.parse(wire).action === 'editor_intent_read'), true);
});
