process.env.HMOS_DEVICE = process.env.HMOS_DEVICE || '127.0.0.1:5555';
process.env.HMOS_REPORT_DIR = process.env.HMOS_REPORT_DIR || require('node:path').resolve(__dirname,'../reports/ui-source/v15/device-baseline');
const {f,a,editorSeek} = require('./inline-device-check.cjs');
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
function capture(label) {
  const tree=a.pickerTree(); const file=path.join(f.out,label+'.json'); assert.ok(!fs.existsSync(file),'fresh report');
  fs.writeFileSync(file,JSON.stringify(tree.map(x=>({n:x.n,parents:x.parents.map(p=>({type:p.type,id:p.id}))})),null,2)+'\n');
  f.d.run('screenshot','--path',path.join(f.out,label+'.png'));
  console.log(tree.filter(x=>x.n.text||x.n.id).map(x=>({type:x.n.type,id:x.n.id,text:x.n.text,bounds:x.n.bounds,parents:x.parents.map(p=>p.type)})));
}
const stage=process.argv[2],label=process.argv[3]; assert.ok(label);
if(stage==='open') {
  assert.equal(editorSeek(n=>n.id==='draft-title','own fixture',true).text,'HMOS-media-20261007-A');
  f.click(editorSeek(n=>n.id==='editor-import','import')); capture(label);
} else if(stage==='capture') {capture(label);}
else if(stage==='click') {
  const value=process.argv[4]; const candidates=a.pickerTree().filter(x=>x.n.text===value&&x.parents.some(p=>p.type==='SheetPage'));
  assert.equal(candidates.length,1,'one observed provider label'); f.click(candidates[0].n); capture(label);
} else throw Error('Explicit observed stage required');
