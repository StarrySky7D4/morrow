const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {test}=require('node:test'),assert=require('node:assert/strict');
const ts=require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
function load(name,dependencies={}){
  const api={},source=fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/model/'+name+'.ets'),'utf8');
  const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText;
  vm.runInNewContext(compiled,{exports:api,Error,setTimeout,clearTimeout,require:key=>dependencies[key]||{}});return api;
}
const playback=load('AttachmentPlayback'),api=load('AttachmentMediaGestures',{'./AttachmentPlayback':playback});
const view=(overrides={})=>Object.assign(new playback.PlaybackView(),{token:'A',kind:'video',phase:'paused',durationMs:120000,positionMs:40000},overrides);
const settle=async()=>{for(let i=0;i<40;i++)await Promise.resolve();};
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
function clock(){let id=0,now=0;const tasks=new Map(),history=new Map();
  return {schedule(fn,delay){const key=++id;tasks.set(key,{fn,at:now+delay});history.set(key,fn);return key;},cancel(key){tasks.delete(key);},
    advance(ms){const end=now+ms;while(true){const due=[...tasks].filter(([,v])=>v.at<=end).sort((a,b)=>a[1].at-b[1].at)[0];if(!due)break;now=due[1].at;tasks.delete(due[0]);due[1].fn();}now=end;},
    force(key){history.get(key)();},get last(){return id;},get pending(){return tasks.size;}};
}
function doubleFixture(initial=view()){
  let current=initial,fullscreen=true;const timer=clock(),changed=[],submitted=[];
  const model=new api.AttachmentDoubleTapSeek(()=>current,()=>fullscreen,a=>changed.push(a),a=>submitted.push(a),timer);
  return {model,timer,changed,submitted,get current(){return current;},set current(v){current=v;},get fullscreen(){return fullscreen;},set fullscreen(v){fullscreen=v;}};
}

test('fullscreen horizontal seek uses actual Material whole-second duration and default 1000 vp sensitivity',()=>{
  const model=new api.AttachmentMediaGestures(),v=view({durationMs:120999,positionMs:40050});model.begin(v,true);
  const a=model.move(100,v,true);assert.equal(a.token,'A');assert.equal(a.deltaMs,12000);assert.equal(a.value,52050);
  v.positionMs=41234;const final=model.finish(v,true);assert.equal(final.value,53234);assert.equal(final.deltaMs,12000);
});
test('horizontal gesture submits once only after finish and subsequent finish is empty',()=>{
  const model=new api.AttachmentMediaGestures(),v=view();model.begin(v,true);assert.equal(model.move(-100,v,true).value,28000);
  assert.equal(model.finish(v,true).value,28000);assert.equal(model.finish(v,true).kind,'');
});
test('Dart negative half-second golden rounds away from zero rather than JavaScript negative zero',()=>{
  for(const [offset,delta] of [[-5,-1000],[5,1000],[-4.99,0],[4.99,0],[-15,-2000],[15,2000]]){
    const model=new api.AttachmentMediaGestures(),v=view({durationMs:100000,positionMs:40000});model.begin(v,true);
    const action=model.move(offset,v,true);assert.equal(action.deltaMs,delta);assert.equal(action.value,40000+delta);
  }
});
test('source-style horizontal out-of-range movements preserve last accepted delta',()=>{
  const model=new api.AttachmentMediaGestures(),v=view();model.begin(v,true);model.move(100,v,true);
  assert.equal(model.move(1000,v,true).kind,'');assert.equal(model.finish(v,true).value,52000);
});
test('zero movement, unknown duration, invalid offsets and non-fullscreen never submit seek',()=>{
  for(const v of [view(),view({durationMs:0}),view({durationMs:NaN}),view({positionMs:NaN})]){
    const model=new api.AttachmentMediaGestures();model.begin(v,true);model.move(0,v,true);model.move(NaN,v,true);
    assert.equal(model.finish(v,true).kind,'');model.begin(v,false);assert.equal(model.move(100,v,false).kind,'');
  }
});
test('lease replacement, fullscreen exit, background disposal and player failure cancel an accepted horizontal seek',()=>{
  for(const mode of ['token','fullscreen','cancel','failed','busy','surface']){
    const model=new api.AttachmentMediaGestures(),v=view();model.begin(v,true);model.move(100,v,true);
    if(mode==='token')v.token='B';if(mode==='cancel')model.cancel();if(mode==='failed')v.phase='failed';if(mode==='busy')v.busy=true;
    if(mode==='surface')v.surfaceReady=false;assert.equal(model.finish(v,mode!=='fullscreen').kind,'',mode);
  }
});
test('short clips below one second and a rounded zero offset do not dispatch',()=>{
  const model=new api.AttachmentMediaGestures(),v=view({durationMs:800,positionMs:0});model.begin(v,true);model.move(300,v,true);assert.equal(model.finish(v,true).kind,'');
});
test('double tap submits left minus 10 seconds or right plus 10 seconds after exactly 400 ms',()=>{
  for(const [x,delta] of [[100,-10000],[800,10000]]){const f=doubleFixture();f.model.trigger(f.current,true,x,900);
    assert.equal(f.changed.at(-1).deltaMs,delta);assert.equal(f.submitted.length,0);f.timer.advance(399);assert.equal(f.submitted.length,0);
    f.timer.advance(1);assert.equal(f.submitted[0].value,40000+delta);assert.equal(f.submitted[0].token,'A');
    f.timer.advance(199);assert.equal(f.changed.at(-1).kind,'seek');f.timer.advance(1);assert.equal(f.changed.at(-1).kind,'');}
});
test('middle third double tap and unavailable/fullscreen-disabled playback do not seek',()=>{
  for(const v of [view(),view({phase:'loading'}),view({busy:true}),view({surfaceReady:false}),view({durationMs:0})]){
    const f=doubleFixture(v);f.model.trigger(v,true,450,900);f.model.trigger(v,false,800,900);f.timer.advance(1000);assert.equal(f.submitted.length,0);
  }
});
test('exact equal-third outer boundaries match source and invalid coordinates cannot seek',()=>{
  for(const [x,direction] of [[300,-1],[600,1]]){const f=doubleFixture();f.model.trigger(f.current,true,x,900);f.timer.advance(400);assert.equal(f.submitted[0].deltaMs,direction*10000);}
  for(const [x,w] of [[-1,900],[901,900],[NaN,900],[10,0],[10,Infinity]]){const f=doubleFixture();f.model.trigger(f.current,true,x,w);f.timer.advance(400);assert.equal(f.submitted.length,0);}
});
test('subsequent tap increments same side by ten seconds and resets 400 ms submission timer',()=>{
  const f=doubleFixture();f.model.trigger(f.current,true,800,900);const old=f.timer.last;f.timer.advance(200);
  assert.equal(f.model.increment(f.current,true,800,900),true);assert.equal(f.changed.at(-1).deltaMs,20000);
  f.timer.force(old);assert.equal(f.submitted.length,0);f.timer.advance(399);assert.equal(f.submitted.length,0);f.timer.advance(1);assert.equal(f.submitted[0].value,60000);
});
test('opposite or middle single tap does not increment pending double-tap feedback',()=>{
  const f=doubleFixture();f.model.trigger(f.current,true,800,900);assert.equal(f.model.increment(f.current,true,100,900),false);
  assert.equal(f.model.increment(f.current,true,450,900),false);f.timer.advance(400);assert.equal(f.submitted[0].deltaMs,10000);
});
test('changing double-tap direction cancels earlier side without replaying its timer',()=>{
  const f=doubleFixture();f.model.trigger(f.current,true,800,900);const old=f.timer.last;f.model.trigger(f.current,true,100,900);
  f.timer.force(old);f.timer.advance(400);assert.equal(f.submitted.length,1);assert.equal(f.submitted[0].deltaMs,-10000);
});
test('double tap follows current playback position at delayed commit and clamps to start/end',()=>{
  for(const [x,start,end] of [[800,115000,120000],[100,2000,0]]){const f=doubleFixture(view({positionMs:start}));f.model.trigger(f.current,true,x,900);f.timer.advance(400);assert.equal(f.submitted[0].value,end);}
  const f=doubleFixture();f.model.trigger(f.current,true,800,900);f.current.positionMs=42000;f.timer.advance(400);assert.equal(f.submitted[0].value,52000);
});
test('late double-tap timers never control a replacement lease even with identical duration and position',()=>{
  const f=doubleFixture();f.model.trigger(f.current,true,800,900);const old=f.timer.last;f.current=view({token:'B'});
  f.timer.force(old);assert.equal(f.submitted.length,0);assert.equal(f.changed.at(-1).kind,'');
});
test('double-tap timers cancel on fullscreen exit, busy release, failure and explicit cancel',()=>{
  for(const mode of ['fullscreen','busy','failed','cancel']){const f=doubleFixture();f.model.trigger(f.current,true,800,900);const old=f.timer.last;
    if(mode==='fullscreen')f.fullscreen=false;if(mode==='busy')f.current.busy=true;if(mode==='failed')f.current.phase='failed';if(mode==='cancel')f.model.cancel();
    f.timer.force(old);assert.equal(f.submitted.length,0,mode);}
});
test('old feedback fade timer cannot erase a newer source indicator',()=>{
  const f=doubleFixture();f.model.trigger(f.current,true,800,900);f.timer.advance(400);const fade=f.timer.last;
  f.current=view({token:'B'});f.model.trigger(f.current,true,100,900);f.timer.force(fade);assert.equal(f.changed.at(-1).token,'B');assert.equal(f.changed.at(-1).kind,'seek');
});
test('controls start hidden and single tap toggles with exact 3-second timeout',()=>{
  const timer=clock(),changed=[],model=new api.AttachmentControlVisibility(v=>changed.push(v),timer);model.bind('A',false);assert.equal(model.view(),false);
  model.tap('A');assert.equal(model.view(),true);timer.advance(2999);assert.equal(model.view(),true);timer.advance(1);assert.equal(model.view(),false);
  model.tap('A');model.tap('A');assert.equal(model.view(),false);assert.equal(timer.pending,0);
});
test('control interaction refreshes hide timer and slider drag keeps controls mounted until release',()=>{
  const timer=clock(),model=new api.AttachmentControlVisibility(()=>{},timer);model.bind('A',true);model.tap('A');const old=timer.last;
  timer.advance(2000);model.interact('A');timer.force(old);assert.equal(model.view(),true);model.hold('A');timer.advance(9000);assert.equal(model.view(),true);
  model.release('A');timer.advance(2999);assert.equal(model.view(),true);timer.advance(1);assert.equal(model.view(),false);
});
test('stale visibility timer and stale tap cannot control replacement or remounted fullscreen controls',()=>{
  const timer=clock(),model=new api.AttachmentControlVisibility(()=>{},timer);model.bind('A',false);model.tap('A');const old=timer.last;
  model.bind('B',true);model.tap('B');timer.force(old);assert.equal(model.view(),true);model.tap('A');assert.equal(model.view(),true);
  model.bind('B',false);assert.equal(model.view(),false);model.tap('B');model.dispose();timer.advance(5000);assert.equal(model.view(),false);
});

function player(){const calls=[],events={};return {calls,events,onState:f=>events.state=f,onPosition:f=>events.position=f,onDuration:f=>events.duration=f,onError:f=>events.error=f,
  load:async()=>{calls.push('load');events.state('initialized');},setSurface:id=>calls.push(['surface',id]),prepare:async()=>{calls.push('prepare');events.duration(120000);events.state('prepared');},
  play:async()=>{calls.push('play');events.state('playing');},pause:async()=>{calls.push('pause');events.state('paused');},seek:v=>calls.push(['seek',v]),release:async()=>calls.push('release')};}
const source=(token='A',kind='audio')=>({token,kind,path:'/cache/morrow-preview-'+token+'/data',uri:'file://bundle/'+token,byteLength:10});
test('token-bound toggle and seek reject delayed controls from an old attachment',async()=>{
  const players=[],model=new playback.AttachmentPlayback(async()=>{const p=player();players.push(p);return p;},()=>{});
  await model.open(source());await settle();await model.open(source('B'));await settle();const calls=players[1].calls.length;
  await model.toggle('A');await model.seek(80000,'A');assert.equal(players[1].calls.length,calls);await model.toggle('B');await settle();await model.seek(80000,'B');assert.deepEqual(players[1].calls.slice(-2),['play',['seek',80000]]);
});
test('destroyed surface observed before initialized is never assigned or prepared',async()=>{
  const gate=deferred(),p=player(),model=new playback.AttachmentPlayback(async()=>{await gate.promise;return p;},()=>{}),opening=model.open(source('A','video'));await settle();
  model.setSurface('A','S1');model.clearSurface('A','S1');gate.resolve();await opening;await settle();assert.deepEqual(p.calls,['load']);assert.equal(model.view().surfaceReady,false);
  model.setSurface('A','S2');await settle();assert.deepEqual(p.calls,['load',['surface','S2'],'prepare']);assert.equal(model.view().surfaceReady,true);
});
test('stale surface destroy cannot clear a newer exact surface or another lease',async()=>{
  const p=player(),model=new playback.AttachmentPlayback(async()=>p,()=>{});await model.open(source('A','video'));model.setSurface('A','S1');await settle();
  model.setSurface('A','S2');await settle();model.clearSurface('A','S1');model.clearSurface('OLD','S2');await settle();assert.equal(model.view().surfaceReady,true);
  await model.toggle('A');await settle();model.clearSurface('A','S2');await settle();assert.equal(model.view().surfaceReady,false);assert.equal(model.view().phase,'paused');
  await model.toggle('A');assert.equal(p.calls.filter(x=>x==='play').length,1);
});
test('destroyed surface cancels queued prepare and replacement admits only the exact latest surface',async()=>{
  const p=player(),model=new playback.AttachmentPlayback(async()=>p,()=>{});await model.open(source('A','video'));await settle();
  model.setSurface('A','S1');model.clearSurface('A','S1');await settle();assert.equal(p.calls.includes('prepare'),false);assert.equal(model.view().surfaceReady,false);
  model.setSurface('A','S2');await settle();assert.equal(p.calls.filter(x=>x==='prepare').length,1);assert.deepEqual(p.calls.filter(Array.isArray),[['surface','S2']]);
});
