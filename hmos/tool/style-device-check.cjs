// Requires the HMOS Settings page open on the test emulator. Uses observed UI
// node bounds, never writes application preference/database files directly.
const cp=require('node:child_process'),fs=require('node:fs'),path=require('node:path');
const cli=process.env.HMOS_CLI || path.join(process.env.APPDATA,'npm/node_modules/@deveco/deveco-cli/dist/cli.js');
const device=process.env.HMOS_DEVICE || '127.0.0.1:5555';
const out=path.resolve(__dirname,'../reports/ui-source/v5');
function run(...args){return cp.execFileSync(process.execPath,[cli,'ui',...args,'--device',device],{encoding:'utf8',timeout:30000,maxBuffer:4*1024*1024});}
function layout(){const raw=run('layout','--format','json');const tree=JSON.parse(raw.slice(raw.indexOf('[')));
 if(flatten(tree).some(x=>(x.n.type || '').startsWith('WindowScene') || x.n.id==='Paf_Permission_Sheet_Window_Builder'))throw Error('HMOS app is not in foreground; stop without gestures');
 return tree;}
function flatten(nodes,parents=[]){return nodes.flatMap(n=>[{n,parents},...flatten(n.children || [],[...parents,n])]);}
function scrollBounds(tree){return flatten(tree).find(x=>x.n.type==='Scroll')?.n.bounds;}
function find(tree,text,option=false){const view=scrollBounds(tree) || tree[0].bounds;return flatten(tree).find(({n,parents})=>{
 const b=n.bounds; if(n.text!==text || !b || b[1]<view[1]+2 || b[3]>view[3]-2 || b[3]-b[1]<8) return false;
 return !option || parents.some(p=>p.type==='Row' && flatten(p.children || []).some(x=>x.n.text==='Aa'));
});}
function seek(text,option=false,up=false){
 for(let i=0;i<12;i++){const tree=layout(),found=find(tree,text,option); if(found)return found.n;
  const b=scrollBounds(tree);if(!b)throw Error('No scroll surface for '+text);
  const x=Math.round((b[0]+b[2])/2),top=Math.round(b[1]+(b[3]-b[1])*.25),bottom=Math.round(b[3]-(b[3]-b[1])*.18);
  run('swipe',String(x),String(up?top:bottom),String(x),String(up?bottom:top));}
 throw Error('Visible UI text not found: '+text);
}
function click(n){const [l,t,r,b]=n.bounds;run('click',String(Math.round((l+r)/2)),String(Math.round((t+b)/2)));}
fs.mkdirSync(out,{recursive:true});
const choices=[['flat','扁平 · 默认'],['neumorphism','Neumorphism · 新拟态'],['paper','纸感'],['clay','黏土'],['fluent','Fluent 流畅'],['brutalist','粗野主义'],['industrial','工业风']];
if(require.main === module) for(const [id,label] of choices){
 click(seek('界面风格',false,true)); click(seek(label,true));
 seek('界面风格',false,true);
 const tree=layout();
 if(!flatten(tree).some(x=>x.n.text===label))throw Error('Selected label missing: '+label);
 fs.writeFileSync(path.join(out,'hmos-'+id+'.json'),JSON.stringify(tree,null,2)+'\n');
 run('screenshot','--path',path.join(out,'hmos-'+id+'.png'));
 console.log('PASS selected '+id);
}
module.exports={run,layout,flatten,find,seek,click,out};
