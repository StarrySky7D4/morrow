'use strict';
const crypto = require('node:crypto');
const { load, deferred } = require('./editor-field-test-harness.cjs');
const model = load('MusicLibrary'), plain = value => JSON.parse(JSON.stringify(value));
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const tick = () => new Promise(resolve => setImmediate(resolve));
function request(id = 'track-A', revision = '0') {
  return Object.assign(new model.MusicImportRequest(), { track_id: id, operation_id: 'import-' + id, expected_revision: revision,
    name: id + '.wav', byte_length: '64', sha256: sha('complete owned audio ' + id) });
}
function importLiteral(r) { return JSON.stringify({ action: 'music_import', music: { action: 'import_file', request: plain(r) } }); }
function readyOperation(r) { return 'morrow-host-music-ready-' + sha('morrow.hmos.music.v1\0ready\0' + r.track_id + '\0' + r.operation_id); }
function track(r, revision = '1', phase = 'ready') {
  const original = importLiteral(r);
  return { track_id: r.track_id, library_revision: revision, import_operation: r.operation_id, name: r.name,
    byte_length: r.byte_length, sha256: r.sha256, phase, source_uri: 'file:///morrow-music/' + r.track_id,
    title: r.name.replace(/\.[^.]+$/, ''), artist: '', duration_ms: '0', lyric_source: '', lyrics_byte_length: '0',
    bytes_retained: phase !== 'pending', request: plain(r), request_json: original, request_sha256: sha(original) };
}
function summary(records = [], patch = {}) {
  const order = records.filter(x => x.phase === 'ready').map(x => x.track_id);
  return { schema_version: 1, kind: 'library', library_revision: records[0]?.library_revision || '0', order,
    selected_track_id: order[0] || '', show_lyrics: false, online_lyrics: false, tracks: plain(records), next_after: '',
    repeated: false, operation_id: '', operation_revision: '', ...patch };
}
function reply(value, effect = 'not_committed', patch = {}) {
  return JSON.stringify({ ok: true, error: '', effect, receipt_revision: effect === 'committed' ? value.operation_revision : '', music: value, ...patch });
}
// A controlled receiver for ETS ownership/request tests, explicitly not Engine,
// native Store, crash/reopen or an implementation of the Rust music policy.
function fixture(initial = [], options = {}) {
  let owned = true, owner = 'music-owner-1', revision = initial[0]?.library_revision || '0';
  const records = initial.map(plain), order = initial.filter(t => t.phase === 'ready').map(t => t.track_id);
  let selected = order[0] || '', show = false, library;
  const wires = [], imports = [], persisted = [], hashes = [], snapshots = [], history = new Map(), lyrics = new Map();
  const current = (kind = 'library', shown = []) => summary(shown, { kind, library_revision: revision, order: order.slice(), selected_track_id: selected, show_lyrics: show });
  function bump() { revision = String(Number(revision) + 1); for (const t of records) t.library_revision = revision; }
  function write(command, receive) {
    const key = command.operation_id, encoded = JSON.stringify(command);
    if (history.has(key)) {
      const previous = history.get(key); if (previous.encoded !== encoded) throw new Error('Changed fixed operation');
      const value = current(command.track_id ? 'track' : 'library', command.track_id ? [records.find(t => t.track_id === command.track_id)] : []);
      return reply({ ...value, operation_id: key, operation_revision: previous.revision, repeated: true }, 'committed');
    }
    if (command.library_revision !== revision) return JSON.stringify({ ok: false, error: 'MusicRevisionConflict', effect: 'not_committed', receipt_revision: '' });
    receive(); bump(); history.set(key, { encoded, revision });
    const value = current(command.track_id ? 'track' : 'library', command.track_id ? [records.find(t => t.track_id === command.track_id)] : []);
    return reply({ ...value, operation_id: key, operation_revision: revision }, 'committed');
  }
  function receiver(wire) {
    const c = JSON.parse(wire).music;
    if (c.action === 'read') {
      const all = records.filter(t => t.track_id > c.after).sort((a, b) => a.track_id.localeCompare(b.track_id));
      const page = all.slice(0, c.limit); return reply({ ...current('library', page), next_after: all.length > c.limit ? page.at(-1).track_id : '' });
    }
    if (c.action === 'track_read') return reply(current('track', [records.find(t => t.track_id === c.track_id)]));
    if (c.action === 'lyrics_read') {
      const t = records.find(t => t.track_id === c.track_id), value = lyrics.get(c.track_id) || { text: '', source: '', lines: [], active_index: -1, untimed: true };
      return reply({ ...current('lyrics', [t]), lyrics: { track_id: t.track_id, ...plain(value) } });
    }
    if (c.action === 'policy') {
      if (!options.policy) throw new Error('Explicit controlled policy result required');
      return reply({ ...current('playback'), playback: plain(options.policy(c)) });
    }
    if (c.action === 'import_begin') {
      if (c.request.expected_revision !== revision) throw new Error('MusicRevisionConflict');
      bump(); const t = track(c.request, revision, 'pending'); records.push(t);
      return reply({ ...current('track', [t]), operation_id: c.request.operation_id, operation_revision: revision }, 'committed');
    }
    if (c.action === 'import_inspect' || c.action === 'reconcile') {
      const t = records.find(x => JSON.stringify(x.request) === JSON.stringify(c.request));
      if (!t) throw new Error('MusicImportMissing');
      if (c.action === 'reconcile' && t.phase === 'pending' && t.bytes_retained) {
        bump(); t.phase = 'ready'; order.push(t.track_id); selected ||= t.track_id;
      }
      const isReady = c.action === 'reconcile' && t.phase === 'ready';
      return reply({ ...current('track', [t]), operation_id: isReady ? readyOperation(t.request) : t.import_operation,
        operation_revision: isReady ? revision : String(Number(t.request.expected_revision) + 1), repeated: true }, isReady ? 'committed' : 'not_committed');
    }
    if (c.action === 'select') return write(c, () => { selected = c.track_id; });
    if (c.action === 'reorder') return write(c, () => { order.splice(0, order.length, ...c.order); });
    if (c.action === 'show_lyrics') return write(c, () => { show = c.flag; });
    if (c.action === 'remove') return write(c, () => {
      const i = order.indexOf(c.track_id); order.splice(i, 1); records.find(t => t.track_id === c.track_id).phase = 'retired';
      if (selected === c.track_id) selected = order[Math.min(i, order.length - 1)] || '';
    });
    if (c.action === 'set_lyrics') return write(c, () => {
      const t = records.find(t => t.track_id === c.track_id); t.lyric_source = c.lyric_source; t.lyrics_byte_length = String(Buffer.byteLength(c.lyrics));
      lyrics.set(t.track_id, { text: c.lyrics, source: c.lyric_source, lines: [], active_index: -1, untimed: true });
    });
    throw new Error('Unexpected controlled route: ' + c.action);
  }
  library = new model.MusicLibrary({
    async send(wire) { wires.push(wire); return options.send ? options.send(wire, receiver, wires.length) : receiver(wire); },
    async hash(wire) { hashes.push(wire); return options.hash ? options.hash(wire) : sha(wire); },
    async persistImport(wire, r) { persisted.push({wire,request:plain(r)}); if(options.persistImport)await options.persistImport(wire,r); },
    async importFile(wire, r) {
      imports.push({ wire, request: plain(r) });
      if (options.importFile) return options.importFile(wire, r, records);
      const t = records.find(x => x.track_id === r.track_id); t.bytes_retained = true; bump(); t.phase = 'ready'; order.push(t.track_id); selected ||= t.track_id;
      return reply({ ...current('track', [t]), operation_id: readyOperation(r), operation_revision: revision }, 'committed');
    },
    owned: () => owned, owner: () => owner, changed: () => { if (library) snapshots.push(library.view()); }
  });
  return { library, wires, imports, persisted, hashes, snapshots, records, order, lyrics,
    setOwned(value) { owned = value; }, setOwner(value) { owner = value; }, bump,
    get revision() { return revision; }, receiver, current, setSelected(value) { selected = value; } };
}
module.exports = { ...model, plain, sha, tick, deferred, request, track, summary, reply, importLiteral, readyOperation, fixture };
