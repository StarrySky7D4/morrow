'use strict';
// Exercise the actual MusicPlayback ETS with controlled transports. These are
// foreground/lease invariants; they do not prove an AVPlayer codec or device.
const {test}=require('node:test'),assert=require('node:assert/strict');
const {settle,deferred,identity,source,player,fixture}=require('./music-playback-test-harness.cjs');

test('background really pauses its exact playing player after caller owner already became false',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();const original=f.sources[0],p=f.players[0];
  f.owner=false;await f.model.pauseForBackground();assert.deepEqual(p.calls,['load','prepare','play','pause']);
  assert.equal(f.model.view().phase,'paused');assert.equal(f.model.view().background,true);
  f.owner=true;f.model.foreground();await settle();assert.equal(p.calls.filter(x=>x==='play').length,1);
  assert.equal(f.sources[0],original);assert.equal(f.owned.has(original),true);await f.model.play();
  assert.equal(p.calls.filter(x=>x==='play').length,2);assert.equal(f.players.length,1);await f.model.close();
});

test('foreground returns before late accepted play acknowledgement and old sound is still paused',async()=>{
  const p=player(),gate=deferred(),f=fixture({create:()=>p});
  p.play=async()=>{p.calls.push('play');await gate.promise;p.listeners.state('playing');};
  await f.model.select(identity());await settle();const playing=f.model.play();await settle();
  const hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve();await Promise.all([playing,hiding]);await settle();
  assert.deepEqual(p.calls,['load','prepare','play','pause']);assert.equal(f.model.view().phase,'paused');
  assert.equal(f.model.view().background,false);assert.equal(f.owned.size,1);await f.model.close();
});

test('new explicit foreground play follows mandatory old-play pause while old queued seek is cancelled',async()=>{
  const p=player(),gate=deferred(),f=fixture({create:()=>p});let plays=0;
  p.play=async()=>{p.calls.push('play');if(++plays===1)await gate.promise;p.listeners.state('playing');};
  await f.model.select(identity());await settle();const playing=f.model.play();await settle();
  const seeking=f.model.seek(4000),hiding=f.model.pauseForBackground();f.model.foreground();const newPlay=f.model.play();
  gate.resolve();await Promise.all([playing,seeking,hiding,newPlay]);await settle();
  assert.deepEqual(p.calls,['load','prepare','play','pause','play']);assert.equal(f.model.view().phase,'playing');
  assert.equal(f.players.length,1);assert.equal(f.sources.length,1);await f.model.close();
});

test('queued old play behind accepted seek cannot dispatch after leave and return, but explicit new play can',async()=>{
  const p=player(),gate=deferred(),f=fixture({create:()=>p});
  p.seek=async value=>{p.calls.push(['seek',value]);await gate.promise;};
  await f.model.select(identity());await settle();const seeking=f.model.seek(2000);await settle();
  const playing=f.model.play(),hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve();
  await Promise.all([seeking,playing,hiding]);await settle();assert.deepEqual(p.calls,['load','prepare',['seek',2000]]);
  assert.equal(f.model.view().phase,'prepared');await f.model.play();await settle();
  assert.deepEqual(p.calls,['load','prepare',['seek',2000],'play']);assert.equal(f.model.view().phase,'playing');await f.model.close();
});

test('late accepted seek rejection after leave and return does not fail or replace stable paused player',async()=>{
  const p=player(),gate=deferred(),f=fixture({create:()=>p});p.seek=async value=>{p.calls.push(['seek',value]);await gate.promise;};
  await f.model.select(identity(),true);await settle();const seeking=f.model.seek(1000);await settle();
  const hiding=f.model.pauseForBackground();f.model.foreground();gate.reject(new Error('old seek rejected'));
  await Promise.all([seeking,hiding]);await settle();assert.equal(f.model.view().phase,'paused');assert.equal(f.model.view().error,'');
  assert.equal(f.owned.size,1);assert.equal(f.sources.length,1);await f.model.play();assert.equal(f.model.view().phase,'playing');await f.model.close();
});

test('late successful admission after leave and return cannot acquire or continue prior autoplay request',async()=>{
  const gate=deferred(),f=fixture({admitSelection:()=>gate.promise});const opening=f.model.select(identity(),true);await settle();
  const hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve(identity('A','2'));await Promise.all([opening,hiding]);await settle();
  assert.equal(f.admissions.length,1);assert.equal(f.sources.length,0);assert.equal(f.players.length,0);assert.equal(f.model.view().phase,'idle');
  f.options.admitSelection=undefined;await f.model.select(identity(),false);await settle();assert.equal(f.players.length,1);assert.equal(f.model.view().phase,'prepared');await f.model.close();
});

test('late same-track admission retains stable player but cancels old play intent after leave and return',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();const original=f.sources[0],gate=deferred();
  f.options.admitSelection=()=>gate.promise;const selecting=f.model.select(identity('A','2'),true);await settle();
  const hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve(identity('A','2'));await Promise.all([selecting,hiding]);await settle();
  assert.equal(f.sources.length,1);assert.equal(f.sources[0],original);assert.equal(f.players[0].calls.filter(x=>x==='play').length,1);
  assert.equal(f.model.view().phase,'paused');await f.model.play();assert.equal(f.model.view().phase,'playing');await f.model.close();
});

test('late same-track admission rejection cannot overwrite retained paused session after leave and return',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();const gate=deferred();f.options.admitSelection=()=>gate.promise;
  const selecting=f.model.select(identity('A','2'),true);await settle();const hiding=f.model.pauseForBackground();f.model.foreground();
  gate.reject(new Error('old selection Unknown'));await assert.rejects(selecting,/Unknown/);await hiding;await settle();
  assert.equal(f.model.view().phase,'paused');assert.equal(f.model.view().error,'');assert.equal(f.sources.length,1);await f.model.close();
});

test('late acquired source after leave and return is released as original owned object without a player',async()=>{
  const gate=deferred(),f=fixture({acquire:()=>gate.promise});const opening=f.model.select(identity(),true);await settle();
  const hiding=f.model.pauseForBackground();f.model.foreground();const original=source();gate.resolve(original);await Promise.all([opening,hiding]);await settle();
  assert.equal(f.sources[0],original);assert.equal(f.players.length,0);assert.equal(f.owned.has(original),false);
  assert.deepEqual(f.calls,[['acquire','A','1'],['release-source','lease-A']]);assert.equal(f.model.view().phase,'idle');
});

test('late created player after leave and return is released before its exact source without loading',async()=>{
  const gate=deferred(),p=player(),f=fixture({create:()=>gate.promise});const opening=f.model.select(identity(),true);await settle();
  const hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve(p);await Promise.all([opening,hiding]);await settle();
  assert.deepEqual(p.calls,['release']);assert.equal(f.owned.size,0);assert.equal(f.model.view().phase,'idle');
  assert.deepEqual(f.calls,[['acquire','A','1'],['create','lease-A'],['release-source','lease-A']]);
});

test('late load completion after leave and return cannot prepare or play and closes existing player/source',async()=>{
  const gate=deferred(),p=player(),f=fixture({create:()=>p});p.load=async()=>{p.calls.push('load');await gate.promise;p.listeners.state('initialized');};
  const opening=f.model.select(identity(),true);await settle();const hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve();
  await Promise.all([opening,hiding]);await settle();assert.deepEqual(p.calls,['load','release']);assert.equal(f.owned.size,0);assert.equal(f.model.view().phase,'idle');
});

test('late rejected factory after leave and return still closes its exact acquired source once',async()=>{
  const gate=deferred(),f=fixture({create:()=>gate.promise});const opening=f.model.select(identity(),true);await settle();
  const hiding=f.model.pauseForBackground();f.model.foreground();gate.reject(new Error('old factory rejected'));
  await assert.rejects(opening,/old factory/);await hiding;await settle();assert.equal(f.sources.length,1);assert.equal(f.players.length,0);
  assert.equal(f.owned.size,0);assert.equal(f.calls.filter(x=>x[0]==='release-source').length,1);assert.equal(f.model.view().phase,'idle');
});

test('late rejected load after leave and return releases actual player before original source without preparing',async()=>{
  const gate=deferred(),p=player(),f=fixture({create:()=>p});p.load=async()=>{p.calls.push('load');await gate.promise;};
  const opening=f.model.select(identity(),true);await settle();const hiding=f.model.pauseForBackground();f.model.foreground();
  gate.reject(new Error('old load rejected'));await assert.rejects(opening,/old load/);await hiding;await settle();
  assert.deepEqual(p.calls,['load','release']);assert.equal(f.owned.size,0);assert.equal(f.model.view().phase,'idle');
});

test('late rejected factory cleanup failure retains original source and does not retry inside rejection handler',async()=>{
  const gate=deferred();let attempts=0,original;const f=fixture({create:()=>gate.promise,releaseSource:async s=>{
    if(original)assert.equal(s,original);original=s;if(++attempts===1)throw new Error('factory source retained');}});
  const opening=f.model.select(identity(),true);await settle();const hiding=f.model.pauseForBackground();f.model.foreground();
  gate.reject(new Error('old factory rejected'));await assert.rejects(opening,/source retained/);await hiding;await settle();
  assert.equal(f.model.view().phase,'cleanup_failed');assert.equal(attempts,1);assert.equal(f.owned.has(original),true);
  await assert.rejects(f.model.close(),/显式重试/);assert.equal(attempts,1);await f.model.retryClose();assert.equal(attempts,2);assert.equal(f.owned.size,0);
});

test('already loaded player finishes late preparation in foreground without autoplay and explicit play remains available',async()=>{
  const gate=deferred(),p=player(),f=fixture({create:()=>p});p.prepare=async()=>{p.calls.push('prepare');await gate.promise;p.listeners.duration(9000);p.listeners.state('prepared');};
  await f.model.select(identity(),true);await settle();const hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve();await hiding;await settle();
  assert.deepEqual(p.calls,['load','prepare']);assert.equal(f.model.view().phase,'prepared');assert.equal(f.owned.size,1);
  await f.model.play();assert.equal(f.model.view().phase,'playing');assert.equal(f.players.length,1);await f.model.close();
});

test('same owned late-source cleanup failure is retained without replay and only retryClose can release it',async()=>{
  const gate=deferred();let attempts=0,original;const f=fixture({acquire:()=>gate.promise,releaseSource:async s=>{
    if(original)assert.equal(s,original);original=s;if(++attempts===1)throw new Error('old exact source retained');}});
  const opening=f.model.select(identity(),true);await settle();const hiding=f.model.pauseForBackground();f.model.foreground();gate.resolve(source());
  await assert.rejects(opening,/retained/);await hiding;await settle();assert.equal(f.model.view().phase,'cleanup_failed');assert.equal(f.owned.has(original),true);
  await assert.rejects(f.model.close(),/显式重试/);await assert.rejects(f.model.select(identity('B','2')),/重试关闭/);assert.equal(attempts,1);
  await f.model.retryClose();assert.equal(attempts,2);assert.equal(f.owned.size,0);assert.equal(f.model.view().phase,'idle');
});

test('selection while background never admits or exports and completed old playback never navigates after return',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();await f.model.pauseForBackground();await f.model.select(identity('B','2'),true);
  assert.equal(f.admissions.length,1);assert.equal(f.sources.length,1);f.model.foreground();f.players[0].listeners.state('completed');await settle();
  assert.equal(f.sources.length,1);assert.equal(f.model.view().phase,'completed');await f.model.close();
});

test('multiple background reports pause once and stale playing after return re-pauses without autoplay',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();await Promise.all([f.model.pauseForBackground(),f.model.pauseForBackground()]);
  assert.equal(f.players[0].calls.filter(x=>x==='pause').length,1);f.model.foreground();f.players[0].listeners.state('playing');await settle();
  assert.equal(f.players[0].calls.filter(x=>x==='pause').length,2);assert.equal(f.players[0].calls.filter(x=>x==='play').length,1);
  assert.equal(f.model.view().phase,'paused');await f.model.close();
});
