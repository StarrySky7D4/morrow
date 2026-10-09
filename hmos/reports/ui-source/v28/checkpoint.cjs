'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const report=__dirname,root=path.resolve(report,'../../..'),repo=path.dirname(root);
const baseline='e4891f89788339773f6b52f926da7215fe255afa';
const project=path.join(root,'.build/device-candidates/dev28-todo-retry-final');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
const git=args=>cp.execFileSync('git',['-C',repo,...args],{maxBuffer:64*1024*1024});
const record=(name,base=repo)=>{const b=fs.readFileSync(path.join(base,name));return {path:name,bytes:b.length,sha256:sha(b)};};
const verify=(items,base=repo)=>{assert.equal(new Set(items.map(x=>x.path)).size,items.length);for(const item of items)assert.deepEqual(record(item.path,base),item,'input drift '+item.path);};
const read=name=>JSON.parse(fs.readFileSync(path.join(report,name)));
const write=(name,value,fresh=true)=>fs.writeFileSync(path.join(report,name),JSON.stringify(value,null,2)+'\n',fresh?{flag:'wx'}:undefined);
const scope='v28 explicit pure Todo formatter/count retry with exact failed input identity; host/model and unsigned API26 build only. Native archives reused byte-for-byte from qualified v27, no new Rust test/build. v28 device acceptance NOT_RUN; separate device observations cover installed v27 only. Full Flutter/Windows parity OPEN.';
if(process.argv[2]==='prepare'){
 assert.equal(git(['rev-parse','HEAD']).toString().trim(),baseline);assert.ok(!fs.existsSync(project),'fresh isolated project required');
 const priorNative=JSON.parse(git(['show',baseline+':hmos/reports/ui-source/v27/native-build-inputs.json']));verify(priorNative.repository_source_inventory);
 for(const a of priorNative.archives){assert.deepEqual(record(a.path,root),{path:a.path,bytes:a.bytes,sha256:a.sha256});assert.equal(record(a.production_path,root).sha256,a.sha256);}
 const native={capturedUtc:new Date().toISOString(),baseline,scope,qualification:'REUSED_V27_NOT_NEW_BUILD',previousEvidence:'hmos/reports/ui-source/v27/native-build-inputs.json',repository_source_inventory:priorNative.repository_source_inventory,archives:priorNative.archives};write('native-reuse-inputs-final.json',native);
 const old=JSON.parse(git(['show',baseline+':hmos/reports/build-manifest.json']));
 const productFile=x=>['hmos/AppScope/','hmos/entry/','hmos/hvigor/'].some(p=>x.startsWith(p))||['hmos/build-profile.json5','hmos/code-linter.json5','hmos/hvigorfile.ts','hmos/oh-package.json5','hmos/oh-package-lock.json5','hmos/scripts/build-hap.ps1'].includes(x);
 const productNames=git(['ls-files','--','hmos']).toString().trim().split('\n').filter(productFile);
 const names=[...new Set([...old.inputs.map(x=>'hmos/'+x.path),...productNames,...native.repository_source_inventory.map(x=>x.path),'hmos/reports/ui-source/v28/checkpoint.cjs','hmos/reports/ui-source/v28/verify-package.ps1','hmos/reports/ui-source/v28/run-sdk.cjs'])].filter(x=>!x.startsWith('hmos/tool/')).sort();
 const snapshot={capturedUtc:new Date().toISOString(),baseline,scope,inputs:names.map(x=>record(x))};write('build-inputs-final.json',snapshot);
 const inputs=[];function copy(relative,bytes,provenance){const target=path.join(project,relative);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes,{flag:'wx'});inputs.push({path:relative,bytes:bytes.length,sha256:sha(bytes),provenance});}
 for(const name of names.filter(productFile))copy(name.slice(5),fs.readFileSync(path.join(repo,name)),{kind:'frozen current repository',source:name});
 const dependencies=path.join(root,'oh_modules');function dependency(relative,parents=new Set()){
  const source=path.join(dependencies,relative),stat=fs.statSync(source),real=fs.realpathSync(source);
  if(stat.isDirectory()){assert.ok(!parents.has(real));const next=new Set(parents);next.add(real);for(const n of fs.readdirSync(source).sort())if(!['node_modules','build','.hvigor'].includes(n))dependency(path.join(relative,n),next);}
  else {assert.ok(stat.isFile());copy('oh_modules/'+relative.replaceAll('\\','/'),fs.readFileSync(source),{kind:'installed SDK dependency'});}
 }dependency('');
 for(const input of [...inputs].filter(x=>x.path.startsWith('entry/src/main/cpp/types/libmorrow/')))copy('entry/oh_modules/libmorrow.so/'+input.path.slice('entry/src/main/cpp/types/libmorrow/'.length),fs.readFileSync(path.join(project,input.path)),{kind:'local type dependency',source:input.path});
 for(const a of native.archives)copy(a.production_path,fs.readFileSync(path.join(root,a.path)),{kind:'reused frozen v27 native archive',...a});
 verify(inputs.map(({path,bytes,sha256})=>({path,bytes,sha256})),project);verify(snapshot.inputs);verify(native.repository_source_inventory);
 write('source-copy-manifest-final.json',{capturedUtc:new Date().toISOString(),baseline,scope,project,inputs});console.log(JSON.stringify({status:'PASS',phase:'prepare',inputs:inputs.length,repositoryInputs:names.length,project,native:'REUSED_V27'}));
}else if(process.argv[2]==='finalize'){
 const snapshot=read('build-inputs-final.json'),native=read('native-reuse-inputs-final.json'),copied=read('source-copy-manifest-final.json');verify(snapshot.inputs);verify(native.repository_source_inventory);verify(copied.inputs.map(({path,bytes,sha256})=>({path,bytes,sha256})),project);
 for(const a of native.archives){assert.deepEqual(record(a.path,root),{path:a.path,bytes:a.bytes,sha256:a.sha256});assert.equal(record(a.production_path,root).sha256,a.sha256);}
 const archive='.build/artifacts/dev28-todo-retry-final/entry-default-unsigned.hap';fs.mkdirSync(path.dirname(path.join(root,archive)),{recursive:true});fs.copyFileSync(path.join(project,'entry/build/default/outputs/default/entry-default-unsigned.hap'),path.join(root,archive),fs.constants.COPYFILE_EXCL);
 const artifact={...record(archive,root),archive_path:archive,signed:false,installed:false};const manifest=JSON.parse(git(['show',baseline+':hmos/reports/build-manifest.json']));manifest.createdUtc=new Date().toISOString();manifest.artifact=artifact;manifest.inputs=snapshot.inputs.map(x=>({path:x.path.slice(5),sha256:x.sha256}));manifest.uiValidation='reports/ui-source/v28/validation.md';manifest.deliveryScope=scope;manifest.nativeInputs='reports/ui-source/v28/native-reuse-inputs-final.json';manifest.nativeArchives=native.archives;
 fs.writeFileSync(path.join(root,'reports/build-manifest.json'),JSON.stringify(manifest,null,2)+'\n');write('artifact-final.json',{createdUtc:manifest.createdUtc,...artifact,scope});snapshot.finishedUtc=manifest.createdUtc;snapshot.sourceIdentityVerified=true;snapshot.artifact=artifact;write('build-inputs-final.json',snapshot,false);console.log(JSON.stringify({status:'PASS',phase:'finalize',artifact}));
}else throw new Error('prepare or finalize required');
