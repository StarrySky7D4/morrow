const d=require('./style-device-check.cjs'),fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const out=path.join(d.out,'verified');fs.mkdirSync(out,{recursive:true});
function capture(name){const tree=d.layout(); fs.writeFileSync(path.join(out,name+'.json'),JSON.stringify(tree,null,2)+'\n'); d.run('screenshot','--path',path.join(out,name+'.png'));console.log('CAPTURE '+name);}
const styles=[['flat','扁平 · 默认'],['neumorphism','Neumorphism · 新拟态'],['paper','纸感'],['clay','黏土'],['fluent','Fluent 流畅'],['brutalist','粗野主义'],['industrial','工业风']];
// Start in Settings; exercise actual mounted surfaces and their layout changes.
if(require.main===module){
 d.click(d.seek('超透',false,true));
 for(const [id,label] of styles){d.click(d.seek('界面风格',false,true));d.click(d.seek(label,true));d.seek('界面风格',false,true);capture('clear-'+id);}
 const hap=path.resolve(__dirname,'../entry/build/default/outputs/default/entry-default-unsigned.hap');
 fs.writeFileSync(path.join(out,'package.json'),JSON.stringify({checkedUtc:new Date().toISOString(),sha256:crypto.createHash('sha256').update(fs.readFileSync(hap)).digest('hex'),note:'Style selection and screenshots on the installed build. Visual inspection is separate from command success.'},null,2)+'\n');
}
module.exports={capture};
