'use strict';
// Real coordinator + real PlatformMusicPlayer; SDK/files callbacks below are
// controlled host providers and are not a claim of device playback success.
const {test}=require('node:test'),assert=require('node:assert/strict');
const {api,settle,deferred,identity,platformFixture}=require('./music-playback-test-harness.cjs');
function integrated(options={}) {
  const f=platformFixture(options),trace=f.calls;let owned=true,owner=true,acquired=0,released=0;
  f.player.prepare=async()=>{trace.push('prepare');f.listeners.stateChange('prepared');};
  f.player.play=async()=>{trace.push('play');f.listeners.stateChange('playing');};
  f.player.pause=async()=>{trace.push('pause');f.listeners.stateChange('paused');};
  const hooks={admitSelection:async id=>api.musicPlaybackIdentityCopy(id),acquire:async()=>{acquired++;return f.source;},
    ownsSource:s=>s===f.source&&owned,createPlayer:s=>f.platform.createMusicPlayer(s,f.files),
    releaseSource:async s=>{assert.equal(s,f.source);assert.equal(trace.at(-1),'fd-close');trace.push('release-source');released++;owned=false;},
    adjacent:()=>undefined,isCurrentOwner:()=>owner,changed:()=>{}};
  const model=new api.MusicPlayback(hooks);return {f,model,trace,acquired:()=>acquired,released:()=>released,owned:()=>owned,setOwner:value=>owner=value};
}
test('real coordinator/platform composition receives actual state callbacks and releases player then FD then exact source',async()=>{
  const h=integrated();await h.model.select(identity(),true);await settle();assert.equal(h.model.view().phase,'playing');
  await h.model.seek(1200);assert.equal(h.model.view().positionMs,0);h.f.listeners.timeUpdate(1200);assert.equal(h.model.view().positionMs,1200);
  await h.model.pauseForBackground();h.model.foreground();await settle();assert.equal(h.model.view().phase,'paused');
  await h.model.close();assert.deepEqual(h.trace.slice(-3),['player-release','fd-close','release-source']);assert.equal(h.released(),1);assert.equal(h.model.view().phase,'idle');
});
test('real platform release failure propagates cleanup retention through coordinator and same-source explicit retry',async()=>{
  const h=integrated({releaseFailure:true});await h.model.select(identity());await settle();await assert.rejects(h.model.close(),/rejected/);
  assert.equal(h.released(),0);assert.equal(h.owned(),true);assert.equal(h.trace.includes('fd-close'),false);assert.equal(h.model.view().phase,'cleanup_failed');
  await assert.rejects(h.model.select(identity('B','2')),/重试关闭/);assert.equal(h.acquired(),1);h.f.options.releaseFailure=false;await h.model.retryClose();
  assert.deepEqual(h.trace.slice(-3),['player-release','fd-close','release-source']);assert.equal(h.released(),1);
});
test('real failed FD close retains the exact descriptor and coordinator retry does not release AVPlayer a second time',async()=>{
  const h=integrated({closeFailure:true});await h.model.select(identity());await settle();await assert.rejects(h.model.dispose(),/FD close/);
  assert.equal(h.released(),0);h.f.options.closeFailure=false;await h.model.retryClose();assert.equal(h.trace.filter(c=>c==='player-release').length,1);
  assert.equal(h.trace.filter(c=>c==='fd-close').length,2);assert.equal(h.released(),1);await h.model.select(identity());assert.equal(h.acquired(),1);
});

test('real platform is paused after caller already lost foreground owner flag and stable FD is reused explicitly on return',async()=>{
  const h=integrated();await h.model.select(identity(),true);await settle();h.setOwner(false);await h.model.pauseForBackground();
  assert.equal(h.trace.at(-1),'pause');assert.equal(h.model.view().phase,'paused');assert.equal(h.released(),0);assert.equal(h.owned(),true);
  h.setOwner(true);h.model.foreground();await settle();assert.equal(h.trace.filter(x=>x==='play').length,1);
  await h.model.play();assert.equal(h.trace.filter(x=>x==='play').length,2);assert.equal(h.trace.filter(x=>x==='open-owned').length,1);
  assert.equal(h.acquired(),1);await h.model.close();assert.deepEqual(h.trace.slice(-3),['player-release','fd-close','release-source']);
});

test('real platform delayed owned open across leave and return cannot autoplay and closes AVPlayer then same FD then lease',async()=>{
  const gate=deferred(),h=integrated({open:()=>gate.promise});const opening=h.model.select(identity(),true);await settle();
  const hiding=h.model.pauseForBackground();h.model.foreground();gate.resolve(h.f.descriptor);await Promise.all([opening,hiding]);await settle();
  assert.equal(h.trace.includes('prepare'),false);assert.equal(h.trace.includes('play'),false);
  assert.deepEqual(h.trace.slice(-3),['player-release','fd-close','release-source']);assert.equal(h.acquired(),1);assert.equal(h.released(),1);
  assert.equal(h.model.view().phase,'idle');
});

test('real late platform playing callback after return triggers actual pause without re-opening or resuming',async()=>{
  const gate=deferred(),h=integrated();h.f.player.play=async()=>{h.trace.push('play');await gate.promise;h.f.listeners.stateChange('playing');};
  await h.model.select(identity());await settle();const playing=h.model.play();await settle();
  const hiding=h.model.pauseForBackground();h.model.foreground();gate.resolve();await Promise.all([playing,hiding]);await settle();
  assert.equal(h.trace.at(-1),'pause');assert.equal(h.trace.filter(x=>x==='play').length,1);assert.equal(h.model.view().phase,'paused');
  assert.equal(h.acquired(),1);assert.equal(h.released(),0);await h.model.close();
});
