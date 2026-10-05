// Executes the actual ArkTS coordinator with controlled replies. These cases
// cover races and pending-work coalescing, not ArkUI/device rendering.
const fs=require('node:fs'),vm=require('node:vm'),path=require('node:path'),assert=require('node:assert/strict');
const {test}=require('node:test');
const ts=require(process.env.HMOS_TYPESCRIPT||'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/model/QueryCoordinator.ets'),'utf8');
const exportsObject={};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText,
  {exports:exportsObject,setTimeout,clearTimeout});
const {QueryConditions,QueryCoordinator}=exportsObject;
const conditions=(values={})=>Object.assign(new QueryConditions(),values);
const turn=()=>new Promise(resolve=>setTimeout(resolve,5));
function harness(debounce=20){
  const calls=[],states=[];
  const q=new QueryCoordinator(c=>new Promise((resolve,reject)=>calls.push({c,resolve,reject})),
    s=>states.push({phase:s.phase,ids:[...s.ids],refreshing:s.refreshing,error:s.error}),debounce);
  return {q,calls,states,last:()=>states.at(-1)};
}
test('obsolete reply is discarded and only latest pending page reaches the owner',async()=>{
  const h=harness();h.q.select(conditions());await turn();assert.equal(h.calls.length,1);
  h.q.select(conditions({section:'小项目'}));h.q.select(conditions({section:'实验室'}));await turn();
  assert.equal(h.calls.length,1);assert.deepEqual(h.last().ids,[]);
  h.calls[0].resolve(['old']);await turn();assert.equal(h.calls.length,2);
  assert.equal(h.calls[1].c.section,'实验室');assert.deepEqual(h.last().ids,[]);
  h.calls[1].resolve(['current']);await turn();assert.deepEqual(h.last().ids,['current']);h.q.dispose();
});
test('typing is debounced while tabs and confirmed content refresh issue immediately',async()=>{
  const h=harness(40);h.q.select(conditions());await turn();h.calls[0].resolve(['a']);await turn();
  h.q.select(conditions({text:'f'}));await turn();h.q.select(conditions({text:'final'}));await turn();
  assert.equal(h.calls.length,1);assert.deepEqual(h.last().ids,[]);
  await new Promise(r=>setTimeout(r,45));assert.equal(h.calls.length,2);assert.equal(h.calls[1].c.text,'final');
  h.calls[1].resolve(['b']);await turn();
  h.q.select(conditions({text:'final',generation:1}));assert.equal(h.last().refreshing,true);assert.deepEqual(h.last().ids,['b']);
  await turn();assert.equal(h.calls.length,3);h.calls[2].resolve(['c']);await turn();
  h.q.select(conditions({text:'final',generation:1,section:'已收藏'}));await turn();
  assert.equal(h.calls.length,4);assert.deepEqual(h.last().ids,[]);h.q.dispose();
});
test('failures clear confirmed membership and explicit retry is a new read',async()=>{
  const h=harness();h.q.select(conditions());await turn();h.calls[0].resolve(['a']);await turn();
  h.q.select(conditions({generation:1}));await turn();h.calls[1].reject(new Error('budget'));
  await turn();assert.equal(h.last().phase,'failed');assert.deepEqual(h.last().ids,[]);
  assert.match(h.last().error,/budget/);h.q.retry();await turn();assert.equal(h.calls.length,3);
  h.calls[2].resolve([]);await turn();assert.equal(h.last().phase,'ready');assert.deepEqual(h.last().ids,[]);h.q.dispose();
});
test('obsolete failure, invalidation and disposal cannot publish or replay pending work',async()=>{
  const h=harness();h.q.select(conditions());await turn();h.q.select(conditions({section:'小项目'}));await turn();
  h.calls[0].reject(new Error('obsolete'));await turn();assert.equal(h.calls.length,2);assert.notEqual(h.last().phase,'failed');
  h.q.invalidate();const count=h.states.length;h.calls[1].resolve(['old']);await turn();assert.equal(h.states.length,count);
  h.q.select(conditions());await turn();h.q.select(conditions({section:'实验室'}));await turn();
  h.q.dispose();const stopped=h.states.length;h.calls[2].resolve(['closed']);await turn();
  assert.equal(h.calls.length,3);assert.equal(h.states.length,stopped);
});
test('selection owns conditions and an unchanged view does not duplicate requests',async()=>{
  const h=harness(),c=conditions();h.q.select(c);c.section='external-change';await turn();
  assert.equal(h.calls[0].c.section,'概览');h.q.select(conditions());await turn();assert.equal(h.calls.length,1);
  h.calls[0].resolve(['a']);await turn();h.q.select(conditions());await turn();assert.equal(h.calls.length,1);h.q.dispose();
});
