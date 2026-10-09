'use strict';
// Actual product coordinator, library, files, player adapter, queue and hashing.
// SDK/provider/Store/AVPlayer seams are controlled; this is not device evidence.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const { track, request, summary, reply, readyOperation, plain, sha, deferred } = require('./music-library-test-harness.cjs');
const root = path.resolve(__dirname, '../entry/src/main/ets');
const sourceNames = ['model/MusicWorkbench.ets','model/MusicUi.ets','model/MusicLibrary.ets','model/MusicFiles.ets',
  'model/MusicPlayback.ets','pages/PlatformMusicPlayer.ets','model/Workbench.ets','model/EditorInputHash.ets'];
async function settle() { for (let i=0; i<12; i++) await new Promise(resolve=>setImmediate(resolve)); }

function fixture(options = {}) {
  const nodes = new Map(), fds = new Map(), trace = [], faults = new Map(), players = [], wires = [], imports = [], histories = new Map();
  let fd = 10, folder = 0, uuid = 0, uris = [], alive = true, foreground = true, owner = 'page-1:1', revision = options.initial?.length ? '1' : '0';
  const records = (options.initial || []).map(plain), order = records.filter(t=>t.phase === 'ready').map(t=>t.track_id), lyrics = new Map();
  const audio = new Map(); for (const t of records) audio.set(t.track_id, Buffer.alloc(Number(t.byte_length), t.track_id.charCodeAt(0)));
  let selected = options.selected || order[0] || '', show = options.show || false;
  const put = (name, bytes, displayName) => nodes.set(name, {kind:'file', bytes:Buffer.from(bytes), name:displayName});
  for (const p of ['/app','/app/files','/app/cache']) nodes.set(p,{kind:'directory'});
  const fail = (op,p) => { if (faults.has(op+':'+p)) throw faults.get(op+':'+p); };
  const node = p => { if (!nodes.has(p)) throw Object.assign(Error('missing controlled file'),{code:13900002}); return nodes.get(p); };
  const stat = n => ({size:n.bytes?.length||0,isFile:()=>n.kind==='file',isDirectory:()=>n.kind==='directory',isSymbolicLink:()=>n.kind==='symlink'});
  const OpenMode = {READ_ONLY:0,WRITE_ONLY:1,READ_WRITE:2,CREATE:64,TRUNC:512,NOFOLLOW:131072};
  const io = { OpenMode,
    async lstat(p) { fail('lstat',p); return stat(node(p)); },
    async stat(id) { const p = fds.get(id).path; fail('stat',p); return stat(node(p)); },
    async mkdir(p) { nodes.set(p,{kind:'directory'}); },
    async mkdtemp(template) { const p=template.replace('XXXXXX',String(++folder).padStart(6,'0')); nodes.set(p,{kind:'directory'}); return p; },
    async listFile(p) { node(p); return [...nodes.keys()].filter(x=>path.posix.dirname(x)===p).map(x=>path.posix.basename(x)); },
    async open(p,flags) { fail('open',p); if ((flags&OpenMode.CREATE)&&!nodes.has(p)) put(p,''); const n=node(p); assert.equal(n.kind,'file');
      if (flags&OpenMode.TRUNC) n.bytes=Buffer.alloc(0); const f={fd:fd++,path:p,name:n.name||path.posix.basename(p)}; fds.set(f.fd,f); trace.push(['open',p,f.fd]); return f; },
    async close(f) { fail('close',f.path); assert.ok(fds.has(f.fd)); fds.delete(f.fd); trace.push(['close',f.path]); },
    async write(id,b) { const n=node(fds.get(id).path),bytes=Buffer.from(b); n.bytes=Buffer.concat([n.bytes,bytes]); return bytes.length; },
    async fsync(id) { trace.push(['fsync',fds.get(id).path]); },
    async readText(p) { return node(p).bytes.toString(); },
    async read(id,b,o) { const bytes=node(fds.get(id).path).bytes.subarray(o.offset,o.offset+o.length); new Uint8Array(b).set(bytes); return bytes.length; },
    async unlink(p) { fail('unlink',p); assert.equal(node(p).kind,'file'); nodes.delete(p); trace.push(['unlink',p]); },
    async rmdir(p) { assert.equal((await io.listFile(p)).length,0); nodes.delete(p); trace.push(['rmdir',p]); }
  };
  const current=(kind='library',shown=[])=>summary(shown,{kind,library_revision:revision,order:order.slice(),selected_track_id:selected,show_lyrics:show});
  const bump=()=>{ revision=String(Number(revision)+1); for(const t of records)t.library_revision=revision; };
  const noWrite=error=>JSON.stringify({ok:false,error,effect:'not_committed',receipt_revision:''});
  function mutation(c,apply) {
    const literal=JSON.stringify(c), prior=histories.get(c.operation_id);
    if (prior && prior.literal!==literal) return noWrite('changed fixed operation');
    if (!prior) { if(c.library_revision!==revision)return noWrite('MusicRevisionConflict'); apply(); bump(); histories.set(c.operation_id,{literal,revision}); }
    const value=current(c.track_id?'track':'library',c.track_id?[records.find(t=>t.track_id===c.track_id)]:[]);
    return reply({...value,operation_id:c.operation_id,operation_revision:histories.get(c.operation_id).revision,repeated:!!prior},'committed');
  }
  function controlledPolicy(c) {
    // An explicit controlled receiver exercising the product response checks.
    // It is neither Rust policy qualification nor an AVPlayer acknowledgement.
    let index=Number(c.index), playing=c.playing, blocked=c.blocked, position=c.position_ms,duration=c.duration_ms,effect='none';
    if(c.music_action==='toggle'&&!blocked&&order.length){playing=!playing;effect=playing?'play':'pause';}
    else if(c.music_action==='seek'){position=String(Math.min(Number(c.value),Number(duration)));effect='seek';}
    else if(['select','next','previous'].includes(c.music_action)&&order.length){
      const target=c.music_action==='next'?index+1:c.music_action==='previous'?index-1:Number(c.value);
      index=(target%order.length+order.length)%order.length;playing=c.flag&&!blocked;position='0';duration='0';effect='open';}
    if(blocked||!order.length)playing=false;
    return {track_id:order[index]||'',index:String(index),playing,blocked,position_ms:position,duration_ms:duration,transport_effect:effect};
  }
  function receiver(wire) {
    const c=JSON.parse(wire).music;
    if(c.action==='read'){
      const all=records.filter(t=>t.track_id>c.after).sort((a,b)=>a.track_id<b.track_id?-1:1),page=all.slice(0,c.limit);
      return reply({...current('library',page),next_after:all.length>c.limit?page.at(-1).track_id:''});
    }
    if(c.action==='policy')return reply({...current('playback'),playback:options.policy?options.policy(c,controlledPolicy):controlledPolicy(c)});
    if(c.action==='lyrics_read'){
      const t=records.find(t=>t.track_id===c.track_id),l=lyrics.get(c.track_id)||{text:'',source:'',lines:[],untimed:true};
      let active=-1;l.lines.forEach((line,i)=>{if(Number(line.time_ms)<=Number(c.position_ms))active=i;});
      return reply({...current('lyrics',[t]),lyrics:{track_id:c.track_id,...plain(l),active_index:active}});
    }
    if(c.action==='import_begin'){
      let t=records.find(t=>t.track_id===c.request.track_id);
      if(!t){if(c.request.expected_revision!==revision)return noWrite('MusicRevisionConflict');bump();t=track(c.request,revision,'pending');records.push(t);}
      return reply({...current('track',[t]),operation_id:c.request.operation_id,operation_revision:String(Number(c.request.expected_revision)+1),repeated:false},'committed');
    }
    if(c.action==='import_inspect'||c.action==='reconcile'){
      const t=records.find(t=>JSON.stringify(t.request)===JSON.stringify(c.request));if(!t)return noWrite('MusicImportMissing');
      if(c.action==='reconcile'&&t.phase==='pending'&&t.bytes_retained){bump();t.phase='ready';order.push(t.track_id);selected||=t.track_id;}
      const ready=c.action==='reconcile'&&t.phase==='ready';
      return reply({...current('track',[t]),operation_id:ready?readyOperation(t.request):t.import_operation,
        operation_revision:ready?revision:String(Number(t.request.expected_revision)+1),repeated:true},ready?'committed':'not_committed');
    }
    if(c.action==='select')return mutation(c,()=>{selected=c.track_id;});
    if(c.action==='reorder')return mutation(c,()=>order.splice(0,order.length,...c.order));
    if(c.action==='show_lyrics')return mutation(c,()=>{show=c.flag;});
    if(c.action==='remove')return mutation(c,()=>{const i=order.indexOf(c.track_id);order.splice(i,1);records.find(t=>t.track_id===c.track_id).phase='retired';if(selected===c.track_id)selected=order[Math.min(i,order.length-1)]||'';});
    if(c.action==='set_lyrics')return mutation(c,()=>{const t=records.find(t=>t.track_id===c.track_id);t.lyric_source=c.lyric_source;t.lyrics_byte_length=String(Buffer.byteLength(c.lyrics));lyrics.set(c.track_id,{text:c.lyrics,source:c.lyric_source,lines:[],untimed:true});});
    throw Error('Unexpected controlled request '+c.action);
  }
  const streamReply=bytes=>JSON.stringify({ok:true,error:'',byte_length:String(bytes.length),sha256:sha(bytes)});
  const native={
    async request(wire){wires.push(wire);trace.push(['request',JSON.parse(wire).music.action]);return options.send?options.send(wire,receiver,wires.length):receiver(wire);},
    async prepareMusicFile(from,to,limit){const bytes=node(fds.get(from).path).bytes;if(bytes.length>limit)throw Error('audio budget');node(fds.get(to).path).bytes=Buffer.from(bytes);trace.push(['prepare',limit]);return streamReply(bytes);},
    async importFile(wire,id){imports.push(wire);const c=JSON.parse(wire).music, t=records.find(t=>t.track_id===c.request.track_id),bytes=node(fds.get(id).path).bytes;
      assert.equal(sha(bytes),c.request.sha256);assert.equal(String(bytes.length),c.request.byte_length);audio.set(t.track_id,Buffer.from(bytes));t.bytes_retained=true;
      const complete=()=>{if(t.phase!=='ready'){bump();t.phase='ready';order.push(t.track_id);selected||=t.track_id;}return reply({...current('track',[t]),operation_id:readyOperation(c.request),operation_revision:revision},'committed');};
      return options.importFile?options.importFile(wire,id,complete,t):complete();},
    async exportFile(wire,id){const c=JSON.parse(wire).music,bytes=audio.get(c.track_id);assert.equal(c.library_revision,revision);assert.equal(c.sha256,sha(bytes));node(fds.get(id).path).bytes=Buffer.from(bytes);trace.push(['export',c.track_id,c.library_revision]);return options.exportFile?options.exportFile(wire,id,streamReply(bytes)):streamReply(bytes);}
  };
  const media={SeekMode:{SEEK_CLOSEST:2},async createAVPlayer(){
    const listeners={},calls=[],p={duration:9000,currentTime:0,on:(name,callback)=>listeners[name]=callback,
      async prepare(){calls.push('prepare');listeners.stateChange('prepared');},
      async play(){calls.push('play');if(options.play)await options.play(p);if(!options.noPlayAck)listeners.stateChange('playing');},
      async pause(){calls.push('pause');if(options.pause)await options.pause(p);if(!options.noPauseAck)listeners.stateChange('paused');},
      seek(value){calls.push(['seek',value]);p.currentTime=value;listeners.timeUpdate(value);},
      async release(){calls.push('release');if(options.release)await options.release(p);},listeners,calls};
    Object.defineProperty(p,'fdSrc',{set:value=>{p.descriptor=value;trace.push(['fdSrc',value.fd]);listeners.stateChange('initialized');}});
    players.push(p);return p;
  }};
  const sdkUtil={generateRandomUUID:()=>String(++uuid).padStart(32,'0'),TextEncoder:class{encodeInto(value){return new Uint8Array(Buffer.from(value));}},
    TextDecoder:{create:(name,o)=>({decodeToString:bytes=>new TextDecoder(name,o).decode(bytes)})}};
  const mocks={
    '@kit.AbilityKit':{},'@kit.BasicServicesKit':{},'@kit.ArkTS':{util:sdkUtil},'@kit.MediaKit':{media},
    '@kit.CoreFileKit':{fileIo:io,picker:{DocumentSelectOptions:class{},DocumentSelectMode:{FILE:1},DocumentViewPicker:class{async select(){if(options.picker)await options.picker();return uris.slice();}}}},
    '@kit.CryptoArchitectureKit':{cryptoFramework:{createMd:name=>{assert.equal(name,'SHA256');const h=crypto.createHash('sha256');return {async update(value){h.update(value.data);},async digest(){return {data:new Uint8Array(h.digest())};}};} }},
    'libmorrow.so':{default:native}
  };
  const cache=new Map();function load(file){
    if(cache.has(file))return cache.get(file);const exports={};cache.set(file,exports);
    const c=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}});
    assert.deepEqual((c.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error),[]);
    vm.runInNewContext(c.outputText,{exports,require:name=>name.startsWith('.')?load(path.resolve(path.dirname(file),name+'.ets')):mocks[name],
      Error,Number,Promise,setTimeout,clearTimeout,encodeURIComponent}, {filename:file});return exports;
  }
  const api=load(path.resolve(root,'model/MusicWorkbench.ets')),views=[];let controller;
  const hooks={owned:()=>alive,foreground:()=>foreground,owner:()=>owner,changed:()=>{if(controller)views.push(controller.view());},
    beforePlay:async()=>{trace.push(['beforePlay']);if(options.beforePlay)await options.beforePlay();}};
  controller=new api.MusicWorkbench({filesDir:'/app/files',cacheDir:'/app/cache'},hooks);
  return {controller,api,nodes,fds,trace,faults,players,wires,imports,records,order,lyrics,audio,put,views,options,receiver,current,bump,
    select:values=>{uris=values;},setAlive:value=>{alive=value;},setForeground:value=>{foreground=value;owner='page-1:'+String(Number(owner.split(':')[1])+1);},
    setSelected:value=>{selected=value;},get revision(){return revision;},get selected(){return selected;},
    async spool(name='recovered.wav',bytes=Buffer.alloc(64,7),literal=''){
      const id='music-'+String(++folder).padStart(6,'0'),dir='/app/files/hmos-music-spool/'+id;
      if(!nodes.has('/app/files/hmos-music-spool'))nodes.set('/app/files/hmos-music-spool',{kind:'directory'});nodes.set(dir,{kind:'directory'});
      const value={spoolId:id,name,byteLength:String(bytes.length),sha256:sha(bytes),requestJson:''};put(dir+'/data',bytes);put(dir+'/metadata.json',JSON.stringify(value));
      if(literal)put(dir+'/request.json',literal);return {id,dir,value,bytes};
    }
  };
}
function ready(id='A'){
  const bytes=Buffer.alloc(64,id.charCodeAt(0)),r=request(id,'0');r.name=id+'.wav';r.sha256=sha(bytes);return track(r,'1');
}
module.exports={fixture,ready,settle,deferred,sha,plain,request,track,reply,sourceNames,root};
