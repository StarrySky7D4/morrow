'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const report=__dirname,root=path.resolve(report,'../../../..'),repo=path.dirname(root);
const baseline='f9c395a7cc7b1d3b0707b52f241565b758114614';
const variant=process.argv[3]||'dev22-lease';assert.ok(['dev22-lease','dev22-lease-final'].includes(variant));
const final=variant==='dev22-lease-final',project=path.join(root,'.build/device-candidates',variant);
const manifestFile=path.join(report,final?'final-source-copy-manifest.json':'source-copy-manifest.json');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const git=args=>cp.execFileSync('git',['-C',repo,...args],{maxBuffer:64*1024*1024});
function verify(manifest){for(const item of manifest.inputs){const bytes=fs.readFileSync(path.join(project,item.path));assert.equal(bytes.length,item.bytes);assert.equal(sha(bytes),item.sha256,'candidate drift '+item.path);}}
if(process.argv[2]==='prepare'){
 assert.ok(!fs.existsSync(project));assert.ok(!fs.existsSync(manifestFile));fs.mkdirSync(project,{recursive:true});
 const names=git(['ls-tree','-r','--name-only',baseline,'--','hmos']).toString('utf8').trim().split('\n').filter(p=>
  ['hmos/AppScope/','hmos/entry/','hmos/hvigor/'].some(prefix=>p.startsWith(prefix))||
  ['hmos/build-profile.json5','hmos/code-linter.json5','hmos/hvigorfile.ts','hmos/oh-package.json5','hmos/oh-package-lock.json5','hmos/scripts/build-hap.ps1'].includes(p));
 const inputs=[];
 function write(relative,bytes,provenance){const target=path.join(project,relative);fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes,{flag:'wx'});inputs.push({path:relative,bytes:bytes.length,sha256:sha(bytes),provenance});}
 const override=final?['entry/src/main/ets/pages/EditorViewLease.ets','entry/src/main/ets/pages/Index.ets']:['entry/src/main/ets/pages/EditorViewLease.ets'];
 for(const name of names){const relative=name.slice('hmos/'.length),original=git(['show',baseline+':'+name]);
  if(override.includes(relative)){const bytes=fs.readFileSync(path.join(root,relative));write(relative,bytes,{kind:'explicit current repair',baseline_sha256:sha(original)});}
  else write(relative,original,{kind:'frozen Git blob',commit:baseline,path:name});
 }
 const dependencies=path.join(root,'oh_modules');
 function dependency(relative,ancestors=new Set()){
  const source=path.join(dependencies,relative),stat=fs.statSync(source),real=fs.realpathSync(source);
  if(stat.isDirectory()){assert.ok(!ancestors.has(real));const next=new Set(ancestors);next.add(real);for(const name of fs.readdirSync(source).sort())if(!['node_modules','build','.hvigor'].includes(name))dependency(path.join(relative,name),next);}
  else{assert.ok(stat.isFile());write('oh_modules/'+relative.replaceAll('\\','/'),fs.readFileSync(source),{kind:'installed SDK project dependency',source});}
 }
 dependency('');
 if(final){for(const item of [...inputs].filter(item=>item.path.startsWith('entry/src/main/cpp/types/libmorrow/'))){
  const relative='entry/oh_modules/libmorrow.so/'+item.path.slice('entry/src/main/cpp/types/libmorrow/'.length);
  write(relative,fs.readFileSync(path.join(project,item.path)),{kind:'entry dependency resolution from frozen Git types',source:item.path,commit:baseline});
 }}
 const native=JSON.parse(fs.readFileSync(path.join(root,'reports/ui-source/v20/retirement/native-build-inputs.json')));
 for(const input of native.repository_source_inventory)assert.equal(sha(git(['show',baseline+':'+input.path])),input.sha256,'historical native source mismatch');
 for(const archive of native.archives){const bytes=fs.readFileSync(path.join(root,archive.path));assert.equal(sha(bytes),archive.sha256);write(archive.production_path,bytes,{kind:'verified immutable b486 native archive',...archive});}
 const manifest={createdUtc:new Date().toISOString(),baseline,project,scope:'Actual product UI candidate: frozen f9 sources plus explicit ViewLease render and restore-state repairs; unchanged b486 native. Parallel new backend sources are excluded.',override,inputs,native_source_manifest:'reports/ui-source/v20/retirement/native-build-inputs.json',historical_native_sources:native.repository_source_inventory.length};
 verify(manifest);fs.writeFileSync(manifestFile,JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({phase:'prepare',project,inputs:inputs.length,baseline,nativeSources:manifest.historical_native_sources}));
}else if(process.argv[2]==='resolve-entry-dependency'){
 const manifest=JSON.parse(fs.readFileSync(manifestFile));verify(manifest);
 fs.copyFileSync(manifestFile,path.join(report,'source-copy-manifest-initial.json'),fs.constants.COPYFILE_EXCL);
 assert.ok(!fs.existsSync(path.join(project,'entry/oh_modules')));
 for(const item of [...manifest.inputs].filter(item=>item.path.startsWith('entry/src/main/cpp/types/libmorrow/'))){
  const relative='entry/oh_modules/libmorrow.so/'+item.path.slice('entry/src/main/cpp/types/libmorrow/'.length),target=path.join(project,relative);
  const bytes=fs.readFileSync(path.join(project,item.path));fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes,{flag:'wx'});
  manifest.inputs.push({path:relative,bytes:bytes.length,sha256:sha(bytes),provenance:{kind:'entry dependency resolution from frozen Git types',source:item.path,commit:baseline}});
 }
 manifest.entryDependencyResolvedUtc=new Date().toISOString();verify(manifest);fs.writeFileSync(manifestFile,JSON.stringify(manifest,null,2)+'\n');
 console.log(JSON.stringify({phase:'resolve-entry-dependency',inputs:manifest.inputs.length}));
}else if(process.argv[2]==='verify'){
 const manifest=JSON.parse(fs.readFileSync(manifestFile));verify(manifest);console.log(JSON.stringify({phase:'verify',status:'PASS',inputs:manifest.inputs.length,project}));
}else if(process.argv[2]==='archive'){
 const manifest=JSON.parse(fs.readFileSync(manifestFile));verify(manifest);
 const relative='entry/build/default/outputs/default/entry-default-unsigned.hap',artifact=path.join(project,relative),bytes=fs.readFileSync(artifact);
 const archive=path.join(root,'.build/artifacts/'+variant+'-checkpoint/entry-default-unsigned.hap');fs.mkdirSync(path.dirname(archive),{recursive:true});fs.copyFileSync(artifact,archive,fs.constants.COPYFILE_EXCL);
 const record={createdUtc:new Date().toISOString(),baseline,project,inputs:manifest.inputs.length,artifact,archive,bytes:bytes.length,sha256:sha(bytes),signed:false,installed:false,scope:manifest.scope};
 fs.writeFileSync(path.join(report,final?'final-artifact.json':'artifact.json'),JSON.stringify(record,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(record));
}else throw Error('prepare/verify/archive required');
