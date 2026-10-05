// Verify raw bytes (hmos/.gitattributes disables line-ending conversion).
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..'),repo=path.resolve(root,'..'),manifest=JSON.parse(fs.readFileSync(path.join(root,'reports/build-manifest.json'),'utf8'));
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const staged=process.argv.includes('--staged');
assert.equal(new Set(manifest.inputs.map(i=>i.path)).size,manifest.inputs.length,'unique build input paths');
for(const input of manifest.inputs){
  assert.equal(hash(fs.readFileSync(path.join(root,input.path))),input.sha256,'disk input '+input.path);
  if(staged){
    const bytes=cp.execFileSync('git',['-C',repo,'show',':hmos/'+input.path.replaceAll('\\','/')],{maxBuffer:32*1024*1024});
    assert.equal(hash(bytes),input.sha256,'staged input '+input.path);
  }
}
const artifact=fs.readFileSync(path.join(root,manifest.artifact.path));
assert.equal(artifact.length,manifest.artifact.bytes);assert.equal(hash(artifact),manifest.artifact.sha256,'HAP bytes');
assert.equal(JSON.parse(fs.readFileSync(path.join(root,'AppScope/app.json5'),'utf8')).app.versionName,manifest.hmosVersion);
console.log(JSON.stringify({result:'PASS',version:manifest.hmosVersion,inputs:manifest.inputs.length,staged,artifactSha256:manifest.artifact.sha256}));
