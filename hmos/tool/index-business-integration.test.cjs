'use strict';
// Actual extracted Index and actual ETS models/Workbench queue. Controlled
// receiver/worker/view seams do not establish Store, ArkUI IME, SDK or device
// acceptance. Foundation tests keep completion as a seam; the final planned
// restore/close tests explicitly enable the actual Index lifecycle methods.
const fs = require('node:fs'), path = require('node:path');
const assert = require('node:assert/strict'), { test, after } = require('node:test');
const { harness, plain, sha, settle, deferred, actualMethod, sourceSHA, modelPaths, assertSourceUnchanged,
  modelIdentities } = require('./index-business-test-harness.cjs');
const action = wire => JSON.parse(wire).action;
const count = (h, type) => h.calls.filter(wire => action(wire) === type).length;
after(() => { assertSourceUnchanged(); console.log('ACTUAL_INDEX_SOURCE_FREEZE=' + JSON.stringify(modelIdentities())); });

test('fresh actual Index/ETS source identities; controlled receiver boundaries are explicit', () => {
  console.log('Index.ets SHA256=' + sourceSHA);
  for (const file of modelPaths) console.log(path.basename(file) + ' SHA256=' + sha(fs.readFileSync(file)));
  assert.ok(actualMethod('save').includes('ensureConfirmed()'));
  assert.ok(actualMethod('businessHooks').includes('workbench.send(wire)'));
});

test('actual new-card Save persists complete raw before pause and durable intent before business admission', async () => {
  const h = harness(); try {
    await h.page.save(); const session = h.page.businessSession;
    assert.equal(session.qualified, true); assert.equal(session.originalRequest, h.state.submission);
    assert.equal(h.finishes.length, 1); assert.equal(h.finishes[0].session, session);
    const raw = h.events.findIndex(e => e.kind === 'native' && e.action === 'draft_save');
    const pause = h.events.findIndex(e => e.kind === 'pause');
    const prepare = h.events.findIndex(e => e.kind === 'admit' && e.action === 'editor_intent_prepare');
    const save = h.events.findIndex(e => e.kind === 'admit' && e.action === 'editor_save');
    assert.ok(raw >= 0 && raw < pause && pause < prepare && prepare < save);
    assert.equal(count(h, 'create'), 0); assert.equal(count(h, 'edit'), 0);
    assert.equal(JSON.parse(session.originalRequest).business.description, 'S1 原文 汉字 🧪 é.');
  } finally { h.dispose(); }
});

test('actual unchanged existing-card Save establishes publication and preserves source/tasks semantics', async () => {
  const h = harness({ mode: 'edit' }); try {
    assert.equal(h.draft.confirmed, undefined); await h.page.save(); const s = h.page.businessSession;
    assert.equal(s.qualified, true); const request = JSON.parse(s.originalRequest);
    assert.equal(request.mode, 'edit'); assert.equal(request.business.source, '0a02aabb');
    assert.equal(request.business.todos, ''); assert.equal(request.publication.draft_id, h.draft.scope.draft_id);
    assert.equal(count(h, 'draft_save'), 1); assert.equal(count(h, 'editor_save'), 1);
  } finally { h.dispose(); }
});

test('actual first business send synchronously admits Workbench before resuming parent debounce', async () => {
  const h = harness(); try {
    await h.page.save(); const admitted = h.events.findIndex(e => e.kind === 'admit' && e.action === 'editor_save');
    const resumed = h.events.findIndex((e, n) => n > admitted && e.kind === 'resume');
    const native = h.events.findIndex(e => e.kind === 'native' && e.action === 'editor_save');
    assert.ok(admitted >= 0 && admitted < resumed && resumed < native);
    assert.equal(h.draft.writesPaused, false);
  } finally { h.dispose(); }
});

test('actual S2 during raw publication cancels unissued proposal while keeping complete input', async () => {
  const h = harness(); try {
    let changed = false; h.setRawIntercept(async () => { if (!changed) { changed = true; h.adopt('description', 'S2 during raw'); } });
    await h.page.save(); assert.equal(count(h, 'editor_intent_prepare'), 0); assert.equal(count(h, 'editor_save'), 0);
    assert.equal(h.draft.current.description.text, 'S2 during raw'); assert.equal(h.draft.writesPaused, false);
    assert.match(h.page.message, /输入已变化/);
  } finally { h.dispose(); }
});

test('first proved no-write intent prepare does not strand the complete editor behind an unprepared session', async () => {
  const h = harness(); try {
    h.setIntercept((wire, p, route) => p.action === 'editor_intent_prepare' ?
      { ok: false, error: 'controlled active quota', effect: 'not_committed', receipt_revision: '' } : route());
    await h.page.save(); assert.equal(count(h, 'editor_save'), 0); assert.equal(h.draft.unknown, false);
    assert.equal(h.draft.current.description.text, 'S1 原文 汉字 🧪 é.');
    assert.equal(h.page.businessPending(), false, 'proven unprepared no-write proposal must permit correction/new Save');
    assert.equal(h.page.editorOpen, true); assert.equal(h.draft.writesPaused, false);
  } finally { h.dispose(); }
});

test('prepare ACK Unknown stays blocked and a later no-write reply cannot release that original uncertainty', async () => {
  const h = harness(); try {
    let first = true; h.setIntercept((wire, p, route) => { if (p.action === 'editor_intent_prepare') {
      if (first) { first = false; route(); throw new Error('lost prepare ACK'); }
      return { ok: false, error: 'later local rejection', effect: 'not_committed', receipt_revision: '' }; } return route(); });
    await h.page.save(); const s = h.page.businessSession; assert.equal(s.pendingNative, 'prepare');
    assert.equal(h.page.businessPending(), true); await h.page.reconcileBusiness('native');
    assert.equal(h.page.businessSession, s); assert.equal(s.pendingNative, 'prepare'); assert.equal(h.page.businessPending(), true);
    assert.equal(count(h, 'editor_save'), 0);
    assert.deepEqual(h.calls.filter(w => action(w) === 'editor_intent_prepare'), [s.originalPrepare, s.originalPrepare]);
  } finally { h.dispose(); }
});

test('actual Workbench tail orders S1 business before later complete S2 raw persistence', async () => {
  const h = harness(), started = deferred(), release = deferred(); try {
    h.setIntercept(async (wire, p, route) => { if (p.action === 'editor_save') { started.resolve(); await release.promise; } return route(); });
    const saving = h.page.save(); await started.promise;
    assert.equal(h.draft.writesPaused, false); h.adopt('description', 'S2 journaled after S1 admission');
    const rawSave = h.draft.flush(); await settle();
    const rawAdmissions = h.events.filter(e => e.kind === 'admit' && e.action === 'draft_save'); assert.equal(rawAdmissions.length, 2);
    assert.equal(h.events.filter(e => e.kind === 'native' && e.action === 'draft_save').length, 1, 'S2 cannot overtake pending native S1 business');
    release.resolve(); await Promise.all([saving, rawSave]);
    const businessAt = h.events.findIndex(e => e.kind === 'native' && e.action === 'editor_save');
    const secondRawAt = h.events.findIndex((e, n) => n > businessAt && e.kind === 'native' && e.action === 'draft_save');
    assert.ok(secondRawAt > businessAt); assert.equal(h.draft.confirmed.generation, '2');
    assert.equal(h.draft.confirmed.values.description.text, 'S2 journaled after S1 admission');
    assert.equal(h.page.businessSession.publication.generation, '1'); assert.equal(h.page.businessSession.qualified, true);
    assert.equal(h.page.businessSession.mayConsume(h.draft.current), false);
  } finally { release.resolve(); h.dispose(); }
});

test('issued-before-first-send S2 blocks auto-save; explicit read-only restore retries fixed S1 literal', async () => {
  const h = harness(); try {
    let changed = false; h.setIntercept((wire, p, route) => { const reply = route();
      if (p.action === 'editor_intent_read' && p.editor_intent_ref.part === 'inspect' && !changed) {
        changed = true; h.adopt('description', 'S2 retained after issue'); }
      return reply; });
    await h.page.save(); const original = h.page.businessSession.originalRequest, registered = h.state.save;
    assert.equal(h.page.businessSession.phase, 'issued'); assert.equal(h.page.businessSession.businessUnknown, false);
    assert.equal(count(h, 'editor_save'), 0); assert.equal(h.draft.current.description.text, 'S2 retained after issue');
    await h.page.reconcileBusiness('save'); assert.equal(count(h, 'editor_save'), 1);
    assert.equal(h.calls.filter(w => action(w) === 'editor_save')[0], registered);
    assert.equal(h.page.businessSession.originalRequest, original); assert.equal(h.page.businessSession.qualified, true);
    assert.equal(h.draft.current.description.text, 'S2 retained after issue'); assert.equal(h.page.businessSession.mayConsume(h.draft.current), false);
    assert.ok(h.calls.slice(h.calls.findIndex(w => action(w) === 'editor_intent_read')).filter(w => action(w) === 'editor_intent_read').length >= 7);
  } finally { h.dispose(); }
});

test('actual selection-only S2 after issue cannot be consumed by same-text S1 success', async () => {
  const h = harness(); try {
    h.setIntercept((wire, p, route) => { if (p.action === 'editor_save') h.page.draftSelectionChanged('title', 1, 1); return route(); });
    await h.page.save(); assert.equal(h.page.businessSession.qualified, true);
    assert.equal(h.draft.current.title.text, '原完整标题'); assert.equal(h.draft.current.title.selection_base, 1);
    assert.equal(h.page.businessSession.mayConsume(h.draft.current), false); assert.equal(h.page.editorOpen, true);
  } finally { h.dispose(); }
});

test('actual first save Unknown retries same session and literal without reconstructing current S2', async () => {
  const h = harness(); try {
    let lost = false; h.setIntercept((wire, p, route) => { const reply = route(); if (p.action === 'editor_save' && !lost) {
      lost = true; h.adopt('description', 'S2 while Unknown'); throw new Error('lost save ACK'); } return reply; });
    await h.page.save(); const session = h.page.businessSession, original = session.originalSave;
    assert.equal(session.businessUnknown, true); await h.page.reconcileBusiness('save');
    assert.equal(h.page.businessSession, session); assert.deepEqual(h.calls.filter(w => action(w) === 'editor_save'), [original, original]);
    assert.equal(session.qualified, true); assert.equal(h.draft.current.description.text, 'S2 while Unknown');
  } finally { h.dispose(); }
});

test('actual committed effect with invalid DTO remains known through attempted save and failed inspection', async () => {
  const h = harness(); try {
    h.setIntercept((wire, p, route) => { const reply = route(); if (p.action === 'editor_save') delete reply.editor_commit;
      if (p.action === 'editor_commit_inspect') throw new Error('inspection unavailable'); return reply; });
    await h.page.save(); const s = h.page.businessSession; assert.equal(s.committed, true); assert.equal(s.qualified, false);
    const before = count(h, 'editor_save'); await h.page.reconcileBusiness('save');
    assert.equal(count(h, 'editor_save'), before); assert.equal(h.page.businessSession, s); assert.equal(s.committed, true);
    await h.page.reconcileBusiness('inspect'); assert.equal(h.page.businessSession, s); assert.equal(s.committed, true);
  } finally { h.dispose(); }
});

test('actual list and five-part restore are read-only; same-card different draft does not bind consumption', async () => {
  const h = harness(); try {
    h.setIntercept((wire, p, route) => { const reply = route(); if (p.action === 'editor_save') throw new Error('save reply unknown'); return reply; });
    await h.page.save(); const view = h.metadataView(); h.page.businessSession = undefined; h.page.businessBinding = undefined;
    const foreignScope = h.draft.scope; foreignScope.draft_id += '-foreign';
    const record = plain(h.draft.confirmed); record.scope = foreignScope;
    const foreign = new h.m.EditorDraftCoordinator(foreignScope, record.values, async () => { throw new Error('unexpected foreign raw write'); },
      () => {}, () => 'foreign-operation', record);
    h.page.editorDraft = foreign; h.page.editorValues = h.m.copyValues(record.values);
    const before = count(h, 'editor_save'); await h.page.loadBusinessIntents(); await h.page.restoreBusinessIntent(view);
    assert.equal(count(h, 'editor_save'), before); assert.equal(h.page.businessBinding, undefined);
    assert.equal(h.page.businessSession.mayConsume(foreign.current), false); foreign.dispose();
  } finally { h.dispose(); }
});

test('actual failed partial restoration leaves original discovery reference and never auto-replays', async () => {
  const h = harness(); try {
    h.setIntercept((wire, p, route) => { const reply = route(); if (p.action === 'editor_save') throw new Error('unknown'); return reply; });
    await h.page.save(); const view = h.metadataView(); h.page.businessSession = undefined; h.page.businessBinding = undefined;
    h.setIntercept((wire, p, route) => { if (p.action === 'editor_intent_read' && p.editor_intent_ref.part === 'publication') throw new Error('partial read'); return route(); });
    const before = count(h, 'editor_save'); await h.page.restoreBusinessIntent(view);
    assert.equal(h.page.businessSession, undefined); assert.equal(h.page.businessRecoveryIdentity, view.proof.intent_id);
    assert.equal(count(h, 'editor_save'), before); assert.match(h.page.message, /未读完整/);
  } finally { h.dispose(); }
});

test('actual old submit rejects create/edit and unfinished strict session blocks ordinary mutation/discard/close', async () => {
  const h = harness(); try {
    for (const type of ['create', 'edit']) await assert.rejects(h.page.submit(h.page.command(type)), /完整原请求日志/);
    assert.equal(h.calls.length, 0);
    h.setIntercept((wire, p, route) => { const reply = route(); if (p.action === 'editor_save') throw new Error('unknown'); return reply; });
    await h.page.save(); const before = h.calls.length;
    await h.page.submit(h.page.command('favorite')); h.page.discardDraft(); await h.page.discardDraftConfirmed(); h.page.closeEditor();
    assert.equal(h.calls.length, before); assert.equal(h.page.editorOpen, true); assert.equal(h.draft.disposed, false);
    assert.equal(h.events.filter(e => e.kind === 'alert').length, 0);
  } finally { h.dispose(); }
});

test('actual ordinary task command remains separate from strict create/edit transport', async () => {
  const h = harness({ mode: 'edit' }); try {
    const command = h.page.command('task_complete'); command.task_id = 'task-fixture'; command.flag = true;
    await h.page.submit(command); assert.equal(count(h, 'task_complete'), 1); assert.equal(count(h, 'editor_save'), 0);
    assert.equal(count(h, 'editor_intent_prepare'), 0); assert.equal(h.page.businessSession, undefined);
  } finally { h.dispose(); }
});

test('actual Store DTO fixture through Index restore/inspect preserves original wire and full historical source', async () => {
  const fixturePath = path.resolve(__dirname, '../reports/ui-source/v24/editor-intent-store-fixture.json');
  const raw = fs.readFileSync(fixturePath), fixture = JSON.parse(raw); assert.equal(fixture.producer, 'fresh actual Store');
  assert.equal(sha(raw), '9c8bd71ee6c42ad965a129973c5943f62a38261eef99310fa3d12af99a9999bb');
  const h = harness(); try {
    h.loadFixture(fixture); h.page.editorOpen = false; h.page.editorDraft = undefined;
    await h.page.restoreBusinessIntent(fixture.issue_reply.editor_intents[0]); assert.equal(count(h, 'editor_save'), 0);
    assert.equal(h.page.businessSession.originalRequest, fixture.request_json);
    await h.page.reconcileBusiness('inspect'); assert.equal(h.page.businessSession.qualified, true);
    assert.deepEqual(plain(h.page.businessSession.confirmed), fixture.inspect_reply.editor_commit);
    assert.equal(h.calls.filter(w => action(w) === 'editor_commit_inspect')[0], fixture.parts.inspect.editor_intents[0].inspect_request_json);
    assert.equal(h.page.businessSession.confirmed.historical_card.source, fixture.inspect_reply.editor_commit.historical_card.source);
  } finally { h.dispose(); }
});

async function registeredClose(h, disposition = 'saved_exact') {
  assert.equal(disposition, 'saved_exact');
  await h.page.save(); const session = h.page.businessSession, binding = h.page.businessBinding;
  h.draft.pauseWrites();
  const prepared = h.m.EditorBusinessHandoffCoordinator.savedExact(session, h.draft, 'fixed-close-op', 'fixed-discard-op',
    h.page.businessHandoffHooks(binding, binding.values, binding.epoch, true));
  // Simulate a native registration surviving restart, including noncanonical
  // stored literal whitespace. Restoration must preserve every inner byte.
  h.state.close = ' ' + JSON.parse(prepared.originalClose).editor_intent_close.request_json + '\n';
  h.state.phase = 'close_planned'; h.state.generation = '3'; h.state.disposition = disposition;
  session.adoptIntentView(h.metadataView()); h.enableActualLifecycle();
  return { session, binding, literal: h.state.close };
}

test('actual planned saved_exact restore reads fixed native close and never invents or dispatches another operation', async () => {
  const h = harness(); try {
    const { session, binding, literal } = await registeredClose(h), before = h.calls.length;
    await h.page.finishBusinessInput(session, binding); const restored = h.page.businessHandoff;
    assert.ok(restored instanceof h.m.EditorBusinessHandoffCoordinator); assert.equal(restored.pendingNative, 'close');
    assert.equal(JSON.parse(restored.originalClose).editor_intent_close.request_json, literal);
    assert.equal(h.page.businessCloseExact, true); assert.equal(h.page.businessHandoffCloseOperation, '');
    assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).editor_intent_ref.part), ['handoff', 'close']);
    assert.equal(count(h, 'editor_intent_close'), 0); assert.equal(count(h, 'draft_continue_business'), 0);
    assert.equal(h.page.editorOpen, true); assert.equal(h.draft.writesPaused, true);
    assert.equal(session.qualified, true); assert.equal(session.committed, true);
  } finally { h.dispose(); }
});

test('actual recovered saved_exact close rejects complete S2 before detach or fixed close replay', async () => {
  const h = harness(); try {
    const { session, binding } = await registeredClose(h); h.adopt('description', 'S2 after fixed close plan');
    await h.page.finishBusinessInput(session, binding); const restored = h.page.businessHandoff, before = h.calls.length;
    await h.page.reconcileBusinessHandoff(); assert.equal(h.calls.length, before);
    assert.equal(h.page.businessHandoff, restored); assert.equal(restored.pendingNative, 'close');
    assert.equal(h.events.filter(e => e.kind === 'controlled-view-detach').length, 0);
    assert.equal(h.draft.current.description.text, 'S2 after fixed close plan'); assert.equal(h.page.editorOpen, true);
    assert.equal(h.draft.disposed, false); assert.equal(session.qualified, true); assert.match(h.page.message, /较新输入/);
  } finally { h.dispose(); }
});

test('actual explicit recovered exact close detaches controlled lease before retrying original close literal', async () => {
  const h = harness(); try {
    const { session, binding, literal } = await registeredClose(h); await h.page.finishBusinessInput(session, binding);
    const original = h.page.businessHandoff.originalClose; await h.page.reconcileBusinessHandoff();
    assert.deepEqual(h.calls.filter(w => action(w) === 'editor_intent_close'), [original]);
    assert.equal(JSON.parse(original).editor_intent_close.request_json, literal);
    const detached = h.events.findIndex(e => e.kind === 'controlled-view-detach');
    const admitted = h.events.findIndex(e => e.kind === 'admit' && e.action === 'editor_intent_close');
    assert.ok(detached >= 0 && detached < admitted); assert.equal(h.draft.disposed, true);
    assert.equal(h.page.editorOpen, false); assert.equal(session.qualified, true); assert.equal(session.phase, 'closed');
    assert.equal(count(h, 'draft_continue_business'), 0); assert.equal(count(h, 'draft_discard'), 0);
  } finally { h.dispose(); }
});

test('actual planned handoff restore preserves fixed child plan and stays read-only until explicit reconciliation', async () => {
  const h = harness(); try {
    await h.page.save(); const session = h.page.businessSession, binding = h.page.businessBinding;
    h.adopt('description', 'S2 original fixed child plan'); h.draft.pauseWrites();
    const childScope = new h.m.DraftScope(); Object.assign(childScope, { card_id: session.confirmed.card_id,
      draft_id: 'fixed-child-id', source_kind: 0, source_revision: session.confirmed.revision, source: session.confirmed.historical_card.source });
    const prepared = await h.m.EditorBusinessHandoffCoordinator.prepare(session, h.draft, childScope, h.draft.current,
      'fixed-plan-op', 'fixed-first-op', 'fixed-retire-op', h.page.businessHandoffHooks(binding, h.draft.current, h.page.editorInputEpoch));
    h.state.handoff = ' ' + prepared.originalHandoffLiteral + '\n';
    h.registerRetirement();
    h.state.phase = 'handoff_planned'; h.state.generation = '3'; session.adoptIntentView(h.metadataView());
    h.enableActualLifecycle(); const before = h.calls.length; await h.page.finishBusinessInput(session, binding);
    const restored = h.page.businessHandoff; assert.ok(restored instanceof h.m.EditorBusinessHandoffCoordinator);
    assert.equal(restored.originalHandoffLiteral, h.state.handoff); assert.equal(restored.scope.draft_id, 'fixed-child-id');
    assert.equal(restored.scope.source, session.confirmed.historical_card.source); assert.equal(restored.pendingNative, 'handoff');
    assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).editor_intent_ref.part), ['handoff', 'retirement', 'close']);
    assert.equal(count(h, 'draft_continue_business'), 0); assert.equal(count(h, 'draft_continue_business_retire'), 0);
    assert.equal(count(h, 'editor_intent_close'), 0); assert.equal(h.page.businessCloseExact, false);
    assert.equal(h.page.editorDraft, h.draft); assert.equal(h.draft.current.description.text, 'S2 original fixed child plan');
  } finally { h.dispose(); }
});

test('actual fresh exact S1 save runs actual close lifecycle after controlled lease detach', async () => {
  const h = harness(); try {
    h.enableActualLifecycle(); await h.page.save(); const session = h.page.businessSession;
    assert.equal(session.qualified, true); assert.equal(session.phase, 'closed'); assert.equal(h.page.editorOpen, false);
    assert.equal(h.draft.disposed, true); assert.equal(count(h, 'editor_save'), 1); assert.equal(count(h, 'editor_intent_close'), 1);
    assert.equal(count(h, 'draft_continue_business'), 0); assert.equal(count(h, 'draft_discard'), 0);
    const close = JSON.parse(JSON.parse(h.calls.find(w => action(w) === 'editor_intent_close')).editor_intent_close.request_json);
    assert.equal(close.disposition, 'saved_exact'); assert.equal(close.parent.proof.draft_id, h.draft.scope.draft_id);
    const detach = h.events.findIndex(e => e.kind === 'controlled-view-detach');
    const closeAdmitted = h.events.findIndex(e => e.kind === 'admit' && e.action === 'editor_intent_close');
    assert.ok(detach >= 0 && detach < closeAdmitted);
  } finally { h.dispose(); }
});

test('actual fresh late S2 then S3 uses one real child writer, confirms latest raw before retirement and fixed close', async () => {
  const h = harness(); try {
    h.enableActualLifecycle(); h.setIntercept((wire, p, route) => { const reply = route();
      if (p.action === 'editor_save') h.adopt('description', 'S2 child proposal raw');
      if (p.action === 'draft_continue_business') h.adopt('title', 'S3 latest complete title 🧪');
      return reply; });
    await h.page.save(); const session = h.page.businessSession, child = h.page.editorDraft;
    assert.ok(child instanceof h.m.EditorDraftCoordinator); assert.notEqual(child, h.draft);
    assert.equal(session.qualified, true); assert.equal(session.phase, 'closed'); assert.equal(h.page.editorOpen, true);
    assert.equal(child.scope.source, session.confirmed.historical_card.source); assert.equal(child.scope.source_revision, session.confirmed.revision);
    assert.equal(child.current.description.text, 'S2 child proposal raw'); assert.equal(child.current.title.text, 'S3 latest complete title 🧪');
    assert.equal(child.confirmed.values.title.text, 'S3 latest complete title 🧪'); assert.equal(child.confirmed.generation, '2');
    assert.equal(child.dirty, false); assert.equal(h.draft.disposed, true); assert.equal(h.page.ownedTodosBaseline, session);
    assert.equal(count(h, 'editor_save'), 1); assert.equal(count(h, 'draft_continue_business'), 1);
    assert.equal(count(h, 'draft_continue_business_retire'), 1); assert.equal(count(h, 'editor_intent_close'), 1);
    const proposal = JSON.parse(h.state.handoff); assert.equal(proposal.child.values.title.text, '原完整标题');
    assert.equal(proposal.child.values.description.text, 'S2 child proposal raw');
    const types = h.calls.map(action), handoff = types.indexOf('draft_continue_business'), raw = types.indexOf('draft_save', handoff);
    const retire = types.indexOf('draft_continue_business_retire'), close = types.indexOf('editor_intent_close');
    assert.ok(handoff >= 0 && handoff < raw && raw < retire && retire < close);
    assert.equal(h.page.businessHandoff, undefined); assert.equal(count(h, 'create'), 0); assert.equal(count(h, 'edit'), 0);
  } finally { h.dispose(); }
});
