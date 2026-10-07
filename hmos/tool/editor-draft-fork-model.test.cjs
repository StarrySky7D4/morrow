'use strict';
// Fresh actual ETS coordination with synthetic native proof/transport and time.
// Native canonical protobuf digests, Store crash recovery and ArkUI events are
// qualified separately; the fixture digests below never claim that evidence.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), { test } = require('node:test'), crypto = require('node:crypto');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const root = path.resolve(__dirname, '../entry/src/main/ets/model');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let n = 0; n < 12; n++) await Promise.resolve(); };
function model() {
  let now = 0, timerSequence = 0;
  const modules = new Map(), timers = new Map();
  function load(name) {
    name = name.replace(/^\.\//, ''); if (modules.has(name)) return modules.get(name);
    const file = path.join(root, name + '.ets'), output = {};
    const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), { fileName: file, reportDiagnostics: true,
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
    assert.equal((compiled.diagnostics || []).filter(x => x.category === ts.DiagnosticCategory.Error).length, 0);
    modules.set(name, output);
    vm.runInNewContext(compiled.outputText, { exports: output, require: load,
      setTimeout(fn, delay) { const id = ++timerSequence; timers.set(id, { fn, at: now + delay }); return id; },
      clearTimeout(id) { timers.delete(id); } });
    return output;
  }
  const draft = load('EditorDraft'), fork = load('EditorDraftFork');
  const tick = async duration => {
    const end = now + duration;
    while (true) {
      const next = [...timers].filter(([, value]) => value.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
      if (!next) break;
      now = next[1].at; timers.delete(next[0]); next[1].fn(); await settle();
    }
    now = end; await settle();
  };
  return { ...draft, ...fork, tick, timers };
}
function harness({ newCard = false, selected = ['A', 'B'], recordChanges = {} } = {}) {
  const m = model(), scope = new m.DraftScope(); scope.card_id = 'card-1'; scope.draft_id = 'parent-1';
  scope.source_kind = newCard ? 1 : 0; scope.source_revision = newCard ? '0' : '9007199254740993'; scope.source = newCard ? '' : 'aabb';
  const values = new m.Values(); values.title.text = '最初😀标题';
  Object.assign(values.description, { text: 'a😀b', selection_base: 3, selection_extent: 1, affinity: 1,
    directional: true, composing_start: 1, composing_end: 3 });
  values.todos.text = '重复\n\n重复\n';
  values.assets = selected.map((id, index) => Object.assign(new m.AssetSelection(), {
    origin: index ? 2 : newCard ? 2 : 0, asset_id: id, aliases: [id + '.png', 'attachment:' + id + '/😀'] }));
  const restored = Object.assign(new m.DraftRecord(), { scope, values, operation_id: 'parent-save',
    generation: '8', current_generation: '8', active: true, current_active: true, request_sha256: 'a'.repeat(64),
    assets: values.assets.map((selection, i) => Object.assign(new m.StoredAsset(), { selection: plain(selection),
      pin_id: 'draft-asset-' + i, display_name: selection.asset_id + '.png', media_type: 'image/png',
      byte_length: i ? '9007199254740993' : '3', sha256: (i ? 'b' : 'c').repeat(64) })), ...recordChanges });
  const calls = [], parentCalls = [], owner = { current: true }; let sequence = 0, changed = 0, child;
  const send = wire => new Promise((resolve, reject) => calls.push({ wire, request: JSON.parse(wire), resolve, reject }));
  const parent = new m.EditorDraftCoordinator(scope, values,
    wire => new Promise((resolve, reject) => parentCalls.push({ wire, request: JSON.parse(wire), resolve, reject })),
    () => { changed++; }, () => 'parent-later-' + (++sequence), restored);
  const childScope = m.copyScope(scope); childScope.draft_id = 'child-1';
  const hooks = { send, changed: () => { changed++; }, operation: () => 'child-later-' + (++sequence), isCurrent: () => owner.current };
  const create = (raw = parent.current, nextScope = childScope, operation = 'fork-first') =>
    new m.EditorDraftForkCoordinator(parent, nextScope, raw, operation, hooks);
  const ack = (coordinator, index, changes = {}) => {
    const call = calls[index], request = call.request, forkFirst = request.action === 'draft_fork';
    const write = forkFirst ? request.fork.child : request.draft;
    const record = new m.DraftRecord(); record.scope = coordinator.scope;
    record.values = Object.assign(new m.Values(), plain(write.values), { assets: plain(write.assets) });
    record.operation_id = write.operation_id; record.generation = m.nextGeneration(write.expected_generation);
    record.current_generation = record.generation; record.active = true; record.current_active = true;
    record.request_sha256 = (forkFirst ? 'd' : 'e').repeat(64);
    record.fork_link = forkFirst ? coordinator.parentProof : plain(coordinator.confirmed.fork_link);
    record.assets = record.values.assets.map((selection, i) => {
      const prior = restored.assets.find(pin => pin.selection.asset_id === selection.asset_id);
      return Object.assign(new m.StoredAsset(), plain(prior), { selection: plain(selection), pin_id: 'draft-asset-' + i });
    });
    if (typeof changes === 'function') changes(record); else Object.assign(record, changes);
    call.resolve(record); return record;
  };
  return { m, parent, restored, childScope, calls, parentCalls, owner, create, ack,
    get changed() { return changed; }, close() { child?.dispose(); parent.dispose(); },
    ownChild(value) { child = value; return child; } };
}

test('pause captures every raw field/metadata/assets without automatic or explicit writes; resume retains them', async () => {
  const h = harness(); h.parent.pauseWrites(); const raw = h.parent.current;
  raw.title.text = '暂停后完整原文'; raw.title.selection_base = 4; raw.title.selection_extent = 1;
  raw.todos.text = '全部\n原行\n'; raw.description.composing_start = 1; raw.assets.reverse();
  h.parent.update(raw); await h.m.tick(10000); assert.deepEqual(plain(h.parent.current), plain(raw));
  assert.equal(h.parentCalls.length, 0); assert.equal(h.parent.dirty, true);
  await assert.rejects(h.parent.flush(), /paused/); await assert.rejects(h.parent.ensureConfirmed(), /paused/);
  await assert.rejects(h.parent.retry(), /paused/); assert.equal(h.parentCalls.length, 0);
  h.parent.resumeWrites(); await h.m.tick(500); assert.equal(h.parentCalls.length, 1);
  assert.deepEqual(h.parentCalls[0].request.draft.values.title, plain(raw.title));
  h.parentCalls[0].reject(new Error('close fixture')); await settle(); h.close();
});

test('pause never cancels issued work, but prevents its completion from issuing later raw', async () => {
  const h = harness(); let raw = h.parent.current; raw.title.text = 'issued'; h.parent.update(raw);
  const issued = h.parent.flush(); h.parent.pauseWrites(); raw = h.parent.current; raw.title.text = 'later'; h.parent.update(raw);
  assert.equal(h.parentCalls.length, 1); const call = h.parentCalls[0], write = call.request.draft;
  const receipt = Object.assign(new h.m.DraftRecord(), plain(h.restored), { values: Object.assign(new h.m.Values(), plain(write.values), { assets: plain(write.assets) }),
    operation_id: write.operation_id, generation: '9', current_generation: '9' });
  call.resolve(receipt); await assert.rejects(issued, /paused/); await h.m.tick(10000);
  assert.equal(h.parent.confirmed.values.title.text, 'issued'); assert.equal(h.parent.current.title.text, 'later');
  assert.equal(h.parentCalls.length, 1); assert.equal(h.parent.dirty, true); h.close();
});

test('fork freezes full source/proof/raw and selected subset order/aliases as origin4 without a business command', async () => {
  const h = harness(); const raw = h.parent.current; raw.assets = [raw.assets[1]]; h.parent.update(raw);
  const fork = h.create(); const frozen = fork.original;
  raw.title.text = 'caller mutation'; raw.assets[0].aliases.length = 0; h.childScope.source = 'ffff';
  const exposed = fork.firstRaw; exposed.todos.text = 'getter mutation'; fork.parentProof.parent_generation = '999';
  const work = fork.begin(); assert.equal(h.calls.length, 1); assert.equal(h.calls[0].wire, frozen);
  const wire = h.calls[0].request; assert.equal(wire.action, 'draft_fork'); assert.equal(wire.fork.child.expected_generation, '0');
  assert.equal(wire.fork.child.values.assets, undefined); assert.equal(wire.fork.child.source, undefined);
  assert.deepEqual(wire.fork.child.assets, [{ origin: 4, asset_id: 'B', aliases: ['B.png', 'attachment:B/😀'] }]);
  assert.equal(wire.fork.child.source_kind, 0); assert.equal(wire.fork.child.source_revision, '9007199254740993');
  assert.equal(wire.fork.child.values.todos.text, '重复\n\n重复\n');
  h.ack(fork, 0); await work; assert.equal(fork.scope.source, 'aabb'); assert.equal(h.parent.writesPaused, true);
  assert.equal(h.parentCalls.length, 0); h.close();
});

test('new-card fork inherits kind1/revision0/empty source and raw legacy rows without business reconciliation', async () => {
  const h = harness({ newCard: true }); const fork = h.create(), work = fork.begin(); h.ack(fork, 0); await work;
  const child = h.ownChild(fork.openChild()); assert.equal(child.scope.source_kind, 1);
  assert.equal(child.scope.source_revision, '0'); assert.equal(child.scope.source, '');
  assert.equal(child.current.todos.text, '重复\n\n重复\n'); await h.m.tick(5000); assert.equal(h.calls.length, 1); h.close();
});

test('latest raw stays on paused parent during first ACK; caller updates child completely before its ordinary save', async () => {
  const h = harness(); const fork = h.create(), work = fork.begin(); let latest = h.parent.current;
  latest.title.text = 'late😀候选'; Object.assign(latest.title, { selection_base: 4, selection_extent: 1, affinity: 1,
    directional: true, composing_start: 2, composing_end: 5 }); latest.todos.text += '\n晚到'; h.parent.update(latest);
  h.ack(fork, 0); await work; const child = h.ownChild(fork.openChild());
  assert.equal(child.current.title.text, '最初😀标题'); assert.deepEqual(plain(h.parent.current), plain(latest));
  child.update(h.parent.current); const flushed = child.flush(); assert.equal(h.calls.length, 2);
  assert.deepEqual(h.calls[1].request.draft.values.title, plain(latest.title));
  assert.equal(h.calls[1].request.draft.values.todos.text, latest.todos.text);
  assert.deepEqual(h.calls[1].request.draft.assets.map(x => x.origin), [3, 3]);
  assert.equal(h.calls[1].request.draft.expected_generation, '1'); h.ack(child, 1); await flushed;
  assert.equal(child.dirty, false); assert.equal(child.confirmed.generation, '2');
  assert.equal(child.current.assets[0].aliases[1], 'attachment:A/😀'); await h.m.tick(10000); assert.equal(h.calls.length, 2); h.close();
});

test('normal fork child keeps fixed lineage and source, promotes parent pins once, and exposes immutable proof', async () => {
  const h = harness(); const fork = h.create(), work = fork.begin(); h.ack(fork, 0); await work;
  const child = h.ownChild(fork.openChild()); assert.equal(fork.openChild(), child);
  const exposed = child.confirmed; exposed.fork_link.child_operation = 'injected'; exposed.request_sha256 = '0'.repeat(64);
  let raw = child.current; raw.description.selection_extent = 2; child.update(raw);
  const sent = child.flush(); h.ack(child, 1); await sent;
  raw = child.current; raw.title.text += ' later'; child.update(raw); const next = child.flush();
  assert.deepEqual(h.calls[2].request.draft.assets.map(x => x.origin), [3, 3]); h.ack(child, 2); await next;
  assert.equal(child.confirmed.fork_link.child_operation, 'fork-first'); assert.equal(child.scope.source, 'aabb');
  assert.equal(child.dirty, false); h.close();
});

test('Unknown first fork preserves one original wire; late raw and explicit retry do not change its operation', async () => {
  const h = harness(); const fork = h.create(), first = fork.begin(); h.calls[0].reject(new Error('lost first receipt'));
  await assert.rejects(first, /lost first/); const exact = fork.pending; const latest = h.parent.current;
  latest.title.text = 'after Unknown'; h.parent.update(latest); await h.m.tick(10000);
  assert.equal(fork.unknown, true); assert.equal(h.calls.length, 1); await assert.rejects(fork.begin(), /original/);
  const retried = fork.retry(); assert.equal(h.calls[1].wire, exact); h.ack(fork, 1, { repeated: true }); await retried;
  assert.equal(fork.unknown, false); assert.equal(fork.pending, undefined); assert.equal(fork.confirmed.values.title.text, '最初😀标题');
  assert.equal(h.parent.current.title.text, 'after Unknown'); await h.m.tick(5000); assert.equal(h.calls.length, 2); h.close();
});

test('known first rejection can explicitly release parent; Unknown reconciliation cannot downgrade or release', async () => {
  const rejected = harness(); const fork = rejected.create(), work = fork.begin();
  rejected.calls[0].reject(new rejected.m.DraftSaveFailure(true, 'known absent')); await assert.rejects(work, /known absent/);
  assert.equal(fork.unknown, false); fork.releaseRejectedParent(); assert.equal(rejected.parent.writesPaused, false);
  await assert.rejects(fork.begin(), /original/); rejected.close();
  const h = harness(); const uncertain = h.create(), first = uncertain.begin(); h.calls[0].reject(new Error('Unknown'));
  await assert.rejects(first); const retry = uncertain.retry(); h.calls[1].reject(new h.m.DraftSaveFailure(true, 'later reject'));
  await assert.rejects(retry); assert.equal(uncertain.unknown, true); assert.throws(() => uncertain.releaseRejectedParent(), /uncertain/);
  assert.equal(h.parent.writesPaused, true); h.close();
});

test('owner replacement cannot install a valid durable old fork or issue its Unknown retry', async () => {
  const h = harness(); const fork = h.create(), work = fork.begin(); h.owner.current = false; h.ack(fork, 0); await work;
  assert.equal(fork.confirmed.generation, '1'); assert.throws(() => fork.openChild(), /owner/);
  assert.throws(() => fork.retirementCommand('retire-old'), /owner/); h.close();
  const retryOwner = harness(); const uncertain = retryOwner.create(), first = uncertain.begin();
  retryOwner.calls[0].reject(new Error('lost')); await assert.rejects(first); retryOwner.owner.current = false;
  await assert.rejects(uncertain.retry(), /owner/); assert.equal(retryOwner.calls.length, 1); retryOwner.close();
});

test('valid historical first commitment with advanced or inactive current state is conflict, never resurrected or replayed', async () => {
  for (const changes of [{ current_generation: '2' }, { current_generation: '2', current_active: false }]) {
    const h = harness(); const fork = h.create(), work = fork.begin(); h.ack(fork, 0, changes);
    await assert.rejects(work, /committed.*changed/); assert.equal(fork.conflicted, true); assert.equal(fork.unknown, false);
    assert.equal(fork.confirmed.generation, '1'); assert.throws(() => fork.openChild(), /not current/);
    await assert.rejects(fork.retry(), /No uncertain/); await h.m.tick(5000); assert.equal(h.calls.length, 1); h.close();
  }
});

test('wrong full source/operation/parent proof/raw/digest/first active state never installs a child', async () => {
  const alterations = [record => { record.scope.source = 'ffff'; }, record => { record.operation_id = 'wrong'; },
    record => { record.fork_link.parent_save_operation = 'wrong-parent'; }, record => { record.fork_link.parent_request_sha256 = '0'.repeat(64); },
    record => { record.values.description.selection_base = 0; }, record => { record.values.todos.text = 'normalized'; },
    record => { record.request_sha256 = ''; }, record => { record.request_sha256 = 'D'.repeat(64); },
    record => { record.request_sha256 = null; }, record => { record.fork_link = false; },
    record => { record.active = false; }, record => { record.active = 'true'; },
    record => { record.current_generation = '01'; }, record => { record.current_generation = '0'; }];
  for (const alter of alterations) {
    const h = harness(); const fork = h.create(), work = fork.begin(); h.ack(fork, 0, alter);
    await assert.rejects(work, /fork|digest|state/i); assert.equal(fork.unknown, true); assert.equal(fork.confirmed, undefined);
    assert.throws(() => fork.openChild(), /not current/); assert.equal(h.parent.confirmed.active, true);
    assert.equal(h.parent.writesPaused, true); h.close();
  }
});

test('receipt pin bytes/metadata and exact inventory aliases/order/identities are validated against the frozen parent', async () => {
  const alterations = [record => { record.assets[0].sha256 = '0'.repeat(64); }, record => { record.assets[0].byte_length = '4'; },
    record => { record.assets[0].display_name = 'other.png'; }, record => { record.assets[0].media_type = 'text/plain'; },
    record => { record.values.assets[0].aliases.reverse(); record.assets[0].selection.aliases.reverse(); },
    record => { record.values.assets.reverse(); record.assets.reverse(); },
    record => { record.values.assets[1].asset_id = 'A'; record.assets[1].selection.asset_id = 'A'; },
    record => { record.assets = []; }, record => { record.consumed_imports = ['duplicate', 'duplicate']; }];
  for (const alter of alterations) {
    const h = harness(); const fork = h.create(), work = fork.begin(); h.ack(fork, 0, alter);
    await assert.rejects(work, /fork|asset|inventory/i); assert.equal(fork.unknown, true);
    assert.equal(fork.confirmed, undefined); assert.equal(h.parent.confirmed.active, true); h.close();
  }
});

test('normal-only paths still reject origin4 without a verified fork, and later receipts cannot alter lineage/source', async () => {
  const h = harness(); let raw = h.parent.current; raw.assets[0].origin = 4;
  assert.throws(() => h.parent.update(raw), /asset/); const fork = h.create(), first = fork.begin(); h.ack(fork, 0); await first;
  const child = h.ownChild(fork.openChild()); raw = child.current; raw.title.text += '!'; child.update(raw);
  const work = child.flush(); h.ack(child, 1, { scope: Object.assign(child.scope, { source: 'ffff' }) });
  await assert.rejects(work, /lineage/); assert.equal(child.unknown, true); assert.equal(child.confirmed.generation, '1'); h.close();
});

test('retirement freezes explicit parent/child identity once; building it alone never sends or retires either journal', async () => {
  const h = harness(); const fork = h.create(), first = fork.begin();
  assert.throws(() => fork.retirementCommand('retire-once'), /not admitted/); h.ack(fork, 0); await first;
  h.ownChild(fork.openChild()); const original = fork.retirementCommand('retire-once');
  assert.equal(fork.retirementCommand('retire-once'), original); assert.throws(() => fork.retirementCommand('new-op'), /cannot change/);
  assert.deepEqual(JSON.parse(original), { action: 'draft_fork_retire', fork_retirement: { card_id: 'card-1', child_draft_id: 'child-1',
    child_operation: 'fork-first', parent_draft_id: 'parent-1', parent_generation: '8', parent_save_operation: 'parent-save',
    parent_request_sha256: 'a'.repeat(64), operation_id: 'retire-once' } });
  assert.equal(h.calls.length, 1); assert.equal(h.parent.confirmed.active, true); assert.equal(h.parent.writesPaused, true); h.close();
});

test('fork constructor rejects duplicate identity, wrong scope, incomplete pins and unavailable parents before transport', () => {
  for (const mutation of [scope => { scope.draft_id = 'parent-1'; }, scope => { scope.card_id = 'other'; },
    scope => { scope.source_revision = '2'; }, scope => { scope.source = 'ffff'; }, scope => { scope.source_kind = 1; }]) {
    const h = harness(); const scope = h.m.copyScope(h.childScope); mutation(scope);
    assert.throws(() => h.create(h.parent.current, scope), /scope/); assert.equal(h.calls.length, 0); assert.equal(h.parent.writesPaused, false); h.close();
  }
  const h = harness(); assert.throws(() => h.create(h.parent.current, h.childScope, 'parent-save'), /operation/);
  const raw = h.parent.current; raw.assets.push({ origin: 2, asset_id: 'unconfirmed-staging', aliases: [] }); h.parent.update(raw);
  assert.throws(() => h.create(), /confirmed parent pin/); assert.equal(h.parent.writesPaused, false); h.close();
  for (const alter of [raw => raw.assets.reverse(), raw => raw.assets[0].aliases.reverse()]) {
    const changed = harness(); const raw = changed.parent.current; alter(raw); changed.parent.update(raw);
    assert.throws(() => changed.create(), /aliases or order/); assert.equal(changed.calls.length, 0); changed.close();
  }
  for (const changes of [{ active: false }, { current_active: false }, { current_generation: '9' }]) {
    const unavailable = harness({ recordChanges: changes }); assert.throws(() => unavailable.create(), /confirmed parent/); unavailable.close();
  }
});

test('development retirement marker binds the exact original parent digest and cannot revive an inactive child', async () => {
  const h = harness(); const fork = h.create(), first = fork.begin(); h.ack(fork, 0); await first;
  const record = h.m.copyRecord(fork.confirmed); record.generation = '2'; record.current_generation = '2';
  record.active = false; record.current_active = false;
  record.fork_retirement = { schema_version: 1, child_draft_id: 'grandchild-1', operation_id: 'retire-child',
    fork_link: { schema_version: 1, parent_draft_id: 'child-1', parent_generation: '1', parent_save_operation: 'fork-first',
      parent_request_sha256: record.request_sha256, child_operation: 'grandchild-first' } };
  h.m.validateRecord(record, true);
  const inactive = h.ownChild(new h.m.EditorDraftCoordinator(record.scope, record.values, () => assert.fail('inactive may not send'),
    () => {}, () => 'inactive-op', record));
  inactive.resumeWrites(); await assert.rejects(inactive.flush(), /journal changed/);
  assert.equal(inactive.conflicted, true); assert.equal(inactive.current.assets[0].origin, 4);
  for (const alter of [r => { r.fork_retirement.operation_id = 'fork-first'; },
    r => { r.fork_retirement.fork_link.parent_request_sha256 = '0'.repeat(64); },
    r => { r.fork_retirement.fork_link.parent_draft_id = 'other-parent'; },
    r => { r.fork_retirement.child_draft_id = 'child-1'; }, r => { r.active = true; }]) {
    const invalid = h.m.copyRecord(record); alter(invalid); assert.throws(() => h.m.validateRecord(invalid, true), /retirement|parent pins/i);
  }
  h.close();
});

test('fresh SDK compiler and actual fork/draft model source identities are emitted', () => {
  for (const name of ['EditorDraft', 'EditorDraftFork']) {
    console.log(name + '.ets SHA256=' + crypto.createHash('sha256').update(fs.readFileSync(path.join(root, name + '.ets'))).digest('hex'));
  }
  assert.equal(typeof model().EditorDraftForkCoordinator, 'function'); console.log('SDK TypeScript=' + ts.version);
});
