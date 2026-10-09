'use strict';
// Immutable actual-source composed host evidence. Files/SDK/provider/Store and
// AVPlayer seams stay controlled; this runner is not device/crash qualification.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process');
const root=path.resolve(__dirname,'../../../..'),label=process.argv[2]||'final';
if(!/^[a-z0-9-]+$/.test(label))throw Error('Invalid immutable evidence label');
const prefix=path.join(__dirname,'music-import-durability-'+label),names={log:prefix+'-tests.log',trace:prefix+'-trace.jsonl',before:prefix+'-inputs-before.json',inputs:prefix+'-inputs.json',result:prefix+'-result.json'};
for(const p of Object.values(names))if(fs.existsSync(p))throw Error('Existing run retained: '+label);
const tests=['hmos/tool/music-files-model.test.cjs','hmos/tool/music-library-model.test.cjs','hmos/tool/music-library-native-dto.test.cjs','hmos/tool/music-workbench-model.test.cjs'];
const local=[...new Set([
 'hmos/entry/src/main/ets/model/MusicFiles.ets','hmos/entry/src/main/ets/model/MusicLibrary.ets','hmos/entry/src/main/ets/model/MusicWorkbench.ets',
 'hmos/entry/src/main/ets/model/MusicUi.ets','hmos/entry/src/main/ets/model/MusicPlayback.ets','hmos/entry/src/main/ets/pages/PlatformMusicPlayer.ets',
 'hmos/entry/src/main/ets/model/Workbench.ets','hmos/entry/src/main/ets/model/EditorInputHash.ets','hmos/entry/src/main/ets/model/EditorFieldPolicy.ets',
 'hmos/entry/src/main/ets/model/EditorDraft.ets','hmos/tool/editor-field-test-harness.cjs','hmos/tool/music-library-test-harness.cjs','hmos/tool/music-workbench-test-harness.cjs',
 'hmos/tool/music-workbench-source-trace.cjs','hmos/reports/ui-source/v29/music-store-fixture-stage1.json','hmos/reports/ui-source/v29/music-store-fixture.json',
 'hmos/reports/ui-source/v31/run-music-import-durability.cjs',...tests])].sort();
const hash=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
function inventory(files,relative){return files.map(p=>{const actual=relative?path.join(root,p):p,stat=fs.lstatSync(actual);if(!stat.isFile()||stat.isSymbolicLink())throw Error('Unsafe input: '+p);const b=fs.readFileSync(actual);return {path:p,bytes:b.length,sha256:hash(b)};});}
const tsRoot=process.env.HMOS_TYPESCRIPT_PATH||'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript';
const runtime=[process.execPath,require.resolve(tsRoot),path.join(tsRoot,'package.json')],before=inventory(local,true),runtimeBefore=inventory(runtime,false),started=new Date();
fs.writeFileSync(names.before,JSON.stringify({before,runtimeBefore},null,2)+'\n');
const run=cp.spawnSync(process.execPath,['--require',path.join(root,'hmos/tool/music-workbench-source-trace.cjs'),'--test','--test-reporter=tap',...tests],
 {cwd:root,encoding:'utf8',maxBuffer:10*1024*1024,env:{...process.env,MUSIC_WORKBENCH_TRACE:names.trace}});
const log=(run.stdout||'')+(run.stderr||'');fs.writeFileSync(names.log,log);
const after=inventory(local,true),runtimeAfter=inventory(runtime,false),observed=new Map();
for(const line of fs.readFileSync(names.trace,'utf8').trim().split('\n'))for(const item of JSON.parse(line).files){const old=observed.get(item.path);if(old&&(old.bytes!==item.bytes||old.sha256!==item.sha256.toUpperCase()))throw Error('Input drift during reads: '+item.path);observed.set(item.path,{...item,sha256:item.sha256.toUpperCase()});}
const actualReads=[...observed.values()].sort((a,b)=>a.path.localeCompare(b.path)),expected=new Map(before.map(x=>[x.path,x]));
const unqualifiedReads=actualReads.filter(item=>!expected.has(item.path)||JSON.stringify(expected.get(item.path))!==JSON.stringify(item));
const required=local.filter(p=>p!=='hmos/reports/ui-source/v31/run-music-import-durability.cjs'),missingReads=required.filter(p=>!observed.has(p));
const number=key=>Number(log.match(new RegExp('^# '+key+' ([0-9.]+)$','m'))?.[1]||0);
const result={schema_version:1,scope:'ACTUAL_SOURCE_COMPOSED_CONTROLLED_HOST_IMPORT_AND_PICKER_BOUNDARIES_ONLY',started_at:started.toISOString(),finished_at:new Date().toISOString(),
 base_commit:'4c04f97e6beb580d33f14f9540db98129fc50180',node:process.execPath,node_version:process.version,typescript_version:JSON.parse(fs.readFileSync(path.join(tsRoot,'package.json'),'utf8')).version,
 tests,exit_code:run.status,signal:run.signal||'',error:run.error?run.error.message:'',count:number('tests'),passed:number('pass'),failed:number('fail'),cancelled:number('cancelled'),skipped:number('skipped'),todo:number('todo'),duration_ms:number('duration_ms'),
 repository_inputs:before.length,actual_repository_reads:actualReads.length,all_inputs_exact:JSON.stringify(before)===JSON.stringify(after),runtime_exact:JSON.stringify(runtimeBefore)===JSON.stringify(runtimeAfter),missing_reads:missingReads,unqualified_reads:unqualifiedReads,
 log:path.basename(names.log),log_sha256:hash(Buffer.from(log)),trace_sha256:hash(fs.readFileSync(names.trace)),
 claims_excluded:['actual process crash/reboot durability','Native Store action execution in this run','Native FD fsync implementation','OHOS DocumentViewPicker lifecycle/grants','actual AVPlayer/audio decoder playback','SDK build','HAP install/device rendering','full Windows/HMOS parity']};
result.ok=run.status===0&&result.count===result.passed&&result.count>=114&&result.failed===0&&result.cancelled===0&&result.skipped===0&&result.todo===0&&result.all_inputs_exact&&result.runtime_exact&&!missingReads.length&&!unqualifiedReads.length;
fs.writeFileSync(names.inputs,JSON.stringify({before,after,runtimeBefore,runtimeAfter,actualReads},null,2)+'\n');fs.writeFileSync(names.result,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));if(!result.ok)process.exitCode=1;
