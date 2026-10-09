'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process'),assert=require('node:assert/strict');
const d=require('./driver.cjs'),repo=path.resolve(__dirname,'../../../../..'),root=path.join(repo,'hmos');
const sourceCommit='ada95e0f119b3d3c3273b8e5bf6da031d3b10c10';
const sha=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
const manifest=JSON.parse(fs.readFileSync(path.join(root,'reports/ui-source/v32/dev32-glass-alignment-a1-artifact.json')));
const hap=path.join(root,manifest.path),data=fs.readFileSync(hap);assert.equal(data.length,manifest.bytes);assert.equal(sha(data),manifest.sha256);assert.equal(manifest.versionCode,1000022);
const inputs=JSON.parse(fs.readFileSync(path.join(root,'reports/ui-source/v32/dev32-glass-alignment-a1-repository-inputs.json'))).inputs;
assert.equal(inputs.length,367);
for(const item of inputs){const b=cp.execFileSync('git',['-C',repo,'show',sourceCommit+':'+item.path],{maxBuffer:64*1024*1024});assert.equal(b.length,item.bytes,item.path);assert.equal(sha(b),item.sha256,item.path);}
const copy=JSON.parse(fs.readFileSync(path.join(root,'reports/ui-source/v32/dev32-glass-alignment-a1-source-copy.json')));
for(const item of copy.inputs){const b=fs.readFileSync(path.join(copy.project,item.path));assert.equal(b.length,item.bytes,item.path);assert.equal(sha(b),item.sha256,item.path);}
const tree=d.read();assert.ok(!d.flat(tree).some(x=>x.n.id==='draft-title'));assert.equal(d.flat(tree).filter(x=>x.n.text==='草稿 4').length,1);
const dump=()=>{const raw=d.hdc('shell','bm','dump','-n','dev.morrow.hmos');return JSON.parse(raw.slice(raw.indexOf('{')));};
const before=dump();assert.equal(before.applicationInfo.versionCode,1000021);
fs.writeFileSync(path.join(__dirname,'bundle-before-dev22.json'),JSON.stringify(before,null,2)+'\n',{flag:'wx'});
d.once('force-stop-dev21-before-update',['shell','aa','force-stop','dev.morrow.hmos']);
d.once('install-qualified-dev22',['install','-r',hap]);
const after=dump();assert.equal(after.applicationInfo.versionCode,1000022);assert.equal(after.applicationInfo.versionName,'0.1.0-hmos-dev.22');
fs.writeFileSync(path.join(__dirname,'bundle-after-dev22.json'),JSON.stringify(after,null,2)+'\n',{flag:'wx'});
d.once('start-qualified-dev22',['shell','aa','start','-a','EntryAbility','-b','dev.morrow.hmos']);
fs.writeFileSync(path.join(__dirname,'installation-dev22.json'),JSON.stringify({observedUtc:new Date().toISOString(),phase:'INSTALLED_AND_LAUNCHED',sourceCommit,device:'127.0.0.1:5555',uuid:'01fc19c8-444a-42f1-9f70-79321a2502b3',hap,hostBytes:data.length,hostSha256:sha(data),versionCode:after.applicationInfo.versionCode,versionName:after.applicationInfo.versionName,scope:'Installed immutable v32/dev22 package, bound to exact committed source and copied build. Current v33 edits are excluded. Bundle version readback and host HAP hash do not prove protected on-device package bytes, all UI pixels or music playback.'},null,2)+'\n',{flag:'wx'});
