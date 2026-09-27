// Explicit fixture writer for the development emulator. Never opens a database
// file or retries a mutation. A failed run requires inspecting the saved UI.
const d=require('./style-device-check.cjs'),cp=require('node:child_process'),assert=require('node:assert/strict');
const hdc='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const nodes=()=>d.flatten(d.layout()).map(x=>x.n);
function back(){cp.execFileSync(hdc,['-t',process.env.HMOS_DEVICE||'127.0.0.1:5555','shell','uitest','uiInput','keyEvent','Back']);}
function type(n,text){assert.ok(n);d.run('text',text,String(Math.round((n.bounds[0]+n.bounds[2])/2)),String(Math.round((n.bounds[1]+n.bounds[3])/2)));back();}
const start=Number(process.argv[2]),end=Number(process.argv[3]);
assert.ok(start>=1 && end>=start && end<=40,'provide explicit inclusive fixture number range');
for(let i=start;i<=end;i++){
  const title='HMOS-masonry-'+String(i).padStart(2,'0');
  let all=nodes();assert.ok(!all.some(n=>n.text==='保存灵感'),'close any existing editor before seeding');
  assert.ok(!all.some(n=>n.text===title),'fixture is already visible; do not replay');
  d.click(all.find(n=>n.text==='新建灵感'));
  type(nodes().filter(n=>n.type==='TextInput').at(-1),title);
  type(nodes().find(n=>n.type==='TextArea'),i%2===1?'LongMasonryCard-'.repeat(20):'Short');
  d.click(nodes().find(n=>n.text==='保存灵感'));
  assert.ok(!nodes().some(n=>n.text==='保存灵感'),'save did not close editor');
  console.log('CREATED '+title);
}
