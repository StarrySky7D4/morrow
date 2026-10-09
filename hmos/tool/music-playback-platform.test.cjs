'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict');
const {settle,deferred,source,platformFixture}=require('./music-playback-test-harness.cjs');

test('actual AVPlayer adapter obtains owned FD and stat extent, never opens a path, and forwards platform callbacks',async()=>{
  const f=platformFixture(),p=await f.platform.createMusicPlayer(f.source,f.files),states=[],times=[],durations=[];
  p.onState(s=>states.push(s));p.onPosition(v=>times.push(v));p.onDuration(v=>durations.push(v));await p.load();
  assert.deepEqual(JSON.parse(JSON.stringify(f.calls)),['open-owned',['stat',77],'player-create',['fdSrc',{fd:77,offset:0,length:10}]]);
  f.listeners.stateChange('prepared');f.listeners.timeUpdate(1200);f.listeners.durationUpdate(8000);assert.deepEqual(states,['initialized','prepared']);assert.deepEqual(times,[0,1200]);assert.deepEqual(durations,[9000,8000]);
  await p.prepare();await p.play();await p.pause();await p.seek(1234.4);await p.release();assert.deepEqual(f.calls.slice(-6),['prepare','play','pause',['seek',1234,2],'player-release','fd-close']);
});
test('actual adapter rejects foreign source or arbitrary path-shaped JSON without any file or player operation',async()=>{
  const f=platformFixture();for(const value of [source(),{token:'x',path:'/any',uri:'file:///any',byteLength:10}]){
    const p=await f.platform.createMusicPlayer(value,f.files);await assert.rejects(p.load(),/租约/);await p.release();}
  assert.deepEqual(f.calls,[]);
});
test('actual descriptor must match private object identity and exact token/hash/length/nonnegative FD',async()=>{
  for(const alter of [d=>d.token='other',d=>d.sha256='b'.repeat(64),d=>d.byteLength=11,d=>d.fd=-1,d=>d.fd=2.2]){
    const f=platformFixture();alter(f.descriptor);const p=await f.platform.createMusicPlayer(f.source,f.files);await assert.rejects(p.load(),/描述符/);
    assert.equal(f.calls.includes('player-create'),false);await p.release();assert.equal(f.calls.at(-1),'fd-close');
  }
  const f=platformFixture();f.descriptorOwned=false;const p=await f.platform.createMusicPlayer(f.source,f.files);await assert.rejects(p.load(),/描述符/);assert.equal(f.calls.includes('player-create'),false);
});
test('actual file stat must prove regular file and exact exported length before AVPlayer creation',async()=>{
  for(const options of [{wrongLength:true},{notFile:true}]){
    const f=platformFixture(options),p=await f.platform.createMusicPlayer(f.source,f.files);await assert.rejects(p.load(),/长度/);assert.equal(f.calls.includes('player-create'),false);await p.release();assert.equal(f.calls.at(-1),'fd-close');}
});
test('player creation failure retains actual descriptor for explicit release',async()=>{
  const f=platformFixture({createFailure:true}),p=await f.platform.createMusicPlayer(f.source,f.files);await assert.rejects(p.load(),/create failed/);
  assert.equal(f.calls.includes('fd-close'),false);await p.release();assert.equal(f.calls.at(-1),'fd-close');
});
test('AVPlayer release rejection retains same descriptor and only explicit retry closes it',async()=>{
  const f=platformFixture({releaseFailure:true}),p=await f.platform.createMusicPlayer(f.source,f.files);await p.load();await assert.rejects(p.release(),/rejected/);
  assert.equal(f.calls.includes('fd-close'),false);f.options.releaseFailure=false;await p.release();assert.deepEqual(f.calls.slice(-2),['player-release','fd-close']);
});
test('failed descriptor close retries exact FD object without releasing already released AVPlayer twice',async()=>{
  const f=platformFixture({closeFailure:true}),p=await f.platform.createMusicPlayer(f.source,f.files);await p.load();await assert.rejects(p.release(),/FD close/);
  f.options.closeFailure=false;await p.release();assert.equal(f.calls.filter(c=>c==='player-release').length,1);assert.equal(f.calls.filter(c=>c==='fd-close').length,2);
});
test('late platform state/time/error callbacks after close starts cannot escape callback generation guard',async()=>{
  const gate=deferred(),f=platformFixture({release:()=>gate.promise}),p=await f.platform.createMusicPlayer(f.source,f.files),events=[];
  p.onState(s=>events.push(s));p.onPosition(v=>events.push(v));p.onDuration(v=>events.push(v));p.onError(v=>events.push(v));await p.load();
  const closing=p.release();await settle();f.listeners.stateChange('playing');f.listeners.timeUpdate(77);f.listeners.durationUpdate(77);f.listeners.error(new Error('late'));
  assert.deepEqual(events,['initialized']);gate.resolve();await closing;
});
test('actual source or descriptor ownership revocation blocks callbacks and every subsequent control',async()=>{
  for(const revoke of [f=>f.owned=false,f=>f.descriptorOwned=false,f=>f.source.sha256='b'.repeat(64)]){
    const f=platformFixture(),p=await f.platform.createMusicPlayer(f.source,f.files),events=[];p.onState(s=>events.push(s));p.onError(e=>events.push(e));await p.load();revoke(f);
    f.listeners.stateChange('playing');f.listeners.error(new Error('ignored'));assert.deepEqual(events,['initialized']);
    for(const method of ['prepare','play','pause'])await assert.rejects(p[method](),/无法控制/);await assert.rejects(p.seek(100),/无法控制/);await p.release();
  }
});
test('closing during pending owned open awaits original descriptor and prevents player creation',async()=>{
  const gate=deferred(),f=platformFixture({open:()=>gate.promise}),p=await f.platform.createMusicPlayer(f.source,f.files);
  const opening=p.load();await settle();let closed=false;const close=p.release().then(()=>closed=true);await settle();assert.equal(closed,false);
  gate.resolve(f.descriptor);await assert.rejects(opening,/描述符/);await close;assert.deepEqual(f.calls,['open-owned','fd-close']);
});
test('closing during pending player creation awaits actual player then releases it before FD close',async()=>{
  const gate=deferred(),f=platformFixture({create:()=>gate.promise}),p=await f.platform.createMusicPlayer(f.source,f.files);
  const opening=p.load();await settle();const close=p.release();await settle();assert.equal(f.calls.includes('fd-close'),false);
  gate.resolve(f.player);await assert.rejects(opening,/来源已失效/);await close;assert.deepEqual(f.calls.slice(-2),['player-release','fd-close']);assert.equal(f.calls.some(c=>Array.isArray(c)&&c[0]==='fdSrc'),false);
});
test('simultaneous platform release callers share original cleanup, including rejection',async()=>{
  const gate=deferred(),f=platformFixture({release:()=>gate.promise}),p=await f.platform.createMusicPlayer(f.source,f.files);await p.load();
  const a=p.release(),b=p.release();assert.equal(a,b);await settle();gate.reject(new Error('busy'));const result=await Promise.allSettled([a,b]);
  assert.equal(result.every(r=>r.status==='rejected'),true);assert.equal(f.calls.filter(c=>c==='player-release').length,1);assert.equal(f.calls.includes('fd-close'),false);
});
test('load cannot be repeated or reopened after release, and invalid seek is never dispatched',async()=>{
  const f=platformFixture(),p=await f.platform.createMusicPlayer(f.source,f.files);await p.load();await assert.rejects(p.load(),/已打开/);
  for(const value of [-1,NaN,Infinity])await assert.rejects(p.seek(value),/位置无效/);await p.release();await assert.rejects(p.play(),/无法控制/);await assert.rejects(p.load(),/已打开/);
  assert.equal(f.calls.filter(c=>c==='open-owned').length,1);
});
