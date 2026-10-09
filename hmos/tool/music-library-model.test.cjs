'use strict';
const { test } = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path');
const { MusicLibrary, MUSIC_TRACK_LIMIT, MUSIC_AUDIO_LIMIT, MUSIC_LYRICS_LIMIT, parseNativeMusicReply, validMusicImportRequest,
  fixture, request, track, summary, reply, plain, sha, importLiteral, readyOperation, deferred, tick } = require('./music-library-test-harness.cjs');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/model/MusicLibrary.ets');
test('actual Library source exports only independent music DTO/coordination and exact platform import hooks', t => {
  const source = fs.readFileSync(sourcePath); t.diagnostic('MusicLibrary.ets SHA256=' + sha(source));
  assert.equal(typeof MusicLibrary, 'function'); assert.equal(MUSIC_TRACK_LIMIT, 512); assert.equal(MUSIC_AUDIO_LIMIT, 150 * 1024 * 1024);
  assert.doesNotMatch(source.toString(), /EditorDraft|EditorBusiness|card_id|draft_id|AVPlayer|fileIo/);
});
test('real Store-produced complete Reply DTO parses without fabricated fields or empty success fallback', () => {
  const filename = path.resolve(__dirname, '../reports/ui-source/v29/music-store-fixture-stage1.json');
  const bytes = fs.readFileSync(filename); assert.equal(sha(bytes), 'b60e2ae4f0e995690fb0b1070fc5be5c5025dbba302cb8cb16a2d0d0b2fbf640');
  const real = JSON.parse(bytes);
  for (const name of ['empty_reply', 'pending_reply', 'ready_reply', 'set_lyrics_reply', 'read_reply', 'lyrics_reply', 'policy_reply']) {
    const value = parseNativeMusicReply(JSON.stringify(real[name])); assert.equal(value.schema_version, 1, name);
  }
  assert.equal(real.pending_reply.music.tracks[0].phase, 'pending'); assert.equal(real.pending_reply.effect, 'committed');
  assert.equal(real.ready_reply.music.tracks[0].bytes_retained, true);
});
test('real Native Pending to complete Ready preserves its original FD import literal and source identity', async () => {
  const real = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../reports/ui-source/v29/music-store-fixture-stage1.json')));
  const wires = [], imports = [], library = new MusicLibrary({ owned: () => true, owner: () => 'actual-fixture-owner', hash: async value => sha(value), changed() {},
    async send(wire) { wires.push(wire); const action = JSON.parse(wire).music.action;
      return JSON.stringify(action === 'read' ? real.empty_reply : real.pending_reply); },
    async importFile(wire, r) { imports.push({ wire, request: plain(r) }); return JSON.stringify(real.ready_reply); } });
  await library.load(); await library.beginImport(real.request);
  const pending = library.view().import_plan; assert.equal(pending.phase, 'pending'); assert.equal(pending.qualified_ready, false);
  assert.equal(pending.original_import, real.pending_reply.music.tracks[0].request_json); assert.equal(imports.length, 0);
  await library.commitImport(); const ready = library.view().import_plan;
  assert.equal(ready.qualified_ready, true); assert.equal(ready.unknown, false); assert.equal(ready.phase, 'ready');
  assert.equal(imports[0].wire, pending.original_import); assert.deepEqual(imports[0].request, real.request);
  assert.deepEqual(plain(ready.record), real.ready_reply.music.tracks[0]); library.finishImport(); assert.equal(library.view().import_plan, undefined);
});
test('real complete lyrics and pure playback proposal use exact native read contracts', async () => {
  const real = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../reports/ui-source/v29/music-store-fixture-stage1.json')));
  const wires = [], library = new MusicLibrary({ owned: () => true, hash: async value => sha(value), changed() {}, importFile: async () => { throw Error('No import allowed'); },
    async send(wire) { wires.push(wire); const action = JSON.parse(wire).music.action;
      return JSON.stringify(action === 'read' ? real.read_reply : action === 'lyrics_read' ? real.lyrics_reply : real.policy_reply); } });
  await library.load(); assert.equal(library.view().loaded, true); const id = real.request.track_id;
  await library.readLyrics(id, 1100); assert.deepEqual(plain(library.view().lyrics), real.lyrics_reply.music.lyrics);
  const result = await library.playbackPolicy({ library_revision: real.read_reply.music.library_revision, index: '0', playing: false, blocked: false,
    position_ms: '0', duration_ms: '3000', music_action: 'toggle', value: '0', flag: false });
  assert.deepEqual(plain(result), real.policy_reply.music.playback); assert.equal(result.transport_effect, 'play');
  assert.ok(wires.every(wire => JSON.parse(wire).action === 'music')); assert.equal(library.view().original_mutation, '');
});
test('read paginates all lexical entries including Pending/Retired then orders only actual Ready playlist', async () => {
  const entries = Array.from({ length: 35 }, (_, i) => track(request('track-' + String(i).padStart(3, '0')), '70', i === 1 ? 'pending' : i === 2 ? 'retired' : 'ready'));
  const f = fixture(entries); f.order.reverse(); f.setSelected('track-030'); await f.library.load();
  assert.equal(f.library.view().records.length, 35); assert.equal(f.library.view().tracks.length, 33);
  assert.equal(f.library.view().tracks[0].track_id, f.order[0]); assert.equal(f.library.view().selected_track_id, 'track-030');
  assert.deepEqual(f.wires.map(wire => JSON.parse(wire).music.after), ['', 'track-015', 'track-031']);
  assert.equal(f.library.identity('track-001'), undefined); assert.equal(f.library.identity('track-002'), undefined);
});
test('read Unknown at a later page retains its exact original page and installs no partial empty playlist', async () => {
  let failed = false, shouldFail = true;
  const f = fixture(Array.from({ length: 18 }, (_, i) => track(request('track-' + String(i).padStart(3, '0')), '40')),
    { send: (wire, receive) => { if (JSON.parse(wire).music.after && shouldFail) { failed = true; throw Error('ReadUnknown'); } return receive(wire); } });
  await f.library.load(); assert.equal(failed, true); assert.equal(f.library.view().loaded, false); assert.equal(f.library.view().unknown, true);
  const original = f.library.view().original_read; shouldFail = false; await f.library.retryRead();
  assert.equal(f.wires.at(-1), original); assert.equal(f.library.view().records.length, 18); assert.equal(f.library.view().loaded, true);
});
test('mismatched cross-page revision, duplicate cursor, missing ready entry and missing DTO fail without empty fallback', async () => {
  for (const change of ['revision', 'cursor', 'missing', 'dto']) {
    const entries = Array.from({ length: 17 }, (_, i) => track(request('track-' + String(i).padStart(3, '0')), '35'));
    const f = fixture(entries, { send: (wire, receive) => {
      const original = JSON.parse(receive(wire)), c = JSON.parse(wire).music;
      if (change === 'dto') delete original.music;
      if (change === 'cursor') original.music.next_after = 'foreign';
      if (change === 'revision' && c.after) { original.music.library_revision = '36'; for (const t of original.music.tracks) t.library_revision = '36'; }
      if (change === 'missing' && c.after) original.music.tracks = [];
      return JSON.stringify(original);
    } });
    await f.library.load(); assert.equal(f.library.view().loaded, false, change); assert.equal(f.library.view().unknown, true, change);
    assert.ok(f.library.view().original_read, change); assert.match(f.library.view().error, /音乐/, change);
  }
});
test('strict complete DTO rejects foreign source, original wire hash, unknown keys, bounds and fabricated committed read', async () => {
  for (const change of [v => v.tracks[0].source_uri = 'file:///other.wav', v => v.tracks[0].request_sha256 = '0'.repeat(64),
    v => v.tracks[0].extra = true, v => v.tracks[0].byte_length = '065', v => v.order.push(v.order[0]),
    v => v.tracks[0].phase = 'ready-but-unverified', v => v.tracks[0].bytes_retained = false, v => v.online_lyrics = true]) {
    const f = fixture([track(request(), '2')], { send: (wire, receive) => { const r = JSON.parse(receive(wire)); change(r.music); return JSON.stringify(r); } });
    await f.library.load(); assert.equal(f.library.view().loaded, false); assert.equal(f.library.view().unknown, true);
  }
  const f = fixture([track(request(), '2')], { send: (wire, receive) => { const r = JSON.parse(receive(wire)); r.effect = 'committed'; r.music.operation_id = 'fake-write'; r.music.operation_revision = '2'; r.receipt_revision = '2'; return JSON.stringify(r); } });
  await f.library.load(); assert.equal(f.library.view().loaded, false); assert.match(f.library.view().error, /只读/);
});
test('Pending committed receipt is not Ready, inspect never invokes an FD import, and explicit commit is exact once', async () => {
  const f = fixture(); await f.library.load(); const r = request(); await f.library.beginImport(r);
  assert.equal(f.library.view().import_plan.phase, 'pending'); assert.equal(f.library.view().import_plan.qualified_ready, false);
  assert.throws(() => f.library.finishImport(), /尚未确认/); await f.library.inspectImport(); assert.equal(f.imports.length, 0);
  const original = f.library.view().import_plan.original_import; await f.library.commitImport();
  assert.equal(f.imports.length, 1); assert.equal(f.imports[0].wire, original); assert.equal(f.library.view().import_plan.qualified_ready, true);
});
test('import Unknown retains exact original FD literal; reconcile may qualify retained bytes without rereading a replacement file', async () => {
  const f = fixture([], { importFile: (_, r, records) => { records.find(t => t.track_id === r.track_id).bytes_retained = true; throw Error('ImportUnknownAfterRetain'); } });
  await f.library.load(); await f.library.beginImport(request()); const plan = f.library.view().import_plan; await f.library.commitImport();
  assert.equal(f.library.view().import_plan.unknown, true); assert.equal(f.library.view().import_plan.original_import, plan.original_import);
  await assert.rejects(f.library.commitImport(), /尚未核对/); assert.throws(() => f.library.finishImport(), /尚未确认/);
  await f.library.reconcileImport(); assert.equal(f.library.view().import_plan.qualified_ready, true); assert.equal(f.imports.length, 1);
  assert.equal(f.wires.at(-1), plan.original_reconcile); assert.equal(f.library.view().import_plan.original_import, plan.original_import);
});
test('unretained Unknown import reconciles only to Pending and permits a later explicit same-literal FD attempt', async () => {
  let fail = true;
  const f = fixture([], { importFile: (wire, r, records) => {
    if (fail) throw Error('NoBytesUnknown'); const t = records.find(t => t.track_id === r.track_id); t.bytes_retained = true;
    return reply(summary([t], { kind: 'track', library_revision: t.library_revision, operation_id: r.operation_id,
      operation_revision: t.library_revision, repeated: true }));
  } });
  await f.library.load(); await f.library.beginImport(request()); const literal = f.library.view().import_plan.original_import;
  await f.library.commitImport(); await f.library.reconcileImport();
  assert.equal(f.library.view().import_plan.phase, 'pending'); assert.equal(f.library.view().import_plan.qualified_ready, false);
  fail = false; await f.library.commitImport(); assert.equal(f.imports[1].wire, literal);
  assert.equal(f.library.view().import_plan.qualified_ready, false); assert.equal(f.library.view().import_plan.unknown, true);
});
test('restored original local import first inspects actual Native plan and does not automatically resend FD bytes', async () => {
  const r = request(), f = fixture([track(r, '1', 'pending')]);
  f.library.restoreImportLiteral(importLiteral(r)); assert.equal(f.wires.length, 0);
  await assert.rejects(f.library.commitImport(), /尚未核对/); await f.library.inspectImport(); assert.equal(f.imports.length, 0);
  assert.equal(f.library.view().import_plan.phase, 'pending'); await f.library.commitImport(); assert.equal(f.imports[0].wire, importLiteral(r));
  assert.equal(f.library.view().import_plan.qualified_ready, true);
});
test('foreign import provenance, literal changes, fake ready stage operation and wrong receipt revision cannot qualify files', async () => {
  for (const change of ['foreign', 'literal', 'operation', 'receipt']) {
    const f = fixture([], { importFile: (wire, r) => {
      const t = track(r, '2'), value = summary([t], { kind: 'track', operation_id: readyOperation(r), operation_revision: '2' });
      if (change === 'foreign') { value.tracks[0].request.operation_id = 'foreign'; value.tracks[0].import_operation = 'foreign'; }
      if (change === 'literal') { value.tracks[0].request_json += ' '; value.tracks[0].request_sha256 = sha(value.tracks[0].request_json); }
      if (change === 'operation') value.operation_id = 'different-stage';
      return reply(value, 'committed', change === 'receipt' ? { receipt_revision: '1' } : {});
    } });
    await f.library.load(); await f.library.beginImport(request()); await f.library.commitImport();
    assert.equal(f.library.view().import_plan.qualified_ready, false, change); assert.equal(f.library.view().import_plan.unknown, true, change);
  }
});
test('stable identity navigation wraps actual ordered Ready tracks and refuses foreign or retired source tuples', async () => {
  const f = fixture(['a', 'b', 'c'].map(id => track(request(id), '6'))); await f.library.load();
  const id = f.library.identity('a'); assert.equal(f.library.adjacent(id, -1).trackId, 'c'); assert.equal(f.library.adjacent(id, 1).trackId, 'b');
  assert.equal(f.library.adjacent({ ...id, sha256: '0'.repeat(64) }, 1), undefined); assert.equal(f.library.adjacent(id, 2), undefined);
  await f.library.reorder(['c', 'a', 'b'], 'sort-1'); assert.equal(f.library.view().selected_track_id, 'a');
  assert.equal(f.library.adjacent(id, -1).trackId, 'c'); assert.equal(f.library.identity('a').sha256, id.sha256);
});
test('select returns the actual newly confirmed revision and deletion/reorder preserve stable remaining selected source', async () => {
  const f = fixture(['a', 'b', 'c'].map(id => track(request(id), '6'))); await f.library.load(); const b = f.library.identity('b');
  const selected = await f.library.select('b', 'select-b'); assert.equal(selected.trackId, 'b'); assert.equal(selected.libraryRevision, '7');
  assert.equal(selected.importOperation, b.importOperation); assert.equal(selected.sha256, b.sha256);
  await f.library.remove('a', 'remove-a'); assert.equal(f.library.view().selected_track_id, 'b'); assert.equal(f.library.identity('b').sha256, b.sha256);
  await f.library.remove('b', 'remove-b'); assert.equal(f.library.view().selected_track_id, 'c'); assert.equal(f.library.identity('b'), undefined);
  assert.equal(f.library.view().records.find(t => t.track_id === 'b').phase, 'retired'); assert.equal(f.imports.length, 0);
});
test('fixed mutation Unknown cannot be discarded or replaced; explicit retry preserves original CAS and operation bytes', async () => {
  let unknown = true;
  const f = fixture(['a', 'b'].map(id => track(request(id), '4')), { send: (wire, receive) => {
    if (JSON.parse(wire).music.action === 'reorder' && unknown) throw Error('WriteUnknown'); return receive(wire);
  } });
  await f.library.load(); await f.library.reorder(['b', 'a'], 'sort-fixed'); const original = f.library.view().original_mutation;
  assert.equal(f.library.view().unknown, true); assert.throws(() => f.library.discardKnownMutation(), /未知/);
  await assert.rejects(f.library.setShowLyrics(true, 'other-write'), /未完成/); unknown = false; await f.library.retryMutation();
  assert.equal(f.wires.filter(w => JSON.parse(w).music.action === 'reorder').length, 2);
  assert.ok(f.wires.filter(w => JSON.parse(w).music.action === 'reorder').every(w => w === original));
  assert.deepEqual(plain(f.library.view().order), ['b', 'a']); assert.equal(f.library.view().selected_track_id, 'a');
});
test('known CAS rejection may be explicitly discarded; late or malformed receipt remains Unknown', async () => {
  const f = fixture([track(request('a'), '2')]); await f.library.load(); f.bump();
  await f.library.setShowLyrics(true, 'stale-CAS'); assert.equal(f.library.view().unknown, false); assert.ok(f.library.view().original_mutation);
  f.library.discardKnownMutation(); await f.library.load(); assert.equal(f.library.view().library_revision, '3');
  const g = fixture([track(request('a'), '2')], { send: (wire, receive) => {
    const r = JSON.parse(receive(wire)); if (JSON.parse(wire).music.action === 'show_lyrics') r.music.operation_id = 'foreign'; return JSON.stringify(r);
  } });
  await g.library.load(); await g.library.setShowLyrics(true, 'ours'); assert.equal(g.library.view().unknown, true);
  assert.throws(() => g.library.discardKnownMutation(), /未知/);
});

test('known no-write does not survive an explicit same-operation retry whose committed reply is lost', async () => {
  let attempts = 0;
  const f = fixture([track(request('a'), '2')], { send: (wire, receive) => {
    if (JSON.parse(wire).music.action !== 'show_lyrics') return receive(wire);
    attempts++;
    if (attempts === 1) return JSON.stringify({ ok: false, error: 'TemporaryStorageFailure', effect: 'not_committed', receipt_revision: '' });
    const actual = receive(wire);
    if (attempts === 2) throw Error('UnknownAfterCommit');
    return actual;
  } });
  await f.library.load(); await f.library.setShowLyrics(true, 'fixed-retry-after-storage');
  const original = f.library.view().original_mutation;
  assert.equal(f.library.view().unknown, false); assert.ok(original);
  await f.library.retryMutation(); assert.equal(f.revision, '3');
  assert.equal(f.library.view().unknown, true); assert.equal(f.library.view().original_mutation, original);
  assert.equal(f.library.view().show_lyrics, false); assert.throws(() => f.library.discardKnownMutation(), /未知/);
  await f.library.retryMutation(); assert.equal(f.library.view().unknown, false); assert.equal(f.library.view().show_lyrics, true);
  assert.equal(f.library.view().original_mutation, '');
  const writes = f.wires.filter(wire => JSON.parse(wire).music.action === 'show_lyrics');
  assert.equal(writes.length, 3); assert.ok(writes.every(wire => wire === original)); assert.equal(f.revision, '3');
});

test('invalid final page is not admitted and the exact same readonly retry can complete the original aggregate', async () => {
  let malformed = true;
  const f = fixture(Array.from({ length: 17 }, (_, i) => track(request('track-' + String(i).padStart(3, '0')), '35')),
    { send: (wire, receive) => {
      const raw = receive(wire);
      if (!JSON.parse(wire).music.after || !malformed) return raw;
      const value = JSON.parse(raw); value.music.tracks[0].phase = 'pending'; return JSON.stringify(value);
    } });
  await f.library.load(); assert.equal(f.library.view().loaded, false); assert.equal(f.library.view().unknown, true);
  assert.match(f.library.view().error, /完整分页/); const original = f.library.view().original_read;
  malformed = false; await f.library.retryRead(); assert.equal(f.wires.at(-1), original);
  assert.equal(f.library.view().loaded, true); assert.equal(f.library.view().unknown, false);
  assert.equal(f.library.view().records.length, 17); assert.equal(f.library.view().tracks.length, 17);
  assert.equal(new Set(f.library.view().records.map(record => record.track_id)).size, 17);
});
test('lyrics persist complete UTF8, independent full untimed display and footer CAS without a new source import', async () => {
  const f = fixture([track(request('a'), '2')]); await f.library.load(); const source = f.library.identity('a');
  const full = '完整本地歌词😀\nsecond line'; await f.library.setLyrics('a', full, '歌词文件', 'lyrics-1'); await f.library.readLyrics('a');
  assert.equal(f.library.view().lyrics.text, full); assert.equal(f.library.view().lyrics.untimed, true); assert.equal(f.library.view().lyrics.active_index, -1);
  await f.library.setShowLyrics(true, 'footer-1'); assert.equal(f.library.view().show_lyrics, true);
  assert.equal(f.library.identity('a').importOperation, source.importOperation); assert.equal(f.library.identity('a').sha256, source.sha256); assert.equal(f.imports.length, 0);
});
test('input limits reject full data instead of slicing 513 tracks, over150MiB audio, malformed Unicode or lyrics over49152 bytes', async () => {
  assert.equal(validMusicImportRequest({ ...request(), byte_length: String(MUSIC_AUDIO_LIMIT + 1) }), false);
  assert.equal(validMusicImportRequest({ ...request(), name: 'protected.ncm' }), false);
  assert.equal(validMusicImportRequest({ ...request(), operation_id: 'morrow-host-fake' }), false);
  assert.throws(() => parseNativeMusicReply(reply(summary([], { order: Array.from({ length: 513 }, (_, i) => 'track-' + i) }))), /音乐/);
  const f = fixture([track(request('a'), '2')]); await f.library.load(); const count = f.wires.length;
  await assert.rejects(f.library.setLyrics('a', '汉'.repeat(MUSIC_LYRICS_LIMIT / 3 + 1), 'local', 'big'), /歌词/);
  await assert.rejects(f.library.setLyrics('a', '\uD800', 'local', 'invalid'), /不完整/);
  assert.equal(f.wires.length, count);
});
test('pure Native playback request uses string coordinates and returns only a proposal; Unknown retry is the same original literal', async () => {
  let failed = true;
  const f = fixture([track(request('a'), '2')], { policy: c => ({ track_id: 'a', index: '0', playing: false, blocked: false, position_ms: c.value,
    duration_ms: c.duration_ms, transport_effect: 'seek' }), send: (wire, receive) => {
    if (JSON.parse(wire).music.action === 'policy' && failed) throw Error('PolicyUnknown'); return receive(wire);
  } });
  await f.library.load(); const proposal = { library_revision: '2', index: '0', playing: false, blocked: false, position_ms: '0', duration_ms: '5000', music_action: 'seek', value: '1250', flag: false };
  assert.equal(await f.library.playbackPolicy(proposal), undefined); const original = f.library.view().original_read;
  proposal.value = '4000'; failed = false; const value = await f.library.retryPlaybackPolicy(); assert.equal(f.wires.at(-1), original);
  assert.equal(value.position_ms, '1250'); assert.equal(value.transport_effect, 'seek'); assert.equal(f.library.view().original_mutation, '');
});
test('owner epoch roundtrip/dispose during actual read, import hash or mutation rejects late installation and keeps original intent', async () => {
  const wait = deferred(), f = fixture([track(request('a'), '2')], { send: async (wire, receive) => { await wait.promise; return receive(wire); } });
  const reading = f.library.load(); await tick(); f.setOwned(false); f.setOwner('music-owner-2'); f.setOwned(true); wait.resolve(); await reading;
  assert.equal(f.library.view().loaded, false); assert.equal(f.library.view().unknown, true); assert.ok(f.library.view().original_read);
  const gate = deferred(), g = fixture([track(request('a'), '2')], { send: async (wire, receive) => {
    if (JSON.parse(wire).music.action === 'show_lyrics') await gate.promise; return receive(wire);
  } });
  await g.library.load(); const writing = g.library.setShowLyrics(true, 'late'); await tick(); g.library.dispose(); gate.resolve(); await writing;
  assert.equal(g.library.view().show_lyrics, false); assert.equal(g.library.view().unknown, true); assert.ok(g.library.view().original_mutation);
});
test('independent public snapshots cannot modify private track, import proposal or fixed retry bytes', async () => {
  const f = fixture(); await f.library.load(); const input = request(); await f.library.beginImport(input);
  input.name = 'tampered.mp3'; const visible = f.library.view(); visible.import_plan.request.name = 'other.wav'; visible.import_plan.original_import = 'other';
  await f.library.commitImport(); assert.equal(JSON.parse(f.imports[0].wire).music.request.name, 'track-A.wav');
  f.library.finishImport(); await f.library.load(); const a = f.library.view(); a.tracks[0].sha256 = '0'.repeat(64); a.order.length = 0;
  assert.equal(f.library.view().order.length, 1); assert.notEqual(f.library.identity('track-A').sha256, '0'.repeat(64));
});
