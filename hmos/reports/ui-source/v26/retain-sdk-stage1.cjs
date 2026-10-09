'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const mapping = [];
for (const name of ['checkpoint.cjs','verify-package.ps1']) {
  const old = fs.readFileSync(path.join(__dirname,name),'utf8');
  fs.writeFileSync(path.join(__dirname,'sdk-stage1-'+name),old,{flag:'wx'});
  let next;
  if (name === 'checkpoint.cjs') {
    next = old.replace(".build/device-candidates/dev26-recovery-foundation-retry1", ".build/device-candidates/dev26-recovery-foundation-retry2")
      .replace(".build/artifacts/dev26-recovery-foundation-retry1/", ".build/artifacts/dev26-recovery-foundation-retry2/");
  } else { next = old.replace(/dev26-recovery-foundation-retry1/g,'dev26-recovery-foundation-retry2'); }
  assert.notEqual(next,old); fs.writeFileSync(path.join(__dirname,name),next);
}
for (const name of ['build-inputs.json','source-copy-manifest.json','artifact.json','hap-build.log','native-package-check.json']) {
  assert.ok(!fs.existsSync(path.join(__dirname,'sdk-stage1-'+name)));
  fs.renameSync(path.join(__dirname,name),path.join(__dirname,'sdk-stage1-'+name));
  mapping.push({original:name,retained:'sdk-stage1-'+name});
}
const hmos = path.resolve(__dirname,'../../..');
fs.writeFileSync(path.join(__dirname,'sdk-stage1-build-manifest.json'),fs.readFileSync(path.join(hmos,'reports/build-manifest.json')),{flag:'wx'});
fs.writeFileSync(path.join(__dirname,'sdk-stage1-retained.json'),JSON.stringify({createdUtc:new Date().toISOString(),
  reason:'The initial SDK product compiled successfully. A subsequent actual Store DTO test exposed Handoff summary rejecting the native serializer\'s omitted optional intent_next_after cursor. Preserve the pre-repair product and qualification; final retry2 will include the narrow model gate correction. Rust/archive source is unchanged and reused by verified byte identity.',
  retainedArtifact:'.build/artifacts/dev26-recovery-foundation-retry1/entry-default-unsigned.hap',mapping},null,2)+'\n',{flag:'wx'});
console.log('Pre-DTO-repair SDK evidence and artifact retained; retry2 paths prepared.');
