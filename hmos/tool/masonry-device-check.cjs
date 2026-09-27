// Requires the 16 explicit masonry fixtures and overview at its top on the
// development emulator. All actions target observed nodes; no database access.
const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const out=process.env.HMOS_REPORT_DIR||path.resolve(__dirname,'../reports/ui-source/v7/device');fs.mkdirSync(out,{recursive:true});
const nodes=()=>d.flatten(d.layout()).map(x=>x.n);
const cards=t=>d.flatten(t).map(x=>x.n).filter(n=>n.id?.startsWith('workspace-card:'));
function label(c){return d.flatten([c]).map(x=>x.n.text).find(t=>t?.startsWith('HMOS-masonry-'));}
function back(){cp.execFileSync('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe',['-t',process.env.HMOS_DEVICE||'127.0.0.1:5555','shell','uitest','uiInput','keyEvent','Back']);}
function editBody(cardId,text){
  const card=cards(d.layout()).find(n=>n.id===cardId);assert.ok(card);
  d.click(d.flatten([card]).find(x=>x.n.text===label(card)).n);
  const field=nodes().find(n=>n.type==='TextArea');assert.ok(field);d.click(field);
  cp.execFileSync('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe',['-t',process.env.HMOS_DEVICE||'127.0.0.1:5555','shell','uitest','uiInput','keyEvent','2072','2017']);
  d.run('text',text);back();
  assert.equal(nodes().find(n=>n.type==='TextArea')?.text,text,'editor received exact replacement text');
  d.click(nodes().find(n=>n.text==='保存灵感'));
  assert.ok(!nodes().some(n=>n.text==='保存灵感'),'revision saved');
}
function capture(name){const t=d.layout();fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(t,null,2)+'\n');d.run('screenshot','--path',path.join(out,name+'.png'));return t;}
function filter(text){const n=nodes().find(n=>n.type==='TextInput'&&n.bounds[1]<120);assert.ok(n);d.run('text',text,String(Math.round((n.bounds[0]+n.bounds[2])/2)),String(Math.round((n.bounds[1]+n.bounds[3])/2)));back();}
function clear(){const n=nodes().find(n=>n.text==='×'&&n.bounds[1]<120);assert.ok(n);d.click(n);}
function gaps(items){
  const columns=new Map();for(const item of items){const x=item.bounds[0];if(!columns.has(x))columns.set(x,[]);columns.get(x).push(item);}
  for(const column of columns.values()){
    column.sort((a,b)=>a.bounds[1]-b.bounds[1]);
    for(let i=1;i<column.length;i++)assert.ok(column[i].bounds[1]-column[i-1].bounds[3]>=19,
      'no overlap and at least 14 vp gap: '+label(column[i-1])+' -> '+label(column[i]));
  }
}
function swipe(up=true){const t=d.layout();const b=d.flatten(t).find(x=>x.n.type==='Scroll')?.n.bounds;assert.ok(b,'workspace scroll node');const x=Math.round((b[0]+b[2])/2),top=b[1]+180,bottom=b[3]-180;d.run('swipe',String(x),String(up?bottom:top),String(x),String(up?top:bottom));}
if(nodes().some(n=>n.text==='×'&&n.bounds[1]<120))clear();
filter('HMOS-masonry-');
if(nodes().some(n=>n.text==='默认顺序')){d.click(nodes().find(n=>n.text==='默认顺序'));d.click(nodes().find(n=>n.text==='标题'));}
let t=capture('masonry-top'),first=cards(t),byTitle=new Map(first.map(c=>[label(c),c]));
gaps(first);
const a=byTitle.get('HMOS-masonry-01'),b=byTitle.get('HMOS-masonry-02'),c=byTitle.get('HMOS-masonry-03');assert.ok(a&&b&&c,'first three fixtures visible');
assert.equal(a.bounds[1],b.bounds[1],'first row shares top');
assert.ok(a.bounds[3]>b.bounds[3]+20,'long card is taller');
assert.equal(c.bounds[0],b.bounds[0],'third card fills the shorter column');
assert.ok(Math.abs(c.bounds[1]-b.bounds[3]-21)<=2,'14 vp vertical gap at density 1.5');
assert.ok(c.bounds[1]<a.bounds[3]+21,'third card does not wait for the taller row');
// Mutation must repaint the same identity, then restore its original favorite.
const button=card=>d.flatten([card]).find(x=>x.n.type==='Button').n;
const icon=card=>d.flatten([button(card)]).find(x=>x.n.type==='Text').n.text;
const original=icon(a);d.click(button(a));
let changed=cards(d.layout()).find(n=>n.id===a.id);assert.ok(changed);assert.notEqual(icon(changed),original,'revision notification refreshes favorite');
d.click(button(changed));changed=cards(d.layout()).find(n=>n.id===a.id);assert.equal(icon(changed),original,'favorite restored');
const originalHeight=b.bounds[3]-b.bounds[1];
editBody(b.id,'LongMasonryCard-'.repeat(20));
const taller=cards(d.layout()).find(n=>n.id===b.id);assert.ok(taller.bounds[3]-taller.bounds[1]>originalHeight+20,'editing grows the same card');gaps(cards(d.layout()));capture('masonry-edited');
editBody(b.id,'Short');
const restored=cards(d.layout()).find(n=>n.id===b.id);assert.ok(Math.abs(restored.bounds[3]-restored.bounds[1]-originalHeight)<=2,'editing shrinks the same card');gaps(cards(d.layout()));
// Reordering must retain the correct title/id association.
const identity=new Map(first.map(card=>[label(card),card.id]));
d.click(nodes().find(n=>n.text==='标题'));d.click(nodes().find(n=>n.text==='默认顺序'));
for(const card of cards(d.layout()))if(identity.has(label(card)))assert.equal(card.id,identity.get(label(card)));
gaps(cards(d.layout()));
d.click(nodes().find(n=>n.text==='默认顺序'));d.click(nodes().find(n=>n.text==='标题'));
capture('masonry-title-sort');
const seen=new Set();let maxMountedInAccessibility=0;
for(let i=0;i<12;i++){
  const current=cards(d.layout());maxMountedInAccessibility=Math.max(maxMountedInAccessibility,current.length);
  gaps(current);
  assert.equal(new Set(current.map(c=>c.id)).size,current.length,'no duplicate visible identities');
  current.forEach(c=>{if(label(c))seen.add(label(c));});
  if(seen.size===16)break;
  swipe();
}
assert.equal(seen.size,16,'all sixteen records remain reachable');capture('masonry-bottom');
for(let i=0;i<8&&!nodes().some(n=>n.text==='A LITTLE SPACE FOR BIG IDEAS');i++)swipe(false);
// Search changes cannot retain stale card nodes.
clear();filter('HMOS-masonry-no-result');
assert.equal(cards(d.layout()).length,0,'empty result removes all cards');assert.ok(nodes().some(n=>n.text==='没有找到匹配的想法'));capture('masonry-empty');
clear();filter('HMOS-masonry-');assert.ok(cards(d.layout()).some(n=>label(n)==='HMOS-masonry-01'),'cards restored after empty result');
d.click(nodes().find(n=>n.text==='标题'));d.click(nodes().find(n=>n.text==='默认顺序'));clear();
capture('masonry-overview-restored');
const result={pass:['shortest-column placement','14 vp gaps','same-id favorite refresh','sort identity','16 records reachable','empty result clears cards','query restoration'],firstThree:[a.bounds,b.bounds,c.bounds],visibleAccessibilityPeak:maxMountedInAccessibility,seen:[...seen].sort(),note:'Accessibility visibility is not a heap or native cache allocation measurement.'};
fs.writeFileSync(path.join(out,'result.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
