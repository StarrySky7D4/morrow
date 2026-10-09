'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),d=require('./driver.cjs');
const manifest=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../v31/device/audio-fixtures.json')));
for(const file of manifest.files){
  assert.match(file.name,/^HMOS-v31-10105badbf93(?:-(?:long|short)\.wav|\.lrc)$/);
  const destination='/mnt/hmdfs/100/account/device_view/local/files/Docs/Download/'+file.name,temp='/data/local/tmp/morrow-v32-fixture-'+file.name;
  for(const candidate of [destination,temp])assert.match(d.hdc('shell','ls','-l',candidate),/No such file or directory/,'No fixture overwrite');
  const bytes=fs.readFileSync(file.localPath);assert.equal(bytes.length,file.bytes);assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),file.sha256);
  const proofPath=path.join(__dirname,file.name+'-shell-transfer.json');assert.ok(!fs.existsSync(proofPath));
  const proof={issuedUtc:new Date().toISOString(),phase:'TEMP_TRANSFER_ISSUED_UNKNOWN',temp,destination,expectedBytes:file.bytes,expectedSha256:file.sha256,scope:'Generated test asset only; standard shell existing file_manager group, no permission or identity changes.'};
  const write=()=>fs.writeFileSync(proofPath,JSON.stringify(proof,null,2)+'\n');write();
  try{
    proof.tempResponse=d.hdc('file','send',file.localPath,temp);assert.match(proof.tempResponse,/FileTransfer finish/);proof.phase='PUBLIC_COPY_ISSUED_UNKNOWN';proof.copyIssuedUtc=new Date().toISOString();write();
    proof.copyResponse=d.hdc('shell','cp','-n',temp,destination);
    proof.actualBytes=Number(d.hdc('shell','stat','-c','%s',destination).trim());proof.actualSha256=d.hdc('shell','sha256sum',destination).trim().split(/\s+/)[0];
    assert.equal(proof.actualBytes,file.bytes);assert.equal(proof.actualSha256,file.sha256);proof.phase='PUBLIC_FIXTURE_VERIFIED';proof.verifiedUtc=new Date().toISOString();
  }catch(error){proof.error=String(error);throw error;}finally{write();}
  console.log(JSON.stringify(proof));
}
