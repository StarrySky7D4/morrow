// Runs the actual ArkTS model against controlled transport replies and time.
// These cases prove journal coordination, not ArkUI rendering or host storage.
const fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../entry/src/main/ets/model/EditorDraft.ets'), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } }).outputText;
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 6; i++) await Promise.resolve(); };

test('settleIssued observes a paused actual in-flight save and leaves full late S2 dirty without sending it', async () => {
  const h = harness({ existing: true, restored: { generation: '8', current_generation: '8' } });
  h.coordinator.update(h.values('issued S1')); const issued = h.coordinator.flush(); h.coordinator.pauseWrites();
  const later = h.values('late S2 汉字🧪', { selection_base: 4, selection_extent: 1, composing_start: 0, composing_end: 1 });
  later.todos.text = 'all\nlate\nraw'; h.coordinator.update(later);
  const observed = h.coordinator.settleIssued(); h.ack(0); const record = await observed; await assert.rejects(issued, /paused/);
  assert.equal(record.values.title.text, 'issued S1'); assert.deepEqual(plain(h.coordinator.current), plain(later));
  assert.equal(h.coordinator.dirty, true); assert.equal(h.coordinator.writesPaused, true); await h.tick(10000);
  assert.equal(h.calls.length, 1); h.coordinator.dispose();
});

test('settleIssued preserves actual failed issued Unknown and never retries or consumes newer input while paused', async () => {
  const h = harness({ existing: true, restored: { generation: '8', current_generation: '8' } });
  h.coordinator.update(h.values('issued S1')); const issued = h.coordinator.flush(); h.coordinator.pauseWrites();
  h.coordinator.update(h.values('late S2')); const observed = h.coordinator.settleIssued(); h.calls[0].reject(new Error('lost actual ACK'));
  await assert.rejects(observed, /lost actual/); await assert.rejects(issued, /lost actual/);
  const fixed = h.coordinator.pending; assert.equal(h.coordinator.unknown, true);
  const current = await h.coordinator.settleIssued(); assert.equal(current.generation, '8'); assert.equal(h.coordinator.pending, fixed);
  assert.equal(h.coordinator.current.title.text, 'late S2'); assert.equal(h.coordinator.dirty, true); assert.equal(h.coordinator.unknown, true);
  await h.tick(10000); assert.equal(h.calls.length, 1); h.coordinator.dispose();
});

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

function harness({ existing = false, restored, initialTitle = '', initialAssets = [], debounce = 500 } = {}) {
  const m = model(), { DraftScope, Values, DraftRecord, EditorDraftCoordinator } = m.api;
  const scope = new DraftScope(); scope.card_id = 'card-1'; scope.draft_id = 'draft-1'; scope.source = 'aabb';
  if (existing) { scope.source_kind = 0; scope.source_revision = '9007199254740993'; }
  const initial = new Values(); initial.title.text = initialTitle;
  initial.assets = plain(initialAssets);
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
    result.values = Object.assign(new Values(), draft.values, { assets: plain(draft.assets || []) }); result.operation_id = draft.operation_id;
    result.assets = result.values.assets.map((selection, i) => {
      const previous = coordinator.confirmed?.assets.find(asset => asset.selection.asset_id === selection.asset_id);
      return Object.assign({ display_name: `${selection.asset_id}.png`, media_type: 'image/png', byte_length: '3', sha256: 'a'.repeat(64) },
        plain(previous), { selection: plain(selection), pin_id: `draft-asset-${i}` });
    });
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

test('successful explicit import preparation establishes an unchanged source journal once; opening does not', async () => {
  const assets = [{ origin: 0, asset_id: 'source-image', aliases: ['attachment:source.png', '😀.png'] }];
  const h = harness({ existing: true, initialAssets: assets }); await h.tick(5000); assert.equal(h.calls.length, 0);
  const prepared = h.coordinator.ensureConfirmed(); await settle(); assert.equal(h.calls.length, 1);
  assert.deepEqual(h.calls[0].request.draft.assets, assets); assert.equal(h.calls[0].request.draft.values.assets, undefined);
  h.ack(0); const record = await prepared; assert.equal(record.generation, '1');
  assert.deepEqual(plain(record.values.assets), assets); assert.equal(h.coordinator.dirty, false);
  const repeated = await h.coordinator.ensureConfirmed(); assert.equal(repeated.generation, '1'); assert.equal(h.calls.length, 1);
  h.coordinator.dispose();
});

test('selected identities and aliases are immutable snapshot input; attachment-only changes save a generation', async () => {
  const h = harness({ existing: true }); const input = h.values('');
  input.assets = [{ origin: 2, asset_id: 'durable-image', aliases: ['attachment:paste.png', '😀'] }];
  h.coordinator.update(input); input.assets[0].aliases[0] = 'caller changed'; input.assets.push({ origin: 2, asset_id: 'injected', aliases: [] });
  await h.tick(500); assert.equal(h.calls.length, 1); const draft = h.calls[0].request.draft;
  assert.deepEqual(draft.assets, [{ origin: 2, asset_id: 'durable-image', aliases: ['attachment:paste.png', '😀'] }]);
  h.ack(0); await settle(); assert.equal(h.coordinator.dirty, false);
  const confirmed = h.coordinator.confirmed; confirmed.assets[0].selection.aliases[0] = 'getter changed';
  confirmed.values.assets.length = 0; assert.equal(h.coordinator.current.assets[0].aliases[0], 'attachment:paste.png');
  const next = h.coordinator.current; next.assets[0].origin = 3; h.coordinator.update(next);
  await h.tick(500); assert.equal(h.calls[1].request.draft.assets[0].origin, 3); h.ack(1); await settle();
  assert.equal(h.coordinator.current.assets[0].origin, 3); h.coordinator.dispose();
});

test('an unknown attachment save retries exact selection bytes and leaves newer removal intact', async () => {
  const h = harness({ existing: true }); const input = h.values('original');
  input.assets = [{ origin: 2, asset_id: 'one-import', aliases: ['raw alias'] }]; h.coordinator.update(input);
  const pending = h.coordinator.flush(); h.calls[0].reject(new Error('lost pinned response')); await assert.rejects(pending, /lost pinned/);
  const exact = h.coordinator.pending; const newer = h.values('newer'); newer.assets = []; h.coordinator.update(newer);
  await h.tick(5000); assert.equal(h.calls.length, 1);
  const retry = h.coordinator.retry(); assert.equal(h.calls[1].serialized, exact); h.ack(1, { repeated: true, consumed_imports: ['import-op'] });
  await retry; assert.equal(h.coordinator.current.assets.length, 0); assert.equal(h.coordinator.confirmed.assets.length, 1);
  assert.equal(h.coordinator.dirty, true); await h.tick(5000); assert.equal(h.calls.length, 2);
  const complete = h.coordinator.flush(); assert.deepEqual(h.calls[2].request.draft.assets, []); h.ack(2); await complete;
  assert.equal(h.coordinator.confirmed.assets.length, 0); h.coordinator.dispose();
});

test('restored selected pins remain read-only and text autosave preserves origin, aliases and inventory', async () => {
  const selected = { origin: 2, asset_id: 'persisted-import', aliases: ['attachment:persisted.png'] };
  const stored = { selection: selected, pin_id: 'draft-asset-0', display_name: 'persisted.png', media_type: 'image/png',
    byte_length: '9007199254740993', sha256: 'b'.repeat(64) };
  const h = harness({ existing: true, initialAssets: [selected], restored: { assets: [stored], consumed_imports: ['durable-import'] } });
  await h.tick(5000); assert.equal(h.calls.length, 0); assert.equal((await h.coordinator.ensureConfirmed()).assets[0].byte_length, '9007199254740993');
  h.coordinator.update(h.values('typed later')); await h.tick(500); assert.deepEqual(h.calls[0].request.draft.assets, [selected]);
  assert.equal(h.calls[0].request.draft.expected_generation, '8'); h.ack(0); await settle();
  assert.equal(h.coordinator.current.assets[0].origin, 2); h.coordinator.dispose();
});

test('missing or altered stored asset evidence cannot certify a selection receipt', async () => {
  for (const altered of [{ assets: [] }, { assets: [{ selection: { origin: 2, asset_id: 'different', aliases: [] },
    pin_id: 'draft-asset-0', display_name: 'x', media_type: 'image/png', byte_length: '1', sha256: 'a'.repeat(64) }] },
    { consumed_imports: ['duplicate', 'duplicate'] }]) {
    const h = harness({ existing: true }); const input = h.values(''); input.assets = [{ origin: 2, asset_id: 'selected', aliases: [] }];
    h.coordinator.update(input); const sent = h.coordinator.flush(); const exact = h.coordinator.pending; h.ack(0, altered);
    await assert.rejects(sent, /asset|inventory/); assert.equal(h.coordinator.unknown, true); assert.equal(h.coordinator.pending, exact);
    assert.equal(h.coordinator.confirmed, undefined); h.coordinator.dispose();
  }
});

test('parent/predecessor, duplicate and oversized attachment selections fail before transport', () => {
  const h = harness({ existing: true });
  for (const assets of [[{ origin: 1, asset_id: 'unsupported', aliases: [] }], [{ origin: 4, asset_id: 'unsupported', aliases: [] }],
    [{ origin: 2, asset_id: 'same', aliases: [] }, { origin: 2, asset_id: 'same', aliases: [] }],
    Array.from({ length: 21 }, (_, i) => ({ origin: 2, asset_id: `asset-${i}`, aliases: [] }))]) {
    const input = h.values(''); input.assets = assets; assert.throws(() => h.coordinator.update(input), /asset/);
  }
  assert.equal(h.calls.length, 0); h.coordinator.dispose();
});

test('ensureConfirmed surfaces failure without silently certifying empty state or retrying it', async () => {
  const h = harness({ existing: true }); const owner = h.coordinator.ensureConfirmed(); await settle();
  h.calls[0].reject(new h.api.DraftSaveFailure(true, 'owner write rejected')); await assert.rejects(owner, /owner write rejected/);
  assert.equal(h.coordinator.confirmed, undefined); await h.tick(5000); assert.equal(h.calls.length, 1);
  h.coordinator.dispose(); await assert.rejects(h.coordinator.ensureConfirmed(), /disposed/);
});

test('a later receipt cannot substitute bytes for the same already confirmed asset identity', async () => {
  const h = harness({ existing: true }); const input = h.values('first');
  input.assets = [{ origin: 2, asset_id: 'stable-identity', aliases: [] }]; h.coordinator.update(input);
  const first = h.coordinator.flush(); h.ack(0); await first;
  h.coordinator.update(h.values('second')); const latest = h.coordinator.flush(); const exact = h.coordinator.pending;
  const original = h.coordinator.confirmed.assets[0];
  h.ack(1, { assets: [{ ...plain(original), sha256: 'f'.repeat(64) }] });
  await assert.rejects(latest, /changed a confirmed asset/); assert.equal(h.coordinator.unknown, true);
  assert.equal(h.coordinator.pending, exact); assert.equal(h.coordinator.confirmed.generation, '1');
  assert.equal(h.coordinator.current.title.text, 'second'); h.coordinator.dispose();
});

test('all twenty selected asset identities and Unicode aliases survive serialization without truncation', async () => {
  const h = harness({ existing: true }); const input = h.values('all selected');
  input.assets = Array.from({ length: 20 }, (_, i) => ({ origin: 2, asset_id: `asset-${i}`, aliases: [`😀-${i}`, `attachment:${i}.png`] }));
  const expected = plain(input.assets); h.coordinator.update(input); const sent = h.coordinator.flush();
  assert.deepEqual(h.calls[0].request.draft.assets, expected); h.ack(0); const record = await sent;
  assert.deepEqual(plain(record.values.assets), expected); assert.equal(record.assets.length, 20); h.coordinator.dispose();
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
