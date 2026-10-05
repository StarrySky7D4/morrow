// Drives only observed controls in the development app. A seed is a new named
// fixture, never a retry of an issued business mutation. No app files are read.
const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const out=process.env.HMOS_REPORT_DIR||path.resolve(__dirname,'../reports/ui-source/v10/device');fs.mkdirSync(out,{recursive:true});
const device=process.env.HMOS_DEVICE||'127.0.0.1:5557',hdc='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const title=process.env.HMOS_FIXTURE_TITLE||'HMOS-markdown-20261005-A';
const markdown='# Markdown Hello 😀\n\n**Bold** and *italic* and ~~old~~ and `inline`.\n\n3. First\n\n   Continuation\n\n   - [x] Child\n\n> A quote\n\n```rust\nlet x = 1;\n\nlet y = 2;\n```\n\n| Left | Right |\n| :--- | ---: |\n| 中文😀 | 42 |\n\n<script>inert()</script>\n\n[unsafe](javascript:alert) ![local](file:///tmp/example.png)';
const tsv='Name\tValue\n中文😀\t42\nSecond\t保留',table='| Name | Value |\n| --- | --- |\n| 中文😀 | 42 |\n| Second | 保留 |';
const checks=[];function pass(name){checks.push(name);fs.writeFileSync(path.join(out,'progress.json'),JSON.stringify({device,title,checks},null,2)+'\n');console.log('PASS '+name);}
const view=()=>d.flatten(d.layout()),nodes=()=>view().map(x=>x.n),id=value=>nodes().find(n=>n.id===value),text=value=>nodes().find(n=>n.text===value);
function click(n){assert.ok(n,'observed control');d.click(n);}
function key(...values){cp.execFileSync(hdc,['-t',device,'shell','uitest','uiInput','keyEvent',...values.map(String)]);}
function capture(name){assert.ok(!fs.existsSync(path.join(out,name+'.png')),'fresh evidence name');fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(d.layout(),null,2)+'\n');d.run('screenshot','--path',path.join(out,name+'.png'));}
// UITest elides non-accessible Row/Column containers. The editor is the last
// ordinary Scroll; its explicitly identified preview/table scrolls are nested.
function editorScroll(v){return v.filter(x=>x.n.type==='Scroll'&&x.n.id!=='markdown-preview'&&x.n.id!=='markdown-table').at(-1)?.n;}
function seek(predicate,label,up=false){for(let i=0;i<7;i++){const v=view(),item=v.find(x=>predicate(x.n));if(item)return item.n;const s=editorScroll(v);assert.ok(s);const [l,t,r,b]=s.bounds;d.run('swipe',String(r-12),String(up?t+70:b-70),String(r-12),String(up?b-70:t+70));}throw Error('visible control '+label);}
function input(field,value){const control=seek(n=>n.id===field,field,field==='draft-title');click(control);key(2072,2017);
  const script=path.join(out,'input-text.sh');fs.writeFileSync(script,"#!/bin/sh\nvalue=$(printf '%s' '"+Buffer.from(value).toString('base64')+"' | base64 -d; printf '.')\nexec uitest uiInput text \"${value%.}\"\n");
  cp.execFileSync(hdc,['-t',device,'file','send',script,'/data/local/tmp/hmos-dev10-input.sh']);const result=cp.execFileSync(hdc,['-t',device,'shell','sh','/data/local/tmp/hmos-dev10-input.sh'],{encoding:'utf8'});
  assert.ok(!/error|fail/i.test(result)||/no error/i.test(result));key('Back');assert.equal(id(field)?.text,value,'full native field readback');}
function copy(field){click(seek(n=>n.id===field,field));key(2072,2017);key(2072,2019);key('Back');}
function toggle(){const item=view().find(x=>x.n.text===String.fromCodePoint(0xf4a1)&&x.parents.some(p=>p.type==='Button'));assert.ok(item,'observed preview control');click(item.n);}
function paste(){click(seek(n=>n.id==='editor-paste','authorized PasteButton',true));assert.ok(!text('剪贴板读取未获授权，请再次点击粘贴按钮。'),'real PasteButton authorization');}
function ready(){for(let i=0;i<6;i++){if(id('draft-status')?.text==='草稿已保留')return;}throw Error('latest journal ack missing');}
function restart(){cp.execFileSync(hdc,['-t',device,'shell','aa','force-stop','dev.morrow.hmos']);cp.execFileSync(hdc,['-t',device,'shell','aa','start','-a','EntryAbility','-b','dev.morrow.hmos']);assert.ok(text('新建灵感'));}
function drafts(){click(nodes().find(n=>n.type==='Button'&&/^草稿(?: [0-9]+)?$/.test(n.text||'')));assert.ok(text('保留的草稿'));}
function row(){return nodes().filter(n=>n.id?.startsWith('draft-row:')).find(n=>d.flatten([n]).some(x=>x.n.text===title));}
function previewTests(alreadyOpen=false){if(!alreadyOpen)toggle();assert.ok(text('Markdown Hello 😀'));capture('formatted-preview');const seen=new Set(),allText=new Set();
  for(let i=0;i<6;i++){const current=nodes();for(const n of current){if(n.id?.startsWith('markdown-block:'))seen.add(n.id.split(':').at(-1));if(n.text)allText.add(n.text);}if(seen.has('table')&&allText.has('图片未导入 · local'))break;const p=id('markdown-preview');assert.ok(p);const [l,t,r,b]=p.bounds;d.run('swipe',String(r-8),String(b-40),String(r-8),String(t+60));}
  assert.ok(seen.has('heading')&&seen.has('code')&&seen.has('table'));capture('code-table-preview');pass('actual native headings, styles, list, quote, code and table projection');assert.ok([...allText].some(value=>value.trim()==='<script>inert()</script>'));assert.ok(allText.has('图片未导入 · local'));pass('HTML stays literal and local images remain inert');toggle();}
function pasteTests(start='table'){if(start==='table'){input('draft-description',tsv);copy('draft-description');paste();assert.ok(id('markdown-preview'));toggle();assert.equal(id('draft-description')?.text,table);capture('tsv-body');pass('system-authorized clipboard TSV converts completely and replaces selected body');}
  input('draft-description','汉字😀');copy('draft-description');click(seek(n=>n.id==='draft-title','paste target title',true));key(2072,2017);key('Back');paste();assert.equal(id('draft-title')?.text,'汉字😀');pass('last-focused title uses raw Unicode clipboard text');input('draft-title',title);
  input('draft-description','x'.repeat(61));copy('draft-description');click(seek(n=>n.id==='draft-title','paste target title',true));key(2072,2017);key('Back');paste();assert.equal(id('draft-title')?.text,title);assert.ok(nodes().some(n=>n.text?.includes('内容未插入')));pass('oversized title paste is refused without altering title');
  input('draft-description',table);ready();click(text('保留草稿'));restart();drafts();const saved=row();assert.ok(saved);click(d.flatten([saved]).find(x=>x.n.text==='恢复编辑')?.n);assert.equal(id('draft-description')?.text,table);assert.equal(id('draft-title')?.text,title);pass('complete converted body survives acknowledged journal and process restart');toggle();assert.ok(id('markdown-table'));capture('restored-table-preview');toggle();
  click(text('保存灵感'));assert.ok(!id('draft-title'));const field=nodes().find(n=>n.type==='TextInput');click(field);d.run('text',title,String(Math.round((field.bounds[0]+field.bounds[2])/2)),String(Math.round((field.bounds[1]+field.bounds[3])/2)));key('Back');
  const card=nodes().filter(n=>n.id?.startsWith('workspace-card:')).find(n=>d.flatten([n]).some(x=>x.n.text===title));assert.ok(card);drafts();assert.ok(!row());click(text('关闭'));pass('explicit save creates one business identity and retires its raw journal');
  const result={result:'PASS',device,title,cardId:card.id,checks};fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
}
if(process.argv.includes('--readonly')) {
 const accepted=JSON.parse(fs.readFileSync(path.join(out,'result.json')));assert.equal(accepted.title,title);assert.ok(!id('draft-title'));
 const card=nodes().find(n=>n.id===accepted.cardId);assert.ok(card);const notice=nodes().find(n=>n.text?.startsWith('已保存 ·'));
 if(notice){const close=nodes().find(n=>n.text==='×'&&Math.abs((n.bounds[1]+n.bounds[3])-(notice.bounds[1]+notice.bounds[3]))<30);click(close);}
 click(d.flatten([card]).find(x=>x.n.text===title)?.n);assert.equal(seek(n=>n.id==='draft-description','confirmed raw body').text,table);toggle();assert.ok(id('markdown-table'));capture('existing-readonly-preview');click(text('保留草稿'));drafts();assert.ok(!row(),'view only must not create a journal');click(text('关闭'));pass('view and preview of confirmed business body consumes no draft');
 restart();drafts();assert.ok(!row());click(text('关闭'));pass('read-only preview remains without a journal after process restart');
 const result={result:'PASS',device,title,cardId:accepted.cardId,checks};fs.writeFileSync(path.join(out,'readonly-result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
} else if(process.argv.includes('--seed')){
 assert.ok(!id('draft-title'));drafts();assert.ok(!row());click(text('关闭'));const field=nodes().find(n=>n.type==='TextInput');assert.ok(!field.text,'fresh query');click(field);d.run('text',title,String(Math.round((field.bounds[0]+field.bounds[2])/2)),String(Math.round((field.bounds[1]+field.bounds[3])/2)));key('Back');assert.ok(!nodes().some(n=>n.id?.startsWith('workspace-card:')),'no business fixture replay');click(text('×'));click(text('新建灵感'));input('draft-title',title);input('draft-description',markdown);previewTests();pasteTests();
} else if(process.argv.includes('--continue-title')) {
 const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json')));assert.equal(prior.title,title);assert.equal(prior.checks.length,3);checks.push(...prior.checks);assert.equal(id('draft-description')?.text,'汉字😀');assert.equal(seek(n=>n.id==='draft-title','fixture title',true).text,title);pasteTests('title');
} else if(process.argv.includes('--continue-inert')) {
 const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json')));assert.equal(prior.title,title);assert.equal(prior.checks.length,1);checks.push(...prior.checks);
 assert.ok(nodes().some(n=>n.text?.trim()==='<script>inert()</script>'));assert.ok(text('图片未导入 · local'));pass('HTML stays literal and local images remain inert');
 const p=id('markdown-preview'),[l,t,r,b]=p.bounds;d.run('swipe',String(r-8),String(t+80),String(r-8),String(t+380));assert.ok(nodes().some(n=>n.text?.includes('let x = 1;')&&n.text?.includes('let y = 2;')),'complete code block visible');capture('code-block-preview');toggle();pasteTests();
} else if(process.argv.includes('--continue-preview')) {
 assert.ok(id('markdown-preview'));assert.ok(text('Markdown Hello 😀'));previewTests(true);assert.equal(seek(n=>n.id==='draft-title','fixture title',true).text,title);pasteTests();
} else if(process.argv.includes('--continue-paste')) {
 assert.equal(id('draft-title')?.text,title);assert.equal(id('draft-description')?.text,markdown);const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json')));assert.equal(prior.title,title);assert.equal(prior.checks.length,2);checks.push(...prior.checks);pasteTests();
} else {throw Error('explicit fresh --seed or inspected --continue-paste required');}
