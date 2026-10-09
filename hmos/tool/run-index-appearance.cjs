'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process'),assert=require('node:assert/strict');
const {verify,baselinePath}=require('./index-appearance-preservation.cjs'),root=path.resolve(__dirname,'..'),repo=path.resolve(root,'..');
const label=process.argv[2]||'initial';assert.match(label,/^[A-Za-z0-9_-]+$/);const output=path.join(root,'reports/ui-source/v33');fs.mkdirSync(output,{recursive:true});
const base=path.join(output,'index-appearance-'+label);for(const tail of ['-inputs-before.json','-inputs-after.json','-result.json','-tests.log','-task-preservation.json'])
  assert.ok(!fs.existsSync(base+tail),'preserve report '+base+tail);
const tsRoot=process.env.HMOS_TYPESCRIPT||'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript';
const inputs=[__filename,path.join(__dirname,'index-appearance-test-harness.cjs'),path.join(__dirname,'index-appearance.test.cjs'),path.join(__dirname,'index-appearance-preservation.cjs'),
  path.join(root,'entry/src/main/ets/pages/Index.ets'),...['Appearance','AppearancePreferences','UiStrings'].map(name=>path.join(root,'entry/src/main/ets/model',name+'.ets')),
  path.join(tsRoot,'package.json'),require.resolve(tsRoot)];
const references=[path.join(repo,'build/win-cloud-20261005/lib/main.dart'),path.join(repo,'build/win-cloud-20261005/lib/storage.dart'),
  path.join(root,'shared/plugins/workbench/src/preferences.rs'),'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/api/@ohos.data.preferences.d.ts',baselinePath];
const snap=files=>files.map(file=>{const data=fs.readFileSync(file);return{path:path.relative(repo,file).replaceAll('\\','/'),byte_length:data.length,
  sha256:crypto.createHash('sha256').update(data).digest('hex')};});
const before={observed_at:new Date().toISOString(),inputs:snap(inputs),references:snap(references)};fs.writeFileSync(base+'-inputs-before.json',JSON.stringify(before,null,2)+'\n');
const run=cp.spawnSync(process.execPath,['--test',path.join(__dirname,'index-appearance.test.cjs')],{encoding:'utf8',maxBuffer:8*1024*1024});
const log=(run.stdout||'')+(run.stderr||'');fs.writeFileSync(base+'-tests.log',log);
const after={observed_at:new Date().toISOString(),inputs:snap(inputs),references:snap(references)};fs.writeFileSync(base+'-inputs-after.json',JSON.stringify(after,null,2)+'\n');
const exact=JSON.stringify(before.inputs)===JSON.stringify(after.inputs)&&JSON.stringify(before.references)===JSON.stringify(after.references),
  count=re=>Number(log.match(re)?.[1]||0),tests=count(/(?:ℹ|#) tests (\d+)/),passed=count(/(?:ℹ|#) pass (\d+)/),failed=count(/(?:ℹ|#) fail (\d+)/),
  skipped=count(/(?:ℹ|#) skipped (\d+)/),cancelled=count(/(?:ℹ|#) cancelled (\d+)/);
let preservation;try{preservation=verify();}catch(error){preservation={qualification:'FAILED',error:error.message};}
fs.writeFileSync(base+'-task-preservation.json',JSON.stringify(preservation,null,2)+'\n');
const summary={qualification:'PASS_SCOPED_ACTUAL_INDEX_APPEARANCE_INTEGRATION',observed_at:after.observed_at,tests,passed,failed,skipped,cancelled,exit_code:run.status,
  source_exact_before_after:exact,input_count:inputs.length,reference_count:references.length,duration_ms:Number(log.match(/(?:ℹ|#) duration_ms ([\d.]+)/)?.[1]||0),
  node_version:process.version,typescript_version:JSON.parse(fs.readFileSync(path.join(tsRoot,'package.json'),'utf8')).version,
  log_sha256:crypto.createHash('sha256').update(log).digest('hex'),inputs:after.inputs,references:after.references,task_preservation:preservation,
  sdk_build:'NOT_RUN',device_persistence:'NOT_RUN',arkui_pixels:'NOT_RUN',
  assertions:['actual namespace has/get distinction and all failed reads block put','actual full UI-to-model snapshot and ownership',
    'serial writes, durable ACK/readback and latest UI candidate','real rendered status/button guards and callbacks',
    'explicit recovery without candidate replay or default overwrite','owner/background/closed-page stale-response protection',
    'fresh foreground waits actual operation termination for status only','shared namespace live barrier across actual Index page replacement',
    'terminal Unknown transfer and explicit recovery before separate preview save'],
  controlled_seams:['ArkData namespace/cache/has/get/put/flush','unrelated lifecycle sibling controllers/startup/native preview','font icon registration','ArkUI attributes/color methods'],
  limitations:['Actual current Index fields/methods and pure ETS models execute; platform service storage and rendered pixels are controlled.',
    'The new same-namespace barrier is in-process only, not cross-process CAS or crash/restart journal qualification.',
    'Task-preservation proof uses the exact locally captured task-agent baseline; that ignored baseline is required only by this qualification runner.']};
if(run.status!==0||tests!==32||passed!==32||failed!==0||skipped!==0||cancelled!==0||!exact||preservation.qualification.startsWith('FAILED'))summary.qualification='FAILED';
fs.writeFileSync(base+'-result.json',JSON.stringify(summary,null,2)+'\n');console.log(JSON.stringify(summary,null,2));if(summary.qualification==='FAILED')process.exitCode=1;
