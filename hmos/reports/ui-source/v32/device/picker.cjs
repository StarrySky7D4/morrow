'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),d=require('./driver.cjs');
function read(){const raw=d.ui('layout','--format','json'),tree=JSON.parse(raw.slice(raw.indexOf('['))),nodes=d.flat(tree);
  assert.ok(nodes.some(x=>x.n.type==='SheetPage'),'Expected actual picker sheet');assert.ok(!nodes.some(x=>x.n.id==='Paf_Permission_Sheet_Window_Builder'),'Permission sheet requires manual input');return tree;}
function click(label,text){const targets=d.flat(read()).filter(x=>x.n.text===text&&x.n.bounds&&x.n.bounds[3]>x.n.bounds[1]);assert.equal(targets.length,1,'One actual provider text target');const[l,t,r,b]=targets[0].n.bounds;return d.once(label,['shell','uitest','uiInput','click',Math.round((l+r)/2),Math.round((t+b)/2)]);}
function capture(label){const tree=read();fs.writeFileSync(path.join(d.out,label+'.json'),JSON.stringify(tree,null,2)+'\n',{flag:'wx'});d.ui('screenshot','--path',path.join(d.out,label+'.png'));
console.log(JSON.stringify(d.flat(tree).filter(x=>x.n.text||x.n.id).map(x=>({id:x.n.id,text:x.n.text,type:x.n.type,bounds:x.n.bounds})).slice(-100)));return tree;}
module.exports={read,click,capture};
if(require.main===module){click(process.argv[2],process.argv[3]);capture(process.argv[4]);}
