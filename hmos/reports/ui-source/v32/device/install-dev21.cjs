'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const d=require('./driver.cjs'),repo=path.resolve(__dirname,'../../../../..'),root=path.join(repo,'hmos');
const manifest=JSON.parse(fs.readFileSync(path.join(root,'reports/ui-source/v31/dev31-music-ui-a1-artifact.json')));
const hap=path.join(root,manifest.path),data=fs.readFileSync(hap),sha=crypto.createHash('sha256').update(data).digest('hex').toUpperCase();
assert.equal(data.length,manifest.bytes);assert.equal(sha,manifest.sha256);assert.equal(manifest.versionCode,1000021);
for(const item of JSON.parse(fs.readFileSync(path.join(root,'reports/ui-source/v31/dev31-music-ui-a1-repository-inputs.json'))).inputs){
  const b=fs.readFileSync(path.join(repo,item.path));assert.equal(b.length,item.bytes,item.path);assert.equal(crypto.createHash('sha256').update(b).digest('hex').toUpperCase(),item.sha256,item.path);
}
const tree=d.read();assert.ok(!d.flat(tree).some(x=>x.n.id==='draft-title'));assert.equal(d.flat(tree).filter(x=>x.n.text==='草稿 4').length,1);
const dump=()=>{const raw=d.hdc('shell','bm','dump','-n','dev.morrow.hmos');return JSON.parse(raw.slice(raw.indexOf('{')));};
const before=dump();assert.equal(before.applicationInfo.versionCode,1000020);
fs.writeFileSync(path.join(__dirname,'bundle-before-dev21.json'),JSON.stringify(before,null,2)+'\n',{flag:'wx'});
d.once('force-stop-dev20-before-update',['shell','aa','force-stop','dev.morrow.hmos']);
d.once('install-qualified-dev21',['install','-r',hap]);
const after=dump();assert.equal(after.applicationInfo.versionCode,1000021);assert.equal(after.applicationInfo.versionName,'0.1.0-hmos-dev.21');
fs.writeFileSync(path.join(__dirname,'bundle-after-dev21.json'),JSON.stringify(after,null,2)+'\n',{flag:'wx'});
d.once('start-qualified-dev21',['shell','aa','start','-a','EntryAbility','-b','dev.morrow.hmos']);
fs.writeFileSync(path.join(__dirname,'installation-dev21.json'),JSON.stringify({observedUtc:new Date().toISOString(),phase:'INSTALLED_AND_LAUNCHED',baseline:'62a8b964bb27840f0348bb6b8ef520142880b9b1',device:'127.0.0.1:5555',uuid:'01fc19c8-444a-42f1-9f70-79321a2502b3',hap,hostBytes:data.length,hostSha256:sha,versionCode:after.applicationInfo.versionCode,versionName:after.applicationInfo.versionName,scope:'Acknowledged update/install and start with bundle version readback. Host package identity verified; no protected on-device package hash, music runtime or UI qualification implied.'},null,2)+'\n',{flag:'wx'});
