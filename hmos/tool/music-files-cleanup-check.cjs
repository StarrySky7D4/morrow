'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process');
const root=path.resolve(__dirname,'../..'),report=path.join(root,'hmos/reports/ui-source/v30');
const label=process.argv[2]||'music-files-cleanup';
if(!/^[A-Za-z0-9_-]{1,80}$/.test(label))throw new Error('Invalid evidence label');
const suite='hmos/tool/music-files-model.test.cjs';
const inputs=['hmos/entry/src/main/ets/model/MusicFiles.ets','hmos/entry/src/main/ets/model/MusicPlayback.ets',suite,
  'hmos/tool/music-files-cleanup-check.cjs'];
const tsRoot='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript';
const runtime=[process.execPath,require.resolve(tsRoot),path.join(tsRoot,'package.json')];
const hash=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
function inventory(files,local){return files.map(p=>{const absolute=local?path.join(root,p):p,stat=fs.lstatSync(absolute);
  if(!stat.isFile()||stat.isSymbolicLink())throw new Error('Input is not a regular file: '+p);
  const bytes=fs.readFileSync(absolute);return {path:local?p:absolute,bytes:bytes.length,sha256:hash(bytes)};});}
fs.mkdirSync(report,{recursive:true});const names={log:label+'-tests.log',inputs:label+'-inputs.json',result:label+'-result.json'};
for(const p of Object.values(names))if(fs.existsSync(path.join(report,p)))throw new Error('Existing evidence retained; use a new label: '+p);
const startedUtc=new Date().toISOString(),before=inventory(inputs,true),runtimeBefore=inventory(runtime,false);
const execution=cp.spawnSync(process.execPath,['--test','--test-reporter=tap',suite],{cwd:root,encoding:'utf8',maxBuffer:4*1024*1024});
const log=(execution.stdout||'')+(execution.stderr||''),after=inventory(inputs,true),runtimeAfter=inventory(runtime,false);
const count=name=>{const m=log.match(new RegExp('^# '+name+' (\\d+)$','m'));return m?Number(m[1]):null;};
const same=JSON.stringify(before)===JSON.stringify(after)&&JSON.stringify(runtimeBefore)===JSON.stringify(runtimeAfter);
const result={startedUtc,finishedUtc:new Date().toISOString(),exitCode:execution.status,signal:execution.signal||'',
  error:execution.error?execution.error.message:'',tests:count('tests'),pass:count('pass'),fail:count('fail'),cancelled:count('cancelled'),skipped:count('skipped'),todo:count('todo'),
  durationMs:Number(log.match(/^# duration_ms ([\d.]+)$/m)?.[1]||0),suiteFiles:1,actualSourceInputs:inputs.length,sourceIdentityVerified:same,
  nodeVersion:process.version,typescriptVersion:JSON.parse(fs.readFileSync(path.join(tsRoot,'package.json'),'utf8')).version,
  log:names.log,logSha256:hash(Buffer.from(log,'utf8')),inputs:names.inputs,
  scope:'Actual MusicFiles/MusicPlayback source with controlled host fs/picker/native prepare/export/import seams. Original-object partial cleanup only. Durable restart cleanup, Native Ready qualification, SDK, HAP, codecs, UI and device are not qualified.'};
result.qualified=execution.status===0&&same&&result.tests>0&&result.tests===result.pass&&result.fail===0&&result.cancelled===0&&result.skipped===0&&result.todo===0;
fs.writeFileSync(path.join(report,names.log),log,'utf8');fs.writeFileSync(path.join(report,names.inputs),JSON.stringify({before,after,runtimeBefore,runtimeAfter},null,2)+'\n','utf8');
fs.writeFileSync(path.join(report,names.result),JSON.stringify(result,null,2)+'\n','utf8');process.stdout.write(JSON.stringify(result,null,2)+'\n');
if(!result.qualified)process.exitCode=1;
