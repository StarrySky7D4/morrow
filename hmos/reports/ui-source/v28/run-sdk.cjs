'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const report=__dirname,repo=path.resolve(report,'../../../..'),project=path.join(repo,'hmos/.build/device-candidates/dev28-todo-retry-final');
const startedUtc=new Date().toISOString();
const r=cp.spawnSync('pwsh',['-NoLogo','-NoProfile','-File',path.join(project,'scripts/build-hap.ps1')],{cwd:project,encoding:'utf8',maxBuffer:64*1024*1024});
const log=(r.stdout||'')+(r.stderr||'');fs.writeFileSync(path.join(report,'sdk-final-build.log'),log,{flag:'wx'});
const proof={startedUtc,finishedUtc:new Date().toISOString(),exitCode:r.status,signal:r.signal,status:r.status===0&&/BUILD SUCCESSFUL/.test(log)?'PASS':'FAILED_OR_UNKNOWN',project,log:'sdk-final-build.log',sha256:crypto.createHash('sha256').update(log).digest('hex').toUpperCase(),scope:'Complete isolated API26 unsigned debug product build; reused unchanged v27 native archives; not installed or device acceptance.'};
fs.writeFileSync(path.join(report,'sdk-final-result.json'),JSON.stringify(proof,null,2)+'\n',{flag:'wx'});console.log(log.slice(-2200));console.log(JSON.stringify(proof));assert.equal(proof.status,'PASS');
