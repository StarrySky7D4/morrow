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
const draft=load('EditorDraft'),{insertPaste,samePasteTarget}=load('EditorPaste');
function value(text,a=-1,b=-1){const v=new draft.TextValue();v.text=text;v.selection_base=a;v.selection_extent=b;return v;}
test('full Unicode, leading spaces and trailing LF survive an unset selection',()=>{
  const initial=value('before: '),out=insertPaste(initial,'  中文 😀\nend\n',100);
  assert.equal(out.text,'before:   中文 😀\nend\n');assert.equal(initial.text,'before: ');
  assert.equal(out.selection_base,out.text.length);assert.equal(out.selection_extent,out.text.length);
});
test('reversed UTF16 selection replaces only the selected emoji',()=>{
  const initial=value('A😀B',3,1),out=insertPaste(initial,'汉字',8);
  assert.equal(out.text,'A汉字B');assert.equal(out.selection_base,3);assert.equal(initial.selection_base,3);
});
test('a caret at a valid UTF16 boundary inserts without replacing neighbors',()=>{
  const out=insertPaste(value('😀end',2,2),'\n',20);assert.equal(out.text,'😀\nend');assert.equal(out.selection_base,3);
});
test('the complete resulting field is limited, never truncated',()=>{
  assert.equal(insertPaste(value('12345',1,4),'abc',5).text,'1abc5');
  assert.throws(()=>insertPaste(value('12345',1,4),'abcd',5),/内容未插入/);
  assert.throws(()=>insertPaste(value(''), '😀'.repeat(31),60),/60/);
});
test('live composing text cannot be replaced by a clipboard result',()=>{
  const initial=value('汉字',0,2);initial.composing_start=0;initial.composing_end=2;
  assert.throws(()=>insertPaste(initial,'new',100),/输入法/);assert.equal(initial.text,'汉字');
});
test('invalid or partial selection sentinels are rejected',()=>{
  for(const [a,b] of [[-1,0],[0,-1],[-2,-2],[0,9],[.5,1],[NaN,1]])assert.throws(()=>insertPaste(value('abc',a,b),'x',20),/选区/);
});
test('a surrogate cannot be split while pasting',()=>{
  for(const [a,b] of [[1,1],[0,1],[1,2]])assert.throws(()=>insertPaste(value('😀abc',a,b),'x',20),/字符内部/);
});
test('a frozen paste target detects raw text, selection and composing drift',()=>{
  const initial=value('😀text',2,3);assert.equal(samePasteTarget(initial,draft.copyText(initial)),true);
  for(const [key,next] of [['text','changed'],['selection_base',0],['selection_extent',4],['composing_start',0],['composing_end',1]]){
    const changed=draft.copyText(initial);changed[key]=next;assert.equal(samePasteTarget(initial,changed),false);
  }
});
