'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process'), assert = require('node:assert/strict');
const repo = path.resolve(__dirname,'../../../..');
const git = args => cp.execFileSync('git',['-C',repo,...args],{maxBuffer:64*1024*1024}).toString();
const tracked = new Set(git(['ls-files','--','hmos/tool','hmos/entry/src/main/ets','hmos/reports/ui-source']).trim().split('\n'));
const registered = new Set(['hmos/reports/ui-source/v26/run-models.cjs',
  'hmos/reports/ui-source/v26/editor-current-v2-store-fixture.json','hmos/reports/ui-source/v26/editor-reopen-history-store-fixture.json']);
const before = JSON.parse(fs.readFileSync(path.join(__dirname,'models-inputs-before.json')));
const after = JSON.parse(fs.readFileSync(path.join(__dirname,'models-inputs-after.json'))); assert.deepEqual(before,after);
const repository = after.filter(input => tracked.has(input.path) || registered.has(input.path));
const runtime = after.filter(input => !tracked.has(input.path) && !registered.has(input.path));
const ignored = new Set(cp.execFileSync('git',['-C',repo,'check-ignore','-z','--stdin'],
  {input:Buffer.from(runtime.map(input => input.path).join('\0')+'\0')}).toString().split('\0').filter(Boolean));
for (const input of runtime) assert.ok(ignored.has(input.path),'Unregistered non-ignored model input: '+input.path);
for (const name of registered) assert.ok(repository.some(input => input.path === name));
for (const [name, data] of [['models-repository-inputs.json',repository],['models-supplemental-inputs.json',runtime]])
  fs.writeFileSync(path.join(__dirname,name),JSON.stringify(data,null,2)+'\n',{flag:'wx'});
const result = {checkedUtc:new Date().toISOString(),status:'PASS',total:after.length,repositoryInputs:repository.length,
  ignoredSupplementalObservedFiles:runtime.length,sourceIdentityVerified:true,
  note:'The broad before/after inventory also observed ignored tool dependencies and historical tester work outputs. Those extra files were stable, but are not claimed as compiler or test-read inputs. Stage verification uses the actual repository/model/test/fixture inputs; ignored supplemental files are not staged.'};
fs.writeFileSync(path.join(__dirname,'models-input-partition.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify(result));
