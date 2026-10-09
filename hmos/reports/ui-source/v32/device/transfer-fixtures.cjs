'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),d=require('./driver.cjs');
const fixture=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../v31/device/audio-fixtures.json')));
for(const file of fixture.files){
  assert.match(file.name,/^HMOS-v31-10105badbf93(?:-(?:long|short)\.wav|\.lrc)$/);
  assert.equal(file.remotePath,'/data/service/el2/100/hmdfs/account/files/Docs/Download/'+file.name);
  const bytes=fs.readFileSync(file.localPath);assert.equal(bytes.length,file.bytes);assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),file.sha256);
  const existing=d.hdc('shell','ls','-l',file.remotePath);assert.match(existing,/No such file or directory/,'Do not overwrite existing generated fixture');
  const proofPath=path.join(__dirname,file.name+'-transfer.json');assert.ok(!fs.existsSync(proofPath),'Do not replay transfer');
  const proof={issuedUtc:new Date().toISOString(),phase:'ISSUED_UNKNOWN',name:file.name,remotePath:file.remotePath,bytes:file.bytes,sha256:file.sha256};
  fs.writeFileSync(proofPath,JSON.stringify(proof,null,2)+'\n',{flag:'wx'});
  try{
    proof.response=d.hdc('file','send',file.localPath,file.remotePath);assert.match(proof.response,/FileTransfer finish/);
    proof.remoteBytes=Number(d.hdc('shell','stat','-c','%s',file.remotePath).trim());proof.remoteSha256=d.hdc('shell','sha256sum',file.remotePath).trim().split(/\s+/)[0];
    assert.equal(proof.remoteBytes,file.bytes);assert.equal(proof.remoteSha256,file.sha256);proof.phase='TRANSFER_VERIFIED';proof.verifiedUtc=new Date().toISOString();
  }catch(error){proof.error=String(error);throw error;}finally{fs.writeFileSync(proofPath,JSON.stringify(proof,null,2)+'\n');}
  console.log(JSON.stringify(proof));
}
