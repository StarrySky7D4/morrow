'use strict';
// Runs actual Handoff/Session/Business/Draft ETS. The shared Session receiver
// below is controlled transport evidence, not Store or device qualification.
// Real Store-produced DTO coverage is added separately using the native fixture.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), { test } = require('node:test'), crypto = require('node:crypto');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
// Reuse only the existing test's load/transport harness. Its assertions and
// test registrations are not evaluated; production logic is loaded from ETS.
const sharedSource = fs.readFileSync(path.join(__dirname, 'editor-business-session-model.test.cjs'), 'utf8');
const marker = sharedSource.indexOf('const action = wire =>'); assert.ok(marker > 0);
const shared = {};
vm.runInNewContext(sharedSource.slice(0, marker)
  .replace("...load('EditorBusinessSession')", "...load('EditorBusinessSession'), ...load('EditorBusinessHandoff')")
  .replace('{ exports, require: load, Uint8Array }', '{ exports, require: load, Uint8Array, setTimeout, clearTimeout }') +
  '\nObject.assign(shared, { model, harness });', { require, __dirname, process, Buffer, TextEncoder, shared, setTimeout, clearTimeout });
const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };

async function harness(t, { dirty = true, assets = ['A', 'B'] } = {}) {
  const base = shared.harness({ assets }), m = base.m, session = await base.ready(); await session.save();
  const parentCalls = [], childCalls = [], calls = [], owner = { current: true, exact: true, imports: true, installed: null };
  const parentRecord = m.copyRecord(base.record);
  const parent = new m.EditorDraftCoordinator(parentRecord.scope, parentRecord.values,
    wire => new Promise((resolve, reject) => parentCalls.push({ wire, resolve, reject })), () => {}, () => 'unexpected-parent-save', parentRecord);
  parent.pauseWrites(); if (dirty) {
    const latest = parent.current; latest.description.text = '晚 S2 🧪 é. 全原文'; latest.description.selection_base = 5;
    latest.description.selection_extent = 1; latest.description.directional = true; latest.description.affinity = 1;
    latest.description.composing_start = 0; latest.description.composing_end = 1;
    latest.todos.text = '甲\n甲\n乙\nS2\n'; latest.todos.selection_base = 4; latest.todos.selection_extent = 0;
    latest.category = '项目'; latest.stage = '进行中'; latest.assets = latest.assets.slice(-1); parent.update(latest);
  }
  const commit = session.confirmed, scope = new m.DraftScope(); Object.assign(scope, { card_id: commit.card_id,
    draft_id: 'business-child-1', source_kind: 0, source_revision: commit.revision, source: commit.historical_card.source });
  const state = { phase: 'issued', generation: '2', disposition: '', literal: '', retirement: '', close: '', first: undefined, current: undefined,
    parentRetired: undefined, firstWriteCount: 0, retirementWriteCount: 0, closeWriteCount: 0 };
  let intercept, draftIntercept, opened, changes = 0;
  function view(part = 'summary') {
    const result = base.view(part); Object.assign(result, { phase: state.phase, current_generation: state.generation,
      current_active: state.phase !== 'closed', close_disposition: state.disposition,
      close_request_json: part === 'close' ? state.close : '', handoff_request_json: part === 'handoff' ? state.literal : '',
      retirement_request_json: part === 'retirement' ? state.retirement : '' }); return result;
  }
  function metadata(part = 'summary', effect = 'not_committed', receipt = '') {
    return { ok: true, error: '', effect, receipt_revision: receipt, editor_intents: [view(part)], intent_next_after: '' };
  }
  function link(proposal) {
    return Object.assign(new m.DraftBusinessLink(), { parent: plain(proposal.parent), intent: plain(proposal.intent),
      plan_operation: proposal.plan_operation, handoff_request_sha256: sha(state.literal), child_operation: proposal.child.operation_id,
      committed_operation: commit.operation, committed_revision: commit.revision, command_sha256: commit.command_sha256,
      content_sha256: commit.content_sha256, request_sha256: commit.request_sha256, publication_sha256: commit.publication_sha256 });
  }
  function firstRecord(proposal) {
    const write = proposal.child, result = m.copyRecord(parentRecord); result.scope = {
      card_id: write.card_id, draft_id: write.draft_id, source_kind: 0, source_revision: commit.revision, source: commit.historical_card.source };
    Object.assign(result, { generation: '1', current_generation: '1', operation_id: write.operation_id,
      request_sha256: sha(JSON.stringify(write)), fork_link: null, fork_retirement: null, business_link: plain(link(proposal)), business_retirement: null,
      values: { ...plain(write.values), assets: plain(write.assets) }, assets: write.assets.map((selection, index) => {
        const old = parentRecord.assets.find(pin => pin.selection.asset_id === selection.asset_id);
        return { ...plain(old), selection: plain(selection), pin_id: 'draft-asset-' + index };
      }), consumed_imports: [] }); return result;
  }
  function route(wire) {
    const p = JSON.parse(wire);
    if (p.action === 'editor_intent_read') return metadata(p.editor_intent_ref.part);
    if (p.action === 'draft_continue_business') {
      const literal = p.business_handoff.request_json, proposal = JSON.parse(literal);
      if (state.literal) assert.equal(literal, state.literal); else {
        state.literal = literal; state.phase = 'handoff_planned'; state.generation = '3'; state.first = firstRecord(proposal);
        state.current = plain(state.first); state.firstWriteCount++;
        state.retirement = ' {"child_request_sha256":' + JSON.stringify(state.first.request_sha256) + ',' +
          JSON.stringify({ schema_version: 1, intent: proposal.intent, plan_operation: proposal.plan_operation,
            handoff_request_sha256: sha(literal), parent: proposal.parent, child_draft_id: proposal.child.draft_id,
            child_operation: proposal.child.operation_id, operation_id: proposal.retirement_operation }).slice(1) + '\n';
      }
      const result = metadata('summary', 'committed', '1'); result.drafts = [plain(state.first)];
      result.drafts[0].current_generation = state.current.current_generation; result.drafts[0].current_active = state.current.current_active; return result;
    }
    if (p.action === 'draft_continue_business_retire') {
      assert.equal(p.business_retirement.request_json, state.retirement);
      if (!state.parentRetired) {
        const proposal = JSON.parse(state.literal), record = m.copyRecord(parentRecord);
        Object.assign(record, { active: false, current_active: false, generation: m.nextGeneration(parentRecord.generation),
          current_generation: m.nextGeneration(parentRecord.generation), business_retirement: {
            schema_version: 1, child_draft_id: scope.draft_id, operation_id: proposal.retirement_operation, business_link: plain(link(proposal)) } });
        state.parentRetired = record; state.retirementWriteCount++;
      }
      const result = metadata('summary', 'committed', state.parentRetired.generation); result.drafts = [plain(state.parentRetired)]; return result;
    }
    if (p.action === 'editor_intent_close') {
      const literal = p.editor_intent_close.request_json, close = JSON.parse(literal);
      if (state.close) assert.equal(literal, state.close); else { state.close = literal; state.closeWriteCount++; }
      state.disposition = close.disposition; state.phase = 'closed'; state.generation = close.disposition === 'saved_exact' ? '4' : '5';
      return metadata('summary', 'committed', state.generation);
    }
    throw new Error('Unexpected handoff receiver action ' + p.action);
  }
  const hooks = { changed: () => { changes++; }, operation: () => 'child-normal-' + (childCalls.length + 1),
    isCurrent: () => owner.current, isExact: () => owner.exact, importsReady: () => owner.imports,
    childCurrent: child => child === owner.installed,
    async send(wire) { const p = JSON.parse(wire); calls.push(wire); return intercept ? intercept(wire, p, () => route(wire)) : route(wire); },
    async sendDraft(wire) {
      childCalls.push(wire); const p = JSON.parse(wire), write = p.draft;
      const routeDraft = () => {
        const result = m.copyRecord(state.current); Object.assign(result, { generation: m.nextGeneration(write.expected_generation),
          current_generation: m.nextGeneration(write.expected_generation), operation_id: write.operation_id, request_sha256: sha(wire),
          values: { ...plain(write.values), assets: plain(write.assets) }, assets: write.assets.map((selection, index) => {
            const old = result.assets.find(pin => pin.selection.asset_id === selection.asset_id);
            return { ...plain(old), selection: plain(selection), pin_id: 'draft-asset-' + index };
          }) }); state.current = plain(result); return result;
      };
      return draftIntercept ? draftIntercept(wire, p, routeDraft) : routeDraft();
    } };
  const freeze = async (firstRaw = parent.current, childScope = scope, ops = ['handoff-plan-1', 'child-first-1', 'parent-retire-1']) =>
    m.EditorBusinessHandoffCoordinator.prepare(session, parent, childScope, firstRaw, ...ops, hooks);
  const open = (model, latest = parent.current, guard = () => owner.current) => { opened = model.openChild(latest, guard); owner.installed = opened; return opened; };
  const ready = async () => { const handoff = await freeze(); await handoff.begin(); await handoff.loadRetirement(); const child = open(handoff);
    await child.flush(); return { handoff, child }; };
  t.after(() => { parent.dispose(); opened?.dispose(); owner.installed?.dispose(); });
  return { base, m, parent, parentRecord, session, scope, state, owner, calls, childCalls, parentCalls, hooks, freeze, open, ready, view, metadata,
    setIntercept(fn) { intercept = fn; }, setDraftIntercept(fn) { draftIntercept = fn; }, get changes() { return changes; } };
}

test('actual handoff freezes complete late S2, exact historical source, ordered confirmed subset and independent identities', async t => {
  const h = await harness(t), latest = h.parent.current, model = await h.freeze(); assert.equal(h.calls.length, 0);
  const frozen = model.originalHandoff, request = JSON.parse(JSON.parse(frozen).business_handoff.request_json);
  assert.equal(request.child.source, undefined); assert.equal(request.child.values.assets, undefined);
  assert.equal(request.child.source_kind, 0); assert.equal(request.child.source_revision, h.session.confirmed.revision);
  assert.equal(request.child.values.description.text, latest.description.text); assert.equal(request.child.values.description.composing_start, 0);
  assert.deepEqual(request.child.assets, plain(latest.assets.map(a => ({ ...a, origin: 4 }))));
  latest.description.text = 'caller mutation'; model.firstRaw.todos.text = 'getter mutation'; h.scope.source = 'ffff';
  assert.equal(model.originalHandoff, frozen); const record = await model.begin();
  assert.equal(record.scope.source, h.session.confirmed.historical_card.source); assert.equal(h.parent.writesPaused, true);
  assert.equal(h.parentCalls.length, 0); assert.equal(model.firstCommitted, true); assert.equal(model.retirementCommitted, false);
});

test('late full S3 is installed in a real child coordinator and saved separately without replacing it with first S2', async t => {
  const h = await harness(t), model = await h.freeze(); let release;
  h.setIntercept((w, p, route) => { const r = route(); return new Promise(resolve => { release = () => resolve(r); }); });
  const first = model.begin(); await settle(); const latest = h.parent.current;
  latest.title.text = 'S3 完整汉字 🧪'; latest.title.selection_base = 3; latest.title.selection_extent = 1;
  latest.todos.text += 'S3\n'; latest.description.composing_end = 4; h.parent.update(latest); h.owner.exact = false;
  release(); await first; h.setIntercept(undefined); const child = h.open(model);
  assert.equal(child.current.title.text, latest.title.text); assert.equal(child.confirmed.values.title.text, h.parentRecord.values.title.text);
  assert.equal(child.dirty, true); await child.flush(); const normal = JSON.parse(h.childCalls[0]).draft;
  assert.equal(normal.values.title.text, latest.title.text); assert.equal(normal.values.description.composing_end, 4);
  assert.equal(normal.assets[0].origin, 3); assert.deepEqual(plain(child.confirmed.business_link), plain(model.confirmed.business_link));
  assert.equal(child.dirty, false); assert.equal(h.parent.current.title.text, latest.title.text); assert.equal(h.parentCalls.length, 0);
});

test('Unknown first uses one fixed literal after owner revocation; known ACK is retained but cannot install into replacement owner', async t => {
  const h = await harness(t), model = await h.freeze(); let lose = true;
  h.setIntercept((w, p, route) => { const r = route(); if (lose) { lose = false; throw new Error('lost first ACK'); } return r; });
  await assert.rejects(model.begin(), /lost first/); assert.equal(model.pendingNative, 'handoff'); const original = h.calls[0];
  h.owner.current = h.owner.exact = false; await model.retry(); assert.equal(h.calls[1], original); assert.equal(h.state.firstWriteCount, 1);
  assert.equal(model.firstCommitted, true); assert.equal(model.confirmed.generation, '1'); assert.throws(() => h.open(model), /owner/);
});

test('committed effect survives malformed DTO without qualifying first child or replaying a business request', async t => {
  const h = await harness(t), model = await h.freeze(); h.setIntercept((w, p, route) => { const r = route(); delete r.drafts; return r; });
  await assert.rejects(model.begin()); assert.equal(model.firstCommitted, true); assert.equal(model.confirmed, undefined);
  assert.equal(model.pendingNative, 'handoff'); assert.equal(h.session.qualified, true); assert.equal(h.session.committed, true);
  h.setIntercept(() => ({ ok: false, error: 'history unavailable', effect: 'not_committed', receipt_revision: '' }));
  await assert.rejects(model.retry()); assert.equal(model.firstCommitted, true); assert.equal(model.pendingNative, 'handoff');
});

test('first complete DTO rejects altered raw, source, immutable link, pin bytes, origin, aliases and reply identity', async t => {
  const changes = [r => r.drafts[0].scope.source = 'ffff', r => r.drafts[0].values.todos.text += 'foreign',
    r => r.drafts[0].business_link.intent.request_sha256 = 'b'.repeat(64), r => r.drafts[0].business_link.parent.generation = '2',
    r => r.drafts[0].business_link.handoff_request_sha256 = 'b'.repeat(64), r => r.drafts[0].business_link.command_sha256 = 'a'.repeat(64),
    r => r.drafts[0].assets[0].sha256 = 'a'.repeat(64), r => r.drafts[0].values.assets[0].aliases = ['foreign'],
    r => r.drafts[0].values.assets[0].origin = 3, r => r.drafts[0].generation = '2',
    r => r.editor_intents[0].proof.prepare_operation = 'foreign', r => r.effect = 'not_committed',
    r => r.receipt_revision = '2', r => r.drafts[0].business_link.extra = true,
    r => r.drafts[0].consumed_imports = ['foreign-import'],
    r => Object.assign(r.editor_intents[0], { phase: 'closed', current_generation: '4', current_active: false, close_disposition: 'saved_exact' }) ];
  for (const change of changes) {
    const h = await harness(t), model = await h.freeze(); h.setIntercept((w, p, route) => { const r = route(); change(r); return r; });
    await assert.rejects(model.begin()); assert.equal(model.confirmed, undefined); assert.equal(model.pendingNative, 'handoff');
    assert.throws(() => h.open(model)); assert.equal(h.session.qualified, true);
  }
});

test('historical first with advanced or inactive current state is retained and never resurrected; explicit current read may restore its actual writer', async t => {
  for (const active of [true, false]) {
    const h = await harness(t), model = await h.freeze(); h.setIntercept((w, p, route) => {
      const r = route(); r.drafts[0].current_generation = '2'; r.drafts[0].current_active = active;
      h.state.current.current_generation = '2'; h.state.current.current_active = active; return r;
    });
    await assert.rejects(model.begin(), /current child changed/); assert.equal(model.confirmed.generation, '1'); assert.equal(model.firstCommitted, true);
    assert.equal(model.conflicted, true); assert.throws(() => h.open(model));
    const current = plain(h.state.current); Object.assign(current, { generation: '2', operation_id: 'actual-current-S3', request_sha256: '9'.repeat(64) });
    current.values.assets.forEach((a, i) => { a.origin = 3; current.assets[i].selection.origin = 3; });
    if (active) { const child = model.openCurrentChild(current, current.values, () => true); h.owner.installed = child;
      assert.equal(child.confirmed.generation, '2'); assert.equal(child.dirty, false); }
    else assert.throws(() => model.openCurrentChild(current, current.values, () => true), /active/);
    assert.equal(h.state.firstWriteCount, 1); assert.equal(h.parentCalls.length, 0);
  }
});

test('native retirement literal is read verbatim including key order/spacing; metadata plan is not a retirement effect', async t => {
  const h = await harness(t), model = await h.freeze(); await model.begin(); const before = h.calls.length;
  await model.loadRetirement(); assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).editor_intent_ref.part), ['handoff', 'retirement']);
  assert.equal(JSON.parse(model.originalRetirement).business_retirement.request_json, h.state.retirement);
  assert.equal(model.retirementCommitted, false); assert.equal(h.state.retirementWriteCount, 0);
  assert.throws(() => model.retire(), /child raw/); const child = h.open(model); await child.flush(); await model.retire();
  assert.equal(h.calls.at(-1), model.originalRetirement); assert.equal(model.retired.active, false); assert.equal(child.confirmed.active, true);
});

test('fixed plan read rejects substitution and Unknown retries that exact read without creating a new child or plan', async t => {
  const h = await harness(t), model = await h.freeze(); await model.begin(); let lose = true;
  h.setIntercept((w, p, route) => { const r = route(); if (p.editor_intent_ref?.part === 'handoff' && lose) { lose = false; throw new Error('lost plan read'); } return r; });
  await assert.rejects(model.loadRetirement(), /lost plan/); const fixed = model.pendingRead; assert.equal(model.pendingNative, '');
  await model.retryRead(); assert.equal(h.calls.at(-1), fixed); assert.equal(model.pendingRead, '');
  h.setIntercept((w, p, route) => { const r = route(); if (p.editor_intent_ref?.part === 'retirement') {
    const x = JSON.parse(r.editor_intents[0].retirement_request_json); x.child_request_sha256 = 'a'.repeat(64);
    r.editor_intents[0].retirement_request_json = JSON.stringify(x); } return r; });
  await assert.rejects(model.loadRetirement(), /retirement plan/); assert.equal(model.originalRetirement, ''); assert.equal(h.state.firstWriteCount, 1);
});

test('retirement requires the actual installed child and latest full raw confirmation, including selection/composition generations', async t => {
  const h = await harness(t), model = await h.freeze(); await model.begin(); await model.loadRetirement(); const child = h.open(model);
  assert.throws(() => model.retire(), /raw\/writer/); await child.flush(); h.owner.installed = null;
  assert.throws(() => model.retire(), /raw\/writer/); h.owner.installed = child;
  const latest = child.current; latest.title.selection_base = 2; child.update(latest); assert.throws(() => model.retire(), /raw\/writer/);
  await child.flush(); await model.retire(); assert.equal(h.state.retirementWriteCount, 1); assert.equal(model.retirementCommitted, true);
});

test('retirement Unknown preserves fixed literal and known business/child, explicit retry after owner revocation settles only original parent', async t => {
  const h = await harness(t), { handoff: model, child } = await h.ready(); let lose = true;
  h.setIntercept((w, p, route) => { const r = route(); if (p.action.endsWith('_retire') && lose) { lose = false; throw new Error('lost retire'); } return r; });
  await assert.rejects(model.retire(), /lost retire/); const fixed = model.originalRetirement; h.owner.current = false;
  await model.retryRetire(); assert.equal(h.calls.at(-1), fixed); assert.equal(h.state.retirementWriteCount, 1);
  assert.equal(model.retirementCommitted, true); assert.equal(model.firstCommitted, true); assert.equal(h.session.qualified, true);
  assert.equal(child.confirmed.active, true); assert.equal(h.parent.current.description.text, '晚 S2 🧪 é. 全原文');
});

test('retirement proof/full parent/pin mismatch cannot qualify cleanup and never releases or overwrites the child', async t => {
  const changes = [r => r.drafts[0].business_retirement.operation_id = 'foreign', r => r.drafts[0].values.title.selection_extent = 0,
    r => r.drafts[0].current_active = true, r => r.drafts[0].request_sha256 = 'b'.repeat(64),
    r => r.drafts[0].assets[0].media_type = 'application/foreign', r => r.drafts[0].fork_retirement = { schema_version: 1 }];
  for (const change of changes) {
    const h = await harness(t), { handoff: model, child } = await h.ready(); h.setIntercept((w, p, route) => { const r = route(); change(r); return r; });
    await assert.rejects(model.retire()); assert.equal(model.retired, undefined); assert.equal(model.retirementCommitted, true);
    assert.equal(model.pendingNative, 'retirement'); assert.equal(child.confirmed.active, true); assert.equal(model.firstCommitted, true);
  }
});

test('retired handoff closes only the intent, keeps child S3 and exposes the same qualified continuation baseline', async t => {
  const h = await harness(t), { handoff: model, child } = await h.ready(); await model.retire(); const current = plain(child.current);
  await model.closeRetired('intent-close-1'); const close = JSON.parse(JSON.parse(model.originalClose).editor_intent_close.request_json);
  assert.deepEqual(close.parent, null); assert.equal(close.expected_generation, '3'); assert.equal(close.plan_operation, 'handoff-plan-1');
  assert.equal(model.closed, true); assert.equal(model.closeCommitted, true); assert.equal(h.session.phase, 'closed');
  assert.equal(h.session.qualified, true); assert.deepEqual(plain(child.current), current); assert.equal(child.confirmed.active, true);
  assert.throws(() => h.session.save(), /active issued/); assert.throws(() => h.session.retrySave(), /historical inspection/);
  await h.session.inspect(); assert.equal(h.session.qualified, true); assert.equal(h.session.originalRequest, h.base.state.submission);
});

test('exact close factory is read-only and freezes full S1, detach boundary can close without reviving the old SDK lease', async t => {
  const h = await harness(t, { dirty: false }), model = h.m.EditorBusinessHandoffCoordinator.savedExact(h.session, h.parent, 'exact-close-1', 'exact-discard-1', h.hooks);
  assert.equal(h.calls.length, 0); assert.equal(h.state.closeWriteCount, 0); h.base.owner.current = h.base.owner.exact = false;
  assert.equal(h.session.mayConsume(h.parent.current), false); await model.close();
  const close = JSON.parse(JSON.parse(model.originalClose).editor_intent_close.request_json);
  assert.equal(close.disposition, 'saved_exact'); assert.equal(close.expected_generation, '2');
  assert.equal(close.parent.proof.save_operation, h.parentRecord.operation_id); assert.equal(close.parent.discard_operation, 'exact-discard-1');
  assert.equal(model.closed, true); assert.equal(h.session.qualified, true); assert.equal(h.state.closeWriteCount, 1);
});

test('exact close refuses any newer text/selection/composition/asset/order or changed fresh input cutoff before transport', async t => {
  const changes = [v => v.title.text += 'late', v => v.title.selection_extent = 0, v => v.todos.composing_start = 0,
    v => v.assets.reverse(), v => v.assets[0].aliases.push('late')];
  for (const change of changes) {
    const h = await harness(t, { dirty: false }), model = h.m.EditorBusinessHandoffCoordinator.savedExact(h.session, h.parent, 'close-1', 'discard-1', h.hooks);
    const latest = h.parent.current; change(latest); h.parent.update(latest); await assert.rejects(model.close());
    assert.equal(h.calls.length, 0); assert.equal(model.originalClose.includes('close-1'), true); assert.equal(h.session.qualified, true);
  }
  const h = await harness(t, { dirty: false }), model = h.m.EditorBusinessHandoffCoordinator.savedExact(h.session, h.parent, 'close-2', 'discard-2', h.hooks);
  h.owner.exact = false; assert.throws(() => model.close(), /owner/); assert.equal(h.calls.length, 0);
});

test('exact close Unknown cannot change identity or consume late S2 by another close; original explicit reconciliation retains known effect', async t => {
  const h = await harness(t, { dirty: false }), model = h.m.EditorBusinessHandoffCoordinator.savedExact(h.session, h.parent, 'close-1', 'discard-1', h.hooks);
  let lose = true; h.setIntercept((w, p, route) => { const r = route(); if (lose) { lose = false; throw new Error('lost close'); } return r; });
  await assert.rejects(model.close(), /lost close/); const fixed = model.originalClose;
  const latest = h.parent.current; latest.description.text = 'later S2 remains'; h.parent.update(latest); h.owner.current = false;
  await model.retryClose(); assert.equal(h.calls.at(-1), fixed); assert.equal(h.state.closeWriteCount, 1);
  assert.equal(h.parent.current.description.text, 'later S2 remains'); assert.equal(model.closed, true);
});

test('closed success restores original transport read-only and explicitly inspects history; it never authorizes a new save', async t => {
  const h = await harness(t, { dirty: false }), model = h.m.EditorBusinessHandoffCoordinator.savedExact(h.session, h.parent, 'close-1', 'discard-1', h.hooks);
  await model.close(); const restoreHooks = { ...h.base.hooks, send: async wire => {
    const p = JSON.parse(wire); if (p.action === 'editor_intent_read') return h.metadata(p.editor_intent_ref.part);
    if (p.action === 'editor_commit_inspect') return h.base.businessReply(); throw new Error('unexpected mutation'); } };
  const restored = await h.m.EditorBusinessSessionCoordinator.restore(h.session.proof, restoreHooks);
  assert.equal(restored.phase, 'closed'); assert.equal(restored.qualified, false); assert.equal(restored.originalInspect, h.session.originalInspect);
  assert.throws(() => restored.retrySave(), /historical inspection/); await restored.inspect(); assert.equal(restored.qualified, true);
  assert.equal(restored.confirmed.historical_card.source, h.session.confirmed.historical_card.source);
});

test('handoff restore reads exact native literals without writes and preserves original first request for explicit reconciliation', async t => {
  const h = await harness(t), original = await h.freeze(); await original.begin(); const before = h.calls.length;
  const restored = await h.m.EditorBusinessHandoffCoordinator.restore(h.session, h.parent, h.hooks);
  assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).editor_intent_ref.part), ['handoff', 'retirement', 'close']);
  assert.equal(restored.originalHandoff, original.originalHandoff); assert.equal(restored.originalRetirement,
    JSON.stringify({ action: 'draft_continue_business_retire', business_retirement: { request_json: h.state.retirement } }));
  assert.equal(restored.pendingNative, 'handoff'); assert.equal(restored.firstCommitted, false); assert.equal(h.state.firstWriteCount, 1);
  await restored.retry(); assert.equal(h.state.firstWriteCount, 1); assert.equal(restored.confirmed.values.description.text, '晚 S2 🧪 é. 全原文');
});

test('freeze guards actual parent in-flight/Unknown/import owner and pin subset authority; whole wire budgets and Unicode fail before native', async t => {
  const h = await harness(t); h.owner.imports = false; await assert.rejects(h.freeze(), /imports/); assert.equal(h.calls.length, 0); h.owner.imports = true;
  const missing = h.parent.current; missing.assets[0].aliases = ['not-confirmed']; h.parent.update(missing);
  await assert.rejects(h.freeze(), /aliases/); assert.equal(h.calls.length, 0);
  const b = await harness(t); const large = b.parent.current; large.description.text = '汉'.repeat(180000); b.parent.update(large);
  await assert.rejects(b.freeze(), /bytes limit/); assert.equal(b.calls.length, 0);
  const u = await harness(t); const incomplete = u.parent.current; incomplete.title.text = '\ud800';
  incomplete.title.selection_base = incomplete.title.selection_extent = -1; u.parent.update(incomplete);
  await assert.rejects(u.freeze(), /Unicode/); assert.equal(u.calls.length, 0);
  const p = await harness(t); p.parent.resumeWrites(); const raw = p.parent.current; raw.title.text += 'write'; p.parent.update(raw);
  const pending = p.parent.flush(); p.parent.pauseWrites(); await assert.rejects(p.freeze(), /queue/);
  p.parentCalls[0].reject(new Error('lost real parent write')); await assert.rejects(pending); await assert.rejects(p.freeze(), /queue/);
  assert.equal(p.calls.length, 0);
});

test('business/raw mixed lineage copy and ordinary child saves retain business15 while rejecting dual incoming/outgoing links', async t => {
  const h = await harness(t), { handoff: model, child } = await h.ready(); const record = child.confirmed;
  const copied = h.m.copyRecord(record); copied.business_link.parent.generation = '2';
  assert.equal(child.confirmed.business_link.parent.generation, h.parentRecord.generation);
  const invalid = plain(record); invalid.fork_link = { schema_version: 1, parent_draft_id: 'raw-parent', parent_generation: '1',
    parent_save_operation: 'raw-save', parent_request_sha256: '1'.repeat(64), child_operation: 'raw-child' };
  assert.throws(() => h.m.validateRecord(invalid, true), /relation/);
  const retirement = plain(h.parentRecord); retirement.active = retirement.current_active = false;
  retirement.generation = retirement.current_generation = h.m.nextGeneration(retirement.generation);
  retirement.business_retirement = { schema_version: 1, child_draft_id: h.scope.draft_id, operation_id: 'parent-retire-1', business_link: plain(model.confirmed.business_link) };
  h.m.validateRecord(retirement, true); retirement.fork_retirement = { schema_version: 1 };
  assert.throws(() => h.m.validateRecord(retirement, true), /relation/);
});

function nativeFixture() {
  const file = path.resolve(__dirname, '../reports/ui-source/v25/editor-handoff-store-fixture.json');
  const bytes = fs.readFileSync(file); assert.equal(sha(bytes), '9af7ccaabaf247bbb06e60cd7302da878f515baa3704495735542bdb783e486f');
  const fixture = JSON.parse(bytes); assert.equal(fixture.schema_version, 1); return fixture;
}
function nativeRead(view) { return { ok: true, error: '', effect: 'not_committed', receipt_revision: '', editor_intents: [plain(view)], intent_next_after: '' }; }
async function nativeContext(kind) {
  const m = shared.model(), actual = nativeFixture()[kind], calls = [];
  const fields = new m.EditorFieldPolicy(async () => { throw new Error('Readonly restore must not perform new projection'); });
  const sender = async wire => {
    calls.push(wire); const request = JSON.parse(wire);
    if (request.action === 'editor_intent_read') return nativeRead(actual.closed_parts[request.editor_intent_ref.part]);
    if (request.action === 'editor_commit_inspect') {
      assert.equal(wire, actual.original_inspect_transport); return plain(actual.original_inspect_reply);
    }
    throw new Error('Fixture readonly context cannot mutate ' + request.action);
  };
  const sessionHooks = { fields, send: sender, changed: () => {}, isCurrent: () => true, isExact: () => false, parentReady: () => false };
  const session = await m.EditorBusinessSessionCoordinator.restore(actual.intent_proof, sessionHooks);
  await session.inspect(); assert.equal(session.qualified, true);
  const hooks = { send: sender, sendDraft: () => { throw new Error('Readonly current-child open cannot write'); }, changed: () => {},
    operation: () => 'unused', isCurrent: () => true, isExact: () => false, importsReady: () => true, childCurrent: () => false };
  return { m, actual, calls, session, hooks };
}

test('fresh actual Store full handoff/retirement/closed seven-part DTO restores exact plan and active native child without replay', async t => {
  const h = await nativeContext('handoff'), before = h.calls.length;
  const model = await h.m.EditorBusinessHandoffCoordinator.restore(h.session, undefined, h.hooks);
  assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).editor_intent_ref.part), ['handoff', 'retirement', 'close']);
  assert.equal(model.originalHandoffLiteral, h.actual.handoff_literal); assert.equal(model.originalRetirement, JSON.stringify(h.actual.retirement_transport));
  assert.equal(model.originalClose, JSON.stringify(h.actual.close_transport)); assert.equal(model.closed, true); assert.equal(model.closeCommitted, true);
  assert.equal(model.firstCommitted, false); assert.equal(model.retirementCommitted, false); // Closed intent phase does not invent other receipt effects.
  const record = h.actual.handoff_reply.drafts[0]; h.m.validateRecord(record, true); h.m.validateRecord(h.actual.retirement_reply.drafts[0], true);
  const child = model.openCurrentChild(record, record.values, () => true); t.after(() => child.dispose());
  assert.equal(child.dirty, false); assert.equal(child.scope.source, h.session.confirmed.historical_card.source);
  assert.equal(child.confirmed.business_link.handoff_request_sha256, sha(h.actual.handoff_literal));
  assert.equal(child.confirmed.values.description.text, 'S2 DTO raw'); assert.equal(h.session.confirmed.historical_card.description, 'S1 汉字 🧪 é.');
  assert.equal(h.calls.filter(w => !['editor_intent_read', 'editor_commit_inspect'].includes(JSON.parse(w).action)).length, 0);
});

test('fresh actual Store saved_exact closed context retains complete original request and inspect bytes without reactivating intent or raw', async () => {
  const h = await nativeContext('saved_exact'); const model = await h.m.EditorBusinessHandoffCoordinator.restore(h.session, undefined, h.hooks);
  assert.equal(model.closed, true); assert.equal(model.originalHandoffLiteral, ''); assert.equal(model.originalClose, JSON.stringify(h.actual.close_transport));
  assert.equal(h.session.originalRequest, h.actual.submission_literal); assert.equal(h.session.qualified, true);
  assert.throws(() => h.session.retrySave(), /historical inspection/); assert.throws(() => model.close(), /parent/);
  assert.equal(h.calls.every(w => ['editor_intent_read', 'editor_commit_inspect'].includes(JSON.parse(w).action)), true);
});
