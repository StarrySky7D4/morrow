'use strict';
// Consume immutable actual development Store DTOs through actual playback
// identity/source APIs. This never obtains an FD, mints a lease or decodes the
// fixture's synthetic bytes, and does not qualify music UI or device playback.
const {test}=require('node:test'),assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {api,PLATFORM,load}=require('./music-playback-test-harness.cjs');
const fixturePath=path.resolve(__dirname,'../reports/ui-source/v29/music-store-fixture.json');
const bytes=fs.readFileSync(fixturePath);
assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),'da6d8df4f50f59555a4064ab29c9b302ce29b85a843deb03f2f5c9eb0d83b9b2');
const dto=JSON.parse(bytes.toString('utf8'));
function identity(track) {
  const value=new api.MusicPlaybackIdentity();value.trackId=track.track_id;value.libraryRevision=track.library_revision;
  value.importOperation=track.import_operation;value.byteLength=Number(track.byte_length);value.sha256=track.sha256;return value;
}
test('actual Store Ready immutable first track maps complete exact identity without borrowing internal ready operation',()=>{
  const reply=dto.ready_reply,track=reply.music.tracks[0],value=identity(track);
  assert.equal(reply.ok,true);assert.equal(reply.effect,'committed');assert.equal(track.phase,'ready');assert.equal(track.bytes_retained,true);
  assert.equal(api.musicPlaybackIdentityValid(value),true);assert.equal(value.importOperation,track.request.operation_id);
  assert.notEqual(value.importOperation,reply.music.operation_id);
  assert.equal(crypto.createHash('sha256').update(track.request_json).digest('hex'),track.request_sha256);
  assert.equal(value.byteLength.toString(),track.request.byte_length);assert.equal(value.sha256,track.request.sha256);
  assert.deepEqual(Object.keys(value).sort(),['byteLength','importOperation','libraryRevision','sha256','trackId']);
});
test('actual selected and later current track retain stable blob identity across confirmed library revisions',()=>{
  const selected=dto.selected_reply.music.tracks[0],current=dto.current_track_reply.music.tracks[0];
  assert.equal(dto.selected_reply.music.selected_track_id,selected.track_id);assert.equal(dto.current_track_reply.music.selected_track_id,current.track_id);
  assert.equal(selected.phase,'ready');assert.equal(current.phase,'ready');assert.notEqual(selected.library_revision,current.library_revision);
  assert.equal(api.musicPlaybackIdentityValid(identity(selected)),true);assert.equal(api.musicPlaybackIdentityValid(identity(current)),true);
  assert.equal(api.musicPlaybackSameTrack(identity(selected),identity(current)),true);
  const oldSource=api.musicPlaybackSourceCopy(identity(selected)),currentSource=api.musicPlaybackSourceCopy(identity(current));
  assert.equal(api.musicPlaybackSameSource(oldSource,currentSource),false);assert.equal(api.musicPlaybackSourceValid(oldSource),false);
});
test('actual full lexical pages supply both ready playlist identities while retaining pending and retired records separately',()=>{
  const records=dto.read_pages.flatMap(reply=>reply.music.tracks),header=dto.read_pages[0].music;
  assert.equal(records.length,18);assert.deepEqual(records.map(t=>t.track_id),records.map(t=>t.track_id).slice().sort());
  for(const reply of dto.read_pages){assert.equal(reply.effect,'not_committed');assert.equal(reply.receipt_revision,'');assert.equal(reply.music.library_revision,header.library_revision);assert.deepEqual(reply.music.order,header.order);}
  const ready=records.filter(t=>t.phase==='ready');assert.deepEqual(ready.map(t=>t.track_id).sort(),header.order.slice().sort());
  for(const track of ready){assert.equal(track.bytes_retained,true);assert.equal(api.musicPlaybackIdentityValid(identity(track)),true);}
  assert.ok(records.some(t=>t.phase==='pending'));assert.ok(records.some(t=>t.phase==='retired'));
  // Native snake-case records, including Ready, are not playable lease DTOs.
  for(const record of records)assert.equal(api.musicPlaybackSourceValid(record),false);
});
test('actual export hash and extent match current identity but its JSON response cannot mint a playback source',()=>{
  const track=dto.current_track_reply.music.tracks[0],value=identity(track),request=dto.export_request,reply=dto.export_reply;
  assert.equal(request.library_revision,value.libraryRevision);assert.equal(request.track_id,value.trackId);assert.equal(request.import_operation,value.importOperation);
  assert.equal(request.byte_length,value.byteLength.toString());assert.equal(request.sha256,value.sha256);
  assert.equal(reply.ok,true);assert.equal(reply.error,'');assert.equal(reply.byte_length,request.byte_length);assert.equal(reply.sha256,request.sha256);
  assert.equal(api.musicPlaybackSourceValid(reply),false);assert.equal(api.musicPlaybackSourceValid(value),false);
  assert.equal('token' in reply,false);assert.equal('fd' in reply,false);assert.equal('path' in reply,false);assert.equal('uri' in reply,false);
});
test('actual Ready track source_uri cannot be fed to AVPlayer or open a descriptor without an owned MusicFiles lease',async()=>{
  const calls=[],platform=load(PLATFORM,{'../model/MusicPlayback':api,'@kit.BasicServicesKit':{},
    '@kit.MediaKit':{media:{createAVPlayer:async()=>{calls.push('player');throw new Error('not admitted');}}},
    '@kit.CoreFileKit':{fileIo:{stat:async()=>{calls.push('stat');throw new Error('no descriptor');}}}});
  const files={ownsSource:()=>false,ownsDescriptor:()=>false,open:async()=>{calls.push('open');throw new Error('not registered');},close:async()=>{calls.push('close');throw new Error('no descriptor');}};
  const track=dto.current_track_reply.music.tracks[0];assert.equal(track.source_uri,'file:///morrow-music/'+track.track_id);
  for(const value of [track,identity(track),dto.export_reply]){
    const player=await platform.createMusicPlayer(value,files);await assert.rejects(player.load(),/租约无效/);await player.release();
  }
  assert.deepEqual(calls,[]);
});
test('actual playback proposal remains read-only instruction and is not a source or platform success acknowledgement',()=>{
  for(const reply of [dto.policy_reply,dto.next_reply,dto.seek_reply,dto.restore_reply]){
    assert.equal(reply.effect,'not_committed');assert.equal(reply.receipt_revision,'');assert.equal(reply.music.kind,'playback');
    assert.equal(api.musicPlaybackSourceValid(reply.music.playback),false);assert.equal('fd' in reply.music.playback,false);
  }
});
