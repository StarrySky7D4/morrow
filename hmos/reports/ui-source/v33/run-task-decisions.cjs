'use strict';
// Hash-bound actual Index/ETS/Workbench source execution. No SDK/device run.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process');
const root=path.resolve(__dirname,'../../../..'),label=process.argv[2]||'a1';
if(!/^[a-z0-9-]+$/.test(label))throw Error('Invalid immutable run label');
const prefix=path.join(__dirname,'task-decisions-'+label),names={before:prefix+'-inputs-before.json',after:prefix+'-inputs-after.json',
 log:prefix+'-tests.log',trace:prefix+'-trace.jsonl',result:prefix+'-result.json'};
for(const name of Object.values(names))if(fs.existsSync(name))throw Error('Existing evidence is retained: '+name);
const tests=['hmos/tool/index-task-decision-integration.test.cjs','hmos/tool/index-business-integration.test.cjs',
 'hmos/tool/index-business-recovery-integration.test.cjs','hmos/tool/index-editor-field-integration.test.cjs',
 'hmos/tool/index-editor-todos-integration.test.cjs','hmos/tool/index-draft-fork-integration.test.cjs'];
const models=()=>fs.readdirSync(path.join(root,'hmos/entry/src/main/ets/model')).filter(name=>name.endsWith('.ets')).map(name=>'hmos/entry/src/main/ets/model/'+name).sort();
const modelNames=models();
const local=[...new Set([...modelNames,'hmos/entry/src/main/ets/pages/Index.ets','hmos/entry/src/main/ets/pages/EditorViewLease.ets',
 'hmos/tool/index-business-test-harness.cjs','hmos/tool/index-business-recovery-test-harness.cjs','hmos/tool/editor-field-test-harness.cjs',
 'hmos/tool/index-task-source-trace.cjs','hmos/reports/ui-source/v33/run-task-decisions.cjs',
 'hmos/reports/ui-source/v24/editor-intent-store-fixture.json','hmos/reports/ui-source/v26/editor-reopen-history-store-fixture.json',
 'hmos/reports/ui-source/v27/editor-card-source-store-fixture.json',...tests])].sort();
const sha=input=>crypto.createHash('sha256').update(input).digest('hex').toUpperCase();
function inventory(files,relative){return files.map(name=>{const filename=relative?path.join(root,name):name,stat=fs.lstatSync(filename);
 if(!stat.isFile()||stat.isSymbolicLink())throw Error('Unsafe source input: '+name);
 const bytes=fs.readFileSync(filename);return {path:name,bytes:bytes.length,sha256:sha(bytes)};});}
const tsRoot=process.env.HMOS_TYPESCRIPT_PATH||process.env.HMOS_TYPESCRIPT||'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript';
const runtime=[process.execPath,require.resolve(tsRoot),path.join(tsRoot,'package.json')];
const before=inventory(local,true),runtimeBefore=inventory(runtime,false),started=new Date();
fs.writeFileSync(names.before,JSON.stringify({before,runtimeBefore},null,2)+'\n',{flag:'wx'});
const args=['--require',path.join(root,'hmos/tool/index-task-source-trace.cjs'),'--test','--test-reporter=tap',...tests];
const run=cp.spawnSync(process.execPath,args,{cwd:root,encoding:'utf8',maxBuffer:20*1024*1024,
 env:{...process.env,INDEX_TASK_SOURCE_TRACE:names.trace}});
const log=(run.stdout||'')+(run.stderr||'');fs.writeFileSync(names.log,log,{flag:'wx'});
const after=inventory(local,true),runtimeAfter=inventory(runtime,false),observed=new Map(),readDrift=[];
if(fs.existsSync(names.trace))for(const line of fs.readFileSync(names.trace,'utf8').trim().split('\n').filter(Boolean)){
 for(const item of JSON.parse(line).files){const prior=observed.get(item.path);
  if(prior&&(prior.bytes!==item.bytes||prior.sha256!==item.sha256))readDrift.push(item.path);observed.set(item.path,item);}
}
const actualReads=[...observed.values()].sort((a,b)=>a.path.localeCompare(b.path)),expected=new Map(before.map(item=>[item.path,item]));
const unqualifiedReads=actualReads.filter(item=>!expected.has(item.path)||JSON.stringify(expected.get(item.path))!==JSON.stringify(item));
const required=local.filter(name=>name!=='hmos/reports/ui-source/v33/run-task-decisions.cjs');
const missingReads=required.filter(name=>!observed.has(name)),modelListUnchanged=JSON.stringify(modelNames)===JSON.stringify(models());
const count=name=>Number(log.match(new RegExp('^# '+name+' ([0-9.]+)$','m'))?.[1]??-1);
const totals={tests:count('tests'),pass:count('pass'),fail:count('fail'),cancelled:count('cancelled'),skipped:count('skipped'),todo:count('todo')};
const expectedTests=tests.reduce((sum,name)=>sum+(fs.readFileSync(path.join(root,name),'utf8').match(/^test\(/gm)||[]).length,0);
const result={schema_version:1,scope:'ACTUAL_INDEX_METHOD_AND_UI_CALLBACK_EXECUTION_WITH_CONTROLLED_NATIVE_PLATFORM_SEAMS_ONLY',
 base_commit:'ada95e0f119b3d3c3273b8e5bf6da031d3b10c10',started_at:started.toISOString(),finished_at:new Date().toISOString(),
 command:args,tests,totals,expected_tests:expectedTests,duration_ms:count('duration_ms'),exit_code:run.status,signal:run.signal||'',error:run.error?.message||'',
 node:process.execPath,node_version:process.version,typescript_version:JSON.parse(fs.readFileSync(path.join(tsRoot,'package.json'),'utf8')).version,
 repository_inputs:local.length,actual_repository_reads:actualReads.length,all_inputs_exact:JSON.stringify(before)===JSON.stringify(after),
 runtime_exact:JSON.stringify(runtimeBefore)===JSON.stringify(runtimeAfter),model_list_unchanged:modelListUnchanged,
 missing_reads:missingReads,unqualified_reads:unqualifiedReads,read_drift:readDrift,
 log_sha256:sha(Buffer.from(log)),trace_sha256:fs.existsSync(names.trace)?sha(fs.readFileSync(names.trace)):'',
 claims_excluded:['Native Rust/Core action execution in this run','ArkUI rendering/event delivery','OHOS lifecycle event delivery','SDK build',
 'HAP/device validation','process crash/restart durability','full Windows/HMOS parity']};
result.ok=run.status===0&&!run.error&&totals.tests===expectedTests&&totals.pass===totals.tests&&totals.fail===0&&totals.cancelled===0&&totals.skipped===0&&totals.todo===0&&
 result.all_inputs_exact&&result.runtime_exact&&modelListUnchanged&&!missingReads.length&&!unqualifiedReads.length&&!readDrift.length;
fs.writeFileSync(names.after,JSON.stringify({after,runtimeAfter,actualReads},null,2)+'\n',{flag:'wx'});
fs.writeFileSync(names.result,JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(result));if(!result.ok)process.exitCode=1;
