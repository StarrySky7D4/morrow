const fs=require('node:fs'),vm=require('node:vm'),path=require('node:path'),assert=require('node:assert/strict');
const {test}=require('node:test');
const ts=require(process.env.HMOS_TYPESCRIPT||'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/model/CardDataSource.ets'),'utf8');
const exportsObject={};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020,experimentalDecorators:true}}).outputText,{exports:exportsObject,Observed:target=>target});
const {CardDataSource}=exportsObject;
const card=(id,revision='1',title=id)=>({id,revision,title});
// Model a renderer retaining keyed nodes across reloads. Only data-change
// notifications invalidate retained content; a reload alone can leave it stale.
function renderer(source){
  const cache=new Map(),events=[];
  const listener={onDataReloaded(){
    const keys=new Set(Array.from({length:source.totalCount()},(_,i)=>source.getData(i).key));
    for(const key of cache.keys())if(!keys.has(key))cache.delete(key);
    events.push('reload');
  },onDataChange(index){cache.delete(source.getData(index).key);events.push(index);}};
  source.registerDataChangeListener(listener);
  return {events,listener,paint(){return Array.from({length:source.totalCount()},(_,i)=>{
    const item=source.getData(i);if(!cache.has(item.key))cache.set(item.key,item.card.title);return [item.key,cache.get(item.key)];
  });}};
}
test('same identity receives new content without invalidating unrelated cards',()=>{
  const s=new CardDataSource(),r=renderer(s);s.replace('overview',[card('a'),card('b')]);r.paint();r.events.length=0;const retained=s.getData(0);
  s.replace('overview',[card('a','2','edited'),card('b')]);
  assert.deepEqual(r.paint(),[['overview:a','edited'],['overview:b','b']]);assert.deepEqual(r.events,[0]);
  assert.equal(s.getData(0),retained);assert.equal(retained.card.title,'edited');
});
test('reordered results refresh revised retained identities and discard removed entries',()=>{
  const s=new CardDataSource(),r=renderer(s);s.replace('overview',[card('a'),card('b'),card('c')]);r.paint();
  s.replace('overview',[card('b','2','favorite'),card('a'),card('d')]);
  assert.deepEqual(r.paint(),[['overview:b','favorite'],['overview:a','a'],['overview:d','d']]);
});
test('moved identities invalidate positional height caches even without a content revision',()=>{
  const s=new CardDataSource(),r=renderer(s);s.replace('overview',[card('a'),card('b'),card('c')]);
  const retained=s.getData(0);r.events.length=0;
  s.replace('overview',[card('b'),card('a'),card('c')]);
  assert.equal(s.getData(1),retained);assert.deepEqual(r.events,['reload',0,1]);
});
test('page identity separates different card presentations and empty queries clear visible data',()=>{
  const s=new CardDataSource(),r=renderer(s);s.replace('overview',[card('a')]);r.paint();
  s.replace('projects',[card('a')]);assert.deepEqual(r.paint(),[['projects:a','a']]);
  s.replace('projects',[]);assert.deepEqual(r.paint(),[]);
  s.replace('projects',[card('a','2','restored')]);assert.deepEqual(r.paint(),[['projects:a','restored']]);
});
test('identical queries do not invalidate nodes and detached views receive no events',()=>{
  const s=new CardDataSource(),r=renderer(s);s.replace('overview',[card('a')]);r.events.length=0;
  s.replace('overview',[card('a')]);assert.deepEqual(r.events,[]);
  s.unregisterDataChangeListener(r.listener);s.replace('overview',[card('a','2')]);assert.deepEqual(r.events,[]);
});
