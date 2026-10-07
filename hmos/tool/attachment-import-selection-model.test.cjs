/* Run the actual AttachmentImportSelection.ets against controlled picker and
 * per-item adapters. These cases are sequencing evidence, not device grants or
 * durable store/pin proof, which remain owned by their existing model checks. */
'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/model/AttachmentImportSelection.ets');
const result = ts.transpileModule(fs.readFileSync(sourcePath, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
  fileName: sourcePath, reportDiagnostics: true,
});
assert.equal(result.diagnostics.filter(item => item.category === ts.DiagnosticCategory.Error).length, 0);
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((a, b) => { resolve = a; reject = b; });
  return { promise, resolve, reject };
};
const settle = async () => { for (let index = 0; index < 20; index++) await Promise.resolve(); };

function harness(initial = {}) {
  const api = {}, calls = [], notifications = [], faults = {};
  vm.runInNewContext(result.outputText, { exports: api, Promise, Number, Set, Error });
  let boundary = { editor_id: 'editor-A', card_id: 'card-A', draft_id: 'draft-A', generation: '7',
    edit_epoch: 0, selected_count: 0, import_count: 0, title: '', blocked: false, ...initial };
  let selection = ['content://test-owned/one.txt', 'content://test-owned/two.txt', 'content://test-owned/three.txt'];
  let number = 0;
  const selected = identity => {
    const next = { ...boundary, generation: String(BigInt(boundary.generation) + 1n),
      edit_epoch: boundary.edit_epoch + 1, selected_count: boundary.selected_count + 1,
      import_count: boundary.import_count + 1 };
    const name = '中文文件-' + next.import_count + '.txt';
    const title = api.attachmentImportTitle(next.title, name);
    if (title !== undefined) next.title = title;
    boundary = next;
    return { outcome: 'selected', identity: plain(identity), name, next: plain(next), error: '' };
  };
  const coordinator = new api.AttachmentImportSelection({
    read() { if (faults.read) throw new Error('synthetic read failure'); return boundary; },
    async select(maximum) {
      calls.push({ action: 'select', maximum });
      if (faults.selectGate) await faults.selectGate.promise;
      if (faults.select) throw new Error('synthetic picker rejection');
      return selection;
    },
    async confirm(frozen) {
      calls.push({ action: 'confirm', frozen: plain(frozen) });
      if (faults.confirmGate) await faults.confirmGate.promise;
      if (faults.confirm) throw new Error('synthetic journal failure');
      if (faults.confirmReturn) return faults.confirmReturn(frozen);
      if (boundary.generation === '0') boundary = { ...boundary, generation: '1' };
      return plain(boundary);
    },
    async importOne(uri, identity, frozen) {
      const call = { action: 'import', uri, identity: plain(identity), frozen: plain(frozen) };
      calls.push(call);
      if (faults.importGate) await faults.importGate.promise;
      if (faults.importThrow) throw new Error('synthetic missing native receipt');
      if (faults.importOne) return faults.importOne(call, selected);
      return selected(identity);
    },
    operation() { return faults.operation ? faults.operation() : 'operation-' + ++number; },
    changed(state) {
      notifications.push(plain(state));
      if (faults.changed) faults.changed(state, coordinator);
    },
  });
  return { api, calls, notifications, faults, coordinator, selected,
    get boundary() { return boundary; }, set boundary(value) { boundary = value; },
    get selection() { return selection; }, set selection(value) { selection = value; },
    imports: () => calls.filter(call => call.action === 'import'),
  };
}

test('all three URI items are imported in order with independent frozen identities and successive generations', async () => {
  const h = harness(), uris = [...h.selection], state = await h.coordinator.run();
  assert.equal(state.phase, 'complete'); assert.equal(state.selected, 3); assert.equal(state.started, 3);
  assert.deepEqual(h.imports().map(call => call.uri), uris);
  assert.deepEqual(h.imports().map(call => call.identity.expected_generation), ['7', '8', '9']);
  assert.deepEqual(h.imports().map(call => call.identity.operation_id), ['operation-1', 'operation-2', 'operation-3']);
  assert.equal(h.boundary.selected_count, 3); assert.equal(h.boundary.title, '中文文件-1.txt');
});

test('picker cancel creates no draft owner, operation or import', async () => {
  const h = harness({ generation: '0' }); h.selection = [];
  const state = await h.coordinator.run(); assert.equal(state.phase, 'cancelled');
  assert.deepEqual(h.calls.map(call => call.action), ['select']); assert.equal(h.boundary.generation, '0');
});

test('valid non-empty selection establishes generation zero owner before any provider preparation', async () => {
  const h = harness({ generation: '0' }); h.selection.length = 1;
  const state = await h.coordinator.run(); assert.equal(state.phase, 'complete');
  assert.deepEqual(h.calls.map(call => call.action), ['select', 'confirm', 'import']);
  assert.equal(h.imports()[0].identity.expected_generation, '1');
});

test('single picker maximum uses both asset and retained import slots', async () => {
  const h = harness({ selected_count: 17, import_count: 18 }); h.selection.length = 2;
  const state = await h.coordinator.run(); assert.equal(h.calls[0].maximum, 2);
  assert.equal(state.phase, 'complete'); assert.equal(h.boundary.import_count, 20);
  assert.equal(h.boundary.selected_count, 19);
});

test('no remaining slot or blocked editor opens no picker and admits no item', async () => {
  for (const initial of [{ selected_count: 20 }, { import_count: 20 }, { blocked: true }]) {
    const h = harness(initial), state = await h.coordinator.run();
    assert.equal(state.phase, 'stopped'); assert.equal(h.calls.length, 0);
  }
});

test('oversized provider return is rejected whole before confirm or first-item import', async () => {
  const h = harness({ selected_count: 19 }), state = await h.coordinator.run();
  assert.equal(state.phase, 'failed'); assert.deepEqual(h.calls.map(call => call.action), ['select']);
});

test('duplicate, empty, control-containing or excessively long URI selection rejects before any item', async () => {
  for (const selection of [['x', 'x'], [''], ['content://own/\u0000file'], ['x'.repeat(16385)], [null]]) {
    const h = harness(); h.selection = selection;
    assert.equal((await h.coordinator.run()).phase, 'failed'); assert.equal(h.imports().length, 0);
    assert.equal(h.calls.filter(call => call.action === 'confirm').length, 0);
  }
});

test('sparse URI selection is rejected before owner confirmation or any first-item import', async () => {
  const h = harness({ generation: '0' });
  h.selection = ['content://test-owned/one.txt', , ];
  assert.equal(h.selection.length, 2);
  assert.equal(Object.hasOwn(h.selection, 1), false);
  const state = await h.coordinator.run();
  assert.equal(state.phase, 'failed'); assert.equal(state.started, 0);
  assert.equal(h.calls.filter(call => call.action === 'confirm').length, 0);
  assert.equal(h.imports().length, 0); assert.equal(h.boundary.generation, '0');
});

test('same display names and bytes at distinct URIs remain independent import items', async () => {
  const h = harness(); h.faults.importOne = (call, selected) => {
    const blank = h.boundary.title.trim().length === 0, receipt = selected(call.identity);
    if (blank) { receipt.next.title = 'same.txt'; h.boundary.title = 'same.txt'; }
    receipt.name = 'same.txt'; return receipt;
  };
  assert.equal((await h.coordinator.run()).selected, 3);
  assert.equal(new Set(h.imports().map(call => call.identity.operation_id)).size, 3);
});

test('new title or generation while the picker is open stops before owner confirmation', async () => {
  for (const change of [{ title: 'new user title', edit_epoch: 1 }, { generation: '8' }, { editor_id: 'replacement' }]) {
    const h = harness(); h.faults.selectGate = deferred(); const pending = h.coordinator.run(); await settle();
    h.boundary = { ...h.boundary, ...change }; h.faults.selectGate.resolve();
    assert.equal((await pending).phase, 'stopped'); assert.equal(h.imports().length, 0);
    assert.equal(h.calls.filter(call => call.action === 'confirm').length, 0);
  }
});

test('generation confirmation cannot adopt user changes made while await was live', async () => {
  const h = harness({ generation: '0' }); h.faults.confirmGate = deferred();
  const pending = h.coordinator.run(); await settle();
  h.boundary = { ...h.boundary, title: '用户新标题', edit_epoch: 1 }; h.faults.confirmGate.resolve();
  assert.equal((await pending).phase, 'stopped'); assert.equal(h.imports().length, 0);
  assert.equal(h.boundary.title, '用户新标题');
});

test('rejected owner confirmation conservatively remains Unknown without reading selected provider URI', async () => {
  const h = harness(); h.faults.confirm = true;
  assert.equal((await h.coordinator.run()).phase, 'unknown'); assert.equal(h.imports().length, 0);
});

test('simultaneous run calls share one live handle and one picker', async () => {
  const h = harness(); h.faults.selectGate = deferred();
  const first = h.coordinator.run(), second = h.coordinator.run(); assert.equal(first, second);
  await settle(); assert.equal(h.calls.filter(call => call.action === 'select').length, 1);
  h.faults.selectGate.resolve(); assert.equal((await first).selected, 3);
});

test('stop during picker waits for the same live handle then admits no provider URI', async () => {
  const h = harness(); h.faults.selectGate = deferred(); const pending = h.coordinator.run(); await settle();
  h.coordinator.stop(); assert.equal(h.coordinator.snapshot().busy, true);
  assert.equal(h.coordinator.run(), pending); h.faults.selectGate.resolve();
  assert.equal((await pending).phase, 'stopped'); assert.equal(h.imports().length, 0);
});

test('stop during issued import retains its late confirmed result and never starts next URI', async () => {
  const h = harness(); h.faults.importGate = deferred(); const pending = h.coordinator.run(); await settle();
  assert.equal(h.imports().length, 1); h.coordinator.stop(); h.faults.importGate.resolve();
  const state = await pending; assert.equal(state.phase, 'stopped'); assert.equal(state.selected, 1);
  assert.equal(h.imports().length, 1); assert.equal(h.boundary.selected_count, 1);
});

test('Ready but unselected receipt is retained and stops all later items', async () => {
  const h = harness(); h.faults.importOne = call => ({ outcome: 'retained', identity: call.identity,
    name: 'one.txt', next: undefined, error: 'draft changed; Ready import retained' });
  const state = await h.coordinator.run(); assert.equal(state.phase, 'stopped');
  assert.equal(state.retained, 1); assert.equal(state.selected, 0); assert.equal(h.imports().length, 1);
});

test('known per-item failure preserves previously selected item and stops without automatic replay', async () => {
  const h = harness(); h.faults.importOne = (call, selected) => h.imports().length === 1 ? selected(call.identity) :
    { outcome: 'failed', identity: call.identity, name: 'two.txt', next: undefined, error: 'quota failed' };
  const state = await h.coordinator.run(); assert.equal(state.phase, 'failed'); assert.equal(state.selected, 1);
  assert.equal(h.imports().length, 2); assert.equal(h.boundary.selected_count, 1);
  await settle(); assert.equal(h.imports().length, 2);
});

test('Unknown returned or thrown item result does not replay and stops remaining selection', async () => {
  for (const thrown of [false, true]) {
    const h = harness();
    if (thrown) h.faults.importThrow = true;
    else h.faults.importOne = call => ({ outcome: 'unknown', identity: call.identity,
      name: 'one.txt', next: undefined, error: 'receipt unavailable' });
    const state = await h.coordinator.run(); assert.equal(state.phase, 'unknown'); assert.equal(h.imports().length, 1);
    await settle(); assert.equal(h.imports().length, 1);
  }
});

test('foreign receipt operation, draft, card or generation is Unknown rather than success', async () => {
  for (const change of [{ operation_id: 'foreign' }, { card_id: 'foreign' }, { draft_id: 'foreign' }, { expected_generation: '4' }]) {
    const h = harness(); h.faults.importOne = call => ({ ...h.selected(call.identity), identity: { ...call.identity, ...change } });
    const state = await h.coordinator.run(); assert.equal(state.phase, 'unknown'); assert.equal(state.selected, 0);
    assert.equal(h.imports().length, 1);
  }
});

test('receipt cannot smuggle a different current boundary into the next item', async () => {
  for (const change of [{ editor_id: 'new editor' }, { generation: '7' }, { edit_epoch: 0 }, { selected_count: 0 }, { import_count: 0 }, { blocked: true }]) {
    const h = harness(); h.faults.importOne = call => {
      const receipt = h.selected(call.identity); receipt.next = { ...receipt.next, ...change }; return receipt;
    };
    assert.equal((await h.coordinator.run()).phase, 'stopped'); assert.equal(h.imports().length, 1);
  }
});

test('adapter cannot accept extra edits or replace title with text other than current title or confirmed filename', async () => {
  for (const change of [{ edit_epoch: 2 }, { title: 'unrelated text' }, { title: '' }]) {
    const h = harness(); h.faults.importOne = call => {
      const receipt = h.selected(call.identity); h.boundary = { ...h.boundary, ...change };
      receipt.next = plain(h.boundary); return receipt;
    };
    assert.equal((await h.coordinator.run()).phase, 'stopped'); assert.equal(h.imports().length, 1);
  }
});

test('new edit after pin confirmation keeps selected result but stops before next URI', async () => {
  const h = harness(); h.faults.importOne = call => {
    const receipt = h.selected(call.identity);
    h.boundary = { ...h.boundary, title: 'newer user title', edit_epoch: h.boundary.edit_epoch + 1 };
    return receipt;
  };
  const state = await h.coordinator.run(); assert.equal(state.phase, 'stopped'); assert.equal(state.selected, 1);
  assert.equal(h.boundary.title, 'newer user title'); assert.equal(h.imports().length, 1);
});

test('duplicate generated operation identity stops before admitting the second item', async () => {
  const h = harness(); h.faults.operation = () => 'same-operation';
  const state = await h.coordinator.run(); assert.equal(state.phase, 'failed'); assert.equal(state.selected, 1);
  assert.equal(h.imports().length, 1);
});

test('valid uint64 generations above Number safe range remain exact across all items', async () => {
  const h = harness({ generation: '9007199254740993' });
  assert.equal((await h.coordinator.run()).phase, 'complete');
  assert.deepEqual(h.imports().map(call => call.identity.expected_generation), ['9007199254740993', '9007199254740994', '9007199254740995']);
});

test('snapshot and callback object mutations cannot replace owned sequencing state', async () => {
  const h = harness(); h.faults.changed = state => { state.selected = 999; state.phase = 'forged'; };
  const snapshot = h.coordinator.snapshot(); snapshot.phase = 'forged';
  const state = await h.coordinator.run(); assert.equal(state.phase, 'complete'); assert.equal(state.selected, 3);
});

test('synchronous observer stop before first URI admission cannot start any import', async () => {
  const h = harness(); h.faults.changed = (state, coordinator) => {
    if (state.phase === 'importing') { h.faults.changed = undefined; coordinator.stop(); }
  };
  const state = await h.coordinator.run(); assert.equal(state.phase, 'stopped'); assert.equal(state.started, 0);
  assert.equal(h.imports().length, 0);
});

test('empty-title proposal matches filename limit and preserves a newly non-empty title', () => {
  const h = harness(), suggest = h.api.attachmentImportTitle;
  assert.equal(suggest('', '附件文件.txt'), '附件文件.txt');
  assert.equal(suggest(' \n\t', '附件文件.txt'), '附件文件.txt');
  assert.equal(suggest('new user title', '附件文件.txt'), undefined);
  assert.equal(suggest('', 'a'.repeat(70)), 'a'.repeat(60));
  assert.equal(suggest('', 'a'.repeat(59) + '😀.txt'), 'a'.repeat(59));
  assert.equal(suggest('', '  '), undefined);
});

test('invalid generation, negative or excessive counts fail closed before picker', async () => {
  for (const initial of [{ generation: '01' }, { generation: '18446744073709551616' },
    { selected_count: -1 }, { import_count: 21 }, { edit_epoch: NaN }]) {
    const h = harness(initial), state = await h.coordinator.run(); assert.equal(state.phase, 'failed'); assert.equal(h.calls.length, 0);
  }
});
