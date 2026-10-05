// Actual development HAP acceptance through observed UI. This creates one
// explicitly named fixture. On failure, inspect the live UI before continuing;
// never rerun --seed against an issued or already existing proposal.
const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const out=process.env.HMOS_REPORT_DIR||path.resolve(__dirname,'../reports/ui-source/v9/draft-device');fs.mkdirSync(out,{recursive:true});
const device=process.env.HMOS_DEVICE||'127.0.0.1:5557',hdc='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const title=process.env.HMOS_FIXTURE_TITLE||'HMOS-draft-20261005-A',body=process.env.HMOS_FIXTURE_RAW||'Raw draft line one\n第二行 😀 preserved',newer='Newer raw draft 😀 preserved after restart';
const nodes=()=>d.flatten(d.layout()).map(x=>x.n),byId=id=>nodes().find(n=>n.id===id),text=t=>nodes().find(n=>n.text===t);
const checks=[];function pass(name){checks.push(name);fs.writeFileSync(path.join(out,'progress.json'),JSON.stringify({device,title,checks},null,2)+'\n');console.log('PASS '+name);}
function capture(name){fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(d.layout(),null,2)+'\n');d.run('screenshot','--path',path.join(out,name+'.png'));}
function click(n){assert.ok(n,'observed control');d.click(n);}
function back(){cp.execFileSync(hdc,['-t',device,'shell','uitest','uiInput','keyEvent','Back']);}
function seekControl(predicate,label){
  for(let i=0;i<5;i++){
    const view=d.flatten(d.layout()),n=view.find(x=>predicate(x.n))?.n;if(n)return n;
    const scroll=view.filter(x=>x.n.type==='Scroll').at(-1)?.n;assert.ok(scroll,'observed editor scroll');
    const [l,t,r,b]=scroll.bounds;d.run('swipe',String(r-10),String(b-50),String(r-10),String(t+100));
  }
  throw Error('visible editor control not found: '+label);
}
function seekInput(id){return seekControl(n=>n.id===id,id);}
function seekText(value){return seekControl(n=>n.text===value,value);}
function input(id,value){
  const n=seekInput(id);assert.ok(n,'input '+id);click(n);
  cp.execFileSync(hdc,['-t',device,'shell','uitest','uiInput','keyEvent','2072','2017']); // Ctrl+A on this observed field.
  // The CLI's Windows argument escaping loses whitespace/newline quoting.
  // A test-owned shell file keeps the generated base64-decoded text ONE argv
  // value to the official UITest command. It does not write app files.
  const local=path.join(out,'input-text.sh');
  fs.writeFileSync(local,"#!/bin/sh\nvalue=$(printf '%s' '"+Buffer.from(value,'utf8').toString('base64')+"' | base64 -d; printf '.')\nexec uitest uiInput text \"${value%.}\"\n");
  cp.execFileSync(hdc,['-t',device,'file','send',local,'/data/local/tmp/hmos-dev9-input-text.sh']);
  const result=cp.execFileSync(hdc,['-t',device,'shell','sh','/data/local/tmp/hmos-dev9-input-text.sh'],{encoding:'utf8'});
  fs.writeFileSync(path.join(out,'input-last-result.txt'),result);
  assert.ok(!/incorrect|fail|error/i.test(result)||/no error/i.test(result),'official UITest input succeeded');back();
  assert.equal(byId(id)?.text,value,'exact visible input readback');
}
function reopened(){cp.execFileSync(hdc,['-t',device,'shell','aa','force-stop','dev.morrow.hmos']);cp.execFileSync(hdc,['-t',device,'shell','aa','start','-a','EntryAbility','-b','dev.morrow.hmos']);assert.ok(text('新建灵感'));}
function rows(){return nodes().filter(n=>n.id?.startsWith('draft-row:'));}
function draftRow(){return rows().find(n=>d.flatten([n]).some(x=>x.n.text===title));}
function openDrafts(){const n=nodes().find(n=>n.type==='Button'&&/^草稿(?: [0-9]+)?$/.test(n.text||''));click(n);assert.ok(text('保留的草稿'));}
function restore(){openDrafts();const row=draftRow();assert.ok(row,'durable fixture row');click(d.flatten([row]).find(x=>x.n.text==='恢复编辑')?.n);assert.equal(byId('draft-title')?.text,title);}
function confirmed(){for(let i=0;i<6;i++){const n=byId('draft-status');if(n?.text==='草稿已保留')return;}throw Error('latest draft not acknowledged');}
function confirmDiscard(){const item=d.flatten(d.layout()).find(x=>x.n.text==='放弃草稿'&&x.parents.some(p=>p.type==='AlertDialog'));assert.ok(item,'observed native discard confirmation');click(item.n);}
function search(value){const n=nodes().find(n=>n.type==='TextInput');assert.ok(n);d.run('text',value,String(Math.round((n.bounds[0]+n.bounds[2])/2)),String(Math.round((n.bounds[1]+n.bounds[3])/2)));back();}
function fixtureCard(){return nodes().filter(n=>n.id?.startsWith('workspace-card:')).find(n=>d.flatten([n]).some(x=>x.n.text===title));}
function dismissReceipt(){
  const view=nodes(),receipt=view.find(n=>n.type==='Text'&&/^(已保存 · 操作修订 |原卡片已变化，草稿文字已保留)/.test(n.text||''));
  if(receipt){const close=view.find(n=>n.text==='×'&&Math.abs((n.bounds[1]+n.bounds[3])-(receipt.bounds[1]+receipt.bounds[3]))<30);assert.ok(close,'observed receipt close');click(close);}
}
if(process.argv.includes('--readonly-check')){
  assert.ok(!byId('draft-title'));openDrafts();assert.ok(!draftRow());click(text('关闭'));search(title);dismissReceipt();
  click(d.flatten([fixtureCard()]).find(x=>x.n.text===title)?.n);assert.ok(!seekInput('draft-todos').text);
  assert.equal(nodes().filter(n=>n.text==='HMOS task C 😀').length,1);capture('unchanged-view');click(text('保留草稿'));
  openDrafts();assert.ok(!draftRow(),'unfocused initialization does not create a journal');click(text('关闭'));pass('view and scroll of unchanged existing card consumes no draft slot');
  reopened();openDrafts();assert.ok(!draftRow());click(text('关闭'));pass('read-only view remains without a journal after restart');
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({result:'PASS',device,title,checks},null,2)+'\n');
}else if(process.argv.includes('--continue-task-discard')){
  assert.ok(!byId('draft-title'),'continue only inspected closed editor');
  const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json'),'utf8'));assert.equal(prior.title,title);assert.equal(prior.checks.length,3);checks.push(...prior.checks);
  openDrafts();assert.ok(!draftRow());click(text('关闭'));assert.equal(fixtureCard()?.id,'workspace-card:b6f32cb3-6cf6-48ac-bf34-772c3356d14f');verifyCommittedTask();
}else if(process.argv.includes('--task-check')||process.argv.includes('--continue-todo')){
  const submitted='HMOS task C 😀',pending='Unsubmitted task C 😀';
  if(process.argv.includes('--task-check')){
  assert.equal(byId('draft-title')?.text,title,'observed existing fixture editor');
  assert.equal(byId('draft-description')?.text,newer);assert.ok(!seekInput('draft-todos').text);
  input('draft-todos',submitted);confirmed();click(text('+'));
  assert.ok(text(submitted),'exact formal task text');assert.ok(!seekInput('draft-todos').text);pass('completed raw todo is submitted once and only its consumed input is cleared');
  input('draft-todos',pending);confirmed();
  }else{
    assert.equal(seekInput('draft-todos').text,pending,'continue only inspected pending raw todo');
    const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json'),'utf8'));assert.equal(prior.title,title);assert.equal(prior.checks.length,1);checks.push(...prior.checks);confirmed();
  }
  click(seekText('收藏'));
  assert.equal(byId('draft-todos')?.text,pending);assert.ok(text('卡片已保存，较新的草稿输入仍保留。原卡片已变化，请核对后处理。'));confirmed();capture('todo-source-conflict');pass('ordinary business mutation preserves pending todo and surfaces source conflict');
  click(text('保留草稿'));reopened();restore();assert.equal(seekInput('draft-todos').text,pending);assert.ok(text(submitted));pass('pending todo survives process restart after source change');
  click(text('放弃草稿'));confirmDiscard();openDrafts();assert.ok(!draftRow());click(text('关闭'));search(title);
  verifyCommittedTask();
}else if(process.argv.includes('--continue-existing')){
  assert.ok(!byId('draft-title'),'continue only inspected closed editor');
  const card=fixtureCard();assert.ok(card,'inspected committed fixture');assert.ok(d.flatten([card]).some(x=>x.n.text===newer));
  const prior=JSON.parse(fs.readFileSync(path.join(out,'progress.json'),'utf8'));assert.equal(prior.title,title);assert.equal(prior.checks.length,5);checks.push(...prior.checks);
  existingChecks(card.id);
}else if(process.argv.includes('--seed')||process.argv.includes('--continue-empty')||process.argv.includes('--continue-title')||process.argv.includes('--continue-body')){
  if(process.argv.includes('--seed')){
    assert.ok(!byId('draft-title'),'fresh seed requires closed editor');openDrafts();assert.ok(!draftRow(),'never replay a named draft');click(text('关闭'));
    search(title);assert.ok(!fixtureCard(),'never replay a named business card');click(text('×'));
    click(text('新建灵感'));
  }
  if(process.argv.includes('--continue-body')) { assert.equal(byId('draft-title')?.text,title);assert.equal(byId('draft-description')?.text,body,'continue only inspected exact raw body');back(); }
  else if(process.argv.includes('--continue-title')) { assert.equal(byId('draft-title')?.text,title);assert.ok(!byId('draft-description')?.text,'continue only inspected untouched body'); }
  else { assert.ok(byId('draft-title')&&!byId('draft-title').text&&!byId('draft-description')?.text,'continue only inspected empty new editor'); }
  confirmed();pass('empty new-card draft is acknowledged without a business card');
  if(!process.argv.includes('--continue-title')&&!process.argv.includes('--continue-body'))input('draft-title',title);
  if(!process.argv.includes('--continue-body'))input('draft-description',body);confirmed();capture('raw-new-draft');
  click(text('保留草稿'));assert.ok(!byId('draft-title'));search(title);assert.ok(!fixtureCard());click(text('×'));pass('close keeps raw values and does not submit business content');
  reopened();restore();assert.equal(byId('draft-description')?.text,body);capture('restored-first');pass('new-card raw Unicode and multiline values recover after process restart');
  input('draft-description',newer);confirmed();capture('newer-acknowledged');
  reopened();restore();assert.equal(byId('draft-description')?.text,newer);pass('latest autosaved raw generation recovers without closing the editor');
  click(text('保存灵感'));assert.ok(!byId('draft-title'),'confirmed save closes editor');search(title);const card=fixtureCard();assert.ok(card);const identity=card.id;assert.ok(d.flatten([card]).some(x=>x.n.text===newer));
  openDrafts();assert.ok(!draftRow());click(text('关闭'));capture('submitted-once');pass('explicit submission creates one business identity and retires its raw draft');
  existingChecks(identity);
}else{throw Error('Use --seed only after verifying a new fixture. This tool never silently resumes a partial mutation.');}
function verifyCommittedTask(){
  dismissReceipt();click(d.flatten([fixtureCard()]).find(x=>x.n.text===title)?.n);
  assert.ok(!seekInput('draft-todos').text,'discarded raw todo does not return');assert.equal(nodes().filter(n=>n.text==='HMOS task C 😀').length,1,'exactly one committed task in reopened editor');
  capture('todo-explicit-discard');pass('explicit raw discard preserves previously committed task');click(text('保留草稿'));click(text('×'));
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify({result:'PASS',device,title,checks,note:'Completed plain input through actual ArkUI; real IME candidate sessions remain untested.'},null,2)+'\n');
}
function existingChecks(identity){
  dismissReceipt();
  click(d.flatten([fixtureCard()]).find(x=>x.n.text===title)?.n);input('draft-description','Existing-card unsubmitted body 😀');confirmed();click(text('保留草稿'));
  const original=fixtureCard();assert.equal(original.id,identity);assert.ok(d.flatten([original]).some(x=>x.n.text===newer));pass('existing-card draft leaves the committed source content unchanged');
  reopened();restore();assert.equal(byId('draft-description')?.text,'Existing-card unsubmitted body 😀');capture('restored-existing');pass('existing-card unsubmitted values recover after restart');
  click(text('放弃草稿'));assert.ok(text('放弃这份草稿？'));click(text('继续编辑'));assert.ok(byId('draft-title'));pass('discard cancellation keeps the editor and draft');
  click(text('放弃草稿'));confirmDiscard();assert.ok(!byId('draft-title'));openDrafts();assert.ok(!draftRow());click(text('关闭'));search(title);assert.equal(fixtureCard().id,identity);assert.ok(d.flatten([fixtureCard()]).some(x=>x.n.text===newer));
  reopened();openDrafts();assert.ok(!draftRow());click(text('关闭'));search(title);assert.equal(fixtureCard().id,identity);capture('discarded-reopened');pass('confirmed discard remains inactive after restart and never changes the business card');
  click(text('×'));
  const result={result:'PASS',device,title,cardId:identity,checks,note:'UI creates one named development fixture. Native model tests cover raw selection/composing metadata; this is not IME session restoration or captured S1/S2 handoff acceptance.'};
  fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
}
