'use strict';
// Fresh bounded composed host tests, with actual read bytes and exact post-run
// checks. SDK and runtime qualification remain separate owned root operations.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process');
const root=path.resolve(__dirname,'../../../..'),label=process.argv[2]||'final';
if(!/^[a-z0-9-]+$/.test(label))throw Error('Invalid immutable run label');
const prefix=path.join(__dirname,'music-workbench-'+label),trace=prefix+'-trace.jsonl',log=prefix+'-tests.log',result=prefix+'-result.json',inventory=prefix+'-inputs.json';
for(const p of [trace,log,result,inventory])if(fs.existsSync(p))throw Error('Run label already exists: '+label);
const tests=['hmos/tool/music-workbench-model.test.cjs','hmos/tool/music-library-model.test.cjs','hmos/tool/music-library-native-dto.test.cjs'];
const tracer=path.join(root,'hmos/tool/music-workbench-source-trace.cjs');
const start=new Date(),run=cp.spawnSync(process.execPath,['--require',tracer,'--test',...tests],{cwd:root,encoding:'utf8',env:{...process.env,MUSIC_WORKBENCH_TRACE:trace},maxBuffer:8*1024*1024});
const output=(run.stdout||'')+(run.stderr||'');fs.writeFileSync(log,output);
const observed=new Map();for(const line of fs.readFileSync(trace,'utf8').trim().split('\n'))for(const item of JSON.parse(line).files){const old=observed.get(item.path);if(old&&(old.bytes!==item.bytes||old.sha256!==item.sha256))throw Error('Cross-process input drift: '+item.path);observed.set(item.path,item);}
const inputs=[...observed.values()].sort((a,b)=>a.path.localeCompare(b.path)).map(item=>{const bytes=fs.readFileSync(path.join(root,item.path)),sha256=crypto.createHash('sha256').update(bytes).digest('hex');return {...item,after_bytes:bytes.length,after_sha256:sha256,exact:bytes.length===item.bytes&&sha256===item.sha256};});
const number=key=>Number(output.match(new RegExp('(?:ℹ |# )'+key+' (\\d+(?:\\.\\d+)?)'))?.[1]||0);
const summary={schema_version:1,scope:'ACTUAL_COMPOSED_HOST_MODELS_AND_STORED_NATIVE_DTO_ONLY',started_at:start.toISOString(),finished_at:new Date().toISOString(),
 node:process.execPath,node_version:process.version,tests,exit_code:run.status,signal:run.signal,passed:number('pass'),failed:number('fail'),count:number('tests'),duration_ms:number('duration_ms'),
 actual_inputs:inputs.length,all_inputs_exact:inputs.every(x=>x.exact),log_sha256:crypto.createHash('sha256').update(fs.readFileSync(log)).digest('hex'),
 claims_excluded:['new native Store execution','new Rust policy execution','SDK build','OHOS picker/FD provider','AVPlayer codec or real playback','device screenshots','full Windows/HMOS parity']};
summary.ok=run.status===0&&summary.passed===summary.count&&summary.count>=64&&summary.failed===0&&summary.all_inputs_exact;
fs.writeFileSync(inventory,JSON.stringify(inputs,null,2)+'\n');fs.writeFileSync(result,JSON.stringify(summary,null,2)+'\n');console.log(JSON.stringify(summary));if(!summary.ok)process.exitCode=1;
