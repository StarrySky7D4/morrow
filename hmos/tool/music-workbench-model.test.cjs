'use strict';
const {test}=require('node:test'),assert=require('node:assert/strict');
const {fixture,ready,settle,deferred,sha,request,track,plain}=require('./music-workbench-test-harness.cjs');
const actions=f=>f.wires.map(w=>JSON.parse(w).music.action);
const audioUri=(f,id,bytes=Buffer.alloc(64,7))=>{const uri='content://music/'+id;f.put(uri,bytes,id+'.wav');return uri;};

test('startup reads actual durable selection and spool metadata without CAS, player creation or autoplay',async()=>{
 const f=fixture({initial:[ready('A'),ready('B')],selected:'B'});await f.controller.load();await settle();
 assert.equal(f.controller.view().library.selected_track_id,'B');assert.equal(f.controller.view().playback.phase,'idle');
 assert.deepEqual(actions(f),['read']);assert.equal(f.players.length,0);assert.equal(f.fds.size,0);
});
test('actual Library selection is committed before file export and actual AVPlayer ACK is the only playing state',async()=>{
 const f=fixture({initial:[ready('A'),ready('B')],noPlayAck:true});await f.controller.load();await f.controller.action('select','B');await settle();
 assert.equal(f.selected,'B');assert.equal(f.controller.view().playback.phase,'prepared');assert.equal(f.controller.view().library.library_revision,'2');
 assert.deepEqual(actions(f),['read','policy','select','read','lyrics_read']);
 assert.ok(f.trace.findIndex(x=>x[0]==='request'&&x[1]==='select')<f.trace.findIndex(x=>x[0]==='export'));
 assert.ok(f.trace.findIndex(x=>x[0]==='beforePlay')<f.trace.findIndex(x=>x[0]==='fdSrc'));
 f.players[0].listeners.stateChange('playing');assert.equal(f.controller.view().playback.phase,'playing');
 await f.controller.dispose();assert.equal(f.fds.size,0);
});
test('already selected track creates no CAS and repeated same track selection retains its exact FD player',async()=>{
 const f=fixture({initial:[ready('A')]});await f.controller.load();await f.controller.action('select','A');await settle();
 const player=f.players[0],descriptor=player.descriptor;await f.controller.action('select','A');await settle();
 assert.equal(actions(f).filter(x=>x==='select').length,0);assert.equal(f.players.length,1);assert.equal(player.descriptor,descriptor);
 await f.controller.dispose();
});
test('pure policy failure never opens a player or replaces real transport with proposed success',async()=>{
 const f=fixture({initial:[ready('A')],policy:()=>{throw Error('policy Unknown');}});await f.controller.load();await f.controller.action('toggle');
 assert.equal(f.players.length,0);assert.equal(f.controller.view().playback.phase,'idle');assert.equal(f.controller.view().canRetryRead,true);
 assert.equal(f.controller.view().library.unknown,true);
});
test('invalid row selection cannot wrap negative policy index into a different valid track',async()=>{
 const f=fixture({initial:[ready('A')]});await f.controller.load();await f.controller.action('select','missing');
 assert.deepEqual(actions(f),['read']);assert.equal(f.players.length,0);assert.match(f.controller.view().error,/不在/);
});
test('native seek proposal reaches actual player with clamped milliseconds, pause requires actual ACK',async()=>{
 const f=fixture({initial:[ready('A')]});await f.controller.load();await f.controller.action('select','A');await settle();
 await f.controller.action('seek','',99999);await settle();assert.ok(f.players[0].calls.some(x=>Array.isArray(x)&&x[0]==='seek'&&x[1]===9000));
 assert.equal(f.controller.view().playback.positionMs,9000);await f.controller.action('toggle');await settle();
 assert.equal(f.controller.view().playback.phase,'paused');assert.ok(f.players[0].calls.includes('pause'));await f.controller.dispose();
});
test('previous and next use actual Native policy and durable selected CAS; EOF also admits the new stable track',async()=>{
 const f=fixture({initial:[ready('A'),ready('B')]});await f.controller.load();await f.controller.action('select','A');await settle();
 await f.controller.action('next');await settle();assert.equal(f.selected,'B');assert.equal(f.controller.view().playback.trackId,'B');
 await f.controller.action('previous');await settle();assert.equal(f.selected,'A');
 f.players.at(-1).listeners.stateChange('completed');await settle();assert.equal(f.selected,'B');assert.equal(f.controller.view().playback.trackId,'B');
 assert.equal(f.trace.filter(x=>x[0]==='beforePlay').length,4);await f.controller.dispose();
});
test('reorder and removal of another track keep the current source FD and playing position',async()=>{
 const f=fixture({initial:[ready('A'),ready('B'),ready('C')]});await f.controller.load();await f.controller.action('select','B');await settle();
 const p=f.players[0],d=p.descriptor;p.listeners.timeUpdate(2500);await f.controller.reorder(['C','B','A']);await settle();
 assert.equal(f.players.length,1);assert.equal(p.descriptor,d);assert.equal(f.controller.view().playback.positionMs,2500);
 await f.controller.action('remove','A');await settle();assert.equal(f.players.length,1);assert.equal(f.selected,'B');assert.equal(f.controller.view().playback.phase,'playing');
 await f.controller.dispose();
});
test('removing current track closes old AVPlayer and FD before opening persisted replacement; last removal stops',async()=>{
 const f=fixture({initial:[ready('A'),ready('B')]});await f.controller.load();await f.controller.action('select','A');await settle();
 const first=f.players[0];await f.controller.action('remove','A');await settle();assert.ok(first.calls.includes('release'));assert.equal(f.selected,'B');
 assert.equal(f.controller.view().playback.trackId,'B');assert.equal(f.controller.view().playback.phase,'playing');
 await f.controller.action('remove','B');await settle();assert.equal(f.selected,'');assert.equal(f.controller.view().playback.phase,'idle');assert.equal(f.fds.size,0);
});
test('whole multi-file batch uses captured grants, unique requests and releases spools only after matching Ready',async()=>{
 const f=fixture();f.select([audioUri(f,'one'),audioUri(f,'two',Buffer.alloc(71,9))]);await f.controller.load();await f.controller.action('add');
 assert.equal(f.imports.length,2);assert.equal(f.records.length,2);assert.equal(new Set(f.records.map(x=>x.import_operation)).size,2);
 assert.equal(f.controller.view().library.tracks.length,2);assert.equal(f.controller.view().spools.length,0);assert.equal(f.fds.size,0);assert.equal(f.players.length,0);
 const original=f.imports[0],id=f.records[0].track_id;assert.equal(JSON.parse(original).music.request.track_id,id);
 assert.ok(f.trace.some(x=>x[0]==='fsync'&&x[1].endsWith('/request.json')));assert.ok(f.records.every(x=>x.bytes_retained&&x.phase==='ready'));
});
test('Unknown first import stops batch and preserves complete source, exact original request and plan',async()=>{
 const f=fixture({importFile:()=>{throw Error('FD import Unknown');}});f.select([audioUri(f,'one'),audioUri(f,'two')]);
 await f.controller.load();await f.controller.action('add');const state=f.controller.view();
 assert.equal(f.imports.length,1);assert.equal(f.records.length,1);assert.equal(state.library.import_plan.unknown,true);assert.equal(state.spools.length,1);
 assert.equal(state.canContinueImport,false);assert.equal(state.canInspectImport,true);assert.equal(state.canFinishImport,false);
 const dir='/app/files/hmos-music-spool/'+state.spools[0].spoolId;assert.equal(f.nodes.get(dir+'/request.json').bytes.toString(),f.imports[0]);assert.ok(f.nodes.has(dir+'/data'));
 await f.controller.action('add');assert.equal(f.imports.length,1);assert.equal(f.records.length,1);
});
test('explicit inspect then reconcile qualifies retained complete audio before separate finish, without FD replay',async()=>{
 const f=fixture({importFile:()=>{throw Error('lost after bytes retained');}});f.select([audioUri(f,'one')]);await f.controller.load();await f.controller.action('add');
 const original=f.imports[0];await f.controller.action('import_inspect');assert.equal(f.controller.view().library.import_plan.phase,'pending');
 await f.controller.action('import_reconcile');assert.equal(f.controller.view().canFinishImport,true);assert.equal(f.imports.length,1);assert.equal(f.imports[0],original);
 assert.equal(f.controller.view().spools.length,1);await f.controller.action('import_finish');assert.equal(f.controller.view().spools.length,0);assert.equal(f.controller.view().library.tracks.length,1);
});
test('Ready spool release failure keeps original cleanup identity; explicit finish retry uses same qualified receipt',async()=>{
 const f=fixture();f.select([audioUri(f,'one')]);await f.controller.load();f.faults.set('unlink:/app/files/hmos-music-spool/music-000001/data',Error('release rejected'));
 await f.controller.action('add');assert.equal(f.controller.view().canFinishImport,true);assert.equal(f.controller.view().spools.length,0);assert.equal(f.imports.length,1);assert.ok(f.nodes.has('/app/files/hmos-music-spool/music-000001/data'));
 f.faults.clear();await f.controller.action('import_finish');assert.equal(f.controller.view().spools.length,0);assert.equal(f.imports.length,1);
});
test('recovered blank sidecar lists all matching actual Pending choices; explicit selection inspects its original literal only',async()=>{
 const bytes=Buffer.alloc(64,7),r=request('pending-A','0');r.name='same.wav';r.sha256=sha(bytes);const a=track(r,'1','pending');
 const s=request('pending-B','0');s.name=r.name;s.sha256=r.sha256;const b=track(s,'1','pending');
 const f=fixture({initial:[a,b]});const spool=await f.spool(r.name,bytes);await f.controller.load();const state=f.controller.view();
 assert.equal(state.spools[0].canStart,false);assert.deepEqual(Array.from(state.spools[0].choices,x=>x.trackId),['pending-A','pending-B']);
 await f.controller.action('spool_resume','',0,spool.id);assert.equal(f.controller.view().library.import_plan,undefined);assert.match(f.controller.view().error,/明确选择/);
 await f.controller.action('spool_resume','pending-B',0,spool.id);assert.equal(f.controller.view().library.import_plan.request.track_id,'pending-B');
 assert.equal(f.controller.view().library.import_plan.original_import,b.request_json);assert.equal(f.imports.length,0);assert.equal(f.controller.view().canContinueImport,true);
 await f.controller.action('import_continue');assert.equal(f.imports[0],b.request_json);assert.equal(f.records.find(t=>t.track_id==='pending-A').phase,'pending');
});
test('recovered persisted request always inspects exact literal and never permits fresh start or guessed ID',async()=>{
 const bytes=Buffer.alloc(64,7),r=request('pending-A','0');r.name='same.wav';r.sha256=sha(bytes);const a=track(r,'1','pending');
 const f=fixture({initial:[a]});const spool=await f.spool(r.name,bytes,a.request_json);await f.controller.load();
 assert.equal(f.controller.view().spools[0].originalSaved,true);assert.equal(f.controller.view().spools[0].canStart,false);
 await f.controller.action('spool_start','',0,spool.id);assert.equal(actions(f).filter(x=>x==='import_begin').length,0);
 await f.controller.action('spool_resume','invented',0,spool.id);assert.equal(f.controller.view().library.import_plan.original_import,a.request_json);assert.equal(f.imports.length,0);
});
test('only recovered unissued cache with no Pending match permits explicit new start',async()=>{
 const f=fixture(),spool=await f.spool();await f.controller.load();assert.equal(f.controller.view().spools[0].canStart,true);
 await f.controller.action('spool_start','',0,spool.id);assert.equal(f.records.length,1);assert.equal(f.imports.length,1);assert.equal(f.controller.view().spools.length,0);
});
test('fixed mutation Unknown blocks new writes and explicit retry reuses same operation bytes',async()=>{
 let lost=true;const f=fixture({initial:[ready('A'),ready('B')],send:(wire,receive)=>{
  const c=JSON.parse(wire).music;if(c.action==='reorder'&&lost){lost=false;receive(wire);throw Error('write ACK lost');}return receive(wire);}});
 await f.controller.load();await f.controller.reorder(['B','A']);const original=f.controller.view().library.original_mutation;
 assert.equal(f.controller.view().canRetryMutation,true);assert.equal(f.controller.view().canDiscardKnownMutation,false);
 await f.controller.action('lyrics_toggle');assert.equal(actions(f).filter(x=>x==='show_lyrics').length,0);
 await f.controller.action('mutation_retry');assert.equal(f.wires.filter(w=>JSON.parse(w).music.action==='reorder').every(w=>w===original),true);
 assert.deepEqual(Array.from(f.controller.view().library.order),['B','A']);assert.equal(f.controller.view().library.unknown,false);
});
test('known no-write retains fixed operation and allows explicit discard, while full lyrics text remains recoverable',async()=>{
 const f=fixture({initial:[ready('A')],send:(wire,receive)=>JSON.parse(wire).music.action==='set_lyrics'?JSON.stringify({ok:false,error:'conflict',effect:'not_committed',receipt_revision:''}):receive(wire)});
 const text='complete 汉字\nlocal lyrics';f.put('content://lyrics',text,'local.lrc');f.select(['content://lyrics']);await f.controller.load();
 await f.controller.action('lyrics_import','A');assert.equal(f.controller.view().unsavedLyrics,text);assert.equal(f.controller.view().canDiscardKnownMutation,true);
 await f.controller.action('mutation_discard_known');assert.equal(f.controller.view().library.original_mutation,'');assert.equal(f.controller.view().unsavedLyrics,text);
});
test('oversized lyrics are fully captured and retained unsaved without cropping or dispatching native write',async()=>{
 const f=fixture({initial:[ready('A')]}),text='汉'.repeat(20000);f.put('content://lyrics',text,'large.lrc');f.select(['content://lyrics']);await f.controller.load();await f.controller.action('lyrics_import','A');
 assert.equal(f.controller.view().unsavedLyrics,text);assert.equal(actions(f).includes('set_lyrics'),false);assert.equal(Buffer.byteLength(f.controller.view().unsavedLyrics),60000);
});
test('Rust parsed lyric lines are cached once; live player positions compute footer without Store reads per callback',async()=>{
 const initial=ready('A'),text='[00:01]First\n[00:03]Second';initial.lyrics_byte_length=String(Buffer.byteLength(text));initial.lyric_source='local.lrc';
 const f=fixture({initial:[initial],show:true});f.lyrics.set('A',{text,source:'local.lrc',lines:[{time_ms:'1000',text:'First'},{time_ms:'3000',text:'Second'}],untimed:false});
 await f.controller.load();await f.controller.action('select','A');await settle();const before=f.wires.length;
 for(let i=0;i<100;i++)f.players[0].listeners.timeUpdate(3500+i);assert.equal(f.wires.length,before);assert.equal(f.controller.view().footerText,'Second');
 await f.controller.action('toggle');await settle();assert.equal(f.controller.view().footerText,'');await f.controller.action('lyrics_view','A');assert.equal(f.controller.view().lyricsText,text);assert.equal(f.controller.view().lyricsOpen,true);
 await f.controller.action('lyrics_close');assert.equal(f.controller.view().lyricsOpen,false);await f.controller.dispose();
});
test('foreground loss invalidates late selected acknowledgement even after returning; no new player is opened',async()=>{
 const gate=deferred();const f=fixture({initial:[ready('A'),ready('B')],send:async(wire,receive)=>{
  if(JSON.parse(wire).music.action==='select'){const r=receive(wire);await gate.promise;return r;}return receive(wire);}});
 await f.controller.load();const pending=f.controller.action('select','B');await settle();f.setForeground(false);await f.controller.background();f.setForeground(true);f.controller.foreground();gate.resolve();await pending;await settle();
 assert.equal(f.players.length,0);assert.equal(f.controller.view().library.unknown,true);assert.equal(f.controller.view().canRetryMutation,true);
});
test('background pauses real player and foreground never automatically restarts it',async()=>{
 const f=fixture({initial:[ready('A')]});await f.controller.load();await f.controller.action('select','A');await settle();f.setForeground(false);await f.controller.background();
 assert.ok(f.players[0].calls.includes('pause'));assert.equal(f.controller.view().playback.background,true);f.setForeground(true);f.controller.foreground();await settle();
 assert.equal(f.players[0].calls.filter(x=>x==='play').length,1);await f.controller.action('toggle');await settle();assert.equal(f.players[0].calls.filter(x=>x==='play').length,2);await f.controller.dispose();
});
test('original player release failure retains actual FD and explicit close retry frees only that resource',async()=>{
 let fail=true;const f=fixture({initial:[ready('A')],release:()=>{if(fail)throw Error('release Unknown');}});await f.controller.load();await f.controller.action('select','A');await settle();
 const descriptor=f.players[0].descriptor;await f.controller.action('retry_close');assert.equal(f.controller.view().playback.phase,'cleanup_failed');assert.ok(f.fds.has(descriptor.fd));
 fail=false;await f.controller.action('retry_close');assert.equal(f.controller.view().playback.phase,'idle');assert.equal(f.fds.size,0);
});
test('music requests remain in its independent action/host contract and never create idea drafts or business mutations',async()=>{
 const f=fixture();f.select([audioUri(f,'one')]);await f.controller.load();await f.controller.action('add');await f.controller.action('toggle');await settle();
 for(const wire of f.wires.concat(f.imports)){const c=JSON.parse(wire);assert.ok(['music','music_import'].includes(c.action));assert.equal('draft_id'in c,false);assert.equal('card_id'in c,false);}
 await f.controller.dispose();
});
test('idle controls permit empty-library add and confirmed selected first toggle without startup player or CAS',async()=>{
 const empty=fixture();await empty.controller.load();assert.equal(empty.controller.view().controlsEnabled,true);assert.equal(empty.controller.view().canWrite,true);
 const f=fixture({initial:[ready('A')]});await f.controller.load();assert.equal(f.controller.view().controlsEnabled,true);assert.equal(f.players.length,0);
 await f.controller.action('toggle');await settle();assert.equal(f.controller.view().playback.phase,'playing');assert.equal(actions(f).filter(x=>x==='select').length,0);await f.controller.dispose();
});
test('lyrics toggle applies explicit target and repeated same target creates no extra native mutation',async()=>{
 const f=fixture({initial:[ready('A')]});await f.controller.load();await f.controller.action('lyrics_toggle','',1);assert.equal(f.controller.view().library.show_lyrics,true);
 await f.controller.action('lyrics_toggle','',1);assert.equal(actions(f).filter(x=>x==='show_lyrics').length,1);await f.controller.action('lyrics_toggle','',0);assert.equal(f.controller.view().library.show_lyrics,false);
 await f.controller.action('lyrics_toggle','',7);assert.equal(actions(f).filter(x=>x==='show_lyrics').length,2);
});
test('lyrics read Unknown cannot prevent explicit pause of already owned playing player',async()=>{
 const f=fixture({initial:[ready('A')],send:(wire,receive)=>{if(JSON.parse(wire).music.action==='lyrics_read')throw Error('lyrics Unknown');return receive(wire);}});
 await f.controller.load();await f.controller.action('select','A');await settle();assert.equal(f.controller.view().library.unknown,true);assert.equal(f.controller.view().playback.phase,'playing');
 assert.equal(f.controller.view().canWrite,false);assert.equal(f.controller.view().controlsEnabled,true);const policies=actions(f).filter(x=>x==='policy').length;
 await f.controller.action('toggle');await settle();assert.equal(f.controller.view().playback.phase,'paused');assert.ok(f.players[0].calls.includes('pause'));assert.equal(actions(f).filter(x=>x==='policy').length,policies);await f.controller.dispose();
});
test('other mutation Unknown cannot block explicit pause and held lyrics remain viewable',async()=>{
 const initial=ready('A'),text='full local lyrics';initial.lyric_source='local.lrc';initial.lyrics_byte_length=String(Buffer.byteLength(text));
 const f=fixture({initial:[initial],send:(wire,receive)=>{const r=receive(wire);return JSON.parse(wire).music.action==='show_lyrics'?'malformed reply':r;}});
 f.lyrics.set('A',{text,source:'local.lrc',lines:[],untimed:true});await f.controller.load();await f.controller.action('select','A');await settle();
 await f.controller.action('lyrics_toggle','',1);assert.equal(f.controller.view().library.unknown,true);await f.controller.action('lyrics_view','A');assert.equal(f.controller.view().lyricsText,text);
 assert.equal(f.controller.view().lyricsOpen,true);await f.controller.action('toggle');await settle();assert.equal(f.controller.view().playback.phase,'paused');await f.controller.dispose();
});
test('media exclusivity requires actual pause ACK and rejects player pause failure or missing ACK',async()=>{
 const f=fixture({initial:[ready('A')]});await f.controller.load();await f.controller.action('select','A');await settle();await f.controller.pauseForMedia();assert.equal(f.controller.view().playback.phase,'paused');await f.controller.dispose();
 const g=fixture({initial:[ready('A')],pause:()=>{throw Error('pause failed');}});await g.controller.load();await g.controller.action('select','A');await settle();await assert.rejects(g.controller.pauseForMedia(),/未确认暂停/);assert.equal(g.controller.view().playback.phase,'failed');await g.controller.dispose();
 const h=fixture({initial:[ready('A')],noPauseAck:true});await h.controller.load();await h.controller.action('select','A');await settle();await assert.rejects(h.controller.pauseForMedia(),/未确认暂停/);assert.equal(h.controller.view().playback.phase,'playing');await h.controller.dispose();
});
test('full snapshot recovered after lost removal ACK retires original owned player without autoplaying replacement',async()=>{
 let lost=true;const f=fixture({initial:[ready('A'),ready('B')],send:(wire,receive)=>{const r=receive(wire);if(JSON.parse(wire).music.action==='remove'&&lost){lost=false;throw Error('removal ACK lost');}return r;}});
 await f.controller.load();await f.controller.action('select','A');await settle();const p=f.players[0];await f.controller.action('remove','A');assert.equal(f.controller.view().playback.phase,'playing');
 await f.controller.action('mutation_retry');await settle();assert.equal(f.selected,'B');assert.equal(f.controller.view().playback.phase,'idle');assert.ok(p.calls.includes('release'));assert.equal(f.players.length,1);assert.equal(f.fds.size,0);
});
test('removal committed but refresh read lost closes retired source only after exact original read is qualified',async()=>{
 let removed=false,lost=true;const f=fixture({initial:[ready('A'),ready('B')],send:(wire,receive)=>{const c=JSON.parse(wire).music;
  if(c.action==='read'&&removed&&lost){lost=false;throw Error('fresh snapshot lost');}const r=receive(wire);if(c.action==='remove')removed=true;return r;}});
 await f.controller.load();await f.controller.action('select','A');await settle();await f.controller.action('remove','A');assert.equal(f.controller.view().canRetryRead,true);assert.equal(f.controller.view().playback.phase,'playing');
 await f.controller.action('read_retry');await settle();assert.equal(f.controller.view().playback.phase,'idle');assert.equal(f.players.length,1);assert.equal(f.fds.size,0);
});
test('footer matches Flutter before first timestamp, empty timed line, untimed first line and known empty lyrics',async()=>{
 const timed=ready('A'),text='[00:01]First\n[00:03]';timed.lyric_source='local.lrc';timed.lyrics_byte_length=String(Buffer.byteLength(text));
 const f=fixture({initial:[timed],show:true});f.lyrics.set('A',{text,source:'local.lrc',lines:[{time_ms:'1000',text:'First'},{time_ms:'3000',text:''}],untimed:false});
 await f.controller.load();await f.controller.action('select','A');await settle();assert.equal(f.controller.view().footerText,'♪ A');f.players[0].listeners.timeUpdate(3500);assert.equal(f.controller.view().footerText,'♪');await f.controller.dispose();
 const raw=ready('A'),plainText='  First line\nSecond line';raw.lyric_source='local.txt';raw.lyrics_byte_length=String(Buffer.byteLength(plainText));
 const g=fixture({initial:[raw],show:true});g.lyrics.set('A',{text:plainText,source:'local.txt',lines:[],untimed:true});await g.controller.load();await g.controller.action('select','A');await settle();assert.equal(g.controller.view().footerText,'First line · 无时间轴');await g.controller.dispose();
 const h=fixture({initial:[ready('A')],show:true});await h.controller.load();await h.controller.action('select','A');await settle();assert.equal(h.controller.view().footerText,'♪ A · 暂无歌词');await h.controller.dispose();
});
test('viewing other track full lyrics does not replace current track cached footer timeline',async()=>{
 const a=ready('A'),b=ready('B');for(const t of [a,b]){t.lyric_source='local.lrc';t.lyrics_byte_length='1';}
 const f=fixture({initial:[a,b],show:true});f.lyrics.set('A',{text:'A',source:'local.lrc',lines:[{time_ms:'0',text:'playing A'}],untimed:false});f.lyrics.set('B',{text:'B',source:'local.lrc',lines:[],untimed:true});
 await f.controller.load();await f.controller.action('select','A');await settle();await f.controller.action('lyrics_view','B');assert.equal(f.controller.view().lyricsText,'B');assert.equal(f.controller.view().footerText,'playing A');await f.controller.dispose();
});
test('explicit unissued begin retry preserves original operation and CAS after lost registration response',async()=>{
 for(const committed of [false,true]){
  let lost=true;const f=fixture({send:(wire,receive)=>{if(JSON.parse(wire).music.action==='import_begin'&&lost){lost=false;if(committed)receive(wire);throw Error('registration Unknown');}return receive(wire);}});
  f.select([audioUri(f,'one')]);await f.controller.load();await f.controller.action('add');assert.equal(f.controller.view().canRetryBegin,true);assert.equal(f.imports.length,0);
  const original=f.controller.view().library.import_plan.original_begin;await f.controller.action('begin_retry');assert.equal(f.controller.view().canContinueImport,true);
  const begins=f.wires.filter(w=>JSON.parse(w).music.action==='import_begin');assert.equal(begins.length,2);assert.equal(begins.every(w=>w===original),true);assert.equal(f.records.length,1);assert.equal(f.imports.length,0);
 }
});
test('unsaved whole lyrics belong only to original track; another viewer does not consume or relabel them',async()=>{
 const f=fixture({initial:[ready('A'),ready('B')]}),text='B'.repeat(60000);f.put('content://lyrics',text,'B.lrc');f.select(['content://lyrics']);
 await f.controller.load();await f.controller.action('lyrics_import','B');assert.equal(f.controller.view().unsavedLyrics,text);assert.equal(f.controller.view().lyricsTitle,'B');assert.equal(f.controller.view().unsavedLyricsTrackId,'B');assert.equal(f.controller.view().unsavedLyricsTitle,'B');
 await f.controller.action('lyrics_view','A');assert.equal(f.controller.view().lyricsTitle,'A');assert.equal(f.controller.view().unsavedLyrics,'');assert.equal(f.controller.view().unsavedLyricsTrackId,'B');assert.equal(f.controller.view().unsavedLyricsTitle,'B');
 assert.ok(f.wires.some(w=>{const c=JSON.parse(w).music;return c.action==='lyrics_read'&&c.track_id==='A';}));
 await f.controller.action('lyrics_view','B');assert.equal(f.controller.view().lyricsTitle,'B');assert.equal(f.controller.view().unsavedLyrics,text);
});
test('actual complete Store-produced 16+2 pages reach coordinator without fabricated entries, player opens or startup writes',async()=>{
 const fs=require('node:fs'),path=require('node:path'),bytes=fs.readFileSync(path.resolve(__dirname,'../reports/ui-source/v29/music-store-fixture.json'));
 assert.equal(sha(bytes),'da6d8df4f50f59555a4064ab29c9b302ce29b85a843deb03f2f5c9eb0d83b9b2');const real=JSON.parse(bytes);
 const f=fixture({send:wire=>{const c=JSON.parse(wire).music;assert.equal(c.action,'read');assert.equal(c.limit,16);
  if(c.after==='')return JSON.stringify(real.read_pages[0]);assert.equal(c.after,real.read_pages[0].music.next_after);return JSON.stringify(real.read_pages[1]);}});
 await f.controller.load();const state=f.controller.view();assert.equal(state.library.records.length,18);assert.equal(state.library.tracks.length,2);assert.equal(state.library.unknown,false);
 assert.deepEqual(plain(state.library.records),real.read_pages.flatMap(p=>p.music.tracks));assert.equal(f.players.length,0);assert.equal(f.fds.size,0);assert.equal(f.imports.length,0);
});
test('partially removed Ready source retains exact release handle through retry_io and never reappears as complete import source',async()=>{
 const f=fixture(),dir='/app/files/hmos-music-spool/music-000001';f.select([audioUri(f,'one')]);await f.controller.load();
 f.faults.set('unlink:'+dir+'/metadata.json',Error('metadata cleanup Unknown'));await f.controller.action('add');const original=f.controller.activeSpool;
 assert.equal(f.controller.view().canFinishImport,true);assert.equal(f.nodes.has(dir+'/data'),false);assert.ok(f.nodes.has(dir+'/metadata.json'));assert.equal(f.controller.view().spools.length,0);
 await f.controller.action('retry_io');assert.equal(f.controller.activeSpool,original);assert.equal(f.controller.files.entries.get(original.spoolId).value,original);assert.equal(f.controller.view().spools.length,0);
 f.faults.clear();await f.controller.action('import_finish');assert.equal(f.controller.view().library.import_plan,undefined);assert.equal(f.nodes.has(dir),false);assert.equal(f.imports.length,1);assert.equal(f.controller.view().library.tracks.length,1);
});
