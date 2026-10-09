'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict');
const {api,settle,deferred,identity,source,player,fixture}=require('./music-playback-test-harness.cjs');

test('actual source identity is canonical u64 CAS, bounded length and lower-case complete hash',()=>{
  assert.equal(api.musicPlaybackIdentityValid(identity('A','18446744073709551615')),true);
  for(const value of ['0','01','18446744073709551616','1e2',1,''])assert.equal(api.musicPlaybackIdentityValid({...identity(),libraryRevision:value}),false);
  for(const value of [0,-1,10.5,150*1024*1024+1,NaN,Infinity])assert.equal(api.musicPlaybackIdentityValid({...identity(),byteLength:value}),false);
  for(const value of ['a'.repeat(63),'A'.repeat(64),'z'.repeat(64),''])assert.equal(api.musicPlaybackIdentityValid({...identity(),sha256:value}),false);
  assert.equal(api.musicPlaybackSameTrack(identity('A','1'),identity('A','2')),true);
  assert.equal(api.musicPlaybackSameSource(source('A','1'),source('A','2')),false);
});
test('explicit selection prepares paused and state/time come from actual transport callbacks',async()=>{
  const f=fixture();await f.model.select(identity());await settle();
  assert.deepEqual(f.players[0].calls,['load','prepare']);assert.equal(f.model.view().phase,'prepared');assert.equal(f.model.view().durationMs,9000);
  assert.equal(api.musicPlaybackCanControl(f.model.view()),true);await f.model.play();assert.equal(f.model.view().phase,'playing');
  await f.model.pause();assert.equal(f.model.view().phase,'paused');await f.model.toggle();assert.equal(f.model.view().phase,'playing');
});
test('select with explicit play prepares before play and does not fabricate seek position acknowledgement',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();assert.deepEqual(f.players[0].calls,['load','prepare','play']);
  await f.model.seek(-5);await f.model.seek(90000);await f.model.seek(NaN);await f.model.seek(Infinity);
  assert.deepEqual(f.players[0].calls.filter(Array.isArray),[['seek',0],['seek',9000]]);assert.equal(f.model.view().positionMs,0);
  f.players[0].listeners.position(7000);assert.equal(f.model.view().positionMs,7000);
});
test('rapid play then pause preserves latest explicit intent without a queued unwanted play',async()=>{
  const f=fixture();await f.model.select(identity());await settle();await Promise.all([f.model.play(),f.model.pause()]);
  assert.deepEqual(f.players[0].calls,['load','prepare']);assert.equal(f.model.view().phase,'prepared');
});
test('a revision-only library update retains the actual same owned player and original acquisition CAS',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();await f.model.select(identity('A','2'),true);
  assert.equal(f.sources.length,1);assert.equal(f.model.view().libraryRevision,'1');assert.equal(f.model.view().phase,'playing');
  f.players[0].listeners.position(700);assert.equal(f.model.view().positionMs,700);
});
test('source byte/hash/import change with same stable track ID requires a separately acquired verified lease',async()=>{
  const f=fixture({acquire:async id=>Object.assign(source(id.trackId,id.libraryRevision),id)});
  await f.model.select(identity());await settle();const changed=identity('A','2');changed.sha256='c'.repeat(64);
  await f.model.select(changed);await settle();assert.equal(f.sources.length,2);assert.equal(f.players[0].calls.at(-1),'release');
  assert.equal(f.owned.has(f.sources[0]),false);assert.equal(f.model.view().libraryRevision,'2');
});
test('next and previous acquire latest exact playlist identities after releasing old player and lease',async()=>{
  const navigation=[],f=fixture({adjacent:(current,d)=>{navigation.push([current.trackId,current.libraryRevision,d]);return identity(d===1?'B':'A','9');}});
  await f.model.select(identity(),true);await settle();await f.model.next();await settle();await f.model.previous();await settle();
  assert.deepEqual(navigation,[['A','1',1],['B','9',-1]]);assert.equal(f.model.view().trackId,'A');
  assert.deepEqual(f.calls,[['acquire','A','1'],['create','lease-A'],['release-source','lease-A'],['acquire','B','9'],['create','lease-B'],['release-source','lease-B'],['acquire','A','9'],['create','lease-A']]);
});
test('completion advances once through playlist and stale old completion cannot advance new track',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();const old=f.players[0];old.listeners.state('completed');old.listeners.state('completed');
  await settle();assert.equal(f.sources.length,2);assert.equal(f.model.view().trackId,'B');old.listeners.state('completed');await settle();assert.equal(f.sources.length,2);
});
test('completed paused selection or absent next track never starts another source',async()=>{
  for(const play of [false,true]){const f=fixture({adjacent:()=>undefined});await f.model.select(identity(),play);await settle();
    f.players[0].listeners.state('completed');await settle();assert.equal(f.sources.length,1);assert.equal(f.model.view().phase,'completed');assert.equal(f.model.view().positionMs,9000);}
});
test('background during accepted play serializes a pause and foreground never resumes automatically',async()=>{
  const p=player(),gate=deferred(),f=fixture({create:()=>p});p.play=async()=>{p.calls.push('play');await gate.promise;p.listeners.state('playing');};
  await f.model.select(identity());await settle();const playing=f.model.play();await settle();const hidden=f.model.pauseForBackground();gate.resolve();
  await Promise.all([playing,hidden]);await settle();assert.deepEqual(p.calls,['load','prepare','play','pause']);assert.equal(f.model.view().background,true);
  f.model.foreground();await settle();assert.equal(f.model.view().phase,'paused');assert.equal(p.calls.filter(x=>x==='play').length,1);
  await f.model.play();assert.equal(p.calls.filter(x=>x==='play').length,2);
});
test('background before delayed preparation blocks autoplay, seek, navigation and late completed next',async()=>{
  const p=player(),gate=deferred(),f=fixture({create:()=>p});p.prepare=async()=>{p.calls.push('prepare');await gate.promise;p.listeners.duration(9000);p.listeners.state('prepared');};
  await f.model.select(identity(),true);await settle();const hidden=f.model.pauseForBackground();gate.resolve();await hidden;await settle();
  await f.model.play();await f.model.seek(100);await f.model.next();p.listeners.state('completed');await settle();
  assert.deepEqual(p.calls,['load','prepare']);assert.equal(f.sources.length,1);f.model.foreground();await settle();assert.equal(p.calls.includes('play'),false);
});
test('owner invalidation rejects controls and callbacks while explicit dispose still releases exact owned resources',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();f.owner=false;const old=f.model.view();
  f.players[0].listeners.position(1000);f.players[0].listeners.state('completed');f.players[0].listeners.error('late');await f.model.next();await f.model.play();await f.model.seek(500);
  assert.equal(f.model.view().positionMs,old.positionMs);assert.equal(f.sources.length,1);await f.model.dispose();assert.equal(f.owned.size,0);
});
test('superseded pending acquire releases actual stale lease without creating or loading a player',async()=>{
  const gate=deferred(),f=fixture({acquire:async id=>id.trackId==='A'?gate.promise:source('B','2')});
  const a=f.model.select(identity());await settle();const b=f.model.select(identity('B','2'));gate.resolve(source());await Promise.all([a,b]);await settle();
  assert.equal(f.players.length,1);assert.equal(f.model.view().trackId,'B');assert.deepEqual(f.calls.slice(0,3),[['acquire','A','1'],['release-source','lease-A'],['acquire','B','2']]);
});
test('close while awaiting acquire retains and releases that exact lease before resolving',async()=>{
  const gate=deferred(),f=fixture({acquire:()=>gate.promise});const opening=f.model.select(identity());await settle();let closed=false;
  const close=f.model.close().then(()=>closed=true);await settle();assert.equal(closed,false);gate.resolve(source());await Promise.all([opening,close]);
  assert.equal(f.players.length,0);assert.equal(f.owned.size,0);assert.equal(f.model.view().phase,'idle');
});
test('late factory after owner loss is released without loading before the owned source is released',async()=>{
  const gate=deferred(),p=player(),f=fixture({create:()=>gate.promise});const opening=f.model.select(identity());await settle();f.owner=false;
  gate.resolve(p);await opening;assert.deepEqual(p.calls,['release']);assert.equal(f.owned.size,0);
});
test('close waits accepted native load and play before release, and callbacks cannot reopen closing session',async()=>{
  const p=player(),gate=deferred(),f=fixture({create:()=>p});p.play=async()=>{p.calls.push('play');await gate.promise;p.listeners.state('playing');};
  await f.model.select(identity());await settle();const play=f.model.play();await settle();const close=f.model.close();await settle();
  assert.equal(p.calls.includes('release'),false);assert.equal(f.model.view().phase,'closing');gate.resolve();await Promise.all([play,close]);
  assert.deepEqual(p.calls.slice(-2),['play','release']);assert.equal(f.model.view().phase,'idle');
});
test('failed player release retains source, forbids select and close replay, retries only same lease explicitly',async()=>{
  const f=fixture();await f.model.select(identity());await settle();const p=f.players[0];let attempts=0;p.release=async()=>{p.calls.push('release');if(++attempts===1)throw new Error('retained');};
  await assert.rejects(f.model.close(),/retained/);assert.equal(f.model.view().phase,'cleanup_failed');assert.equal(f.owned.size,1);assert.equal(f.calls.some(c=>c[0]==='release-source'),false);
  await assert.rejects(f.model.select(identity('B','2')),/重试关闭/);await assert.rejects(f.model.close(),/显式重试/);assert.equal(attempts,1);
  await f.model.retryClose();assert.equal(attempts,2);assert.equal(f.owned.size,0);assert.equal(f.model.view().phase,'idle');
});
test('source cleanup failure never re-releases successful player, retries same source object without minting token',async()=>{
  let attempts=0,seen;const f=fixture({releaseSource:async s=>{if(seen)assert.equal(s,seen);seen=s;if(++attempts===1)throw new Error('private lease retained');}});
  await f.model.select(identity());await settle();await assert.rejects(f.model.dispose(),/retained/);assert.equal(f.owned.size,1);await f.model.retryClose();
  assert.equal(f.players[0].calls.filter(x=>x==='release').length,1);assert.equal(attempts,2);assert.equal(f.sources.length,1);await f.model.select(identity());assert.equal(f.sources.length,1);
});
test('concurrent close callers share cleanup and never turn a failed first close into implicit retry',async()=>{
  const f=fixture();await f.model.select(identity());await settle();const gate=deferred();f.players[0].release=async()=>{f.players[0].calls.push('release');await gate.promise;throw new Error('retained');};
  const a=f.model.close(),b=f.model.close();await settle();gate.resolve();const results=await Promise.allSettled([a,b]);
  assert.equal(results.every(r=>r.status==='rejected'),true);assert.equal(f.players[0].calls.filter(x=>x==='release').length,1);assert.equal(f.owned.size,1);
});
test('already queued replacement cannot bypass a cleanup failure or acquire another lease',async()=>{
  const f=fixture();await f.model.select(identity());await settle();const gate=deferred();f.players[0].release=async()=>{await gate.promise;throw new Error('retained');};
  const a=f.model.select(identity('B','2'));await settle();const b=f.model.select(identity('A','3'));gate.resolve();await Promise.allSettled([a,b]);
  assert.equal(f.sources.length,1);assert.equal(f.model.view().phase,'cleanup_failed');
});
test('malformed or unowned acquisition never creates AVPlayer and retains its exact acquired object for close',async()=>{
  const f=fixture({acquire:()=>source('B','1')});await assert.rejects(f.model.select(identity()),/原请求/);assert.equal(f.players.length,0);assert.equal(f.model.view().phase,'failed');await f.model.close();assert.equal(f.owned.size,0);
  const g=fixture();g.hooks.ownsSource=()=>false;await assert.rejects(g.model.select(identity()),/原请求/);assert.equal(g.players.length,0);await g.model.close();assert.equal(g.owned.size,0);
});
test('source mutations and stale tokens cannot issue controls, while view and identity snapshots cannot alter active session',async()=>{
  const f=fixture(),id=identity();await f.model.select(id);await settle();id.trackId='B';const v=f.model.view();v.token='other';v.phase='playing';
  await f.model.play('old-token');assert.deepEqual(f.players[0].calls,['load','prepare']);assert.equal(f.model.view().trackId,'A');
  f.sources[0].sha256='c'.repeat(64);f.players[0].listeners.position(700);await f.model.play();assert.equal(f.model.view().positionMs,0);assert.equal(f.players[0].calls.includes('play'),false);await f.model.close();
});
test('prepare or transport error is sticky until explicit new selection and never recovered by late prepared callback',async()=>{
  const p=player(),f=fixture({create:()=>p});p.prepare=async()=>{p.calls.push('prepare');throw new Error('unsupported codec');};
  await f.model.select(identity(),true);await settle();assert.equal(f.model.view().phase,'failed');assert.equal(f.model.view().error,'unsupported codec');
  p.listeners.state('prepared');p.listeners.duration(99999);await f.model.play();assert.equal(f.model.view().phase,'failed');assert.deepEqual(p.calls,['load','prepare']);await f.model.close();
});
test('disposed coordinator never accepts new selection but failed cleanup remains explicitly recoverable',async()=>{
  const f=fixture();await f.model.dispose();await f.model.select(identity(),true);assert.equal(f.sources.length,0);assert.equal(f.model.view().phase,'idle');
});
test('failed command after real playing acknowledgement pauses sound while retaining the original failure and lease',async()=>{
  const p=player(),f=fixture({create:()=>p});p.play=async()=>{p.calls.push('play');p.listeners.state('playing');throw new Error('play acknowledgement rejected');};
  await f.model.select(identity());await settle();await f.model.play();await settle();
  assert.deepEqual(p.calls,['load','prepare','play','pause']);assert.equal(f.model.view().phase,'failed');assert.equal(f.model.view().error,'play acknowledgement rejected');assert.equal(f.owned.size,1);
  p.listeners.state('playing');await settle();assert.equal(p.calls.filter(x=>x==='pause').length,2);assert.equal(f.model.view().phase,'failed');await f.model.close();
});
test('manual selection, navigation and end all await confirmed selected identity before acquiring new CAS revision',async()=>{
  let revision=10;const f=fixture({admitSelection:async id=>Object.assign(identity(id.trackId,String(++revision)),{sha256:id.sha256,importOperation:id.importOperation,byteLength:id.byteLength})});
  await f.model.select(identity(),true);await settle();assert.equal(f.sources[0].libraryRevision,'11');await f.model.next();await settle();
  assert.equal(f.sources[1].libraryRevision,'12');f.players[1].listeners.state('completed');await settle();assert.equal(f.sources[2].libraryRevision,'13');assert.equal(f.admissions.length,3);
});
test('Unknown/rejected selection admission never acquires or plays, and player does not retry persisted operation itself',async()=>{
  let attempts=0;const f=fixture({admitSelection:async()=>{attempts++;throw new Error('selected operation Unknown');}});
  await assert.rejects(f.model.select(identity(),true),/Unknown/);await settle();assert.equal(attempts,1);assert.equal(f.sources.length,0);assert.equal(f.model.view().phase,'failed');
  await f.model.play();await f.model.next();assert.equal(attempts,1);
});
test('foreign admitted source identity is rejected before export, even if a fresh revision is valid',async()=>{
  const f=fixture({admitSelection:async()=>identity('B','2')});await assert.rejects(f.model.select(identity()),/原曲目/);assert.equal(f.sources.length,0);
});
test('late successful selection admission preserves its external fact but owner loss cannot acquire playback source',async()=>{
  const gate=deferred(),f=fixture({admitSelection:()=>gate.promise});const selecting=f.model.select(identity(),true);await settle();f.owner=false;
  gate.resolve(identity('A','2'));await selecting;assert.equal(f.admissions.length,1);assert.equal(f.sources.length,0);
});
test('same stable owned track admits persisted selected revision while preserving original FD and lease CAS',async()=>{
  let revision=1;const f=fixture({admitSelection:async id=>identity(id.trackId,String(revision++))});await f.model.select(identity(),true);await settle();await f.model.select(identity('A','2'),true);await settle();
  assert.equal(f.admissions.length,2);assert.equal(f.sources.length,1);assert.equal(f.model.view().libraryRevision,'1');assert.equal(f.model.view().phase,'playing');
});
test('mutating admission or acquire argument cannot substitute a foreign source for the immutable requested identity',async()=>{
  const f=fixture({admitSelection:async id=>{id.trackId='B';return id;}});await assert.rejects(f.model.select(identity()),/原曲目/);assert.equal(f.sources.length,0);
  const g=fixture({acquire:async id=>{id.trackId='B';return Object.assign(source('B','1'),id);}});await assert.rejects(g.model.select(identity()),/原请求/);assert.equal(g.players.length,0);await g.model.close();
});
test('lost current lease during same-track admission cannot overwrite it with a new acquired lease',async()=>{
  const f=fixture();await f.model.select(identity(),true);await settle();const gate=deferred();f.options.admitSelection=()=>gate.promise;
  const selecting=f.model.select(identity('A','2'),true);await settle();f.sources[0].sha256='c'.repeat(64);gate.resolve(identity('A','2'));
  await assert.rejects(selecting,/原歌曲租约/);assert.equal(f.sources.length,1);assert.equal(f.model.view().phase,'failed');await f.model.close();
});
test('registered source still requires a typed nonempty bounded private lease token',async()=>{
  for(const token of [1,'','bad token','x'.repeat(257)]){
    const f=fixture({acquire:async()=>Object.assign(source(),{token})});await assert.rejects(f.model.select(identity()),/原请求/);assert.equal(f.players.length,0);await f.model.close();}
});
test('latest explicit play cancels a queued pause behind the in-flight accepted play',async()=>{
  const gate=deferred(),p=player(),f=fixture({create:()=>p});p.play=async()=>{p.calls.push('play');await gate.promise;p.listeners.state('playing');};
  await f.model.select(identity());await settle();const first=f.model.play();await settle();const pausing=f.model.pause(),latest=f.model.play();
  gate.resolve();await Promise.all([first,pausing,latest]);await settle();assert.deepEqual(p.calls,['load','prepare','play']);assert.equal(f.model.view().phase,'playing');await f.model.close();
});
