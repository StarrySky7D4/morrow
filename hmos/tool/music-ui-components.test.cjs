'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const { harness } = require('./music-ui-component-test-harness.cjs');

test('actual component uses selected stable id and only actual playing phase', () => {
  const h = harness(); assert.equal(h.page.current().track_id, 'track-b'); assert.equal(h.page.playing(), false);
  h.view.playback.phase = 'playing'; assert.equal(h.page.playing(), true);
  h.view.playback.trackId = 'track-a'; assert.equal(h.page.playing(), false);
  h.view.library.selected_track_id = 'missing'; assert.equal(h.page.current(), undefined); assert.equal(h.page.canControl(), false);
});
test('actual time display and seek clamp invalid, negative and out-of-duration input', () => {
  const h = harness(); assert.equal(h.page.clock(61001), '1:01'); assert.equal(h.page.clock(NaN), '0:00'); assert.equal(h.page.clock(-1), '0:00');
  h.page.issue('seek', '', 150000.8); assert.deepEqual(h.calls[0], ['seek', '', 120000, '']);
  h.page.issue('seek', '', -20); assert.deepEqual(h.calls[1], ['seek', '', 0, '']);
  h.page.issue('seek', '', NaN); assert.equal(h.calls.length, 2);
  h.view.playback.durationMs = 0; h.page.issue('seek', '', 1); assert.equal(h.calls.length, 2); assert.equal(h.page.positionMs(), 0);
});
test('actual callbacks use Controller contract and gate unknown, import and pending writes', () => {
  const h = harness(); h.page.issue('add'); h.page.issue('select', 'track-c'); h.page.issue('lyrics_toggle', '', 1);
  assert.deepEqual(h.calls, [['add', '', 0, ''], ['select', 'track-c', 0, ''], ['lyrics_toggle', '', 1, '']]);
  for (const field of ['unknown', 'busy']) { h.view.library[field] = true; h.page.issue('remove', 'track-b'); assert.equal(h.calls.length, 3); h.view.library[field] = false; }
  h.view.library.original_mutation = 'original'; h.page.issue('add'); assert.equal(h.calls.length, 3); h.view.library.original_mutation = '';
  h.view.library.import_plan = {}; h.page.issue('add'); assert.equal(h.calls.length, 3); h.view.library.import_plan = undefined;
  h.view.canWrite = false; h.page.issue('select', 'track-a'); assert.equal(h.calls.length, 3);
});
test('actual playlist mutation rejects missing identity and out-of-range move', () => {
  const h = harness(); h.page.issue('move', 'track-a', -1); h.page.issue('move', 'track-c', 1); h.page.issue('move', 'track-b', 2);
  h.page.issue('remove', 'missing'); assert.equal(h.calls.length, 0);
  h.page.issue('move', 'track-b', -1); assert.deepEqual(h.calls, [['move', 'track-b', -1, '']]);
  h.view.library.records = new Array(512).fill({}); h.page.issue('add'); assert.equal(h.calls.length, 1);
});
test('actual transport rejects background, owner gone, busy and failed sessions', () => {
  const h = harness(); h.page.issue('toggle'); assert.equal(h.calls.length, 1);
  for (const phase of ['idle', 'loading', 'failed', 'closing', 'cleanup_failed']) { h.view.playback.phase = phase; h.page.issue('toggle'); assert.equal(h.calls.length, 1); }
  h.view.playback.phase = 'paused'; h.view.playback.background = true; h.page.issue('toggle'); assert.equal(h.calls.length, 1);
  h.view.playback.background = false; h.view.working = true; h.page.issue('toggle'); assert.equal(h.calls.length, 1);
  h.view.working = false; h.page.aboutToDisappear(); h.page.issue('toggle'); assert.equal(h.calls.length, 1);
});
test('actual selected idle track starts by explicit toggle while seek stays disabled', () => {
  const h = harness(); h.view.playback.phase = 'idle'; h.view.playback.trackId = ''; h.view.playback.durationMs = 0;
  h.page.issue('toggle'); assert.deepEqual(h.calls, [['toggle', '', 0, '']]); h.page.issue('seek', '', 1000); assert.equal(h.calls.length, 1);
});
test('actual owned playing pause remains available during library Unknown and ongoing mutation', () => {
  const h = harness(); h.view.playback.phase = 'playing'; h.view.playback.token = 'actual-owned-player-token';
  h.view.controlsEnabled = false; h.view.canWrite = false; h.view.library.unknown = true; h.view.library.original_mutation = 'original';
  h.view.working = true; h.view.library.busy = true; h.page.issue('toggle'); assert.deepEqual(h.calls, [['toggle', '', 0, '']]);
  h.page.issue('next'); h.page.issue('seek', '', 1000); h.page.issue('select', 'track-a'); assert.equal(h.calls.length, 1);
  h.view.playback.busy = true; h.page.issue('toggle'); assert.equal(h.calls.length, 1); h.view.playback.busy = false;
  h.view.playback.background = true; h.page.issue('toggle'); assert.equal(h.calls.length, 1);
});
test('actual recovery flags bypass ordinary write gate without inventing operations', () => {
  const h = harness(); h.view.controlsEnabled = false; h.view.library.unknown = true;
  const pairs = [['canRetryRead', 'read_retry'], ['canRetryMutation', 'mutation_retry'], ['canDiscardKnownMutation', 'mutation_discard_known'],
    ['canRetryBegin', 'begin_retry'], ['canInspectImport', 'import_inspect'], ['canContinueImport', 'import_continue'],
    ['canReconcileImport', 'import_reconcile'], ['canFinishImport', 'import_finish']];
  for (const [flag, action] of pairs) { h.page.issue(action); assert.equal(h.calls.some(c => c[0] === action), false); h.view[flag] = true;
    h.page.issue(action); assert.deepEqual(h.calls.at(-1), [action, '', 0, '']); h.view[flag] = false; }
});
test('actual retained spool resume accepts only exact actual pending choice and spool', () => {
  const h = harness(); h.view.spools = [{ spoolId: 'music-one', name: 'actual.wav', canStart: false, choices: [{ trackId: 'pending-native-id', title: 'Actual pending', phase: 'Pending' }] }];
  h.page.issue('spool_resume', 'invented-id', 0, 'music-one'); h.page.issue('spool_resume', 'pending-native-id', 0, 'invented-spool');
  h.page.issue('spool_start', '', 0, 'music-one'); assert.equal(h.calls.length, 0);
  h.page.issue('spool_resume', 'pending-native-id', 0, 'music-one'); assert.deepEqual(h.calls, [['spool_resume', 'pending-native-id', 0, 'music-one']]);
  h.view.spools[0].canStart = true; h.page.issue('spool_start', '', 0, 'music-one'); assert.deepEqual(h.calls[1], ['spool_start', '', 0, 'music-one']);
  h.page.issue('spool_discard', '', 0, 'music-one'); assert.equal(h.calls.length, 2);
});
test('actual saved original spool has an inspect entry without a fabricated track or new request', () => {
  const h = harness(); h.view.spools = [{ spoolId: 'music-original', name: 'original.wav', originalSaved: true, canStart: false, choices: [] }];
  h.page.issue('spool_resume', 'invented-id', 0, 'music-original'); h.page.issue('spool_start', '', 0, 'music-original'); assert.equal(h.calls.length, 0);
  h.page.issue('spool_resume', '', 0, 'music-original'); assert.deepEqual(h.calls, [['spool_resume', '', 0, 'music-original']]);
  assert.match(h.panelSource, /if \(spool\.originalSaved\).*'spool_resume', '', spool\.spoolId/);
});
test('actual resource recovery uses cleanup_failed and preserves unavailable controls', () => {
  const h = harness(); h.page.issue('retry_close'); h.page.issue('retry_io'); assert.equal(h.calls.length, 0);
  h.view.playback.phase = 'cleanup_failed'; h.page.issue('retry_close'); assert.deepEqual(h.calls[0], ['retry_close', '', 0, '']);
  h.view.diagnostics = ['Original fd close is unconfirmed']; h.page.issue('retry_io'); h.page.issue('cleanup_cache'); assert.equal(h.calls.length, 3);
  h.view.working = true; h.page.issue('retry_close'); h.page.issue('cleanup_cache'); assert.equal(h.calls.length, 3);
});
test('actual full unsaved lyrics remain viewable while mutation is Unknown', () => {
  const h = harness(); h.view.library.unknown = true; h.view.library.original_mutation = 'original-full-lyrics'; h.view.controlsEnabled = false;
  h.view.unsavedLyrics = '[00:01.00]汉字 🧪 é\n' + 'full uncropped text\n'.repeat(5000);
  h.page.issue('lyrics_import'); assert.equal(h.calls.length, 0); h.page.issue('lyrics_view'); assert.deepEqual(h.calls, [['lyrics_view', '', 0, '']]);
  assert.equal(h.dialog.lyricsText(), h.view.unsavedLyrics); h.view.lyricsText = 'persisted text'; assert.equal(h.dialog.lyricsText(), h.view.unsavedLyrics);
  h.dialog.close(); assert.deepEqual(h.calls.at(-1), ['lyrics_close', '', 0, '']);
});
test('actual unsaved original lyrics entry keeps its real track id and original title', () => {
  const h = harness(); h.view.unsavedLyricsTrackId = 'track-c'; h.view.unsavedLyricsTitle = 'Original track C';
  h.view.library.selected_track_id = 'track-a'; h.view.library.unknown = true; h.view.controlsEnabled = false;
  h.view.lyricsTitle = 'Viewer track A'; h.view.lyricsSource = 'Saved source A';
  h.page.issue('lyrics_view', 'track-c'); assert.deepEqual(h.calls, [['lyrics_view', 'track-c', 0, '']]);
  assert.match(h.panelSource, /'查看未保存的原歌词', 'lyrics_view', this\.view\.unsavedLyricsTrackId/);
  h.view.unsavedLyrics = 'Complete original track C candidate'; assert.equal(h.dialog.lyricsTitle(), 'Original track C');
  assert.equal(h.dialog.lyricsSource(), '已读取完整歌词，尚未保存。'); assert.equal(h.dialog.lyricsText(), h.view.unsavedLyrics);
  assert.match(h.footerSource, /if \(!this\.view\.unsavedLyrics && this\.view\.lyricsUntimed\)/);
  h.view.unsavedLyrics = ''; h.view.lyricsText = 'Saved complete track A lyrics';
  assert.equal(h.dialog.lyricsTitle(), 'Viewer track A'); assert.equal(h.dialog.lyricsSource(), 'Saved source A'); assert.equal(h.dialog.lyricsText(), h.view.lyricsText);
  h.page.issue('lyrics_view', 'outside'); assert.equal(h.calls.length, 1);
});
test('actual lyrics status refuses the previously selected track text', () => {
  const h = harness(); h.view.library.lyrics = { track_id: 'track-a', text: 'wrong lyrics', source: 'source' }; assert.equal(h.page.hasCurrentLyrics(), false);
  h.view.library.lyrics.track_id = 'track-b'; assert.equal(h.page.hasCurrentLyrics(), true);
});
function dragHarness() {
  const h = harness(); h.page.togglePlaylist(); h.page.listArea(h.area(100, 120));
  h.view.library.order.forEach((id, n) => h.page.rowArea(id, h.area(100 + n * 40))); return h;
}
test('actual held reorder moves by stable identities and measured target half', () => {
  const h = dragHarness(), track = h.view.library.tracks[0]; h.page.beginDrag(h.page.ticket(track)); h.page.dragMoved(h.event(190)); h.page.finishDrag(true);
  assert.deepEqual(h.reorders, [['track-b', 'track-a', 'track-c']]); assert.equal(h.timers.size, 0);
  h.page.beginDrag(h.page.ticket(track)); h.page.dragMoved(h.event(215)); h.page.finishDrag(true);
  assert.deepEqual(h.reorders[1], ['track-b', 'track-c', 'track-a']);
});
test('actual held reorder rejects revision, import identity and order drift', () => {
  for (const mutate of [h => h.view.library.library_revision = '8', h => h.view.library.tracks[0].import_operation = 'other-import',
    h => h.view.library.tracks[0].sha256 = 'f'.repeat(64), h => h.view.library.order.reverse()]) {
    const h = dragHarness(); h.page.beginDrag(h.page.ticket(h.view.library.tracks[0])); h.page.dragMoved(h.event(215)); mutate(h); h.page.finishDrag(true);
    assert.deepEqual(h.reorders, []); assert.equal(h.timers.size, 0);
  }
});
test('actual held reorder rejects removed, external and out-of-viewport target', () => {
  for (const y of [50, 240, NaN]) { const h = dragHarness(); h.page.beginDrag(h.page.ticket(h.view.library.tracks[0])); h.page.dragMoved(h.event(y)); h.page.finishDrag(true); assert.deepEqual(h.reorders, []); }
  const h = dragHarness(); const forged = h.page.ticket(h.view.library.tracks[0]); forged.trackId = 'outside'; h.page.beginDrag(forged); assert.equal(h.timers.size, 0);
  const ticket = h.page.ticket(h.view.library.tracks[0]); h.page.beginDrag(ticket); h.page.dragMoved(h.event(215)); h.view.canWrite = false; h.page.finishDrag(true); assert.deepEqual(h.reorders, []);
});
test('actual drag edge scrolling shares retained scroller and stops on owner loss', () => {
  const h = dragHarness(), scroller = h.page.playlistScroller; h.page.beginDrag(h.page.ticket(h.view.library.tracks[0]));
  h.page.dragMoved(h.event(215)); h.tick(); assert.deepEqual(scroller.moves, [[0, 12]]);
  h.page.dragMoved(h.event(105)); h.tick(); assert.deepEqual(scroller.moves.at(-1), [0, -12]);
  h.view.library.unknown = true; h.tick(); assert.equal(h.timers.size, 0); assert.equal(h.page.drag, undefined);
  h.page.togglePlaylist(); h.page.togglePlaylist(); assert.equal(h.page.playlistScroller, scroller); assert.equal(h.page.playlistVisited, true);
});
test('actual collapse, cancellation and disappearance stop held reorder timers', () => {
  const h = dragHarness(); h.page.beginDrag(h.page.ticket(h.view.library.tracks[0])); h.page.dragMoved(h.event(215)); h.page.togglePlaylist();
  assert.equal(h.timers.size, 0); assert.deepEqual(h.reorders, []); assert.equal(h.page.playlistVisited, true);
  h.page.togglePlaylist(); h.page.beginDrag(h.page.ticket(h.view.library.tracks[0])); h.page.aboutToDisappear(); assert.equal(h.timers.size, 0); assert.deepEqual(h.reorders, []);
});
test('actual drag refuses multiple fingers and unmeasured or corrupt geometry', () => {
  const h = dragHarness(); h.page.rowArea('track-c', h.area(NaN)); h.page.beginDrag(h.page.ticket(h.view.library.tracks[0]));
  h.page.dragMoved(h.event(215)); h.page.finishDrag(true); assert.deepEqual(h.reorders, []);
  h.page.rowArea('track-c', h.area(180)); h.page.beginDrag(h.page.ticket(h.view.library.tracks[0]));
  h.page.dragMoved({ fingerList: [{ globalY: 215 }, { globalY: 215 }] }); h.page.finishDrag(true); assert.deepEqual(h.reorders, []);
});
test('actual revision-only CAS preserves fixed-row measurements; changed order discards them', () => {
  const h = dragHarness(); h.view.library.library_revision = '8'; h.page.viewChanged();
  assert.equal(h.page.rowAreas.get('track-c').revision, '8'); h.page.beginDrag(h.page.ticket(h.view.library.tracks[0])); h.page.dragMoved(h.event(215)); h.page.finishDrag(true);
  assert.equal(h.reorders.length, 1); h.view.library.order.reverse(); h.page.viewChanged(); assert.equal(h.page.rowAreas.size, 0);
});
test('actual Footer follows playing and lyrics flag, with default tips preserved', () => {
  const h = harness(); h.view.footerText = 'Native timeline line'; h.view.library.show_lyrics = true;
  assert.equal(h.footer.footerText(), h.footer.tipText); h.view.playback.phase = 'playing'; assert.equal(h.footer.footerText(), 'Native timeline line');
  h.footer.toggleLyrics(); assert.deepEqual(h.calls, [['lyrics_toggle', '', 0, '']]);
  h.view.library.show_lyrics = false; assert.equal(h.footer.footerText(), h.footer.tipText); h.footer.toggleLyrics(); assert.deepEqual(h.calls[1], ['lyrics_toggle', '', 1, '']);
  h.view.playback.trackId = 'track-a'; h.footer.toggleLyrics(); assert.equal(h.calls.length, 2); assert.equal(h.footer.footerText(), h.footer.tipText);
});
test('actual Footer rejects Unknown, cleanup, disabled and background persistent toggle', () => {
  const h = harness(); h.view.playback.phase = 'playing';
  for (const mutate of [() => h.view.library.unknown = true, () => h.view.library.original_mutation = 'original', () => h.view.canWrite = false,
    () => h.view.working = true, () => h.view.playback.background = true]) { mutate(); h.footer.toggleLyrics(); assert.equal(h.calls.length, 0); }
});
test('actual dialog preserves complete Unicode and 1 MiB candidate text without cropping', () => {
  const h = harness(), value = '[00:00.00]🧪 汉字 é\n' + 'line\n'.repeat(190000); h.view.lyricsText = value;
  assert.equal(h.dialog.lyricsText(), value); assert.equal(h.dialog.lyricsText().length, value.length);
  assert.match(h.footerSource, /Text\(this\.lyricsText\(\)\)/); const dialog = h.footerSource.slice(h.footerSource.indexOf('export struct LyricsDialogContent'));
  assert.doesNotMatch(dialog, /maxLines\(|substring\(|slice\(|textOverflow\(/); assert.match(dialog, /copyOption\(CopyOptions\.InApp\)/);
});
test('actual component source aligns Flutter scales and retained playlist layout', () => {
  const h = harness(); assert.match(h.panelSource, /duration: 380/); assert.match(h.panelSource, /width\(36\)\.height\(40\)/);
  assert.match(h.panelSource, /width\(48\)\.height\(48\)/); assert.match(h.panelSource, /Math\.min\(240,/);
  assert.match(h.panelSource, /fontSize\(12\)\.fontWeight\(600\)/); assert.match(h.panelSource, /if \(this\.playlistVisited\) \{ this\.playlist\(\) \}/);
  assert.doesNotMatch(h.panelSource, /if \(this\.expanded\) \{ this\.playlist\(\) \}/);
  assert.match(h.panelSource, /height\(this\.expanded \? undefined : 0\)/);
  assert.doesNotMatch(h.panelSource, /backdropBlur\(|\.shadow\(/);
  const footer = h.footerSource.slice(0, h.footerSource.indexOf('export struct LyricsDialogContent'));
  assert.doesNotMatch(footer, /backdropBlur\(|\.shadow\(|\.border\(|linearGradient\(/);
  assert.match(footer, /fontSize\(11\)\.lineHeight\(18\.7\)/); assert.match(footer, /maxLines\(3\)/);
  assert.doesNotMatch(h.panelSource, /private position\(/); assert.match(h.panelSource, /Progress\(\{ value: 0, type: ProgressType\.Linear \}\)/);
});
