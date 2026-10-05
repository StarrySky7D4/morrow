// Runs the actual ArkTS model against controlled transport replies and time.
// These cases prove journal coordination, not ArkUI rendering or host storage.
const fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../entry/src/main/ets/model/EditorDraft.ets'), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } }).outputText;
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };

function model() {
  let now = 0, sequence = 0;
  const timers = new Map(), api = {};
  vm.runInNewContext(compiled, { exports: api, setTimeout(fn, delay) {
    const id = ++sequence; timers.set(id, { fn, at: now + delay }); return id;
  }, clearTimeout(id) { timers.delete(id); } });
  const tick = async milliseconds => {
    const end = now + milliseconds;
    while (true) {
      const next = [...timers].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!next) break;
      now = next[1].at; timers.delete(next[0]); next[1].fn(); await settle();
    }
    now = end; await settle();
  };
  return { api, tick };
}

function harness({ existing = false, restored, initialTitle = '', debounce = 500 } = {}) {
  const m = model(), { DraftScope, Values, DraftRecord, EditorDraftCoordinator } = m.api;
  const scope = new DraftScope(); scope.card_id = 'card-1'; scope.draft_id = 'draft-1'; scope.source = 'aabb';
  if (existing) { scope.source_kind = 0; scope.source_revision = '9007199254740993'; }
  const initial = new Values(); initial.title.text = initialTitle;
  const calls = [], states = []; let operations = 0;
  const restoredRecord = restored ? Object.assign(new DraftRecord(), {
    scope, values: initial, operation_id: 'restored-operation', generation: '8', current_generation: '8',
    active: true, current_active: true, ...restored,
  }) : undefined;
  let coordinator;
  coordinator = new EditorDraftCoordinator(scope, initial,
    serialized => new Promise((resolve, reject) => calls.push({ serialized, request: JSON.parse(serialized), resolve, reject })),
    () => states.push({ current: plain(coordinator.current), confirmed: plain(coordinator.confirmed),
      dirty: coordinator.dirty, saving: coordinator.saving, unknown: coordinator.unknown,
      conflicted: coordinator.conflicted, error: coordinator.error, pending: coordinator.pending }),
    () => `operation-${++operations}`, restoredRecord, debounce);
  const values = (text, changes = {}) => {
    const value = coordinator.current; value.title.text = text;
    Object.assign(value.title, changes); return value;
  };
  const ack = (index, changes = {}) => {
    const call = calls[index], draft = call.request.draft, result = new DraftRecord();
    result.scope = Object.assign(new DraftScope(), { card_id: draft.card_id, draft_id: draft.draft_id,
      source_kind: draft.source_kind, source_revision: draft.source_revision, source: scope.source });
    result.values = draft.values; result.operation_id = draft.operation_id;
    result.generation = m.api.nextGeneration(draft.expected_generation);
    result.current_generation = result.generation; result.active = true; result.current_active = true;
    Object.assign(result, changes); call.resolve(result); return result;
  };
  return { ...m, coordinator, calls, states, values, ack, scope, initial, restoredRecord };
}

test('new cards durably save the initial empty snapshot; unchanged existing cards do not', async () => {
  const fresh = harness(), existing = harness({ existing: true });
  await fresh.tick(499); assert.equal(fresh.calls.length, 0);
  await fresh.tick(1); assert.equal(fresh.calls.length, 1);
  assert.equal(fresh.calls[0].request.draft.values.title.text, '');
  assert.equal(fresh.calls[0].request.draft.source, undefined);
  fresh.ack(0); await settle(); assert.equal(fresh.coordinator.dirty, false);
  await existing.tick(5000); assert.equal(existing.calls.length, 0);
  assert.equal(await existing.coordinator.flush(), undefined);
  fresh.coordinator.dispose(); existing.coordinator.dispose();
});

test('debounce coalesces input and owns scope and full UTF-16 editing values', async () => {
  const h = harness({ existing: true });
  h.scope.card_id = 'external'; h.initial.title.text = 'external';
  const input = h.values('😀a', { selection_base: 1, selection_extent: 3, affinity: 1,
    directional: true, composing_start: 0, composing_end: 2 });
  h.coordinator.update(input); input.title.text = 'caller-mutated';
  await h.tick(300); h.coordinator.update(h.values('😀ab', { selection_extent: 4 }));
  await h.tick(499); assert.equal(h.calls.length, 0); await h.tick(1);
  assert.equal(h.calls.length, 1); const request = h.calls[0].request.draft;
  assert.equal(request.card_id, 'card-1'); assert.equal(request.values.title.text, '😀ab');
  assert.equal(request.values.title.selection_base, 1); assert.equal(request.values.title.composing_end, 2);
  const current = h.coordinator.current; current.title.text = 'getter-mutated';
  assert.equal(h.coordinator.current.title.text, '😀ab'); h.ack(0); await settle(); h.coordinator.dispose();
});

test('an older receipt confirms history but never replaces newer input; automatic S2 is debounced', async () => {
  const h = harness({ existing: true }); h.coordinator.update(h.values('S1')); await h.tick(500);
  h.coordinator.update(h.values('S2')); h.coordinator.update(h.values('S3'));
  await h.tick(900); assert.equal(h.calls.length, 1); h.ack(0); await settle();
  assert.equal(h.coordinator.current.title.text, 'S3'); assert.equal(h.coordinator.confirmed.values.title.text, 'S1');
  assert.equal(h.coordinator.dirty, true); await h.tick(499); assert.equal(h.calls.length, 1);
  await h.tick(1); assert.equal(h.calls.length, 2); assert.equal(h.calls[1].request.draft.values.title.text, 'S3');
  assert.equal(h.calls[1].request.draft.expected_generation, '1'); h.ack(1); await settle();
  assert.equal(h.coordinator.dirty, false); h.coordinator.dispose();
});

test('flush waits for the issued save and then confirms all latest input before resolving', async () => {
  const h = harness({ existing: true }); h.coordinator.update(h.values('S1')); await h.tick(500);
  h.coordinator.update(h.values('S2')); let completed = false;
  const flushing = h.coordinator.flush().then(r => { completed = true; return r; });
  assert.equal(h.coordinator.flush(), h.coordinator.flush()); h.ack(0); await settle();
  assert.equal(h.calls.length, 2); assert.equal(completed, false);
  h.coordinator.update(h.values('S3')); h.ack(1); await settle();
  assert.equal(h.calls.length, 3); assert.equal(completed, false); h.ack(2);
  const final = await flushing; assert.equal(final.values.title.text, 'S3'); assert.equal(final.generation, '3');
  assert.equal(h.coordinator.dirty, false); await h.tick(2000); assert.equal(h.calls.length, 3); h.coordinator.dispose();
});

test('unknown outcomes preserve exact serialized bytes and explicit retry stops at original S1', async () => {
  const h = harness({ existing: true }); h.coordinator.update(h.values(' S1 raw '));
  const failed = h.coordinator.flush(); h.calls[0].reject(new Error('lost reply'));
  await assert.rejects(failed, /lost reply/); const original = h.coordinator.pending;
  h.coordinator.update(h.values('S2')); await h.tick(5000); assert.equal(h.calls.length, 1);
  await assert.rejects(h.coordinator.flush(), /lost reply/); assert.equal(h.coordinator.pending, original);
  const retry = h.coordinator.retry(); assert.equal(h.calls[1].serialized, original);
  h.ack(1, { repeated: true }); const reconciled = await retry;
  assert.equal(reconciled.values.title.text, ' S1 raw '); assert.equal(h.coordinator.current.title.text, 'S2');
  assert.equal(h.coordinator.dirty, true); await h.tick(5000); assert.equal(h.calls.length, 2);
  const latest = h.coordinator.flush(); assert.equal(h.calls[2].request.draft.expected_generation, '1');
  h.ack(2); assert.equal((await latest).values.title.text, 'S2'); h.coordinator.dispose();
});

test('known rejection clears the proposal, surfaces flush failure and does not replay failed S1', async () => {
  const h = harness({ existing: true }); h.coordinator.update(h.values('rejected'));
  const failed = h.coordinator.flush(); h.calls[0].reject(new h.api.DraftSaveFailure(true, 'not committed'));
  await assert.rejects(failed, /not committed/); assert.equal(h.coordinator.unknown, false);
  assert.equal(h.coordinator.pending, undefined); assert.match(h.coordinator.error, /not committed/);
  await h.tick(5000); assert.equal(h.calls.length, 1);
  h.coordinator.update(h.values('corrected')); await h.tick(500); assert.equal(h.calls.length, 2);
  assert.notEqual(h.calls[1].request.draft.operation_id, h.calls[0].request.draft.operation_id);
  assert.equal(h.calls[1].request.draft.expected_generation, '0'); h.ack(1); await settle(); h.coordinator.dispose();
});

test('a different S2 already observed during rejected S1 may autosave without replaying S1', async () => {
  const h = harness({ existing: true }); h.coordinator.update(h.values('rejected')); await h.tick(500);
  h.coordinator.update(h.values('different S2')); h.calls[0].reject(new h.api.DraftSaveFailure(true, 'not committed'));
  await settle(); await h.tick(499); assert.equal(h.calls.length, 1); await h.tick(1);
  assert.equal(h.calls.length, 2); assert.equal(h.calls[1].request.draft.values.title.text, 'different S2');
  h.ack(1); await settle(); h.coordinator.dispose();
});

test('a known rejection of retry cannot erase an already uncertain operation', async () => {
  const h = harness({ existing: true }); h.coordinator.update(h.values('uncertain'));
  const failed = h.coordinator.flush(); h.calls[0].reject(new Error('lost'));
  await assert.rejects(failed); const original = h.coordinator.pending;
  const retry = h.coordinator.retry(); h.calls[1].reject(new h.api.DraftSaveFailure(true, 'retry unavailable'));
  await assert.rejects(retry); assert.equal(h.coordinator.unknown, true); assert.equal(h.coordinator.pending, original);
  await h.tick(5000); assert.equal(h.calls.length, 2); h.coordinator.dispose();
});

test('restoring a confirmed draft is read-only and snapshots are detached', async () => {
  const h = harness({ restored: {}, initialTitle: 'restored' });
  h.restoredRecord.values.title.text = 'external'; await h.tick(5000); assert.equal(h.calls.length, 0);
  const confirmed = await h.coordinator.flush(); assert.equal(confirmed.values.title.text, 'restored');
  confirmed.values.title.text = 'changed return'; assert.equal(h.coordinator.confirmed.values.title.text, 'restored');
  h.coordinator.update(h.values('next')); await h.tick(500); assert.equal(h.calls[0].request.draft.expected_generation, '8');
  h.ack(0); await settle(); h.coordinator.dispose();
});

test('inactive or advanced restored journals block flush and autosave', async () => {
  for (const restored of [{ active: false }, { current_active: false }, { current_generation: '9' }]) {
    const h = harness({ restored, initialTitle: 'old' }); h.coordinator.update(h.values('new'));
    assert.equal(h.coordinator.conflicted, true); await assert.rejects(h.coordinator.flush(), /changed/);
    await h.tick(5000); assert.equal(h.calls.length, 0); h.coordinator.dispose();
  }
});

test('historical or mismatched receipts cannot certify latest input', async () => {
  for (const changes of [{ current_generation: '2' }, { current_active: false },
    { operation_id: 'unrelated' }, { generation: '2', current_generation: '2' }]) {
    const h = harness({ existing: true }); h.coordinator.update(h.values('proposal'));
    const flushing = h.coordinator.flush(); h.ack(0, changes);
    await assert.rejects(flushing); assert.equal(h.coordinator.confirmed, undefined);
    assert.equal(h.coordinator.dirty, true); assert.equal(h.coordinator.unknown, true);
    await h.tick(5000); assert.equal(h.calls.length, 1); h.coordinator.dispose();
  }
});

test('disposal stops publication and new work while leaving an issued operation uncancelled', async () => {
  const h = harness({ existing: true }); h.coordinator.update(h.values('issued'));
  const flushing = h.coordinator.flush(); const count = h.states.length, serialized = h.coordinator.pending;
  h.coordinator.dispose(); h.ack(0); await assert.rejects(flushing, /disposed/);
  assert.equal(h.states.length, count); assert.equal(h.coordinator.pending, serialized);
  assert.throws(() => h.coordinator.update(h.values('closed')), /disposed/);
  await assert.rejects(h.coordinator.flush(), /disposed/); await h.tick(5000); assert.equal(h.calls.length, 1);
});

test('selection-only and composing-only changes are real generations; invalid UTF-16 offsets fail before send', async () => {
  const h = harness({ existing: true, initialTitle: '😀a' });
  h.coordinator.update(h.values('😀a', { selection_base: 1, selection_extent: -1, composing_start: -1, composing_end: 2 }));
  await h.tick(500); assert.equal(h.calls.length, 1); h.ack(0); await settle();
  h.coordinator.update(h.values('😀a', { composing_start: 0, composing_end: 2 }));
  assert.equal(h.coordinator.dirty, true); await h.tick(500); assert.equal(h.calls.length, 2);
  h.ack(1); await settle();
  assert.throws(() => h.coordinator.update(h.values('😀a', { selection_extent: 4 })), /UTF-16/);
  assert.throws(() => h.coordinator.update(h.values('😀a', { composing_start: 2, composing_end: 1 })), /UTF-16/);
  assert.throws(() => h.coordinator.update(h.values('😀a', { affinity: 2 })), /UTF-16/);
  h.coordinator.dispose();
});

test('decimal generation increments preserve u64 precision and reject overflow/noncanonical values', () => {
  const { api } = model();
  assert.equal(api.nextGeneration('9'), '10'); assert.equal(api.nextGeneration('999'), '1000');
  assert.equal(api.nextGeneration('9007199254740993'), '9007199254740994');
  assert.equal(api.nextGeneration('18446744073709551614'), '18446744073709551615');
  for (const input of ['18446744073709551615', '18446744073709551616', '01', '-1', '', '1.0']) {
    assert.throws(() => api.nextGeneration(input), /generation/);
  }
});

test('pre-send generation exhaustion is an explicit error and never an unknown transport outcome', async () => {
  const h = harness({ existing: true, restored: {
    generation: '18446744073709551615', current_generation: '18446744073709551615',
  }, initialTitle: 'baseline' });
  h.coordinator.update(h.values('new input')); await h.tick(500);
  assert.equal(h.calls.length, 0); assert.equal(h.coordinator.pending, undefined);
  assert.equal(h.coordinator.unknown, false); assert.match(h.coordinator.error, /exhausted/);
  await assert.rejects(h.coordinator.flush(), /exhausted/); h.coordinator.dispose();
});

test('disposing before debounce prevents new writes', async () => {
  const h = harness(); h.coordinator.dispose(); await h.tick(5000); assert.equal(h.calls.length, 0);
});
