// Tests actual ArkTS code with injectable preference reads and writes.
// No business Store writes, native UI gestures or device qualification here.
const fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../entry/src/main/ets/model/CardOrder.ets'), 'utf8');
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } }).outputText;
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
const P = 'workbench.page.overview', Q = 'workbench.page.projects';
function harness(initialRaw) {
  const api = {}, calls = [], states = []; let disk = initialRaw, readPause, currentPage = P;
  vm.runInNewContext(compiled, { exports: api, Error });
  let coordinator;
  coordinator = new api.CardOrderPreferences(
    () => readPause ? new Promise((resolve, reject) => { readPause.resolve = resolve; readPause.reject = reject; }) : Promise.resolve(disk),
    serialized => new Promise((resolve, reject) => calls.push({ serialized, request: JSON.parse(serialized), resolve, reject })),
    () => states.push({ page: currentPage, manual: coordinator.manual(currentPage), order: [...coordinator.order(currentPage, ['a','b','c'])],
      revision: coordinator.revision, busy: coordinator.busy, loading: coordinator.loading, error: coordinator.error, snapshot: plain(coordinator.snapshot) }));
  const ack = index => { disk = calls[index].serialized; calls[index].resolve(); };
  const switchPage = (page, all = ['a','b','c'], visible = all, token = page) => {
    currentPage = page; coordinator.setView(page, all, visible, token);
  };
  return { api, coordinator, calls, states, ack, switchPage, raw: () => disk,
    disk(value) { disk = value; }, pauseRead() { readPause = {}; return readPause; } };
}
const persisted = (orders = {}, manual = []) => JSON.stringify({ version: 1, orders, manual });

test('stable page identities include the five Flutter pages and exclude trash', () => {
  const h = harness(); assert.equal(h.api.cardOrderPage('概览'), P);
  assert.equal(h.api.cardOrderPage('进行中'), Q); assert.equal(h.api.cardOrderPage(Q), Q);
  assert.equal(h.api.cardOrderPage('回收站'), ''); h.api.cardOrderPages.push('bad');
  assert.equal(h.api.cardOrderPage('bad'), ''); h.coordinator.dispose();
});

test('restore retains source v1 schema, manual flags and page-specific orders', async () => {
  const h = harness(persisted({[P]:['c','a','b'],[Q]:['b','c','a']},[P])); await h.coordinator.restore();
  assert.equal(h.calls.length,0); assert.equal(h.coordinator.manual(P),true); assert.equal(h.coordinator.manual(Q),false);
  assert.deepEqual([...h.coordinator.order(P,['a','b','c'])],['c','a','b']);
  assert.deepEqual([...h.coordinator.order(Q,['a','b','c'])],['b','c','a']); assert.equal(h.coordinator.error,'');
});

test('order projects surviving saved IDs and appends new cards without destructive pruning', async () => {
  const h = harness(persisted({[P]:['c','removed','a','b']},[P])); await h.coordinator.restore();
  assert.deepEqual([...h.coordinator.order(P,['a','b','c','new'])],['c','a','b','new']);
  assert.deepEqual([...h.coordinator.order(P,['a','a','new'])],['a','new']);
  assert.deepEqual(h.coordinator.snapshot.orders[P] && [...h.coordinator.snapshot.orders[P]], ['c','removed','a','b']);
  assert.deepEqual([...h.coordinator.order('__proto__',['a','b'])],['a','b']); assert.equal(h.calls.length,0);
});

test('moveRelative uses target identity after removal, including before/after and no-ops', () => {
  const h=harness(); const a=['a','b','c','d'];
  assert.deepEqual([...h.api.moveRelative(a,'a','c',true)],['b','c','a','d']);
  assert.deepEqual([...h.api.moveRelative(a,'d','b',false)],['a','d','b','c']);
  assert.deepEqual([...h.api.moveRelative(a,'a','a',false)],a);
  assert.deepEqual([...h.api.moveRelative(a,'missing','a',false)],a); assert.deepEqual(a,['a','b','c','d']);
});

test('filtered moves replace visible slots and leave hidden card anchors unchanged', async () => {
  const h=harness(); const all=['a','hidden1','b','hidden2','c','d'], visible=['a','b','c','d'];
  h.coordinator.setView(P,all,visible,'filter-1'); const revision=h.coordinator.revision;
  const moving=h.coordinator.move(P,'d','a',false,all,visible,revision,'filter-1');
  assert.equal(h.coordinator.busy,true); assert.equal(h.coordinator.manual(P),true);
  assert.deepEqual([...h.coordinator.order(P,all)],['d','hidden1','a','hidden2','b','c']);
  h.ack(0); assert.equal(await moving,true);
  assert.deepEqual(h.calls[0].request,{version:1,orders:{[P]:['d','hidden1','a','hidden2','b','c']},manual:[P]});
});

test('visible query order may differ from full order and is merged into visible slots exactly', async () => {
  const h=harness(); const all=['a','hidden','b','c'], visible=['c','b','a'];
  h.coordinator.setView(P,all,visible,'title'); const moving=h.coordinator.move(P,'a','c',false,all,visible,h.coordinator.revision,'title');
  assert.deepEqual([...h.coordinator.order(P,all)],['a','hidden','c','b']); h.ack(0); await moving;
});

test('a successful move prunes removed IDs, appends new ones, and does not change other pages', async () => {
  const h=harness(persisted({[P]:['gone','b','a'],[Q]:['z','y']},[Q])); await h.coordinator.restore();
  const moving=h.coordinator.move(P,'new','b',false,['a','b','new'],['b','a','new']);
  h.ack(0); await moving; assert.deepEqual([...h.coordinator.snapshot.orders[P]],['new','b','a']);
  assert.deepEqual([...h.coordinator.snapshot.orders[Q]],['z','y']); assert.equal(h.coordinator.manual(Q),true);
});

test('disabling and enabling manual order preserves its saved identities', async () => {
  const h=harness(persisted({[P]:['b','a']},[P])); await h.coordinator.restore();
  const disabling=h.coordinator.select(P,false); assert.equal(h.coordinator.manual(P),false); h.ack(0); await disabling;
  const enabling=h.coordinator.select(P,true); h.ack(1); await enabling;
  assert.equal(h.coordinator.manual(P),true); assert.deepEqual([...h.coordinator.order(P,['a','b'])],['b','a']);
  assert.equal(await h.coordinator.select(P,true),true); assert.equal(h.calls.length,2);
});

test('write failure rolls back the exact prior order/manual state and never auto-replays', async () => {
  const h=harness(persisted({[P]:['a','hidden','b']},[])); await h.coordinator.restore(); const before=plain(h.coordinator.snapshot);
  const moving=h.coordinator.move(P,'b','a',false,['a','hidden','b'],['a','b']);
  assert.deepEqual([...h.coordinator.order(P,['a','hidden','b'])],['b','hidden','a']);
  h.calls[0].reject(new Error('write failed')); await assert.rejects(moving,/write failed/);
  assert.deepEqual(plain(h.coordinator.snapshot),before); assert.equal(h.coordinator.manual(P),false);
  assert.equal(h.coordinator.busy,false); assert.equal(h.coordinator.error,'write failed'); await settle(); assert.equal(h.calls.length,1);
});

test('failed mode selection restores the previous manual mode and retained order', async () => {
  const h=harness(persisted({[P]:['b','a']},[P])); await h.coordinator.restore();
  const selecting=h.coordinator.select(P,false); h.calls[0].reject(new Error('')); await assert.rejects(selecting);
  assert.equal(h.coordinator.manual(P),true); assert.deepEqual([...h.coordinator.order(P,['a','b'])],['b','a']);
  assert.equal(h.coordinator.error,'Card order could not be saved');
});

test('busy and loading reject new changes instead of issuing concurrent writes or reads', async () => {
  const h=harness(); const paused=h.pauseRead(); const restoring=h.coordinator.restore();
  assert.equal(h.coordinator.loading,true); assert.equal(await h.coordinator.select(P,true),false);
  assert.equal(await h.coordinator.move(P,'a','b',true,['a','b'],['a','b']),false); paused.resolve(undefined); await restoring;
  const selecting=h.coordinator.select(P,true); assert.equal(await h.coordinator.select(Q,true),false);
  assert.equal(await h.coordinator.move(Q,'a','b',true,['a','b'],['a','b']),false);
  await h.coordinator.restore(); assert.equal(h.calls.length,1); h.ack(0); await selecting;
});

test('invalid candidate identities, page, duplicates, missing targets and overflow never write', async () => {
  const h=harness(); const cases=[
    [P,'a','a',false,['a','b'],['a','b']], [P,'a','x',false,['a','b'],['a','b']],
    [P,'a','b',false,['a','b'],['a','a','b']], [P,'a','b',false,['a','a','b'],['a','b']],
    [P,'a','b',false,['a'],['a','b']], ['trash','a','b',false,['a','b'],['a','b']],
    [P,'a','b',false,['a','b',''],['a','b']], [P,'a','b',false,['a','b',...Array.from({length:255},(_,i)=>`x${i}`)],['a','b']],
  ];
  for(const args of cases) assert.equal(await h.coordinator.move(...args),false);
  assert.equal(await h.coordinator.select('trash',true),false); assert.equal(h.calls.length,0);
});

test('view tokens and model revision reject stale drags, even for identical identity arrays', async () => {
  const h=harness(), all=['a','b','c']; h.coordinator.setView(P,all,all,'query-1'); const old=h.coordinator.revision;
  h.coordinator.setView(P,all,all,'query-2'); assert.equal(await h.coordinator.move(P,'a','b',true,all,all,old,'query-1'),false);
  const selecting=h.coordinator.select(Q,true); h.ack(0); await selecting;
  assert.equal(await h.coordinator.move(P,'a','b',true,all,all,old,'query-2'),false);
  h.coordinator.setView(Q,all,all,'query-2'); assert.equal(await h.coordinator.move(P,'a','b',true,all,all,h.coordinator.revision,'query-2'),false);
  assert.equal(await h.coordinator.move(Q,'a','b',true,all,['b','a','c'],h.coordinator.revision,'query-2'),false);
  h.coordinator.setView('',[],[],''); assert.equal(await h.coordinator.move(Q,'a','b',true,all,all,h.coordinator.revision,'query-2'),false);
  assert.equal(h.calls.length,1);
});

test('switching pages during a write does not publish the old page mode into the current page', async () => {
  for(const fail of [false,true]) {
    const h=harness(); h.switchPage(P); const selecting=h.coordinator.select(P,true); h.switchPage(Q);
    fail ? h.calls[0].reject(new Error('old page failed')) : h.ack(0);
    if(fail) await assert.rejects(selecting); else await selecting;
    const state=h.states.at(-1); assert.equal(state.page,Q); assert.equal(state.manual,false);
    assert.deepEqual(state.order,['a','b','c']); assert.equal(h.coordinator.view.page,Q);
  }
});

test('serialized preferences recover the acknowledged state after restart', async () => {
  const h=harness(); const all=['a','b','c']; const moving=h.coordinator.move(P,'c','a',false,all,all);
  h.ack(0); await moving; const selecting=h.coordinator.select(Q,true); h.ack(1); await selecting;
  const restarted=harness(h.raw()); await restarted.coordinator.restore();
  assert.deepEqual(plain(restarted.coordinator.snapshot),plain(h.coordinator.snapshot)); assert.equal(restarted.calls.length,0);
});

test('caller mutations of input arrays, views and returned snapshots cannot change a pending request', async () => {
  const h=harness(); const all=['a','hidden','b'], visible=['a','b']; h.coordinator.setView(P,all,visible,'view');
  const detached=h.coordinator.view; detached.visible[0]='mutated'; assert.equal(h.coordinator.view.visible[0],'a');
  const moving=h.coordinator.move(P,'b','a',false,all,visible,h.coordinator.revision,'view');
  all[0]='external'; visible[0]='external'; const snapshot=h.coordinator.snapshot;
  snapshot.orders[P][0]='getter'; snapshot.manual.length=0;
  assert.deepEqual(h.calls[0].request.orders[P],['b','hidden','a']); h.ack(0); await moving;
  assert.deepEqual([...h.coordinator.snapshot.orders[P]],['b','hidden','a']); assert.equal(h.coordinator.manual(P),true);
});

test('invalid persisted JSON is rejected atomically without partially replacing prior preferences', async () => {
  const h=harness(persisted({[P]:['b','a']},[P])); await h.coordinator.restore(); const before=plain(h.coordinator.snapshot);
  const invalid=['not JSON',JSON.stringify({version:2,orders:{},manual:[]}),JSON.stringify({version:1,orders:{[P]:['a']},manual:1}),
    persisted({[P]:['a','a']},[P]),persisted({[P]:Array.from({length:257},(_,i)=>String(i))},[]),
    persisted({evil:['a']},[]),persisted({},[P,P]),JSON.stringify({version:1,orders:null,manual:[]})];
  for(const raw of invalid) {h.disk(raw);await h.coordinator.restore();assert.ok(h.coordinator.error);assert.deepEqual(plain(h.coordinator.snapshot),before);}
  assert.equal(h.calls.length,0);
});

test('exact 256-card candidates can be persisted without truncation', async () => {
  const h=harness(); const all=Array.from({length:256},(_,i)=>`card-${i}`);
  const moving=h.coordinator.move(P,'card-255','card-0',false,all,all); h.ack(0); await moving;
  assert.equal(h.calls[0].request.orders[P].length,256); assert.equal(h.calls[0].request.orders[P][0],'card-255');
  assert.deepEqual(new Set(h.calls[0].request.orders[P]),new Set(all));
});

test('dispose prevents late read/write callbacks from updating the current UI', async () => {
  const restoring=harness(); const read=restoring.pauseRead(); const waiting=restoring.coordinator.restore();
  const readStates=restoring.states.length; restoring.coordinator.dispose(); read.resolve(persisted({[P]:['a']},[P])); await waiting;
  assert.equal(restoring.states.length,readStates); assert.equal(restoring.coordinator.manual(P),false);
  for(const fail of [false,true]) {
    const h=harness(); const selecting=h.coordinator.select(P,true); const states=h.states.length; h.coordinator.dispose();
    fail ? h.calls[0].reject(new Error('late')) : h.ack(0);
    if(fail) await assert.rejects(selecting); else assert.equal(await selecting,false);
    assert.equal(h.states.length,states); assert.equal(await h.coordinator.select(Q,true),false);
    await h.coordinator.restore(); assert.equal(h.calls.length,1); assert.equal(h.coordinator.view,undefined);
  }
});
