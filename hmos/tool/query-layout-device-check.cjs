// Runs after explicitly seeding masonry fixtures 01..04 in dev8. Accepts the
// observed portrait/landscape display; no hidden density or database changes.
const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const out=process.env.HMOS_REPORT_DIR||path.resolve(__dirname,'../reports/ui-source/v8/layout-device');fs.mkdirSync(out,{recursive:true});
const mode=process.argv[2];assert.ok(['portrait','landscape'].includes(mode));
const nodes=t=>d.flatten(t||d.layout()).map(x=>x.n),cards=t=>nodes(t).filter(n=>n.id?.startsWith('workspace-card:'));
const label=c=>d.flatten([c]).map(x=>x.n.text).find(t=>t?.startsWith('HMOS-masonry-'));
const scroll=t=>nodes(t).find(n=>n.type==='Scroll').bounds;
function swipe(up=true){const b=scroll(d.layout()),x=Math.round((b[0]+b[2])/2),top=Math.round(b[1]+(b[3]-b[1])*.3),bottom=Math.round(b[1]+(b[3]-b[1])*.75);d.run('swipe',String(x),String(up?bottom:top),String(x),String(up?top:bottom));}
function smallSwipe(){const t=d.layout(),b=scroll(t),a=cards(t).find(c=>label(c)==='HMOS-masonry-01');assert.ok(a);const x=Math.round((a.bounds[0]+a.bounds[2])/2),bottom=b[3]-240;d.run('swipe',String(x),String(bottom),String(x),String(bottom-200),'--speed','400');const moved=cards(d.layout()).find(c=>label(c)==='HMOS-masonry-01');assert.ok(moved&&moved.bounds[1]<a.bounds[1],'scroll started inside a card, above the fixed footer');}
function seek(text,up=true){for(let i=0;i<8;i++){const t=d.layout(),b=scroll(t),n=nodes(t).find(n=>n.text===text&&n.bounds[1]>=b[1]&&n.bounds[3]<=b[3]);if(n)return n;swipe(up);}throw Error('visible label '+text);}
function gaps(items){const columns=new Map();for(const c of items){if(!columns.has(c.bounds[0]))columns.set(c.bounds[0],[]);columns.get(c.bounds[0]).push(c);}for(const col of columns.values()){col.sort((a,b)=>a.bounds[1]-b.bounds[1]);for(let i=1;i<col.length;i++)assert.ok(col[i].bounds[1]-col[i-1].bounds[3]>=40,'14vp at density3 without overlap');}return columns.size;}
function capture(name){assert.ok(!fs.existsSync(path.join(out,name+'.png')),'use a fresh report directory for a new run');const t=d.layout();fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(t,null,2)+'\n');d.run('screenshot','--path',path.join(out,name+'.png'));return t;}
let t=d.layout();assert.deepEqual(t[0].bounds.slice(2),mode==='portrait'?[1320,2232]:[2232,1320]);
// Acknowledging the confirmed save notice does not alter the stored record.
const notice=nodes(t).find(n=>n.text?.startsWith('已保存 ·'));
if(notice){const close=nodes(t).find(n=>n.text==='×'&&n.bounds[1]>=notice.bounds[1]-30);assert.ok(close);d.click(close);}
const field=nodes().filter(n=>n.type==='TextInput').sort((a,b)=>a.bounds[1]-b.bounds[1])[0];assert.ok(field);
if(field.text!=='HMOS-masonry-'){
  assert.ok(!field.text,'do not append to a different query');
  d.run('text','HMOS-masonry-',String(Math.round((field.bounds[0]+field.bounds[2])/2)),String(Math.round((field.bounds[1]+field.bounds[3])/2)));
  const cp=require('node:child_process');cp.execFileSync('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe',['-t',process.env.HMOS_DEVICE||'127.0.0.1:5557','shell','uitest','uiInput','keyEvent','Back']);
}
const sort=seek(mode==='portrait'?'默认顺序':'标题');d.click(sort);
d.click(nodes().find(n=>n.text==='标题'));
let first;
for(let i=0;i<7;i++){
  t=d.layout();const b=scroll(t),current=cards(t),a=current.find(c=>label(c)==='HMOS-masonry-01');
  if(a&&a.bounds[1]>=b[1]&&a.bounds[1]<b[3]-100){
    if(mode==='portrait'||current.some(c=>label(c)==='HMOS-masonry-03')){first=current;break;}
    smallSwipe();
  }else{swipe();}
}
assert.ok(first,'first sorted fixture visible');gaps(first);capture(mode+'-sorted');
const firstXs=new Set(first.filter(c=>label(c)).map(c=>c.bounds[0]));assert.equal(firstXs.size,mode==='portrait'?1:2);
if(mode==='landscape'){
  const a=first.find(c=>label(c)==='HMOS-masonry-01'),b=first.find(c=>label(c)==='HMOS-masonry-02'),c=first.find(c=>label(c)==='HMOS-masonry-03');
  assert.ok(a&&b&&c);assert.equal(a.bounds[1],b.bounds[1]);assert.ok(a.bounds[3]>b.bounds[3]+35);assert.equal(c.bounds[0],b.bounds[0]);assert.ok(Math.abs(c.bounds[1]-b.bounds[3]-42)<=3);
}
const identities=new Map(),seen=new Set();
for(let i=0;i<7;i++){t=d.layout();const current=cards(t);gaps(current);for(const card of current){const title=label(card);if(title){if(identities.has(title))assert.equal(identities.get(title),card.id);else identities.set(title,card.id);seen.add(title);}}if(seen.size===4)break;swipe();}
assert.equal(seen.size,4,'all four query results reachable');capture(mode+'-records');
const titleSort=seek('标题',false);d.click(titleSort);d.click(nodes().find(n=>n.text==='默认顺序'));
t=d.layout();for(const c of cards(t)){if(identities.has(label(c)))assert.equal(c.id,identities.get(label(c)));}gaps(cards(t));capture(mode+'-reordered');
// Leave title order selected, ready for the other orientation.
d.click(seek('默认顺序',false));d.click(nodes().find(n=>n.text==='标题'));
const result={result:'PASS',mode,display:t[0].bounds.slice(2),density:3,columns:mode==='portrait'?1:2,checks:['native query title order','column count','14vp gaps/no overlap','four results reachable','identity after reorder'],ids:Object.fromEntries(identities)};
fs.writeFileSync(path.join(out,mode+'-result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
