// Visible controls only. Each seed creates named development fixtures exactly
// once. An interrupted run resumes from an inspected stage, never by replay.
process.env.HMOS_DEVICE=process.env.HMOS_DEVICE||'127.0.0.1:5557';
const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const out=process.env.HMOS_REPORT_DIR||path.resolve(__dirname,'../reports/ui-source/v11/device');fs.mkdirSync(out,{recursive:true});
const device=process.env.HMOS_DEVICE||'127.0.0.1:5557',hdc='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const prefix='HMOS-flow-20261005-',names=['A','B','C'].map(x=>prefix+x),checks=[];
const view=()=>d.flatten(d.layout()),nodes=()=>view().map(x=>x.n),id=value=>nodes().find(n=>n.id===value),text=value=>nodes().filter(n=>n.text===value).at(-1);
function click(n){assert.ok(n,'observed control');assert.ok(n.bounds[3]-n.bounds[1]>12,'control visible');d.click(n);}
function key(...values){cp.execFileSync(hdc,['-t',device,'shell','uitest','uiInput','keyEvent',...values.map(String)]);}
function capture(name){assert.ok(!fs.existsSync(path.join(out,name+'.png')),'fresh evidence');fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(d.layout(),null,2)+'\n');d.run('screenshot','--path',path.join(out,name+'.png'));}
function pass(name){checks.push(name);fs.writeFileSync(path.join(out,'progress.json'),JSON.stringify({device,prefix,checks},null,2)+'\n');console.log('PASS '+name);}
function seek(predicate,label,up=false,which='workspace'){
 for(let i=0;i<12;i++){const v=view(),item=v.find(x=>predicate(x.n)&&x.n.bounds[3]-x.n.bounds[1]>18);if(item)return item.n;
  const list=v.filter(x=>x.n.type==='Scroll');const s=which==='detail'?v.find(x=>x.n.id==='detail-scroll')?.n:which==='editor'?list.at(-1)?.n:list[0]?.n;
  assert.ok(s,'scroll '+label);const [l,t,r,b]=s.bounds;const direction=i<6?up:!up;d.run('swipe',String(r-12),String(direction?t+90:b-90),String(r-12),String(direction?b-90:t+90));}
 throw Error('visible '+label);
}
function inputControl(control,value){click(control);key(2072,2017);const script=path.join(out,'input-text.sh');
 fs.writeFileSync(script,"#!/bin/sh\nvalue=$(printf '%s' '"+Buffer.from(value).toString('base64')+"'|base64 -d; printf '.')\nexec uitest uiInput text \"${value%.}\"\n");
 cp.execFileSync(hdc,['-t',device,'file','send',script,'/data/local/tmp/hmos-dev11-input.sh']);const r=cp.execFileSync(hdc,['-t',device,'shell','sh','/data/local/tmp/hmos-dev11-input.sh'],{encoding:'utf8'});assert.ok(!/error|fail/i.test(r)||/no error/i.test(r));key('Back');
}
function input(field,value){inputControl(seek(n=>n.id===field,field,field==='draft-title','editor'),value);assert.equal(id(field)?.text,value);}
function search(value){const field=nodes().find(n=>n.type==='TextInput');assert.ok(field&&!id('draft-title'));inputControl(field,value);for(let i=0;i<4;i++)nodes();}
function dismissNotice(){const list=nodes();if(!list.some(n=>n.id==='card-detail'||n.id==='draft-title')){
 const close=list.find(n=>n.type==='Text'&&n.text==='×'&&n.bounds[1]>500);if(close)click(close);}}
function cards(){return nodes().filter(n=>n.id?.startsWith('workspace-card:'));}
function card(name,up=true){seek(n=>n.type==='Text'&&n.text===name,name,up);const match=view().find(x=>x.n.type==='Text'&&x.n.text===name);const parent=match.parents.find(p=>p.id?.startsWith('workspace-card:'));assert.ok(parent);return parent;}
function menu(name){dismissNotice();const c=card(name);click(seek(n=>n.id==='card-menu:'+c.id.slice('workspace-card:'.length),'card menu',false));}
function action(name,value){menu(name);click(text(value));for(let i=0;i<3;i++)nodes();}
function sort(value){click(seek(n=>n.id==='card-sort','sort',true));click(text(value));for(let i=0;i<4;i++)nodes();}
function order(){const seen=[];for(let i=0;i<10;i++){for(const c of cards()){const t=d.flatten([c]).map(x=>x.n.text).find(x=>names.includes(x));if(t&&!seen.includes(t))seen.push(t);}if(seen.length===3)break;
 const s=view().find(x=>x.n.type==='Scroll')?.n;assert.ok(s);d.run('swipe',String(s.bounds[2]-15),String(s.bounds[3]-90),String(s.bounds[2]-15),String(s.bounds[1]+90));}return seen;}
function restart(){cp.execFileSync(hdc,['-t',device,'shell','aa','force-stop','dev.morrow.hmos']);cp.execFileSync(hdc,['-t',device,'shell','aa','start','-a','EntryAbility','-b','dev.morrow.hmos']);assert.ok(text('新建灵感'));}
function assertNoDraft(target){click(nodes().find(n=>n.type==='Button'&&/^草稿(?: [0-9]+)?$/.test(n.text||'')));assert.ok(text('保留的草稿'));assert.ok(!view().some(x=>x.n.text===target&&x.parents.some(p=>p.id?.startsWith('draft-row:'))));click(text('关闭'));}
function readOnly(){dismissNotice();search('HMOS-markdown-20261005-A');click(seek(n=>n.type==='Text'&&n.text==='HMOS-markdown-20261005-A','existing business'));
 assert.ok(id('detail-title'));assert.ok(!id('draft-title'));assert.ok(id('markdown-table'));capture('reading-markdown');click(id('detail-copy'));assert.ok(text('已复制卡片文字'));
 click(id('detail-close'));assertNoDraft('HMOS-markdown-20261005-A');restart();assertNoDraft('HMOS-markdown-20261005-A');pass('Markdown reading and native copy create no journal, including restart');}
function seed(){search(prefix);assert.equal(cards().length,0,'no replay of named fixtures');assertNoDraft(names[0]);
 for(const name of names){click(text('新建灵感'));input('draft-title',name);input('draft-description','# '+name+'\n\nNative **reading** body.');click(text('保存灵感'));assert.ok(!id('draft-title'));}
 search(prefix);sort('标题');assert.deepEqual(order(),names);pass('three unique fixtures saved and title-sorted through actual editor');}
function ordering(){sort('标题');action(names[0],'后移卡片');card(names[1]);assert.deepEqual(order(),[names[1],names[0],names[2]]);capture('custom-order');pass('card-menu nudge enables manual order');
 action(names[0],'收藏');action(names[2],'收藏');click(seek(n=>n.text==='仅收藏','favorites filter',true));
 action(names[2],'前移卡片');click(seek(n=>n.text==='全部','all filter',true));card(names[1]);assert.deepEqual(order(),[names[1],names[2],names[0]]);pass('filtered nudge retains the hidden B card slot');
 restart();search(prefix);card(names[1],false);assert.deepEqual(order(),[names[1],names[2],names[0]]);pass('per-page manual order survives process restart');
}
function detailEditing(){click(seek(n=>n.type==='Text'&&n.text===names[1],names[1],true));assert.ok(id('detail-title'));capture('reading-detail');click(id('detail-edit'));assert.equal(id('draft-title')?.text,names[1]);
 input('draft-description','# Revised B\n\nKept **source** and 中文😀.');click(text('保存灵感'));assert.ok(!id('draft-title'));dismissNotice();click(seek(n=>n.type==='Text'&&n.text===names[1],names[1],true));assert.ok(text('Revised B'));capture('revised-detail');click(id('detail-close'));assertNoDraft(names[1]);pass('explicit detail edit saves one existing identity and refreshes Markdown');
 action(names[1],'转为项目');click(seek(n=>n.type==='Text'&&n.text===names[1],names[1],true));assert.ok(text('进行中'));click(id('detail-close'));pass('context menu moves inspiration to project through CAS');
}
function result(){const result={result:'PASS',device,prefix,checks};fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));}
if(require.main===module) {
 if(process.argv.includes('--readonly')){readOnly();result();}
 else if(process.argv.includes('--seed')){seed();ordering();detailEditing();result();}
 else if(process.argv.includes('--continue-seeded')){
  const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json')));assert.equal(prior.prefix,prefix);assert.equal(prior.checks.length,1);checks.push(...prior.checks);
  search(prefix);sort('标题');assert.deepEqual(order(),names);ordering();detailEditing();result();
 }
 else if(process.argv.includes('--continue-revised')){
  const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json')));assert.equal(prior.prefix,prefix);assert.equal(prior.checks.length,4);checks.push(...prior.checks);
  assert.ok(!id('draft-title'));dismissNotice();click(seek(n=>n.type==='Text'&&n.text===names[1],names[1],true));assert.equal(id('detail-title')?.text,names[1]);assert.ok(text('Revised B'));
  capture('revised-detail');click(id('detail-close'));assertNoDraft(names[1]);pass('explicit detail edit saves one existing identity and refreshes Markdown');
  action(names[1],'转为项目');dismissNotice();click(seek(n=>n.type==='Text'&&n.text===names[1],names[1],true));assert.ok(text('进行中'));capture('project-detail');click(id('detail-close'));pass('context menu moves inspiration to project through CAS');result();
 }
 else {throw Error('explicit --readonly or fresh --seed required');}
}
module.exports={d,view,nodes,id,text,click,key,capture,pass,seek,input,search,dismissNotice,cards,card,menu,action,sort,order,restart,assertNoDraft,names,prefix,out};
