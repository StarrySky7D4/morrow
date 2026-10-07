// Explicit observed dev14 stages on one public development fixture. No automatic replay.
process.env.HMOS_DEVICE = process.env.HMOS_DEVICE || '127.0.0.1:5555';
process.env.HMOS_REPORT_DIR = process.env.HMOS_REPORT_DIR || require('node:path').resolve(__dirname, '../reports/ui-source/v14/device');
const { f, a, editorSeek } = require('./inline-device-check.cjs');
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const name = 'HMOS-media-20261007-A';
const files = { audio: 'HMOS-dev14-tone.wav', video: 'HMOS-dev14-bars.mp4', pdf: 'HMOS-dev14-generic.pdf' };
function record(stage, details = {}) {
  const file = path.join(f.out, 'progress-media.json');
  const prior = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file)) : { name, device: process.env.HMOS_DEVICE, checks: [] };
  assert.equal(prior.name, name); assert.ok(!prior.checks.some(x => x.stage === stage), 'completed stage cannot replay');
  prior.checks.push({ stage, ...details }); fs.writeFileSync(file, JSON.stringify(prior, null, 2) + '\n'); console.log('PASS', stage, details);
}
function asset(kind) {
  const n = editorSeek(n => n.text === files[kind], files[kind]);
  const entry = f.view().find(x => x.n.text === n.text && x.parents.some(p => p.id?.startsWith('draft-asset:')));
  assert.ok(entry); return entry.parents.find(p => p.id?.startsWith('draft-asset:')).id.slice('draft-asset:'.length);
}
function seed() {
  f.dismissNotice(); f.search(name); assert.equal(f.cards().length, 0); f.assertNoDraft(name);
  f.click(f.text('新建灵感')); f.input('draft-title', name);
  f.input('draft-description', '# Public media fixture\n\n12-second generated audio/video and a generated PDF.');
  f.capture('raw-media-fixture'); record('seed');
}
function importFile(kind) {
  assert.equal(editorSeek(n => n.id === 'draft-title', 'fixture title', true).text, name);
  assert.ok(!f.view().some(x => x.n.text === files[kind]), 'no import replay');
  f.click(editorSeek(n => n.id === 'editor-import', 'import'));
  let tree = a.pickerTree(); const confirm = tree.find(x => x.n.text === '知道了'); if (confirm) { f.click(confirm.n); }
  for (const text of ['浏览', '我的手机', 'Download']) { const item = a.pickerTree().find(x => x.n.text === text); if (item) { f.click(item.n); } }
  const file = a.pickerText(files[kind]); assert.ok(file, 'observed own public file'); f.click(file);
  const done = a.pickerTree().find(x => x.n.id === 'dialog_confirm'); if (done) { f.click(done.n); }
  const id = asset(kind); assert.ok(f.text('草稿已保留')); f.capture('imported-' + kind); record('import-' + kind, { asset_id: id });
}
function preview(kind) {
  const id = asset(kind); f.click(editorSeek(n => n.id === 'draft-asset-preview:' + id, 'media preview'));
  let n = f.nodes(); assert.ok(!n.some(x => x.id === 'attachment-media-playback-failure' || x.id === 'attachment-media-read-failure'));
  assert.equal(n.find(x => x.id === 'attachment-media-toggle')?.text, '播放', 'no autoplay');
  assert.equal(n.find(x => x.id === 'attachment-media-time')?.text, '0:00 / 0:12');
  assert.ok(n.some(x => x.id === (kind === 'audio' ? 'attachment-audio-canvas' : 'attachment-video-surface')));
  f.capture(kind + '-prepared'); record('preview-' + kind);
}
function playback(kind) {
  f.click(f.id('attachment-media-toggle'));
  let n = f.nodes(); assert.equal(n.find(x => x.id === 'attachment-media-toggle')?.text, '暂停');
  assert.match(n.find(x => x.id === 'attachment-media-time')?.text || '', /^0:0[2-9] \/ 0:12$/);
  f.click(n.find(x => x.id === 'attachment-media-toggle')); n = f.nodes(); assert.equal(n.find(x => x.id === 'attachment-media-toggle')?.text, '播放');
  const before = n.find(x => x.id === 'attachment-media-time')?.text; n = f.nodes(); assert.equal(n.find(x => x.id === 'attachment-media-time')?.text, before, 'paused time stable');
  const slider = n.find(x => x.id === 'attachment-media-seek'); assert.ok(slider); const [l,t,r,b] = slider.bounds;
  f.d.run('click', String(Math.round(l + (r-l)*0.65)), String(Math.round((t+b)/2)));
  n = f.nodes(); assert.match(n.find(x => x.id === 'attachment-media-time')?.text || '', /^0:0[6-9] \/ 0:12$/);
  f.capture(kind + '-paused-seek'); record('playback-' + kind, { before, after: n.find(x => x.id === 'attachment-media-time')?.text });
}
function fullscreen(kind) {
  f.click(f.id('attachment-media-fullscreen')); let n = f.nodes();
  assert.equal(n.find(x => x.id === 'attachment-media-fullscreen')?.text, '退出全屏');
  assert.ok(!n.some(x => x.id === 'attachment-fullscreen-failure')); const surface = n.find(x => x.id === 'attachment-video-surface' || x.id === 'attachment-audio-canvas');
  f.capture(kind + '-fullscreen'); f.key('Back'); n = f.nodes();
  assert.equal(n.find(x => x.id === 'attachment-media-fullscreen')?.text, '全屏');
  assert.ok(n.some(x => x.id === 'attachment-preview-close')); assert.ok(!n.some(x => x.id === 'attachment-fullscreen-failure'));
  f.capture(kind + '-fullscreen-restored'); record('fullscreen-' + kind, { surfaceBounds: surface?.bounds });
}
function close(kind) {
  f.click(f.id('attachment-preview-close')); assert.ok(!f.id('attachment-preview-close'));
  assert.ok(!f.nodes().some(n => n.id === 'attachment-cleanup-retry')); record('close-' + kind);
}
function openPdf() {
  const id = asset('pdf'); f.click(editorSeek(n => n.id === 'draft-asset-open:' + id, 'system file open'));
  const tree = a.pickerTree(); fs.writeFileSync(path.join(f.out, 'pdf-system-window.json'), JSON.stringify(tree.map(x => x.n), null, 2) + '\n');
  f.d.run('screenshot', '--path', path.join(f.out, 'pdf-system-window.png')); console.log(tree.map(x => ({type:x.n.type,id:x.n.id,text:x.n.text})).filter(x=>x.id||x.text));
}
function retain() {
  assert.equal(editorSeek(n => n.id === 'draft-title', 'own draft title', true).text, name);
  assert.ok(f.text('草稿已保留')); f.click(f.text('保留草稿'));
  assert.ok(!f.id('draft-title')); record('retain-before-update');
}
function restore() {
  f.restart(); f.click(f.nodes().find(n => n.type === 'Button' && /^草稿(?: [0-9]+)?$/.test(n.text || '')));
  const entry = f.view().find(x => x.n.text === name && x.parents.some(p => p.id?.startsWith('draft-row:'))); assert.ok(entry);
  const row = entry.parents.find(p => p.id?.startsWith('draft-row:')); f.click(f.d.flatten([row]).find(x => x.n.text === '恢复编辑').n);
  assert.equal(editorSeek(n => n.id === 'draft-title', 'restored own title', true).text, name); record('restore-final-package');
}
const stage = process.argv[2];
if (stage === 'seed') seed();
else if (stage?.startsWith('import-')) importFile(stage.slice(7));
else if (stage?.startsWith('preview-')) preview(stage.slice(8));
else if (stage?.startsWith('playback-')) playback(stage.slice(9));
else if (stage?.startsWith('fullscreen-')) fullscreen(stage.slice(11));
else if (stage?.startsWith('close-')) close(stage.slice(6));
else if (stage === 'open-pdf') openPdf();
else if (stage === 'retain') retain();
else if (stage === 'restore') restore();
else throw Error('Explicit observed stage required');
