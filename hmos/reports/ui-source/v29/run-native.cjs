'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const repo=path.resolve(__dirname,'../../../..');
const nativeFile=path.join(__dirname,'native-build-inputs.json');
const inventory=JSON.parse(fs.readFileSync(nativeFile)).repository_source_inventory;
const sha=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
function verify(){for(const x of inventory){const b=fs.readFileSync(path.join(repo,x.path));assert.equal(b.length,x.bytes);assert.equal(sha(b),x.sha256,x.path);}}
verify();
for(const [abi,name] of [['arm64-v8a','native-arm64-build.log'],['x86_64','native-x64-build.log']]){
 const began=new Date().toISOString();
 const r=cp.spawnSync('pwsh',['-NoLogo','-NoProfile','-File',path.join(__dirname,'build-native.ps1'),'-Abi',abi,'-RepositoryRoot',repo],{cwd:repo,encoding:'utf8',maxBuffer:64*1024*1024});
 const log=(r.stdout||'')+(r.stderr||'');fs.writeFileSync(path.join(__dirname,name),log,{flag:'wx'});verify();
 const proof={startedUtc:began,finishedUtc:new Date().toISOString(),abi,exitCode:r.status,signal:r.signal,status:r.status===0?'PASS':'FAILED_OR_UNKNOWN',sourceIdentityVerified:true,inputs:inventory.length,log:name,sha256:sha(Buffer.from(log)),scope:'Fresh locked/offline OHOS release archive; compile only, no device qualification.'};
 fs.writeFileSync(path.join(__dirname,name.replace('.log','-result.json')),JSON.stringify(proof,null,2)+'\n',{flag:'wx'});console.log(log.slice(-1000));console.log(JSON.stringify(proof));assert.equal(r.status,0);
}