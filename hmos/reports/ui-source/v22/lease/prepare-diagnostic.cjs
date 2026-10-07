'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const report=__dirname,root=path.resolve(report,'../../../..'),base=JSON.parse(fs.readFileSync(path.join(report,'source-copy-manifest.json')));
const project=path.join(root,'.build/device-candidates/dev22-lease-diagnostic'),sha=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
assert.ok(!fs.existsSync(project));fs.mkdirSync(project,{recursive:true});const inputs=[];
for(const item of base.inputs){let bytes=fs.readFileSync(path.join(base.project,item.path));assert.equal(sha(bytes),item.sha256);let diagnostic=false;
 if(item.path==='entry/src/main/ets/pages/EditorViewLease.ets'){
  let text=bytes.toString();const needle='    this.mountedOwner = this.owner;';assert.ok(text.includes(needle));text=text.replace(needle,needle+"\n    console.info('HMOS_TODO_DIAG leaseOwner=' + this.owner + ' mounted=' + this.mountedOwner);");bytes=Buffer.from(text);diagnostic=true;
 }
 if(item.path==='entry/src/main/ets/pages/EditorTodos.ets'){
  let text=bytes.toString();const needle='  private sourceChanged(): void {';assert.ok(text.includes(needle));text=text.replace(needle,needle+"\n    console.info('HMOS_TODO_DIAG source alive=' + this.alive + ' owner=' + this.ownerKey + ' revision=' + this.revision + ' enabled=' + this.editingEnabled + ' current=' + this.isCurrent(this.ownerKey, this.revision, copyText(this.source)));");bytes=Buffer.from(text);diagnostic=true;
 }
 const target=path.join(project,item.path);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes,{flag:'wx'});inputs.push({path:item.path,bytes:bytes.length,sha256:sha(bytes),before_sha256:item.sha256,diagnostic});
}
const manifest={createdUtc:new Date().toISOString(),project,baseline:base.baseline,inputs,scope:'Temporary console-only ownership diagnostics. Frozen UI candidate/native; no raw text logged and no new business backend included.'};
fs.writeFileSync(path.join(report,'diagnostic-copy-manifest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({project,inputs:inputs.length,diagnosticFiles:inputs.filter(i=>i.diagnostic).length}));
