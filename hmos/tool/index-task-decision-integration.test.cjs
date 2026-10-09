'use strict';
// Actual Index task guards/actions/menu/submit/retry and actual Workbench queue.
// Native Store/provider replies stay controlled; not SDK/UI/device qualification.
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {harness,plain,settle,deferred,assertSourceUnchanged}=require('./index-business-test-harness.cjs');
const source=fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/pages/Index.ets'),'utf8');
const ts=require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
function fixture(t,options={}){
 const h=harness({mode:'edit'}),p=h.page;t.after(()=>{assertSourceUnchanged();h.dispose();});
 const card={...plain(p.cards[0]),content_kind:'v2',tasks:options.tasks||[{id:'task-a',text:'same',completion:2},{id:'task-b',text:'same',completion:2}]};
 p.cards=[card];p.detailId=options.editor?'':card.id;p.editorOpen=!!options.editor;
 // A nonempty independent raw todo prevents unrelated business continuation;
 // the actual submit still flushes the original draft before task dispatch.
 h.adopt('todos','independent raw todo');
 const history=new Map(),mutationWires=[];let stored=plain(card);
 function receive(wire,c){
  if(history.has(c.operation)){const prior=history.get(c.operation);assert.equal(wire,prior.wire);return {ok:true,error:'',effect:'committed',receipt_revision:prior.revision,cards:[plain(stored)],drafts:[]};}
  const current=plain(stored);if(current.source!==c.source)return {ok:false,error:'CAS conflict',effect:'not_committed',receipt_revision:'',cards:[]};
  const target=current.tasks.find(task=>task.id===c.task_id);if(!target)return {ok:false,error:'unknown TaskId',effect:'not_committed',receipt_revision:'',cards:[]};
  target.completion=c.flag?1:0;current.revision=String(BigInt(current.revision)+1n);current.source=Buffer.from('controlled-source:'+c.operation+':'+current.revision).toString('hex');
  stored=plain(current);history.set(c.operation,{wire,revision:current.revision});return {ok:true,error:'',effect:'committed',receipt_revision:current.revision,cards:[current],drafts:[]};
 }
 h.setIntercept(async(wire,c,route)=>{
  if(c.action!=='task_toggle')return route();mutationWires.push(wire);return options.reply?options.reply(wire,c,()=>receive(wire,c),h):receive(wire,c);
 });
 const owner=options.editor?p.editorViewOwner:'',snapshot=()=>({card:plain(p.cards[0]),owner,epoch:p.editorInputEpoch});
 return {...h,p,owner,history,mutationWires,snapshot,decision:(s,id,flag)=>p.taskAction(s.card,s.card.tasks.find(task=>task.id===id),flag,s.owner,s.epoch)};
}
function actualCallback(kind,p,c,task,owner,epoch){
 const start=source.indexOf('  '+(kind==='checkbox'?'TaskCompletionControl':'TaskPendingDecision')+'(');
 const end=source.indexOf('\n  '+(kind==='checkbox'?'@Builder':'private pageCards('),start+1),body=source.slice(start,end);
 const re=kind==='checkbox'?/\.onChange\((\(value: boolean\) => this\.taskAction\(c, task, value, owner, epoch\))\)/g:/\.onClick\((\(\) => this\.taskAction\(c, task, (?:true|false), owner, epoch\))\)/g;
 const arrows=[...body.matchAll(re)].map(x=>x[1]);assert.equal(arrows.length,kind==='checkbox'?1:2,'exact actual UI callbacks');
 const code=ts.transpileModule('exports.callbacks=['+arrows.join(',')+'];',{compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.CommonJS}}).outputText;
 const context={exports:{},c,task,owner,epoch};vm.runInNewContext('(function(){'+code+'}).call(page)',{...context,page:p});return context.exports.callbacks;
}
test('pending ordinary toggle and retired generic entrypoint cannot dispatch either context',async t=>{
 for(const editor of [false,true]){const h=fixture(t,{editor}),s=h.snapshot();await h.decision(s,'task-a',undefined);h.p.mutate('task_toggle',s.card.tasks[0]);await settle();assert.equal(h.mutationWires.length,0);assert.equal(h.p.cards[0].tasks[0].completion,2);}
});
test('actual pending button callbacks send true/false per TaskId in detail and editor after actual raw flush',async t=>{
 for(const editor of [false,true])for(const flag of [true,false]){const h=fixture(t,{editor}),s=h.snapshot(),callbacks=actualCallback('pending',h.p,s.card,s.card.tasks[0],s.owner,s.epoch);
  await callbacks[flag?0:1]();assert.equal(h.mutationWires.length,1);const c=JSON.parse(h.mutationWires[0]);assert.equal(c.task_id,'task-a');assert.equal(c.flag,flag);assert.equal(c.source,s.card.source);
  assert.equal(h.p.cards[0].tasks[0].completion,flag?1:0);assert.equal(h.p.cards[0].tasks[1].completion,2);assert.equal(h.p.pending,'');
  if(editor)assert.ok(h.events.findIndex(e=>e.kind==='admit'&&e.action==='draft_save')<h.events.findIndex(e=>e.kind==='admit'&&e.action==='task_toggle'));
  assert.equal(h.draft.current.todos.text,'independent raw todo');assert.equal(h.p.taskText,'independent raw todo');
 }
});
test('duplicate names are decided independently on actual revised snapshot, preserving order and distinct operations',async t=>{
 const h=fixture(t);await h.decision(h.snapshot(),'task-a',true);await h.decision(h.snapshot(),'task-b',false);assert.deepEqual(plain(h.p.cards[0].tasks).map(task=>[task.id,task.completion]),[['task-a',1],['task-b',0]]);
 const commands=h.mutationWires.map(JSON.parse);assert.notEqual(commands[0].operation,commands[1].operation);assert.notEqual(commands[0].source,commands[1].source);assert.equal(h.p.cards[0].tasks.every(task=>task.text==='same'),true);
});
test('actual ordinary checkbox callbacks preserve 0/1 explicit directions without pending promotion',async t=>{
 for(const editor of [false,true])for(const completion of [0,1]){const h=fixture(t,{editor,tasks:[{id:'a',text:'ordinary',completion}]}),s=h.snapshot();
  const callbacks=actualCallback('checkbox',h.p,s.card,s.card.tasks[0],s.owner,s.epoch);await callbacks[0](completion===0);assert.equal(JSON.parse(h.mutationWires[0]).flag,completion===0);assert.equal(h.p.cards[0].tasks[0].completion,1-completion);
 }
});
test('pending menus provide both real directions and retain editor rename/reorder/remove actions',async t=>{
 for(const editor of [false,true])for(const flag of [true,false]){const h=fixture(t,{editor}),s=h.snapshot(),menu=h.p.taskMenu(s.card,s.card.tasks[0],s.owner,s.epoch);
  assert.deepEqual(plain(menu).slice(0,2).map(x=>x.value),['确认已完成','确认未完成']);assert.equal(menu.length,editor?6:2);assert.equal(menu.every(x=>typeof x.action==='function'),true);
  if(editor)assert.deepEqual(plain(menu).slice(2).map(x=>x.value),['重命名','上移','下移','移除待办']);menu[flag?0:1].action();await settle();assert.equal(JSON.parse(h.mutationWires[0]).flag,flag);
 }
});
test('ordinary menu offers only opposite completion while pending menu has two choices',t=>{
 for(const completion of [0,1]){const h=fixture(t,{tasks:[{id:'a',text:'ordinary',completion}]}),s=h.snapshot(),menu=h.p.taskMenu(s.card,s.card.tasks[0],s.owner,s.epoch);assert.equal(menu.length,1);assert.equal(menu[0].value,completion===0?'确认已完成':'确认未完成');}
});
test('stale rendered source/task/epoch/editor owner/menu cannot mint a new decision',async t=>{
 for(const editor of [false,true])for(const change of ['source','revision','removed','completion','text','epoch','page','foreground','selected','route']){
  const h=fixture(t,{editor}),s=h.snapshot(),menu=h.p.taskMenu(s.card,s.card.tasks[0],s.owner,s.epoch);const c=h.p.cards[0];
  if(change==='source')c.source='aabb';if(change==='revision')c.revision='8';if(change==='removed')c.tasks.shift();if(change==='completion')c.tasks[0].completion=0;if(change==='text')c.tasks[0].text='changed';
  if(change==='epoch')h.p.editorInputEpoch++;if(change==='page')h.p.pageAlive=false;if(change==='foreground')h.p.foreground=false;if(change==='selected')h.p.selected='other';
  if(change==='route'){if(editor)h.p.editorViewOwner='replacement';else h.p.detailId='';}
  menu[0].action();await h.decision(s,'task-a',false);await settle();assert.equal(h.mutationWires.length,0,editor+':'+change);
 }
});
test('all write blockers, unknown task state, incomplete/legacy DTO and invalid explicit value forbid decisions',async t=>{
 for(const editor of [false,true])for(const blocker of ['busy','pending','dirty','taskEditId','attachmentWorking','pasteWorking','fieldValidationWorking','draftWorking','draftRetiring','draftRetirementUnknown','draftConflict','draftCaptureIncomplete','rawFork','rawForkWorking','editorBoundary','businessWorking','notReady','deleted','unknownTask','duplicateIds','legacy','invalidFlag']){
  const h=fixture(t,{editor}),s=h.snapshot();let flag=true;
  if(blocker==='notReady')h.p.ready=false;else if(blocker==='deleted')h.p.cards[0].deleted=true;else if(blocker==='unknownTask'){h.p.cards[0].tasks[0].completion=3;s.card.tasks[0].completion=3;}
  else if(blocker==='duplicateIds'){h.p.cards[0].tasks[1].id='task-a';s.card.tasks[1].id='task-a';}else if(blocker==='legacy'){h.p.cards[0].content_kind='legacy';s.card.content_kind='legacy';}
  else if(blocker==='invalidFlag')flag='true';else h.p[blocker]=['pending','taskEditId','editorBoundary'].includes(blocker)?'original-pending':blocker==='rawFork'?{}:true;
  await h.decision(s,'task-a',flag);assert.equal(h.mutationWires.length,0,editor+':'+blocker);
 }
});
test('registration of raw input must remain owned through actual flush before task dispatch',async t=>{
 const h=fixture(t,{editor:true}),s=h.snapshot(),gate=deferred();h.setRawIntercept(async()=>{await gate.promise;});const run=h.decision(s,'task-a',true);await settle();h.p.editorInputEpoch++;gate.resolve();await run;
 assert.equal(h.mutationWires.length,0);assert.equal(h.p.pending,'');assert.equal(h.draft.current.todos.text,'independent raw todo');
});

test('actual raw flush cannot outlive retirement, source, page lease or complete original input',async t=>{
 for(const change of ['retiring','retirementUnknown','restoring','retired','source','owner','disposed','rawText','foreground']){
  const h=fixture(t,{editor:true}),s=h.snapshot(),gate=deferred(),started=deferred();
  h.setRawIntercept(async()=>{started.resolve();await gate.promise;});const run=h.decision(s,'task-a',true);await started.promise;
  if(change==='retiring')h.p.draftRetiring=true;if(change==='retirementUnknown')h.p.draftRetirementUnknown=true;if(change==='restoring')h.p.draftRestoreInput=true;
  if(change==='retired')h.p.draftRetiredIdentity=h.draft.scope.draft_id;if(change==='source')h.p.cards[0].source='aabb';if(change==='owner')h.p.editorViewOwner='replaced-view';
  if(change==='disposed')h.draft.dispose();if(change==='rawText')h.adopt('todos','S2 complete raw while flush');if(change==='foreground')h.p.foreground=false;
  gate.resolve();await run;assert.equal(h.mutationWires.length,0,change);assert.equal(h.p.pending,'',change);
  if(change!=='disposed')assert.equal(h.draft.current.todos.text,change==='rawText'?'S2 complete raw while flush':'independent raw todo',change);
 }
});

test('task reply loses its current view ownership on actual field or lifecycle change and explicit retry retains exact wire',async t=>{
 for(const editor of [false,true])for(const change of ['epoch','selected','detail','editorOpen','viewOwner','attachmentOwner','draft','page','foreground','rawInput']){
  if(!editor&&['viewOwner','rawInput'].includes(change))continue;
  const started=deferred(),gate=deferred();let first=true;
  const h=fixture(t,{editor,reply:async(wire,c,receive)=>{const reply=receive();if(first){first=false;started.resolve();await gate.promise;}return reply;}}),s=h.snapshot();
  const originalDraft=h.p.editorDraft;const run=h.decision(s,'task-a',true);await started.promise;
  if(change==='epoch')h.p.editorInputEpoch++;if(change==='selected')h.p.selected='different-card';if(change==='detail')h.p.detailId='different-card';
  if(change==='editorOpen')h.p.editorOpen=!editor;if(change==='viewOwner')h.p.editorViewOwner='replacement-view';if(change==='attachmentOwner')h.p.attachmentEditorIdentity='replacement-identity';
  if(change==='draft')h.p.editorDraft=undefined;if(change==='page')h.p.pageAlive=false;if(change==='foreground')h.p.foreground=false;if(change==='rawInput')h.adopt('description','S2 later full body input');
  const original=h.p.pending;gate.resolve();await run;
  assert.equal(h.p.pending,original,editor+':'+change);assert.equal(h.p.cards[0].tasks[0].completion,2,editor+':'+change);assert.equal(h.p.submittedTaskCard.source,s.card.source);
  // Explicit reconciliation is a new foreground response boundary, while the
  // prior card/CAS/operation/TaskId/flag/wire remain untouched.
  h.p.pageAlive=true;h.p.foreground=true;h.p.selected=s.card.id;h.p.detailId=editor?'':s.card.id;h.p.editorOpen=editor;h.p.editorDraft=originalDraft;
  h.p.editorViewOwner='reacquired-view';h.p.attachmentEditorIdentity='reacquired-view';h.p.editorInputEpoch++;
  await h.p.retry();assert.equal(h.mutationWires.length,2);assert.equal(h.mutationWires[1],original);assert.equal(h.p.pending,'');assert.equal(h.p.cards[0].tasks[0].completion,1);
  if(change==='rawInput'){assert.equal(h.draft.current.description.text,'S2 later full body input');assert.equal(h.p.description,'S2 later full body input');}
 }
});

test('Unknown cannot retry against a replacement card, while a neutral foreground explicit reconciliation uses the original target',async t=>{
 let first=true;const h=fixture(t,{reply:(wire,c,receive)=>{if(first){first=false;receive();throw Error('lost task receipt');}return receive();}});
 await h.decision(h.snapshot(),'task-a',false);const original=h.p.pending;h.p.selected='different-card';h.p.detailId='different-card';await h.p.retry();assert.equal(h.mutationWires.length,1);assert.equal(h.p.pending,original);
 h.p.selected='';h.p.detailId='';await h.p.retry();assert.equal(h.mutationWires.length,2);assert.equal(h.mutationWires[1],original);assert.equal(h.p.pending,'');assert.equal(h.p.cards[0].tasks[0].completion,0);
});

test('actual menu rename/reorder/remove retain their original TaskIds and confirmation ownership',async t=>{
 {const h=fixture(t,{editor:true}),s=h.snapshot();h.p.taskMenu(s.card,s.card.tasks[0],s.owner,s.epoch)[2].action();assert.equal(h.p.taskEditId,'task-a');assert.equal(h.p.taskRenameValue.text,'same');assert.equal(h.mutationWires.length,0);}
 {const h=fixture(t,{editor:true}),s=h.snapshot();h.p.taskMenu(s.card,s.card.tasks[0],s.owner,s.epoch)[4].action();await settle();const command=h.calls.map(JSON.parse).find(c=>c.action==='task_reorder');assert.deepEqual(command.order,['task-b','task-a']);assert.equal(command.source,s.card.source);}
 for(const change of ['none','epoch','source','owner','foreground']){
  const h=fixture(t,{editor:true}),s=h.snapshot();let dialog;h.p.getUIContext=()=>({showAlertDialog:value=>{dialog=value;}});
  h.p.taskMenu(s.card,s.card.tasks[0],s.owner,s.epoch)[5].action();assert.ok(dialog);
  if(change==='epoch')h.p.editorInputEpoch++;if(change==='source')h.p.cards[0].source='aabb';if(change==='owner')h.p.editorViewOwner='different-owner';if(change==='foreground')h.p.foreground=false;
  dialog.secondaryButton.action();await settle();const commands=h.calls.map(JSON.parse).filter(c=>c.action==='task_remove');assert.equal(commands.length,change==='none'?1:0,change);
  if(commands.length){assert.equal(commands[0].task_id,'task-a');assert.equal(commands[0].source,s.card.source);}
 }
});
test('Unknown outcome before/after commit blocks replacement direction and retry reuses original wire/CAS/operation/flag',async t=>{
 for(const committed of [false,true]){let fail=true;const h=fixture(t,{reply:(wire,c,receive,base)=>{if(fail){fail=false;if(committed){const r=receive();base.page.cards=plain(r.cards);}throw Error('Lost task response');}return receive();}}),s=h.snapshot();
  await h.decision(s,'task-a',true);const original=h.p.pending;assert.equal(original,h.mutationWires[0]);await h.decision(h.snapshot(),'task-a',false);assert.equal(h.mutationWires.length,1);
  await h.p.retry();assert.equal(h.mutationWires.length,2);assert.equal(h.mutationWires[1],original);assert.equal(h.p.pending,'');assert.equal(h.p.cards[0].tasks[0].completion,1);
 }
});
test('malformed task successes and false no-commit receipts retain original wire and actual pending view',async t=>{
 for(const change of ['effect','receiptMissing','receiptZero','receiptLeadingZero','receiptOverU64','receiptFuture','targetMissing','duplicateCards','missingDTO','unknownCompletion','wrongFlag','wrongOtherTask','wrongTaskOrder','oldSource','sameOldRevision','malformedNoCommit']){
  const h=fixture(t,{reply:(wire,c,receive)=>{const r=receive();if(change==='effect')r.effect='unknown';if(change==='receiptMissing')delete r.receipt_revision;if(change==='receiptZero')r.receipt_revision='0';if(change==='receiptLeadingZero')r.receipt_revision='08';if(change==='receiptOverU64')r.receipt_revision='18446744073709551616';if(change==='receiptFuture')r.receipt_revision='9';if(change==='targetMissing')r.cards=[];if(change==='duplicateCards')r.cards.push(plain(r.cards[0]));if(change==='missingDTO')delete r.cards[0].tasks;if(change==='unknownCompletion')r.cards[0].tasks[0].completion=3;if(change==='wrongFlag')r.cards[0].tasks[0].completion=0;if(change==='wrongOtherTask')r.cards[0].tasks[1].completion=1;if(change==='wrongTaskOrder')r.cards[0].tasks.reverse();if(change==='oldSource')r.cards[0].source=c.source;if(change==='sameOldRevision')r.cards[0].revision='7';if(change==='malformedNoCommit')return {ok:false,error:'claimed no commit',effect:'not_committed',receipt_revision:'8'};return r;}}),s=h.snapshot();
  await h.decision(s,'task-a',true);assert.equal(h.p.pending,h.mutationWires[0],change);assert.equal(h.p.cards[0].tasks[0].completion,2,change);assert.equal(h.p.submittedTaskCard.source,s.card.source,change);
 }
});
test('real known no-commit clears only original operation and leaves pending task/raw text untouched',async t=>{
 const h=fixture(t,{reply:()=>({ok:false,error:'CAS conflict',effect:'not_committed',receipt_revision:''})}),s=h.snapshot();await h.decision(s,'task-a',false);
 assert.equal(h.p.pending,'');assert.equal(h.p.submittedTaskCard,undefined);assert.equal(h.p.cards[0].tasks[0].completion,2);assert.equal(h.draft.current.todos.text,'independent raw todo');assert.match(h.p.message,/未提交/);
});
test('historical receipt accepts Native latest state or removed/deleted task without restoring old decision',async t=>{
 for(const later of ['different','removed','deleted']){const h=fixture(t,{reply:(wire,c,receive)=>{const r=receive();r.cards[0].revision='9';r.cards[0].source=Buffer.from('newer actual controlled view').toString('hex');if(later==='different')r.cards[0].tasks[0].completion=0;if(later==='removed')r.cards[0].tasks.shift();if(later==='deleted')r.cards[0].deleted=true;return r;}});
  await h.decision(h.snapshot(),'task-a',true);assert.equal(h.p.pending,'');assert.equal(h.p.cards[0].revision,'9');if(later==='different')assert.equal(h.p.cards[0].tasks[0].completion,0);if(later==='removed')assert.equal(h.p.cards[0].tasks.some(task=>task.id==='task-a'),false);if(later==='deleted')assert.equal(h.p.cards[0].deleted,true);
 }
});
test('receipt comparisons retain full canonical u64 precision beyond JS safe integers',t=>{
 const h=fixture(t),original=plain(h.p.cards[0]);original.revision='9007199254740993';h.p.submittedTaskCard=original;
 const cmd={id:original.id,source:original.source,task_id:'task-a',flag:true},current=plain(original);current.source='aabb';current.revision='9007199254740994';current.tasks[0].completion=1;
 const r={ok:true,error:'',effect:'committed',receipt_revision:'9007199254740994',cards:[current]};assert.equal(h.p.taskReceipt(cmd,r),true);r.receipt_revision='9007199254740995';assert.equal(h.p.taskReceipt(cmd,r),false);
 original.revision='18446744073709551613';current.revision='18446744073709551615';r.receipt_revision='18446744073709551614';assert.equal(h.p.taskReceipt(cmd,r),true);r.receipt_revision='18446744073709551616';assert.equal(h.p.taskReceipt(cmd,r),false);
});
