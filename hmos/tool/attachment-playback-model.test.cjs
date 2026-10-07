const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {test}=require('node:test'),assert=require('node:assert/strict');
const ts=require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const compile=source=>ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText;
const api={};vm.runInNewContext(compile(fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/model/AttachmentPlayback.ets'),'utf8')),{exports:api,Error});
const settle=async()=>{for(let i=0;i<40;i++)await Promise.resolve();};
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
function player(){
  const calls=[],listeners={};
  const p={calls,listeners,
    onState:f=>listeners.state=f,onPosition:f=>listeners.position=f,onDuration:f=>listeners.duration=f,onError:f=>listeners.error=f,
    load:async()=>{calls.push('load');listeners.state('initialized');},
    setSurface:id=>calls.push(['surface',id]),
    prepare:async()=>{calls.push('prepare');listeners.duration(9000);listeners.state('prepared');},
    play:async()=>{calls.push('play');listeners.state('playing');},pause:async()=>{calls.push('pause');listeners.state('paused');},
    seek:ms=>calls.push(['seek',ms]),release:async()=>calls.push('release')};
  return p;
}
function fixture(factory){
  const players=[],sources=[],views=[];
  const model=new api.AttachmentPlayback(async source=>{sources.push(source);const p=factory?await factory(source):player();players.push(p);return p;},()=>views.push(model.view()));
  const source=(token='A',kind='audio')=>({token,kind,path:'/cache/morrow-preview-'+token+'/data',uri:'file://bundle/cache/'+token,byteLength:10});
  return {model,players,sources,views,source};
}

test('audio prepares paused with time controls and never starts automatically',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();
  assert.deepEqual(f.players[0].calls,['load','prepare']);assert.equal(f.model.view().phase,'prepared');
  assert.equal(f.model.view().durationMs,9000);assert.equal(api.playbackCanControl(f.model.view()),true);
  await f.model.toggle();assert.equal(f.model.view().phase,'playing');await f.model.toggle();assert.equal(f.model.view().phase,'paused');
});

test('video waits for its exact token surface and prepares once irrespective of callback order',async()=>{
  const f=fixture();await f.model.open(f.source('V','video'));await settle();
  assert.deepEqual(f.players[0].calls,['load']);assert.equal(f.model.view().phase,'loading');
  f.model.setSurface('OLD','old');assert.deepEqual(f.players[0].calls,['load']);
  f.model.setSurface('V','surface-1');f.model.setSurface('V','surface-2');await settle();
  assert.equal(f.players[0].calls.filter(x=>x==='prepare').length,1);
  assert.deepEqual(f.players[0].calls.filter(Array.isArray),[['surface','surface-2'],['surface','surface-2']]);
  assert.equal(f.model.view().phase,'prepared');
});

test('close before awaited open admission cannot create a player or expose a deleted lease',async()=>{
  const f=fixture();const opening=f.model.open(f.source());await f.model.dispose();await opening;await settle();
  assert.equal(f.sources.length,0);assert.equal(f.model.view().phase,'idle');
});

test('close awaits late factory creation and releases without loading that source',async()=>{
  const factory=deferred(),f=fixture(()=>factory.promise);const opening=f.model.open(f.source());await settle();
  let closed=false;const closing=f.model.dispose().then(()=>{closed=true;});await settle();assert.equal(closed,false);
  const p=player();factory.resolve(p);await opening;await closing;
  assert.deepEqual(p.calls,['release']);assert.equal(f.model.view().phase,'idle');
});

test('close waits for pending native initialization before releasing the player',async()=>{
  const loading=deferred(),p=player();p.load=async()=>{p.calls.push('load');await loading.promise;p.listeners.state('initialized');};
  const f=fixture(()=>p),opening=f.model.open(f.source());await settle();let closed=false;
  const closing=f.model.dispose().then(()=>{closed=true;});await settle();assert.equal(closed,false);assert.deepEqual(p.calls,['load']);
  loading.resolve();await opening;await closing;assert.deepEqual(p.calls,['load','release']);
});

test('close awaits an already executing play instead of releasing its descriptor mid-command',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();const p=f.players[0],playing=deferred();
  p.play=async()=>{p.calls.push('play');await playing.promise;p.listeners.state('playing');};
  const accepted=f.model.toggle();await settle();const closing=f.model.dispose();await settle();
  assert.equal(p.calls.includes('release'),false);assert.equal(f.model.view().phase,'closing');
  playing.resolve();await accepted;await closing;assert.deepEqual(p.calls.slice(-2),['play','release']);
});

test('surface observed before native initialization is used only after initialized',async()=>{
  const creating=deferred(),f=fixture(()=>creating.promise),opening=f.model.open(f.source('V','video'));await settle();
  f.model.setSurface('V','early-surface');const p=player();creating.resolve(p);await opening;await settle();
  assert.deepEqual(p.calls,['load',['surface','early-surface'],'prepare']);
});

test('failed prepare remains failed with controls disabled and releasable ownership',async()=>{
  const p=player();p.prepare=async()=>{p.calls.push('prepare');throw new Error('codec unavailable');};
  const f=fixture(()=>p);await f.model.open(f.source());await settle();assert.equal(f.model.view().phase,'failed');
  assert.equal(f.model.view().error,'codec unavailable');await f.model.toggle();assert.equal(p.calls.includes('play'),false);
  await f.model.dispose();assert.equal(p.calls.at(-1),'release');
});

test('rejected release keeps resource ownership and an explicit close can retry',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();const p=f.players[0];let n=0;
  p.release=async()=>{p.calls.push('release');if(++n===1)throw new Error('release rejected');};
  await assert.rejects(f.model.dispose(),/媒体资源释放失败/);assert.equal(f.model.view().phase,'cleanup_failed');
  assert.equal(f.model.view().token,'A');await f.model.dispose();assert.equal(n,2);assert.equal(f.model.view().phase,'idle');
});

test('reopening never bypasses a failed previous release or starts another source',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();f.players[0].release=async()=>{throw new Error('retained');};
  await assert.rejects(f.model.open(f.source('B')),/媒体资源释放失败/);assert.equal(f.sources.length,1);assert.equal(f.model.view().token,'A');
});

test('concurrent close callers share one cleanup and stale events cannot update a replacement',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();const old=f.players[0],release=deferred();
  old.release=async()=>{old.calls.push('release');await release.promise;};
  const a=f.model.dispose(),b=f.model.dispose();await settle();assert.equal(old.calls.filter(x=>x==='release').length,1);
  old.listeners.state('playing');old.listeners.duration(100000);assert.equal(f.model.view().phase,'closing');
  release.resolve();await Promise.all([a,b]);await f.model.open(f.source('B'));await settle();
  old.listeners.error('old failure');old.listeners.position(7000);assert.equal(f.model.view().token,'B');assert.equal(f.model.view().positionMs,0);
});

test('latest concurrent opening wins with no native work from a superseded admission',async()=>{
  const f=fixture();await Promise.all([f.model.open(f.source('A')),f.model.open(f.source('B'))]);await settle();
  assert.equal(f.sources.length,1);assert.equal(f.sources[0].token,'B');assert.equal(f.model.view().token,'B');
});

test('controls reject invalid positions and serialize accepted actions before release',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();const p=f.players[0],playing=deferred();
  p.play=async()=>{p.calls.push('play');await playing.promise;p.listeners.state('playing');};
  const accepted=f.model.toggle();const ignored=f.model.toggle();const closing=f.model.dispose();await settle();
  assert.equal(p.calls.filter(x=>x==='play').length,0);await Promise.all([accepted,ignored,closing]);
  assert.equal(p.calls.at(-1),'release');
  await f.model.open(f.source('B'));await settle();const next=f.players[1];
  await f.model.seek(NaN);await f.model.seek(Infinity);await f.model.seek(-5);await settle();await f.model.seek(100000);await settle();
  assert.deepEqual(next.calls.filter(Array.isArray),[['seek',0],['seek',9000]]);
});

test('background pauses an in-flight accepted play without restoring autoplay',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();const p=f.players[0],playing=deferred();
  p.play=async()=>{p.calls.push('play');await playing.promise;p.listeners.state('playing');};
  p.pause=async()=>{assert.equal(f.model.view().phase,'playing');p.calls.push('pause');p.listeners.state('paused');};
  const accepted=f.model.toggle();await settle();const hidden=f.model.pauseForBackground();playing.resolve();
  await accepted;await hidden;await settle();assert.equal(f.model.view().phase,'paused');
  assert.equal(p.calls.filter(x=>x==='play').length,1);assert.equal(p.calls.filter(x=>x==='pause').length,1);
});

test('late disappearing-surface pause for an old token cannot pause a newly playing session',async()=>{
  const f=fixture();await f.model.open(f.source('A'));await settle();await f.model.open(f.source('B'));await settle();
  await f.model.toggle();assert.equal(f.model.view().phase,'playing');await f.model.pauseForBackground('A');await settle();
  assert.equal(f.model.view().phase,'playing');assert.equal(f.players[1].calls.includes('pause'),false);
  await f.model.pauseForBackground('B');assert.equal(f.model.view().phase,'paused');
});

test('media failure is visible and callbacks cannot silently recover or automatically replay',async()=>{
  const f=fixture();await f.model.open(f.source());await settle();const p=f.players[0];p.listeners.error('decoder failed');
  p.listeners.state('prepared');p.listeners.position(1200);p.listeners.duration(90000);await f.model.toggle();
  assert.equal(f.model.view().phase,'failed');assert.equal(f.model.view().error,'decoder failed');assert.equal(f.model.view().positionMs,0);
  assert.deepEqual(p.calls,['load','prepare']);await f.model.dispose();assert.equal(p.calls.at(-1),'release');
});

test('source and view snapshots cannot be mutated to substitute another file',async()=>{
  const f=fixture(),source=f.source();await f.model.open(source);await settle();source.path='/other';
  assert.equal(f.sources[0].path,'/cache/morrow-preview-A/data');const view=f.model.view();view.token='other';view.phase='playing';
  assert.equal(f.model.view().token,'A');assert.equal(f.model.view().phase,'prepared');
  assert.equal(api.playbackTime(3723456),'62:03');assert.equal(api.playbackTime(NaN),'0:00');
});

function platformFixture(options={}){
  const events=[],callbacks={},file={fd:77},p={duration:9000,currentTime:0,
    on:(key,value)=>callbacks[key]=value,prepare:async()=>{},play:async()=>{},pause:async()=>{},seek:()=>{},
    release:async()=>{events.push('player-release');if(options.releaseFailure)throw new Error('player busy');}};
  Object.defineProperty(p,'fdSrc',{set:value=>{events.push(['fdSrc',value]);callbacks.stateChange('initialized');}});
  const mocks={
    '@kit.MediaKit':{media:{createAVPlayer:async()=>{events.push('player-create');if(options.createFailure)throw new Error('create failed');return p;},VideoScaleType:{VIDEO_SCALE_TYPE_SCALED_ASPECT:2},SeekMode:{SEEK_CLOSEST:2}}},
    '@kit.CoreFileKit':{fileIo:{OpenMode:{READ_ONLY:0,NOFOLLOW:0o400000},open:async(filePath,mode)=>{events.push(['open',filePath,mode]);return file;},stat:async fd=>{events.push(['stat',fd]);return {size:options.wrongLength?11:10,isFile:()=>true};},close:async closed=>{events.push('fd-close');assert.equal(closed,file);if(options.closeFailure)throw new Error('close failed');}}},
    '@kit.BasicServicesKit':{},'../model/AttachmentPlayback':api,'../model/UiStrings':{uiText:x=>x}};
  const source=fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/pages/AttachmentMediaPreview.ets'),'utf8').split('@Component')[0];
  const exported={};vm.runInNewContext(compile(source),{exports:exported,require:key=>mocks[key],Error});
  return {events,exported,options,p};
}

test('actual SDK adapter uses readonly nofollow FD with exact byte extent, then release before fd close',async()=>{
  const f=platformFixture(),p=await f.exported.createAttachmentPlayer({token:'T',path:'/cache/verified/data',uri:'file://private',kind:'audio',byteLength:10});
  p.onState(()=>{});await p.load();assert.deepEqual(JSON.parse(JSON.stringify(f.events)),[['open','/cache/verified/data',0o400000],['stat',77],'player-create',['fdSrc',{fd:77,offset:0,length:10}]]);
  await p.release();assert.deepEqual(f.events.slice(-2),['player-release','fd-close']);
});

test('actual SDK adapter retains descriptor when native release fails and retries only after explicit close',async()=>{
  const f=platformFixture({releaseFailure:true}),p=await f.exported.createAttachmentPlayer({path:'/cache/verified/data',kind:'audio',byteLength:10});
  await p.load();await assert.rejects(p.release(),/player busy/);assert.equal(f.events.includes('fd-close'),false);
  f.options.releaseFailure=false;await p.release();assert.deepEqual(f.events.slice(-2),['player-release','fd-close']);
});

test('actual SDK adapter retains failed FD close after player release, retry never re-releases the player',async()=>{
  const f=platformFixture({closeFailure:true}),p=await f.exported.createAttachmentPlayer({path:'/cache/verified/data',kind:'audio',byteLength:10});
  await p.load();await assert.rejects(p.release(),/close failed/);f.options.closeFailure=false;await p.release();
  assert.equal(f.events.filter(x=>x==='player-release').length,1);assert.equal(f.events.filter(x=>x==='fd-close').length,2);
});

test('actual SDK adapter holds partial initialization resources for cleanup and never feeds mismatched bytes to AVPlayer',async()=>{
  for(const options of [{createFailure:true},{wrongLength:true}]){
    const f=platformFixture(options),p=await f.exported.createAttachmentPlayer({path:'/cache/verified/data',kind:'video',byteLength:10});
    await assert.rejects(p.load());assert.equal(f.events.some(x=>Array.isArray(x)&&x[0]==='fdSrc'),false);
    await p.release();assert.equal(f.events.at(-1),'fd-close');
  }
});
