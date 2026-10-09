'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const mapping = [];
for (const name of ['checkpoint.cjs', 'build-native.ps1', 'verify-package.ps1']) {
  const bytes = fs.readFileSync(path.join(__dirname,name));
  fs.writeFileSync(path.join(__dirname,'stage1-'+name),bytes,{flag:'wx'});
  const old = bytes.toString('utf8'), next = old.replace(/dev26-recovery-foundation/g,'dev26-recovery-foundation-retry1');
  assert.notEqual(next,old); fs.writeFileSync(path.join(__dirname,name),next);
}
for (const name of ['native-build-inputs.json','native-arm64-build.log','native-x64-build.log']) {
  assert.ok(!fs.existsSync(path.join(__dirname,'stage1-'+name)));
  fs.renameSync(path.join(__dirname,name),path.join(__dirname,'stage1-'+name));
  mapping.push({original:name,retained:'stage1-'+name});
}
fs.writeFileSync(path.join(__dirname,'stage1-retained-native.json'),JSON.stringify({createdUtc:new Date().toISOString(),
  reason:'Native production source remains unchanged. Native worker strengthened current_v2 test to edit actual title/body and regenerated fixture DTOs after the first complete inventory. Final qualification will re-capture and rebuild using the final frozen source/test inventory; original archives and logs remain intact.',
  originalArchives:'.build/editor-input-native/dev26-recovery-foundation/',mapping},null,2)+'\n',{flag:'wx'});
console.log('Native stage1 retained; final retry1 uses fresh paths.');
