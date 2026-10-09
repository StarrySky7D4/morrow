'use strict';
const { test } = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path');
const { MusicLibrary, parseNativeMusicReply, plain, sha } = require('./music-library-test-harness.cjs');
const filename = path.resolve(__dirname, '../reports/ui-source/v29/music-store-fixture.json');
const bytes = fs.readFileSync(filename), real = JSON.parse(bytes);
function actualLibrary(send) {
  const wires = [], imports = [];
  const library = new MusicLibrary({ owned: () => true, owner: () => 'actual-native-library-owner', changed() {}, hash: async value => sha(value),
    async send(wire) { wires.push(wire); return JSON.stringify(await send(JSON.parse(wire).music)); },
    async importFile(wire) { imports.push(wire); throw Error('No FD dispatch authorized by this readonly fixture'); } });
  return { library, wires, imports };
}
function page(command) {
  assert.equal(command.action, 'read'); assert.equal(command.limit, 16);
  const index = command.after === '' ? 0 : 1;
  if (index) assert.equal(command.after, real.read_pages[0].music.next_after);
  return real.read_pages[index];
}

test('final complete Store fixture identity and every original music Reply retain the exact native schema', () => {
  assert.equal(bytes.length, 65127); assert.equal(sha(bytes), 'da6d8df4f50f59555a4064ab29c9b302ce29b85a843deb03f2f5c9eb0d83b9b2');
  let count = 0;
  for (const [name, original] of Object.entries(real)) {
    if (original && original.music) { const value = parseNativeMusicReply(JSON.stringify(original)); assert.equal(value.schema_version, 1, name); count++; }
  }
  for (const original of real.read_pages) { parseNativeMusicReply(JSON.stringify(original)); count++; }
  assert.equal(count, 22);
  assert.throws(() => parseNativeMusicReply(JSON.stringify(real.stale_revision_error_reply)), /MusicRevisionConflict/);
  assert.throws(() => parseNativeMusicReply(JSON.stringify(real.changed_request_error_reply)), /MusicImportChanged/);
});

test('actual Store 16+2 pages preserve 2 Ready, 15 Pending and retained Retired with no partial or playable fallback', async () => {
  const f = actualLibrary(page); await f.library.load();
  const view = f.library.view(); assert.equal(view.loaded, true); assert.equal(view.library_revision, '26');
  assert.equal(view.records.length, 18); assert.equal(view.tracks.length, 2);
  assert.equal(view.records.filter(record => record.phase === 'pending').length, 15);
  const retired = view.records.find(record => record.phase === 'retired');
  assert.equal(retired.bytes_retained, true); assert.equal(f.library.identity(retired.track_id), undefined);
  assert.deepEqual(plain(view.order), real.read_pages[0].music.order);
  assert.deepEqual(plain(view.records), real.read_pages.flatMap(value => value.music.tracks));
  for (const record of view.records.filter(record => record.phase === 'pending')) assert.equal(f.library.identity(record.track_id), undefined);
  assert.equal(f.wires.length, 2); assert.equal(f.imports.length, 0);
});

test('actual retained Pending is inspected then reconciled to actual Ready without importing replacement bytes', async () => {
  const f = actualLibrary(command => {
    assert.deepEqual(command.request, real.retained_request);
    if (command.action === 'import_inspect') return real.retained_pending_reply;
    assert.equal(command.action, 'reconcile'); return real.reconcile_reply;
  });
  const original = real.retained_pending_reply.music.tracks[0].request_json;
  f.library.restoreImportLiteral(original); assert.equal(f.wires.length, 0);
  await f.library.inspectImport(); const pending = f.library.view().import_plan;
  assert.equal(pending.phase, 'pending'); assert.equal(pending.record.bytes_retained, true); assert.equal(pending.qualified_ready, false);
  assert.equal(pending.original_import, original); await f.library.reconcileImport();
  const ready = f.library.view().import_plan;
  assert.equal(ready.qualified_ready, true); assert.equal(ready.phase, 'ready'); assert.equal(ready.unknown, false);
  assert.deepEqual(plain(ready.record), real.reconcile_reply.music.tracks[0]);
  assert.equal(ready.original_import, original); assert.equal(f.imports.length, 0);
  assert.deepEqual(f.wires, [pending.original_inspect, pending.original_reconcile]);
  f.library.finishImport(); assert.equal(f.library.view().import_plan, undefined);
});

test('actual full current track and Rust seek/next/restore proposals preserve native contracts and cause no platform transport', async () => {
  const f = actualLibrary(command => {
    if (command.action === 'read') return page(command);
    if (command.action === 'track_read') { assert.equal(command.track_id, 'fixture-second'); assert.equal(command.library_revision, '26'); return real.current_track_reply; }
    assert.equal(command.action, 'policy');
    const replies = { seek: real.seek_reply, next: real.next_reply, restore: real.restore_reply };
    assert.ok(replies[command.music_action]); return replies[command.music_action];
  });
  await f.library.load(); await f.library.readTrack('fixture-second'); assert.equal(f.library.view().unknown, false);
  const input = { library_revision: '26', index: '0', playing: true, blocked: false, position_ms: '0', duration_ms: '3000', value: '9000', flag: false };
  for (const action of ['seek', 'next', 'restore']) {
    const value = await f.library.playbackPolicy({ ...input, music_action: action });
    assert.deepEqual(plain(value), real[action + '_reply'].music.playback, action);
  }
  assert.equal(f.library.view().original_mutation, ''); assert.equal(f.library.view().selected_track_id, 'fixture-second');
  assert.equal(f.imports.length, 0);
});
