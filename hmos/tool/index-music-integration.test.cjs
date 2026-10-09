'use strict';
// Verbatim Index entrypoints joined to the actual MusicWorkbench/Library/Files/
// Playback/Platform/queue/hash composition. Store, document and AVPlayer APIs
// remain controlled seams; this is not an ArkUI render or device qualification.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { fixture, ready, settle, deferred } = require('./music-workbench-test-harness.cjs');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const filename = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets');
const source = fs.readFileSync(filename, 'utf8');
const methods = ['musicAction', 'musicReorder', 'beforeMusicPlay', 'toggleAttachmentMedia', 'foregroundChanged', 'start'];
function extract(name) {
  const match = new RegExp('^  private (?:async )?' + name + '\\(', 'm').exec(source);
  assert.ok(match, 'actual Index method ' + name);
  const prefix = 'class Extracted {\n', parsed = ts.createSourceFile('index-music.ts', prefix + source.slice(match.index) + '\n}', ts.ScriptTarget.ES2020, true, ts.ScriptKind.TS);
  const node = parsed.statements[0]?.members[0]; assert.ok(ts.isMethodDeclaration(node));
  return source.slice(match.index, match.index + node.end - prefix.length);
}
const compiled = ts.transpileModule('export class ActualIndexMusic {\n' + methods.map(extract).join('\n') + '\n}',
  { fileName: filename, reportDiagnostics: true, compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
assert.deepEqual((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), []);
function page(f) {
  const events = [], exports = {};
  vm.runInNewContext(compiled.outputText, { exports, Error, Command: class { action = ''; }, workbench: {
    async send(wire) { events.push(['store', JSON.parse(wire).action]); return { ok: true, cards: [{ id: 'ordinary-card' }] }; }
  } }, { filename });
  const p = new exports.ActualIndexMusic();
  Object.assign(p, { pageAlive: true, foreground: true, music: f.controller, musicView: f.controller.view(), musicEpoch: 0,
    mediaToggleWorking: false, attachmentPreviewOpen: false, attachmentPreviewClosing: false, attachmentPreviewKind: 'audio',
    attachmentPreviewEpoch: 1, mediaView: { token: 'attachment-1' }, attachmentPreviewFailure: '', editorOpen: false,
    editorInputEpoch: 0, directInput: { stop() {} }, inputPolicy: { stop() {} }, fieldPolicy: { stop() {} },
    todoDragPosition() {}, exitMediaFullscreen() {}, fileOpen: undefined, clipboardPaste: undefined,
    getUIContext: () => ({ getHostContext: () => ({ filesDir: '/app/files' }) }),
    workspaceConditionsChanged: () => events.push(['workspace']), loadDrafts: async () => events.push(['drafts']),
    loadBusinessIntents: async () => events.push(['business']),
    mediaPlayback: { async pauseForBackground() { events.push(['attachment-pause']); }, view: () => ({ phase: 'idle' }),
      async toggle(token) { events.push(['attachment-toggle', token]); } }
  });
  f.options.beforePlay = () => p.beforeMusicPlay();
  return { p, events };
}
async function playing(f, p) { await f.controller.load(); await p.musicAction('toggle'); await settle(); assert.equal(f.controller.view().playback.phase, 'playing'); }

test('Index startup reads music after business recovery without opening a player or persisting selection', async () => {
  const f = fixture({ initial: [ready()], selected: 'A' }), { p, events } = page(f);
  await p.start(); await settle();
  assert.equal(p.ready, true); assert.equal(p.busy, false); assert.equal(p.cards[0].id, 'ordinary-card');
  assert.deepEqual(events.slice(0, 4), [['store', 'open'], ['workspace'], ['drafts'], ['business']]);
  assert.equal(f.players.length, 0); assert.equal(f.fds.size, 0);
  assert.ok(f.wires.length > 0); assert.ok(f.wires.every(w => JSON.parse(w).music.action === 'read'));
});
test('Index forwards actual first playback and rejects music while the media preview owns the foreground', async () => {
  const f = fixture({ initial: [ready()] }), { p } = page(f); await playing(f, p);
  p.attachmentPreviewOpen = true; await assert.rejects(p.beforeMusicPlay(), /附件媒体预览/);
  assert.equal(f.players.length, 1); assert.equal(f.controller.view().playback.phase, 'playing');
  await f.controller.dispose();
});
test('Index background uses separate page ownership, pauses and retains the original lease without foreground autoplay', async () => {
  const f = fixture({ initial: [ready()] }), { p } = page(f); await playing(f, p);
  const token = f.controller.view().playback.token, fd = f.players[0].descriptor.fd, wires = f.wires.length;
  p.foreground = false; f.setForeground(false); p.foregroundChanged(); await settle();
  assert.equal(p.musicEpoch, 1); assert.equal(f.controller.view().playback.phase, 'paused');
  p.foreground = true; f.setForeground(true); p.foregroundChanged(); await settle();
  assert.equal(p.musicEpoch, 2); assert.equal(f.controller.view().playback.phase, 'paused');
  assert.equal(f.controller.view().playback.token, token); assert.ok(f.fds.has(fd));
  assert.equal(f.players.length, 1); assert.equal(f.wires.length, wires);
  await f.controller.dispose();
});
test('Index does not dispatch music or media controls for departed, background or stale-token views', async () => {
  const f = fixture({ initial: [ready()] }), { p, events } = page(f);
  p.pageAlive = false; await p.musicAction('toggle'); await p.musicReorder(['A']);
  p.pageAlive = true; p.foreground = false; await p.musicAction('toggle');
  p.foreground = true; p.attachmentPreviewOpen = true; await p.toggleAttachmentMedia('stale');
  assert.equal(f.wires.length, 0); assert.equal(f.players.length, 0); assert.equal(events.length, 0);
});
test('Index attachment toggle awaits actual music pause before another player may play', async () => {
  const f = fixture({ initial: [ready()] }), { p, events } = page(f); await playing(f, p);
  p.attachmentPreviewOpen = true;
  p.mediaPlayback.toggle = async token => { assert.equal(f.controller.view().playback.phase, 'paused'); events.push(['attachment-toggle', token]); };
  await p.toggleAttachmentMedia('attachment-1');
  assert.equal(events.filter(e => e[0] === 'attachment-toggle').length, 1);
  assert.equal(f.players[0].calls.filter(c => c === 'pause').length, 1);
  await f.controller.dispose();
});
test('Index refuses attachment playback when an actual music pause fails', async () => {
  const f = fixture({ initial: [ready()] }), { p, events } = page(f); await playing(f, p);
  p.attachmentPreviewOpen = true; f.players[0].pause = async () => { throw Error('actual SDK pause rejected'); };
  await p.toggleAttachmentMedia('attachment-1'); await settle();
  assert.equal(events.filter(e => e[0] === 'attachment-toggle').length, 0);
  assert.ok(p.attachmentPreviewFailure); assert.equal(f.controller.view().playback.phase, 'failed');
  await f.controller.dispose();
});
test('Index invalidates an attachment toggle that returns after its preview epoch changes', async () => {
  const f = fixture({ initial: [ready()] }), { p, events } = page(f); await playing(f, p);
  const wait = deferred(), original = f.players[0].pause;
  f.players[0].pause = async () => { await wait.promise; await original(); };
  p.attachmentPreviewOpen = true; const pending = p.toggleAttachmentMedia('attachment-1'); await settle();
  p.attachmentPreviewEpoch++; wait.resolve(); await pending;
  assert.equal(events.filter(e => e[0] === 'attachment-toggle').length, 0); assert.equal(p.mediaToggleWorking, false);
  await f.controller.dispose();
});
