// Runs only against the three accepted dev11 fixtures and observed controls.
const f=require('./card-workflow-device-check.cjs'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const checks=[];function pass(label){checks.push(label);f.pass(label);}
function nav(label){f.click(f.text(String.fromCodePoint(0xf8b6)));f.click(f.text(label));for(let i=0;i<3;i++)f.nodes();}
function sortText(){return f.d.flatten([f.seek(n=>n.id==='card-sort','sort label',true)]).map(x=>x.n.text);}
function drag(){
 f.dismissNotice();f.search(f.prefix);f.card(f.names[1],true);
 const b=f.card(f.names[1]),c=f.card(f.names[2],false);const start=f.id('card-drag:'+b.id.slice('workspace-card:'.length));
 assert.ok(start&&start.bounds[3]-start.bounds[1]>=85,'fully visible native drag handle');assert.ok(c.bounds[3]-c.bounds[1]>=630,'fully visible target card');
 f.capture('before-drag');const [l,t,r,bottom]=start.bounds,[cl,ct,cr,cb]=c.bounds;
 f.d.run('drag',String(Math.round((l+r)/2)),String(Math.round((t+bottom)/2)),String(Math.round((cl+cr)/2)),String(Math.round(ct+(cb-ct)*.7)),'--speed','600');
 for(let i=0;i<3;i++)f.nodes();f.card(f.names[2],true);assert.deepEqual(f.order(),[f.names[2],f.names[1],f.names[0]]);f.capture('after-drag');pass('native long-hold drag inserts B after C and saves manual order');
 f.restart();f.search(f.prefix);f.card(f.names[2],false);assert.deepEqual(f.order(),[f.names[2],f.names[1],f.names[0]]);pass('native drag order survives process restart');
}
function pages(){nav('灵感收件箱');assert.ok(sortText().includes('默认顺序'),'inbox starts with its own nonmanual choice');f.sort('标题');f.action(f.names[0],'后移卡片');assert.ok(sortText().includes('自定义顺序'));
 nav('概览');assert.ok(sortText().includes('自定义顺序'));f.card(f.names[2],false);assert.deepEqual(f.order(),[f.names[2],f.names[1],f.names[0]]);pass('inbox manual selection and order do not overwrite overview preferences');
 const title=f.seek(n=>n.type==='Text'&&n.text===f.names[2],'long-press title',true);const [l,t,r,b]=title.bounds;
 f.d.run('longclick',String(Math.round((l+r)/2)),String(Math.round((t+b)/2)));assert.ok(f.text('查看详情'));assert.ok(f.text('前移卡片'));f.capture('longpress-menu');
 f.click(f.text('查看详情'));assert.equal(f.id('detail-title')?.text,f.names[2]);f.click(f.id('detail-close'));pass('native body long press opens card menu and routes to reading detail');
}
function tasks(){
 f.dismissNotice();f.click(f.seek(n=>n.type==='Text'&&n.text===f.names[1],'project detail',true));f.click(f.id('detail-edit'));
 f.input('draft-todos','dev11 stable TaskId');f.click(f.seek(n=>n.type==='Button'&&n.text==='+','explicit task add',false,'editor'));
 for(let i=0;i<4;i++)f.nodes();assert.equal(f.id('draft-todos')?.text||'','');
 // Explicitly discard only the current editor raw draft after the task receipt.
 // The committed task is a separate business mutation and remains on the card.
 f.click(f.seek(n=>n.type==='Button'&&n.text==='放弃草稿','discard current raw',false,'editor'));f.click(f.text('放弃草稿'));
 for(let i=0;i<3;i++)f.nodes();assert.ok(!f.id('draft-title'));f.dismissNotice();f.click(f.seek(n=>n.type==='Text'&&n.text===f.names[1],'project detail',true));
 const task=f.seek(n=>n.id?.startsWith('detail-task:'),'TaskId checkbox',false,'detail');const taskId=task.id;assert.equal(task.text,'○');f.click(task);
 for(let i=0;i<3;i++)f.nodes();assert.equal(f.id(taskId)?.text,'✓');f.capture('task-detail');pass('detail checkbox keeps stable TaskId and confirms one task completion');
 f.click(f.seek(n=>n.id==='detail-stage','detail stage',false,'detail'));f.click(f.text('推进中'));for(let i=0;i<3;i++)f.nodes();
 assert.ok(f.text('推进中'));assert.equal(f.id(taskId)?.text,'✓');f.capture('stage-detail');f.click(f.id('detail-close'));pass('detail stage CAS refresh retains completed task and latest source');
}
if(require.main===module){
 const accepted=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../reports/ui-source/v11/device-final/result.json')));assert.equal(accepted.result,'PASS');assert.equal(accepted.checks.length,6);
 if(process.argv.includes('--drag'))drag();else if(process.argv.includes('--pages'))pages();else if(process.argv.includes('--tasks'))tasks();else throw Error('explicit mode required');
 const hap=path.resolve(__dirname,'../entry/build/default/outputs/default/entry-default-unsigned.hap');
 fs.writeFileSync(path.join(f.out,process.argv[2].slice(2)+'-result.json'),JSON.stringify({result:'PASS',checks,hapSha256:crypto.createHash('sha256').update(fs.readFileSync(hap)).digest('hex').toUpperCase()},null,2)+'\n');
}
