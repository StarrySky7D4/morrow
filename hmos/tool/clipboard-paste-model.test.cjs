'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),{test}=require('node:test');
const ts=require(process.env.HMOS_TYPESCRIPT||'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modules=new Map();
function load(name){if(modules.has(name))return modules.get(name);const exports={};modules.set(name,exports);
 const source=path.resolve(__dirname,'../entry/src/main/ets/model/'+name+'.ets');
 const compiled=ts.transpileModule(fs.readFileSync(source,'utf8'),{fileName:source,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020},reportDiagnostics:true});
 assert.equal((compiled.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error).length,0);
 vm.runInNewContext(compiled.outputText,{exports,require:n=>load(n.replace(/^\.\//,'')),setTimeout,clearTimeout,encodeURIComponent});return exports;}
const {ClipboardPaste}=load('ClipboardPaste'),{TextValue,copyText}=load('EditorDraft');
const imageId='asset-draft-import-'+ 'b'.repeat(64),reference='clipboard-'+ 'a'.repeat(64)+'-image-1.png';
function plan(){return {text:'Start\n\n![图片](attachment:'+reference+')',warnings:['original retained'],attachments:[
 {key:'source',name:'rich.html',byte_length:'111',sha256:'a'.repeat(64),references:[],image:false},
 {key:'image',name:reference,byte_length:'68',sha256:'c'.repeat(64),references:[reference],image:true}]};}
function fixture(options={}){let boundary={editor_id:'editor',card_id:'card',draft_id:'draft',generation:'2',selected_count:0,import_count:0,target:new TextValue(),blocked:false};
 boundary.target.text='before after';boundary.target.selection_base=7;boundary.target.selection_extent=12;
 const calls=[],hooks={read:()=>({...boundary,target:copyText(boundary.target)}),operation:()=> 'operation-'+(calls.length+1),
 confirm:async()=>hooks.read(),importOne:async(item,identity)=>{calls.push({kind:'import',item,identity});
  if(options.importOverride)return options.importOverride(item,identity,boundary,hooks);
  boundary={...boundary,generation:String(Number(boundary.generation)+1),selected_count:boundary.selected_count+1,import_count:boundary.import_count+1};
  return {outcome:'selected',identity:{...identity},asset_id:item.key==='image'?imageId:'asset-draft-import-'+'d'.repeat(64),next:hooks.read()};},
 commitText:async value=>{calls.push({kind:'text',value:copyText(value)});boundary={...boundary,target:copyText(value),generation:String(Number(boundary.generation)+1)};return hooks.read();}};
 return {coordinator:new ClipboardPaste(hooks),hooks,calls,read:()=>hooks.read(),change:values=>boundary={...boundary,...values}};}
test('exact pins are confirmed sequentially before portable text and caret persist',async()=>{
 const f=fixture(),result=await f.coordinator.run(plan(),20000);assert.equal(result.phase,'complete');assert.equal(result.selected,2);assert.equal(result.text_inserted,true);
 assert.deepEqual(f.calls.map(x=>x.kind),['import','import','text']);assert.equal(f.calls[0].identity.expected_generation,'2');assert.equal(f.calls[1].identity.expected_generation,'3');
 assert.equal(f.read().target.text,'before Start\n\n![图片](attachment:'+imageId+')');assert.equal(f.read().target.selection_base,f.read().target.text.length);
 assert.equal(f.read().target.text.includes(reference),false);
});
test('full expanded plan and final insertion budget are checked before any import',async()=>{
 for(const setup of [f=>f.change({selected_count:19}),f=>f.change({import_count:19}),f=>{const value=f.read().target;value.composing_start=0;f.change({target:value});}]){
  const f=fixture();setup(f);const result=await f.coordinator.run(plan(),4);assert.equal(result.phase,'stopped');assert.equal(f.calls.length,0);
 }
 const f=fixture(),p=plan();p.attachments[1].references=p.attachments[0].references=['collision'];const result=await f.coordinator.run(p,20000);assert.equal(result.phase,'stopped');assert.equal(f.calls.length,0);
});
test('target selection or raw text changing during confirmation never imports',async()=>{
 for(const mutation of [value=>value.text+='new',value=>value.selection_base=0,value=>value.composing_start=0]){
  const f=fixture();f.hooks.confirm=async()=>{const value=f.read().target;mutation(value);f.change({target:value});return f.read();};
  const result=await f.coordinator.run(plan(),20000);assert.equal(result.phase,'stopped');assert.equal(f.calls.length,0);
 }
});
test('wrong owner, operation receipt, pin ID or target cannot authorize the next import or text',async()=>{
 const mutations=[receipt=>receipt.identity.operation_id='other',receipt=>receipt.next.card_id='other',receipt=>receipt.asset_id='business-alias',receipt=>receipt.next.target.text='new'];
 for(const mutation of mutations){const f=fixture({importOverride:async(item,identity,boundary,hooks)=>{
  const next={...hooks.read(),generation:'3',selected_count:1,import_count:1};const receipt={outcome:'selected',identity:{...identity},asset_id:imageId,next};mutation(receipt);return receipt;}});
  const result=await f.coordinator.run(plan(),20000);assert.equal(result.phase,'stopped');assert.equal(f.calls.length,1);assert.equal(result.text_inserted,false);
 }
});
test('Unknown stops later admissions without a native replay or placeholder insertion',async()=>{
 const f=fixture({importOverride:async(item,identity)=>({outcome:'unknown',identity,asset_id:'',next:undefined})});
 const result=await f.coordinator.run(plan(),20000);assert.equal(result.phase,'stopped');assert.equal(f.calls.length,1);assert.equal(f.read().target.text,'before after');
});
test('stop while one native import is live observes that result and admits nothing else',async()=>{
 let release;const waiting=new Promise(resolve=>release=resolve);const f=fixture({importOverride:async(item,identity,boundary,hooks)=>{
  await waiting;f.change({generation:'3',selected_count:1,import_count:1});return {outcome:'selected',identity,asset_id:imageId,next:hooks.read()};}});
 const first=f.coordinator.run(plan(),20000),second=f.coordinator.run(plan(),20000);assert.equal(first,second);
 await new Promise(resolve=>setTimeout(resolve,0));assert.equal(f.calls.length,1);f.coordinator.stop();release();const result=await first;
 assert.equal(result.phase,'stopped');assert.equal(result.selected,1);assert.equal(f.calls.length,1);
});
test('copy plan metadata before awaits and refuse a text save without a new confirmed generation',async()=>{
 const f=fixture(),p=plan(),run=f.coordinator.run(p,20000);p.text='mutated';p.attachments[1].name='other';assert.equal((await run).phase,'complete');assert.equal(f.calls[1].item.name,reference);
 const stopped=fixture();stopped.hooks.commitText=async value=>({...stopped.read(),target:value});const result=await stopped.coordinator.run(plan(),20000);
 assert.equal(result.phase,'stopped');assert.equal(result.text_inserted,false);assert.equal(result.selected,2);
});
