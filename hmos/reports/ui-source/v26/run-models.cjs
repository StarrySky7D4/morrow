'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, repo = path.resolve(report, '../../../..');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
function files(relative) {
  return fs.readdirSync(path.join(repo, relative), { withFileTypes: true }).sort((a,b) => a.name.localeCompare(b.name))
    .flatMap(item => item.isDirectory() ? files(relative + '/' + item.name) : [relative + '/' + item.name]);
}
const names = [...new Set([...files('hmos/tool'), ...files('hmos/entry/src/main/ets'),
  ...files('hmos/reports/ui-source').filter(name => /store-fixture\.json$/.test(name)),
  'hmos/reports/ui-source/v26/run-models.cjs'])].sort();
const inventory = () => names.map(name => { const bytes = fs.readFileSync(path.join(repo,name));
  return {path:name,bytes:bytes.length,sha256:sha(bytes)}; });
const write = (name,value) => fs.writeFileSync(path.join(report,name),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
const before = inventory(); write('models-inputs-before.json',before);
const tests = fs.readdirSync(path.join(repo,'hmos/tool')).filter(name => name.endsWith('.test.cjs')).sort()
  .map(name => path.join(repo,'hmos/tool',name));
const started = new Date().toISOString();
const result = cp.spawnSync(process.execPath,['--test',...tests],{cwd:repo,encoding:'utf8',maxBuffer:64*1024*1024});
const log = (result.stdout || '') + (result.stderr || '');
fs.writeFileSync(path.join(report,'models-final-tests.log'),log,{flag:'wx'});
const after = inventory(); write('models-inputs-after.json',after); assert.deepEqual(after,before,'Actual model/test input drift');
const proof = {startedUtc:started,finishedUtc:new Date().toISOString(),exitCode:result.status,signal:result.signal,
  suites:tests.length,inputs:names.length,sourceIdentityVerified:true,log:'models-final-tests.log',sha256:sha(Buffer.from(log)),
  scope:'Actual-source host tool/models with Store-produced fixture DTOs; controlled lifecycle/transport seams are not device acceptance. Index v26 recovery/new-mode integration OPEN.'};
write('models-result.json',proof); console.log(log.slice(-1400)); console.log(JSON.stringify(proof));
assert.equal(result.status,0,'Model test failure');
