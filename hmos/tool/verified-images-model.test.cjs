const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {test} = require('node:test'), assert = require('node:assert/strict');
const ts = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../entry/src/main/ets/model/VerifiedImages.ets'), 'utf8');
const compiled = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText;
const settle = async () => {for(let i=0;i<16;i++)await Promise.resolve();};
function fixture(){
  const api={},calls=[],released=[];vm.runInNewContext(compiled,{exports:api,Error});
  const model=new api.VerifiedImages(target=>new Promise((resolve,reject)=>calls.push({target,resolve,reject})),async token=>released.push(token),()=>{});
  const target=(id='A',revision='1')=>({id,name:id+'.png',request:JSON.stringify({id,revision}),byte_length:'3'});
  const lease=(token='T')=>({token,path:'/cache/'+token,uri:'file://bundle/cache/'+token});
  return {model,calls,released,target,lease};
}
test('same asset references deduplicate and frozen exact request survives caller mutation',async()=>{
  const f=fixture(),target=f.target();f.model.update([target,target]);target.request='changed';
  assert.equal(f.calls.length,1);assert.notEqual(f.calls[0].target.request,'changed');f.calls[0].resolve(f.lease());await settle();
  assert.equal(f.model.views().length,1);assert.equal(f.model.views()[0].phase,'ready');
  f.model.update([f.target()]);assert.equal(f.calls.length,1);assert.deepEqual(f.released,[]);
});
test('replacement during native read releases late token and serializes next request',async()=>{
  const f=fixture();f.model.update([f.target()]);f.model.update([f.target('A','2')]);assert.equal(f.calls.length,1);
  f.calls[0].resolve(f.lease('OLD'));await settle();assert.deepEqual(f.released,['OLD']);assert.equal(f.calls.length,2);
  assert.equal(f.model.views()[0].path,'');f.calls[1].resolve(f.lease('NEW'));await settle();assert.equal(f.model.views()[0].path,'/cache/NEW');
});
test('removal immediately hides prior pixels and releases only removed image',async()=>{
  const f=fixture();f.model.update([f.target(),f.target('B')]);f.calls[0].resolve(f.lease('A'));await settle();
  f.calls[1].resolve(f.lease('B'));await settle();f.model.update([f.target('B')]);
  assert.deepEqual(f.released,['A']);assert.equal(f.model.views().length,1);assert.equal(f.model.views()[0].id,'B');
});
test('dispose releases current and subsequently completed lease without publishing',async()=>{
  const f=fixture();f.model.update([f.target(),f.target('B')]);f.calls[0].resolve(f.lease('A'));await settle();f.model.dispose();
  f.calls[1].resolve(f.lease('B'));await settle();assert.deepEqual(f.released,['A','B']);assert.equal(f.model.views().length,0);
  f.model.update([f.target('C')]);assert.equal(f.calls.length,2);
});
test('decode failure retains explicit retry and ignores an old path callback',async()=>{
  const f=fixture();f.model.update([f.target()]);f.calls[0].resolve(f.lease());await settle();
  f.model.decodeFailed('A','/cache/OLD');assert.equal(f.model.views()[0].phase,'ready');
  f.model.decodeFailed('A','/cache/T');assert.equal(f.model.views()[0].phase,'failed');assert.equal(f.model.views()[0].path,'');assert.deepEqual(f.released,['T']);
  f.model.update([f.target()]);assert.equal(f.calls.length,1);f.model.retry('A');assert.equal(f.calls.length,2);
  f.calls[1].resolve(f.lease('NEXT'));await settle();f.model.decodeFailed('A','/cache/T');assert.equal(f.model.views()[0].phase,'ready');
});
test('read failure never automatically retries and does not block other image requests',async()=>{
  const f=fixture();f.model.update([f.target(),f.target('B')]);f.calls[0].reject(new Error('hash mismatch'));await settle();
  assert.equal(f.model.views()[0].error,'hash mismatch');assert.equal(f.calls.length,2);f.calls[1].resolve(f.lease('B'));await settle();
  assert.equal(f.model.views()[1].phase,'ready');f.model.update([f.target(),f.target('B')]);assert.equal(f.calls.length,2);
});
test('malformed lease cannot publish and known token is released',async()=>{
  const f=fixture();f.model.update([f.target()]);f.calls[0].resolve({token:'BAD',path:'/cache/BAD',uri:''});await settle();
  assert.deepEqual(f.released,['BAD']);assert.equal(f.model.views()[0].phase,'failed');assert.equal(f.model.views()[0].path,'');
});
test('caller cannot mutate display snapshot to inject an image source',async()=>{
  const f=fixture();f.model.update([f.target()]);f.calls[0].resolve(f.lease());await settle();const view=f.model.views()[0];view.uri='https://injected';view.path='/other';
  assert.equal(f.model.views()[0].path,'/cache/T');assert.equal(f.model.views()[0].uri,'file://bundle/cache/T');
});
