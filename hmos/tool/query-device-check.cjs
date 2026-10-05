// Development-device UI acceptance. --seed explicitly creates one fixture
// through the editor; it never edits a database or repeats failed mutations.
const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const out=process.env.HMOS_REPORT_DIR||path.resolve(__dirname,'../reports/ui-source/v8/query-device');fs.mkdirSync(out,{recursive:true});
const device=process.env.HMOS_DEVICE||'127.0.0.1:5557',hdc='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const title='HMOS-query-20261005',hypothesis='HypOnlyDev8',conclusion='ConOnlyDev8';
const nodes=()=>d.flatten(d.layout()).map(x=>x.n),cards=()=>nodes().filter(n=>n.id?.startsWith('workspace-card:'));
const back=()=>cp.execFileSync(hdc,['-t',device,'shell','uitest','uiInput','keyEvent','Back']);
function input(n,text){assert.ok(n,'observed input');d.run('text',text,String(Math.round((n.bounds[0]+n.bounds[2])/2)),String(Math.round((n.bounds[1]+n.bounds[3])/2)));back();assert.ok(nodes().some(v=>v.type===n.type&&v.text===text),'exact input readback');}
function swipeScroll(last=false,up=true){const scrolls=nodes().filter(n=>n.type==='Scroll'),b=scrolls.at(last?-1:0)?.bounds;assert.ok(b);const x=Math.round((b[0]+b[2])/2),top=Math.round(b[1]+(b[3]-b[1])*.2),bottom=Math.round(b[1]+(b[3]-b[1])*.8);d.run('swipe',String(x),String(up?bottom:top),String(x),String(up?top:bottom));}
function capture(name){const tree=d.layout();fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(tree,null,2)+'\n');d.run('screenshot','--path',path.join(out,name+'.png'));return tree;}
function search(text){const n=nodes().filter(n=>n.type==='TextInput').sort((a,b)=>a.bounds[1]-b.bounds[1])[0];input(n,text);}
function clear(){const n=nodes().find(n=>n.text==='×');assert.ok(n,'clear query');d.click(n);}
function navigate(label){const t=d.layout(),all=d.flatten(t).map(x=>x.n),header=all.filter(n=>n.type==='Button'&&n.bounds[1]<t[0].bounds[3]*.15).sort((a,b)=>a.bounds[0]-b.bounds[0])[0];assert.ok(header);d.click(header);d.click(nodes().find(n=>n.text===label));}
function matched(){const c=cards().find(n=>d.flatten([n]).some(x=>x.n.text===title));assert.ok(c,'hypothesis/conclusion result resolves actual card identity');return c;}
if(process.argv.includes('--seed')||process.argv.includes('--continue-fixture')){
  let all=nodes();assert.ok(!cards().some(c=>d.flatten([c]).some(x=>x.n.text===title)),'do not replay a visible fixture');
  if(!all.some(n=>n.text==='保存灵感')){d.click(all.find(n=>n.text==='新建灵感'));all=nodes();}
  const field=all.filter(n=>n.type==='TextInput').at(-1);
  if(process.argv.includes('--continue-fixture')){assert.equal(field.text,title,'explicitly resume only the inspected fixture draft');}
  else{assert.ok(!field.text,'only an untouched new editor may be seeded');input(field,title);}
  input(nodes().find(n=>n.type==='TextArea'),'QueryBodyDev8');
  for(let i=0;i<6&&!nodes().some(n=>n.text==='放在哪里');i++)swipeScroll(true);
  d.click(nodes().find(n=>n.type==='Select'));d.click(nodes().find(n=>n.text==='实验'));
  for(let i=0;i<6&&nodes().filter(n=>n.type==='TextInput'&&n.bounds[1]>270).length<2;i++)swipeScroll(true);
  const fields=nodes().filter(n=>n.type==='TextInput').filter(n=>n.bounds[1]>270);
  assert.equal(fields.length,2,'visible hypothesis/conclusion inputs');input(fields[0],hypothesis);
  const remaining=nodes().filter(n=>n.type==='TextInput').filter(n=>n.bounds[1]>270);
  input(remaining.at(-1),conclusion);d.click(nodes().find(n=>n.text==='保存灵感'));
  assert.ok(!nodes().some(n=>n.text==='保存灵感'),'confirmed save closed editor');
}
assert.ok(!nodes().some(n=>n.text==='保存灵感'),'test requires closed editor');
search(hypothesis);const card=matched(),identity=card.id;capture('hypothesis');
clear();search(conclusion);assert.equal(matched().id,identity);capture('conclusion');
// Same-view content refresh keeps confirmed membership, but revised nodes must repaint.
const button=c=>d.flatten([c]).find(x=>x.n.type==='Button').n;
const icon=c=>d.flatten([button(c)]).find(x=>x.n.type==='Text').n.text;
const original=icon(matched());d.click(button(matched()));assert.notEqual(icon(matched()),original);
navigate('已收藏');assert.equal(matched().id,identity);capture('favorite-query');
navigate('实验室');assert.equal(matched().id,identity);assert.ok(nodes().some(n=>n.text===hypothesis));capture('lab-query');
navigate('概览');d.click(button(matched()));assert.equal(icon(matched()),original);
clear();search('Dev8NoSuchResult');assert.equal(cards().length,0);assert.ok(nodes().some(n=>n.text==='没有找到匹配的想法'));capture('empty');
clear();search(hypothesis);assert.equal(matched().id,identity);
cp.execFileSync(hdc,['-t',device,'shell','aa','force-stop','dev.morrow.hmos']);
cp.execFileSync(hdc,['-t',device,'shell','aa','start','-a','EntryAbility','-b','dev.morrow.hmos']);
search(conclusion);assert.equal(matched().id,identity);capture('reopened-query');
clear();
const result={result:'PASS',device,cardId:identity,checks:['hypothesis-only search','conclusion-only search','same-id favorite revision repaint','favorites query','lab query','empty result removes old rows','restore after empty','query after process restart'],note:'Uses one explicitly created development fixture and observed UI controls; no database edits.'};
fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
