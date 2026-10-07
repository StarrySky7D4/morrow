const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const assert=require('node:assert/strict'),{test}=require('node:test');
const ts=require(process.env.HMOS_TYPESCRIPT||'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modules=new Map();
function load(name){
  if(modules.has(name))return modules.get(name);
  const exports={};modules.set(name,exports);
  const source=fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/model/'+name+'.ets'),'utf8');
  const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText;
  vm.runInNewContext(compiled,{exports,require:n=>load(n.replace(/^\.\//,'')),setTimeout,clearTimeout});return exports;
}
const draft=load('EditorDraft'),{insertPaste,insertPasteChecked,samePasteTarget}=load('EditorPaste');
const {createPolicy,deferred}=require('./editor-field-test-harness.cjs');
function value(text,a=-1,b=-1){const v=new draft.TextValue();v.text=text;v.selection_base=a;v.selection_extent=b;return v;}
test('full Unicode, leading spaces and trailing LF survive an unset selection',()=>{
  const initial=value('before: '),out=insertPaste(initial,'  中文 😀\nend\n');
  assert.equal(out.text,'before:   中文 😀\nend\n');assert.equal(initial.text,'before: ');
  assert.equal(out.selection_base,out.text.length);assert.equal(out.selection_extent,out.text.length);
});
test('reversed UTF16 selection replaces only the selected emoji',()=>{
  const initial=value('A😀B',3,1),out=insertPaste(initial,'汉字');
  assert.equal(out.text,'A汉字B');assert.equal(out.selection_base,3);assert.equal(initial.selection_base,3);
});
test('a caret at a valid UTF16 boundary inserts without replacing neighbors',()=>{
  const out=insertPaste(value('😀end',2,2),'\n');assert.equal(out.text,'😀\nend');assert.equal(out.selection_base,3);
});
test('the complete resulting field is grapheme limited, never UTF16 truncated',async()=>{
  const h=createPolicy();
  assert.equal((await insertPasteChecked(value('x'.repeat(60),1,59),'😀'.repeat(58),'title',h.policy,h.owned)).text,'x'+'😀'.repeat(58)+'x');
  await assert.rejects(insertPasteChecked(value('x'.repeat(60),1,59),'😀'.repeat(59),'title',h.policy,h.owned),/60/);
  assert.equal((await insertPasteChecked(value(''),'😀'.repeat(60),'title',h.policy,h.owned)).text.length,120);
});
test('live composing text cannot be replaced by a clipboard result',()=>{
  const initial=value('汉字',0,2);initial.composing_start=0;initial.composing_end=2;
  assert.throws(()=>insertPaste(initial,'new'),/输入法/);assert.equal(initial.text,'汉字');
});
test('invalid or partial selection sentinels are rejected',()=>{
  for(const [a,b] of [[-1,0],[0,-1],[-2,-2],[0,9],[.5,1],[NaN,1]])assert.throws(()=>insertPaste(value('abc',a,b),'x'),/选区/);
});
test('a surrogate cannot be split while pasting',()=>{
  for(const [a,b] of [[1,1],[0,1],[1,2]])assert.throws(()=>insertPaste(value('😀abc',a,b),'x'),/字符内部/);
});
test('a frozen paste target detects raw text, selection and composing drift',()=>{
  const initial=value('😀text',2,3);assert.equal(samePasteTarget(initial,draft.copyText(initial)),true);
  for(const [key,next] of [['text','changed'],['selection_base',0],['selection_extent',4],['composing_start',0],['composing_end',1]]){
    const changed=draft.copyText(initial);changed[key]=next;assert.equal(samePasteTarget(initial,changed),false);
  }
});
test('new editing text, selection, affinity, direction or IME after an async check prevents insertion',async()=>{
  for(const [key,next] of [['text','new'],['selection_base',0],['selection_extent',0],['affinity',1],['directional',true],['composing_start',0]]){
    const wait=deferred(),h=createPolicy({wait:()=>wait.promise}),initial=value('existing',2,2);
    const pending=insertPasteChecked(initial,'😀','title',h.policy,h.owned);initial[key]=next;wait.resolve();
    await assert.rejects(pending,/变化/);
  }
});
test('async paste checks the complete prefix plus inserted text plus suffix',async()=>{
  const h=createPolicy(),initial=value('a'.repeat(59)+'z',59,59);
  await assert.rejects(insertPasteChecked(initial,'x','title',h.policy,h.owned),/超限/);
  assert.equal(h.requests[0].text,'a'.repeat(59)+'xz');assert.equal(initial.text,'a'.repeat(59)+'z');
});
test('replacing a selection can combine graphemes across the insertion boundary',async()=>{
  const h=createPolicy(),initial=value('e'+'x'.repeat(59),1,1);
  const result=await insertPasteChecked(initial,'\u0301','title',h.policy,h.owned);
  assert.equal(result.text.length,61);assert.equal(h.requests[0].text,'e\u0301'+'x'.repeat(59));assert.equal(result.selection_base,2);
});
test('policy worker failure rejects the full paste without changing the input',async()=>{
  const h=createPolicy({send:()=>{throw new Error('worker failed');}}),initial=value('original',0,8);
  await assert.rejects(insertPasteChecked(initial,'replacement','title',h.policy,h.owned),/worker failed/);assert.equal(initial.text,'original');assert.equal(h.requests.length,1);
});
