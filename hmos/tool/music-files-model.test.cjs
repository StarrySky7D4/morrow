/* Actual MusicFiles ETS with controlled picker/FD/native transport seams.
 * These host checks do not qualify OHOS providers, codecs or music Store. */
'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict'),fsHost=require('node:fs'),path=require('node:path'),vm=require('node:vm'),crypto=require('node:crypto');
const ts=require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const sourceName=path.resolve(__dirname,'../entry/src/main/ets/model/MusicFiles.ets');
const source=fsHost.readFileSync(sourceName,'utf8'),compiled=ts.transpileModule(source,{fileName:sourceName,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}});
assert.equal((compiled.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
function fixture(){
 const nodes=new Map(),fds=new Map(),trace=[],faults=new Map();let fd=10,folder=0,uris=['content://test/audio'],exportError=false;
 const put=(p,b,name)=>nodes.set(p,{kind:'file',bytes:Buffer.from(b),name});
 for(const p of ['/app','/app/files','/app/cache'])nodes.set(p,{kind:'directory'});
 const bytes=Buffer.from('RIFF controlled audio bytes');put('content://test/audio',bytes,'test.wav');
 const err=code=>Object.assign(new Error('controlled filesystem fault'),{code});
 const node=p=>{const n=nodes.get(p);if(!n)throw err(13900002);return n;};
 const stat=n=>({size:n.bytes?.length||0,isFile:()=>n.kind==='file',isDirectory:()=>n.kind==='directory',isSymbolicLink:()=>n.kind==='symlink'});
 const failure=(op,p)=>{if(faults.has(op+':'+p))throw faults.get(op+':'+p);};
 const mode={READ_ONLY:0,WRITE_ONLY:1,READ_WRITE:2,CREATE:64,TRUNC:512,NOFOLLOW:131072};
 const io={OpenMode:mode,
  async lstat(p){failure('lstat',p);return stat(node(p));},async stat(id){const p=fds.get(id).path;failure('stat',p);return stat(node(p));},
  async mkdir(p){nodes.set(p,{kind:'directory'});},async mkdtemp(template){const p=template.replace('XXXXXX',String(++folder).padStart(6,'0'));nodes.set(p,{kind:'directory'});return p;},
  async listFile(p){node(p);return [...nodes.keys()].filter(x=>path.posix.dirname(x)===p).map(x=>path.posix.basename(x));},
  async open(p,flags){failure('open',p);if((flags&mode.CREATE)&&!nodes.has(p))put(p,'');const n=node(p);if(n.kind!=='file')throw err(13900003);if(flags&mode.TRUNC)n.bytes=Buffer.alloc(0);const f={fd:fd++,name:n.name||path.posix.basename(p),path:p};fds.set(f.fd,f);trace.push(['open',p,f.fd]);return f;},
  async close(f){failure('close',f.path);assert.ok(fds.has(f.fd),'owned open FD');fds.delete(f.fd);trace.push(['close',f.path]);},
  async write(id,buffer){const f=fds.get(id);failure('write',f.path);const n=node(f.path),b=Buffer.from(buffer);n.bytes=Buffer.concat([n.bytes,b]);return b.length;},
  async fsync(id){const p=fds.get(id).path;failure('fsync',p);trace.push(['fsync',p]);},async readText(p){return node(p).bytes.toString();},
  async read(id,buffer,options){const n=node(fds.get(id).path),b=n.bytes.subarray(options.offset,options.offset+options.length);new Uint8Array(buffer).set(b);return b.length;},
  async unlink(p){failure('unlink',p);assert.equal(node(p).kind,'file');nodes.delete(p);trace.push(['unlink',p]);},
  async rmdir(p){failure('rmdir',p);assert.equal((await io.listFile(p)).length,0);nodes.delete(p);trace.push(['rmdir',p]);}
 };
 const fileReply=b=>JSON.stringify({ok:true,error:'',byte_length:String(b.length),sha256:sha(b)});
 const native={async prepareMusicFile(from,to,limit){trace.push(['prepare',limit]);const b=node(fds.get(from).path).bytes;if(b.length>limit)throw err(13900099);node(fds.get(to).path).bytes=Buffer.from(b);return fileReply(b);}};
 const workbench={async exportFd(wire,id){trace.push(['export',wire]);node(fds.get(id).path).bytes=Buffer.from(bytes);return exportError?fileReply(Buffer.from('wrong')):fileReply(bytes);}};
 const playbackExports={};const playbackSource=fsHost.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/model/MusicPlayback.ets'),'utf8');
 const playbackCompiled=ts.transpileModule(playbackSource,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}});
 vm.runInNewContext(playbackCompiled.outputText,{exports:playbackExports,setTimeout,clearTimeout});
 const exports={};vm.runInNewContext(compiled.outputText,{exports,require:name=>{
  if(name==='@kit.CoreFileKit')return {fileIo:io,picker:{DocumentSelectOptions:class {},DocumentSelectMode:{FILE:1},DocumentViewPicker:class {async select(){return uris;}}}};
  if(name==='@kit.ArkTS')return {util:{TextEncoder:class {encodeInto(v){return new Uint8Array(Buffer.from(v));}},TextDecoder:{create:(encoding,options)=>({decodeToString:input=>new TextDecoder(encoding,options).decode(input)})}}};
  if(name==='libmorrow.so')return {default:native};if(name==='./Workbench')return {workbench};if(name==='./MusicPlayback')return playbackExports;return {};
 }});
 const files=new exports.MusicFiles({filesDir:'/app/files',cacheDir:'/app/cache'});
 const identity=()=>Object.assign(new playbackExports.MusicPlaybackIdentity(),{trackId:'track-1',libraryRevision:'2',importOperation:'import-1',byteLength:bytes.length,sha256:sha(bytes)});
 const request=p=>JSON.stringify({action:'music_import',music:{action:'import_file',request:{schema_version:1,track_id:'track-1',operation_id:'import-1',expected_revision:'1',name:p.name,byte_length:p.byteLength,sha256:p.sha256}}});
 return {files,exports,nodes,fds,trace,faults,bytes,put,identity,request,select:v=>{uris=v;},exportFail:()=>{exportError=true;},newFiles:()=>new exports.MusicFiles({filesDir:'/app/files',cacheDir:'/app/cache'})};
}
async function prepare(f){await f.files.selectUris(1);return f.files.prepareUri('content://test/audio');}
test('music capture requires one selected grant; cancellation and unsupported extensions do not admit a spool',async()=>{
 const f=fixture();await assert.rejects(f.files.prepareUri('content://test/audio'),/不是/);
 f.select([]);assert.deepEqual(Array.from(await f.files.selectUris(1)),[]);f.files.discardSelection();
 f.put('content://test/audio',f.bytes,'bad.exe');f.select(['content://test/audio']);await f.files.selectUris(1);await assert.rejects(f.files.prepareUri('content://test/audio'),/标准音频/);assert.equal(f.fds.size,0);assert.equal((await f.files.recover()).length,0);
});
test('whole native capture receives music150MiB cap and actual spool metadata without a persistent provider URI',async()=>{
 const f=fixture(),p=await prepare(f);assert.equal(f.trace.find(x=>x[0]==='prepare')[1],150*1024*1024);assert.equal(p.byteLength,String(f.bytes.length));assert.equal(p.sha256,sha(f.bytes));assert.equal(p.requestJson,'');
 assert.equal(JSON.stringify(p).includes('content://'),false);assert.equal(f.fds.size,0);assert.equal(f.nodes.get('/app/files/hmos-music-spool/'+p.spoolId+'/data').bytes.compare(f.bytes),0);
});
test('explicit pre-begin persistence writes and closes exact wire without opening source or dispatching Native',async()=>{
 const f=fixture(),p=await prepare(f),wire=f.request(p),dir='/app/files/hmos-music-spool/'+p.spoolId;
 const from=f.trace.length;await f.files.persistImport(p,wire);assert.equal(p.requestJson,wire);assert.equal(f.fds.size,0);
 assert.equal(f.nodes.get(dir+'/request.json').bytes.toString(),wire);assert.ok(f.trace.slice(from).some(x=>x[0]==='fsync'&&x[1]===dir+'/request.json'));
 assert.equal(f.trace.slice(from).some(x=>x[0]==='open'&&x[1]===dir+'/data'),false);
 const metadata=f.nodes.get(dir+'/metadata.json').bytes.toString();assert.equal(JSON.parse(metadata).requestJson,'');
 const recovered=(await f.newFiles().recover())[0];assert.equal(recovered.requestJson,wire);
 await f.files.persistImport(p,wire);assert.equal(f.nodes.get(dir+'/request.json').bytes.toString(),wire);assert.equal(f.fds.size,0);
});
test('pre-begin fsync and close failures retain original bytes and only same original request can be acknowledged later',async()=>{
 for(const op of ['fsync','close']){
  const f=fixture(),p=await prepare(f),wire=f.request(p),dir='/app/files/hmos-music-spool/'+p.spoolId;
  f.faults.set(op+':'+dir+'/request.json',Error('RequestACKUnknown'));await assert.rejects(f.files.persistImport(p,wire),/RequestACKUnknown/);
  assert.equal(p.requestJson,'');assert.equal(f.nodes.get(dir+'/request.json').bytes.toString(),wire);assert.ok(f.nodes.has(dir+'/data'));
  f.faults.clear();await f.files.retryPendingIo();await assert.rejects(f.files.persistImport(p,wire.replace('import-1','import-2')),/不能更换/);
  await f.files.persistImport(p,wire);assert.equal(p.requestJson,wire);assert.equal(f.fds.size,0);
 }
});
test('disappeared or changed pre-begin request cannot be recreated or silently reused for FD dispatch',async()=>{
 for(const change of ['missing','changed']){
  const f=fixture(),p=await prepare(f),wire=f.request(p),requestPath='/app/files/hmos-music-spool/'+p.spoolId+'/request.json';await f.files.persistImport(p,wire);
  if(change==='missing')f.nodes.delete(requestPath);else f.put(requestPath,wire.replace('import-1','import-2'));
  let sends=0;await assert.rejects(f.files.importPrepared(p,wire,async()=>{sends++;return '';}),/缺失|不能更换/);assert.equal(sends,0);
  assert.equal(f.nodes.has(requestPath),change!=='missing');if(change==='changed')assert.equal(f.nodes.get(requestPath).bytes.toString(),wire.replace('import-1','import-2'));
 }
});
test('partial request write is preserved after restart and never appears as a complete new import source',async()=>{
 const f=fixture(),p=await prepare(f),wire=f.request(p),dir='/app/files/hmos-music-spool/'+p.spoolId;f.faults.set('write:'+dir+'/request.json',Error('WriteUnknown'));
 await assert.rejects(f.files.persistImport(p,wire),/WriteUnknown/);assert.ok(f.nodes.has(dir+'/request.json'));assert.equal(f.nodes.get(dir+'/request.json').bytes.length,0);
 const fresh=f.newFiles();assert.equal((await fresh.recover()).length,0);assert.ok(f.nodes.has(dir+'/data'));assert.ok(f.nodes.has(dir+'/request.json'));assert.match(fresh.diagnostics().join(' '),/已保留/);
});
test('exact music request is fsynced before dispatch and Unknown retains its original request and full bytes',async()=>{
 const f=fixture(),p=await prepare(f),wire=f.request(p),directory='/app/files/hmos-music-spool/'+p.spoolId;let calls=0;
 await assert.rejects(f.files.importPrepared(p,wire,async (sent,id)=>{calls++;assert.equal(sent,wire);assert.equal(f.nodes.get(directory+'/request.json').bytes.toString(),wire);assert.ok(f.trace.some(x=>x[0]==='fsync'&&x[1]===directory+'/request.json'));assert.ok(f.fds.has(id));throw Error('Unknown');}),/Unknown/);
 assert.equal(p.requestJson,wire);assert.equal(f.fds.size,0);assert.equal((await f.files.recover())[0],p);
 const changed=wire.replace('import-1','import-2');await assert.rejects(f.files.importPrepared(p,changed,async()=>{calls++;return '';}),/不能更换/);assert.equal(calls,1);
 const recovered=(await f.newFiles().recover())[0];assert.equal(recovered.requestJson,wire);assert.equal(recovered.sha256,sha(f.bytes));
});
test('forged mutable spool or malformed request cannot enter FD dispatch',async()=>{
 const f=fixture(),p=await prepare(f);p.sha256='0'.repeat(64);await assert.rejects(f.files.importPrepared(p,f.request(p),async()=>''),/身份/);
 const g=fixture(),q=await prepare(g),extra=JSON.parse(g.request(q));extra.music.request.extra='path';await assert.rejects(g.files.importPrepared(q,JSON.stringify(extra),async()=>''),/原请求/);assert.equal(q.requestJson,'');
});
test('unexpected or symlink cache is retained and blocks new admission rather than being automatically deleted',async()=>{
 const f=fixture(),p=await prepare(f),dir='/app/files/hmos-music-spool/'+p.spoolId;f.put(dir+'/unexpected','x');assert.equal((await f.newFiles().recover()).length,0);assert.ok(f.nodes.has(dir+'/data'));
 f.files.discardSelection();await assert.rejects(f.files.selectUris(1),/未知/);assert.ok(f.nodes.has(dir+'/unexpected'));
});
test('export mismatch cannot admit a player source and complete export returns no path or URI',async()=>{
 const bad=fixture();bad.exportFail();await assert.rejects(bad.files.acquire(bad.identity()),/完整来源/);assert.equal(bad.fds.size,0);
 const f=fixture(),s=await f.files.acquire(f.identity());assert.equal(f.files.ownsSource(s),true);assert.equal('path' in s,false);assert.equal('uri' in s,false);assert.equal(f.trace.find(x=>x[0]==='export')[1],JSON.stringify({action:'music_export',music:{action:'export',library_revision:'2',track_id:'track-1',import_operation:'import-1',byte_length:String(f.bytes.length),sha256:sha(f.bytes)}}));await f.files.releaseSource(s);
});
test('registered descriptor must close before source removal; failed close retains original resource for explicit retry',async()=>{
 const f=fixture(),s=await f.files.acquire(f.identity()),d=await f.files.open(s);assert.equal(f.files.ownsDescriptor(s,d),true);await assert.rejects(f.files.releaseSource(s),/尚未关闭/);
 const p=f.fds.get(d.fd).path;f.faults.set('close:'+p,Error('CloseUnknown'));await assert.rejects(f.files.close(d),/CloseUnknown/);assert.equal(f.files.ownsDescriptor(s,d),true);assert.equal(f.nodes.has(p),true);
 f.faults.delete('close:'+p);await f.files.close(d);await f.files.releaseSource(s);assert.equal(f.fds.size,0);assert.equal(f.files.ownsSource(s),false);
});
test('forged source and descriptor cannot open, close or delete the original registry-owned audio',async()=>{
 const f=fixture(),s=await f.files.acquire(f.identity());await assert.rejects(f.files.open({...s}),/来源/);const d=await f.files.open(s);await assert.rejects(f.files.close({...d}),/句柄/);assert.equal(f.files.ownsDescriptor(s,d),true);await f.files.close(d);s.sha256='0'.repeat(64);await assert.rejects(f.files.releaseSource(s),/身份/);
});
test('owned cache deletion failure retains the same token and does not affect other adapter namespaces',async()=>{
 const f=fixture(),s=await f.files.acquire(f.identity()),p='/app/cache/'+s.token+'/data';f.faults.set('unlink:'+p,Error('CleanupUnknown'));await assert.rejects(f.files.releaseSource(s),/CleanupUnknown/);assert.equal(f.files.ownsSource(s),true);f.faults.delete('unlink:'+p);await f.files.releaseSource(s);assert.equal(f.files.ownsSource(s),false);
});
test('export writer close rejection does not admit an unreachable active source; original handle remains for explicit cleanup',async()=>{
 const f=fixture(),p='/app/cache/morrow-music-000001/data';f.faults.set('close:'+p,Error('WriterCloseUnknown'));
 await assert.rejects(f.files.acquire(f.identity()),/WriterCloseUnknown/);assert.equal(f.files.leases.size,0);assert.equal(f.fds.size,1);assert.ok(f.nodes.has(p));
 await f.files.cleanupInactivePlaybackCache();assert.ok(f.nodes.has(p),'pending FD prevents deletion');f.faults.delete('close:'+p);await f.files.retryPendingIo();await f.files.cleanupInactivePlaybackCache();assert.equal(f.fds.size,0);assert.equal(f.nodes.has(p),false);
});
test('capture closes destination even when source close rejects and retains source handle plus complete prepared bytes',async()=>{
 const f=fixture();f.faults.set('close:content://test/audio',Error('SourceCloseUnknown'));await f.files.selectUris(1);
 await assert.rejects(f.files.prepareUri('content://test/audio'),/SourceCloseUnknown/);assert.equal(f.fds.size,1);assert.ok(f.trace.some(x=>x[0]==='close'&&x[1].endsWith('/data')));
 f.faults.delete('close:content://test/audio');await f.files.retryPendingIo();assert.equal(f.fds.size,0);const p=(await f.files.recover())[0];assert.equal(p.sha256,sha(f.bytes));
});
test('explicit local lyrics picker reads the whole UTF8 text and rejects damaged encoding without cropping',async()=>{
 const f=fixture(),text='[00:01.50]汉字 🧪\nplain';f.put('content://test/audio',text,'本地.lrc');const selected=await f.files.pickLyrics();assert.equal(selected.text,text);assert.equal(selected.byteLength,String(Buffer.byteLength(text)));assert.equal(selected.sha256,sha(Buffer.from(text)));assert.equal(f.fds.size,0);
 const g=fixture();g.put('content://test/audio',Buffer.from([0xff]),'bad.txt');await assert.rejects(g.files.pickLyrics());assert.equal(g.fds.size,0);
});
test('metadata close rejection retains the original FD and spool until explicit close acknowledgment',async()=>{
 const f=fixture(),dir='/app/files/hmos-music-spool/music-000001',p=dir+'/metadata.json';f.faults.set('close:'+p,Error('MetadataCloseUnknown'));
 await assert.rejects(prepare(f),/MetadataCloseUnknown/);assert.equal(f.fds.size,1);assert.ok(f.nodes.has(dir+'/data'));assert.ok(f.nodes.has(p));
 f.faults.delete('close:'+p);await f.files.retryPendingIo();assert.equal(f.fds.size,0);const recovered=(await f.files.recover())[0];assert.equal(recovered.sha256,sha(f.bytes));
});
test('request close failure prevents dispatch and symlink request is rejected without truncating the original',async()=>{
 const f=fixture(),p=await prepare(f),dir='/app/files/hmos-music-spool/'+p.spoolId,wire=f.request(p);let calls=0;
 f.faults.set('close:'+dir+'/request.json',Error('RequestCloseUnknown'));await assert.rejects(f.files.importPrepared(p,wire,async()=>{calls++;return '';}),/RequestCloseUnknown/);
 assert.equal(calls,0);assert.equal(f.fds.size,1);assert.equal(f.nodes.get(dir+'/request.json').bytes.toString(),wire);await assert.rejects(f.files.release(p),/句柄/);
 f.faults.delete('close:'+dir+'/request.json');await f.files.retryPendingIo();assert.equal(f.fds.size,0);
 await assert.rejects(f.files.importPrepared(p,wire.replace('import-1','import-2'),async()=>{calls++;return '';}),/不能更换/);
 assert.equal(calls,0);await f.files.importPrepared(p,wire,async sent=>{calls++;assert.equal(sent,wire);return 'same original';});assert.equal(calls,1);
 calls=0;
 const g=fixture(),q=await prepare(g),request='/app/files/hmos-music-spool/'+q.spoolId+'/request.json';g.nodes.set(request,{kind:'symlink',bytes:Buffer.from('keep')});
 await assert.rejects(g.files.importPrepared(q,g.request(q),async()=>{calls++;return '';}),/已变化/);assert.equal(calls,0);assert.equal(g.nodes.get(request).bytes.toString(),'keep');
});
test('partial player open close rejection retains original FD and prevents removal until explicit retry',async()=>{
 const f=fixture(),s=await f.files.acquire(f.identity()),p='/app/cache/'+s.token+'/data';f.faults.set('stat:'+p,Error('StatUnknown'));f.faults.set('close:'+p,Error('PartialCloseUnknown'));
 await assert.rejects(f.files.open(s),/PartialCloseUnknown/);assert.equal(f.fds.size,1);assert.equal(f.files.ownsSource(s),true);await assert.rejects(f.files.releaseSource(s),/句柄/);assert.ok(f.nodes.has(p));
 f.faults.delete('close:'+p);await f.files.retryPendingIo();assert.equal(f.fds.size,0);await f.files.releaseSource(s);assert.equal(f.files.ownsSource(s),false);
});

test('partial spool metadata unlink retains exact cleanup object through retry_io and recover without replaying absent data',async()=>{
 const f=fixture(),p=await prepare(f),dir='/app/files/hmos-music-spool/'+p.spoolId,wire=f.request(p);let sends=0;
 await f.files.importPrepared(p,wire,async()=>{sends++;return 'controlled Ready qualification';});
 f.faults.set('unlink:'+dir+'/metadata.json',Error('PartialMetadataCleanup'));
 await assert.rejects(f.files.release(p),/PartialMetadataCleanup/);assert.equal(f.nodes.has(dir+'/data'),false);
 assert.equal(f.files.entries.get(p.spoolId).value,p);assert.equal(f.files.entries.get(p.spoolId).releaseStarted,true);
 f.faults.clear();await f.files.retryPendingIo();assert.equal((await f.files.recover()).length,0,'partial cleanup is never presented as complete import data');
 assert.equal(f.files.entries.get(p.spoolId).value,p);assert.equal(p.requestJson,wire);assert.match(f.files.diagnostics().join(' '),/继续原清理/);
 await assert.rejects(f.files.importPrepared(p,wire,async()=>{sends++;return '';}),/只能继续原清理/);assert.equal(sends,1);assert.equal(f.fds.size,0);
 await assert.rejects(f.files.release({...p}),/身份/);assert.ok(f.nodes.has(dir+'/metadata.json'));
 await f.files.release(p);assert.equal(f.files.entries.has(p.spoolId),false);assert.equal(f.nodes.has(dir),false);assert.equal(sends,1);
});

test('failed first unlink marks only cleanup intent and still cannot masquerade as a complete recoverable replay source',async()=>{
 const f=fixture(),p=await prepare(f),dir='/app/files/hmos-music-spool/'+p.spoolId;
 f.faults.set('unlink:'+dir+'/data',Error('DataCleanupRejected'));await assert.rejects(f.files.release(p),/DataCleanupRejected/);
 assert.ok(f.nodes.has(dir+'/data'));assert.equal((await f.files.recover()).length,0);assert.equal(f.files.entries.get(p.spoolId).value,p);
 let sends=0;await assert.rejects(f.files.importPrepared(p,f.request(p),async()=>{sends++;return '';}),/只能继续原清理/);assert.equal(sends,0);
 f.faults.clear();await f.files.release(p);assert.equal(f.nodes.has(dir),false);
});

test('late request unlink failure retains original exact bytes while already removed data and metadata stay out of recover',async()=>{
 const f=fixture(),p=await prepare(f),wire=f.request(p),dir='/app/files/hmos-music-spool/'+p.spoolId;
 await f.files.importPrepared(p,wire,async()=> 'controlled Ready qualification');f.faults.set('unlink:'+dir+'/request.json',Error('RequestCleanupRejected'));
 await assert.rejects(f.files.release(p),/RequestCleanupRejected/);assert.equal(f.nodes.has(dir+'/data'),false);assert.equal(f.nodes.has(dir+'/metadata.json'),false);
 assert.equal(f.nodes.get(dir+'/request.json').bytes.toString(),wire);assert.equal((await f.files.recover()).length,0);
 assert.equal(f.files.entries.get(p.spoolId).value,p);f.faults.clear();await f.files.release(p);assert.equal(f.nodes.has(dir),false);assert.equal(p.requestJson,wire);
});

test('empty-directory removal failure is retained through recover and only same-object explicit release retry clears it',async()=>{
 const f=fixture(),p=await prepare(f),dir='/app/files/hmos-music-spool/'+p.spoolId;f.faults.set('rmdir:'+dir,Error('DirectoryCleanupRejected'));
 await assert.rejects(f.files.release(p),/DirectoryCleanupRejected/);assert.ok(f.nodes.has(dir));assert.equal([...f.nodes.keys()].filter(n=>path.posix.dirname(n)===dir).length,0);
 assert.equal((await f.files.recover()).length,0);assert.equal(f.files.entries.get(p.spoolId).value,p);f.faults.clear();await f.files.release(p);assert.equal(f.nodes.has(dir),false);
});

test('unexpected children added after partial cleanup are retained and block original cleanup retry',async()=>{
 const f=fixture(),p=await prepare(f),dir='/app/files/hmos-music-spool/'+p.spoolId;f.faults.set('unlink:'+dir+'/metadata.json',Error('RetainKnownSource'));
 await assert.rejects(f.files.release(p),/RetainKnownSource/);f.faults.clear();f.put(dir+'/unknown','keep');const remaining=f.nodes.get(dir+'/metadata.json').bytes.toString();
 assert.equal((await f.files.recover()).length,0);await assert.rejects(f.files.release(p),/未知内容/);
 assert.equal(f.files.entries.get(p.spoolId).value,p);assert.equal(f.nodes.get(dir+'/unknown').bytes.toString(),'keep');assert.equal(f.nodes.get(dir+'/metadata.json').bytes.toString(),remaining);
});

test('fresh adapter cannot mint partial cleanup ownership from restart leftovers or silently delete them',async()=>{
 const f=fixture(),p=await prepare(f),wire=f.request(p),dir='/app/files/hmos-music-spool/'+p.spoolId;
 await f.files.importPrepared(p,wire,async()=> 'controlled Ready qualification');f.faults.set('unlink:'+dir+'/metadata.json',Error('RetainedPartialCleanup'));
 await assert.rejects(f.files.release(p),/RetainedPartialCleanup/);f.faults.clear();const reopened=f.newFiles();
 assert.equal((await reopened.recover()).length,0);assert.equal(reopened.entries.has(p.spoolId),false);await assert.rejects(reopened.release(p),/身份/);
 assert.equal(f.nodes.get(dir+'/request.json').bytes.toString(),wire);assert.ok(f.nodes.has(dir+'/metadata.json'));assert.equal(f.nodes.has(dir+'/data'),false);
 assert.equal((await f.files.recover()).length,0);await f.files.release(p);assert.equal(f.nodes.has(dir),false);
});

test('unsafe directory rejection before unlink does not mint cleanup ownership or remove an existing complete source',async()=>{
 const f=fixture(),p=await prepare(f),dir='/app/files/hmos-music-spool/'+p.spoolId;f.put(dir+'/unknown','keep');
 await assert.rejects(f.files.release(p),/未知内容/);assert.equal(f.files.entries.get(p.spoolId).releaseStarted,false);
 assert.equal(f.nodes.get(dir+'/data').bytes.compare(f.bytes),0);assert.equal(f.nodes.get(dir+'/unknown').bytes.toString(),'keep');
});
