// Root-owned, explicitly observed dev.15 stages. Loading this module operates
// no device. A mutation is journaled BEFORE injection and is never replayed on
// an interrupted observation; use capture/reconcile instead.
'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const assert = require('node:assert/strict');
process.env.HMOS_DEVICE = process.env.HMOS_DEVICE || '127.0.0.1:5555';
process.env.HMOS_REPORT_DIR = process.env.HMOS_REPORT_DIR || path.resolve(__dirname, '../reports/ui-source/v15/multiselect-device');
const f = require('./card-workflow-device-check.cjs'), a = require('./attachment-device-check.cjs');
const name = 'HMOS-multi-20261007-A';
const body = name + '\n\nPublic sequential multi-file import fixture. No business card until the explicit save stage.';
const files = { audio: 'HMOS-dev14-tone.wav', video: 'HMOS-dev14-bars.mp4', pdf: 'HMOS-dev14-generic.pdf' };
const journal = path.join(f.out, 'progress-multiselect.json');
const hdc = 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const unique = entries => [...new Map(entries.map(x => [JSON.stringify([x.n.id, x.n.type, x.n.text, x.n.bounds]), x])).values()];

function state() {
  const value = fs.existsSync(journal) ? JSON.parse(fs.readFileSync(journal, 'utf8')) : {
    fixture: name, body, device: process.env.HMOS_DEVICE, expected_package_sha256: process.env.HMOS_HAP_SHA256 || null,
    stages: {}, picker_click_order: [], expected_order: [], draft_id: null, raw_card_id: null,
    business_card_id: null, business_save_attempts: 0, pins: [],
  };
  assert.equal(value.fixture, name); assert.equal(value.body, body); assert.equal(value.device, process.env.HMOS_DEVICE);
  return value;
}
function write(value) { fs.writeFileSync(journal, JSON.stringify(value, null, 2) + '\n'); }
function complete(stage, details = {}) {
  const value = state(); assert.ok(value.stages[stage], 'a recorded attempt is required');
  assert.notEqual(value.stages[stage].phase, 'complete', 'completed stage cannot replay');
  Object.assign(value, details);
  Object.assign(value.stages[stage], { phase: 'complete', completed_utc: new Date().toISOString() });
  write(value); console.log('PASS', stage, details); return value;
}
function mutation(stage, work) {
  const value = state(); assert.ok(!value.stages[stage], 'attempt exists; inspect and reconcile, never replay ' + stage);
  value.stages[stage] = { phase: 'attempted', attempted_utc: new Date().toISOString() };
  if (stage === 'business-save') { assert.equal(value.business_save_attempts, 0); value.business_save_attempts = 1; }
  write(value);
  try { return complete(stage, work() || {}); }
  catch (error) {
    const observed = state(); Object.assign(observed.stages[stage], { phase: 'unknown', observation_error: String(error) });
    write(observed); throw error;
  }
}
function visible(entry) {
  const [l, t, r, b] = entry.n.bounds || []; if (!(r - l > 8 && b - t > 18)) return false;
  const x = (l + r) / 2, y = (t + b) / 2;
  return entry.parents.filter(p => p.bounds && (p.type === 'Scroll' || p === entry.parents[0])).every(p =>
    x >= p.bounds[0] && x <= p.bounds[2] && y >= p.bounds[1] && y <= p.bounds[3]);
}
// Select the outer editor Scroll, not the nested body/table Scroll. Swipe its
// right edge and require the target's center to be inside all scroll clips.
function editorSeek(predicate, label, up = false) {
  for (let index = 0; index < 12; index++) {
    const view = f.view(), item = unique(view.filter(x => predicate(x.n) && visible(x)));
    if (item.length) { assert.equal(item.length, 1, 'one observed editor target ' + label); return item[0].n; }
    const anchor = view.find(x => ['draft-title', 'draft-description', 'editor-import', 'markdown-preview'].includes(x.n.id)) ||
      view.find(x => x.n.id?.startsWith('draft-asset:'));
    const scroll = anchor?.parents.filter(p => p.type === 'Scroll').at(-1);
    assert.ok(scroll, 'observed outer editor Scroll ' + label);
    const [l, t, r, b] = scroll.bounds;
    f.d.run('swipe', String(r - 10), String(up ? t + 90 : b - 90), String(r - 10), String(up ? b - 90 : t + 90));
  }
  throw Error('visible editor control ' + label);
}
function fullTree() {
  const raw = f.d.run('layout', '--mode', 'full', '--all-windows', '--format', 'json');
  return { raw, entries: unique(f.d.flatten(JSON.parse(raw.slice(raw.indexOf('['))))) };
}
function capture(label) {
  assert.match(label, /^[a-z0-9][a-z0-9-]{0,70}$/);
  for (const suffix of ['.json', '.png', '-windows.json']) assert.ok(!fs.existsSync(path.join(f.out, label + suffix)), 'fresh evidence name');
  const value = fullTree(); fs.writeFileSync(path.join(f.out, label + '.json'), value.raw);
  fs.writeFileSync(path.join(f.out, label + '-windows.json'), f.d.run('window', 'list', '--all', '--format', 'json'));
  f.d.run('screenshot', '--path', path.join(f.out, label + '.png'));
  return value.entries;
}
function pickerTarget(predicate, label, requireSheet = true) {
  const entries = fullTree().entries;
  assert.ok(entries.some(x => x.n.type === 'SheetPage'), 'observed provider SheetPage');
  const matches = unique(entries.filter(x => predicate(x.n) && visible(x) &&
    (!requireSheet || x.parents.some(p => p.type === 'SheetPage'))));
  assert.equal(matches.length, 1, 'exact observed provider control ' + label); return matches[0].n;
}
function pickerClickLabel(text, label) {
  assert.ok(['知道了', '浏览', '我的手机', 'Download'].includes(text), 'own explicit navigation only');
  const control = pickerTarget(n => n.text === text, text);
  return mutation('picker-nav:' + label, () => { f.click(control); capture(label); });
}
function observePickerMaximum(observedText, label) {
  assert.ok(typeof observedText === 'string' && /20/.test(observedText), 'root supplies the actually observed maximum/count text');
  const control = pickerTarget(n => n.text === observedText, observedText, false); capture(label);
  const value = state(); value.picker_maximum_observation = { text: control.text, maximum: 20, evidence: label }; write(value);
  return value.picker_maximum_observation;
}
function fileName(kind) { assert.ok(Object.hasOwn(files, kind), 'own public fixture kind'); return files[kind]; }
function clickOwnFile(kind, label) {
  const filename = fileName(kind), value = state(); assert.ok(value.stages.open?.phase === 'complete');
  assert.ok(!value.picker_click_order.includes(filename), 'same file is never toggled/reselected automatically');
  const control = pickerTarget(n => n.text === filename, filename);
  return mutation('picker-file:' + kind, () => {
    f.click(control); const observed = state(); observed.picker_click_order.push(filename); write(observed);
    capture(label); return {};
  });
}
// Root supplies the exact observed confirmation ID/text. No Done button is
// assumed, and no confirmation is injected after an automatic provider return.
function confirm(observedId, label, observedText = undefined, requireSheet = true) {
  assert.ok(observedId || observedText, 'explicit observed confirmation identity');
  const value = state(); assert.equal(value.picker_click_order.length, 3, 'three own file clicks observed');
  const control = pickerTarget(n => (!observedId || n.id === observedId) && (!observedText || n.text === observedText),
    observedId || observedText, requireSheet);
  return mutation('confirm', () => { f.click(control); capture(label); return { confirmation_control: { id: control.id, text: control.text } }; });
}
function rowAssets(prefix) {
  return unique(f.view().filter(x => x.n.id?.startsWith(prefix))).map(x => {
    const names = f.d.flatten([x.n]).map(y => y.n.text).filter(text => Object.values(files).includes(text));
    assert.equal(new Set(names).size, 1, 'one exact public filename per observed asset');
    return { asset_id: x.n.id.slice(prefix.length), name: names[0] };
  });
}
function editorSnapshot() {
  const title = editorSeek(n => n.id === 'draft-title', 'fixture title', true).text || '';
  const description = editorSeek(n => n.id === 'draft-description', 'fixture body').text;
  assert.equal(description, body, 'exact unchanged own raw body');
  const pins = rowAssets('draft-asset:'); assert.equal(pins.length, new Set(pins.map(x => x.asset_id)).size);
  assert.equal(f.id('draft-status')?.text, '草稿已保留', 'confirmed latest draft, no inferred Unknown success');
  return { title, pins };
}
function inputBody() {
  const control = editorSeek(n => n.id === 'draft-description', 'own body'); f.click(control); f.key(2072, 2017);
  const inputFile = path.join(f.out, 'multiselect-input-text.sh');
  fs.writeFileSync(inputFile, "#!/bin/sh\nvalue=$(printf '%s' '" + Buffer.from(body).toString('base64') +
    "'|base64 -d; printf '.')\nexec uitest uiInput text \"${value%.}\"\n");
  cp.execFileSync(hdc, ['-t', process.env.HMOS_DEVICE, 'file', 'send', inputFile, '/data/local/tmp/hmos-dev15-multiselect-input.sh']);
  const reply = cp.execFileSync(hdc, ['-t', process.env.HMOS_DEVICE, 'shell', 'sh', '/data/local/tmp/hmos-dev15-multiselect-input.sh'], { encoding: 'utf8' });
  assert.ok(!/error|fail/i.test(reply) || /no error/i.test(reply)); f.key('Back');
}
function openDrafts() {
  const controls = unique(f.view().filter(x => x.n.type === 'Button' && /^草稿(?: [0-9]+)?$/.test(x.n.text || '')));
  assert.equal(controls.length, 1); const count = Number(controls[0].n.text.slice(2).trim() || 0); f.click(controls[0].n);
  assert.ok(f.text('保留的草稿')); return count;
}
function draftRows(expectedCount) {
  const collected = new Map();
  for (let index = 0; index < 18; index++) {
    const view = f.view(), rows = unique(view.filter(x => x.n.id?.startsWith('draft-row:')));
    for (const row of rows) collected.set(row.n.id, row.n);
    if (collected.size === expectedCount) return [...collected.values()];
    const scroll = rows[0]?.parents.filter(p => p.type === 'Scroll').at(-1); assert.ok(scroll, 'complete visible draft inventory');
    const [l, t, r, b] = scroll.bounds; f.d.run('swipe', String(r - 10), String(b - 70), String(r - 10), String(t + 70));
  }
  throw Error('could not observe all retained draft IDs; no fresh seed allowed');
}
function ownDraft(rows) {
  const matches = rows.filter(row => f.d.flatten([row]).some(x => (x.n.text || '').includes(name)));
  assert.equal(matches.length, 1, 'one exact own raw body'); return matches[0];
}
function seekDraftRestore(draftId) {
  for (let index = 0; index < 18; index++) {
    const view = f.view(), target = view.find(x => x.n.text === '恢复编辑' &&
      x.parents.some(p => p.id === 'draft-row:' + draftId) && visible(x));
    if (target) return target.n;
    const row = view.find(x => x.n.id === 'draft-row:' + draftId) || view.find(x => x.n.id?.startsWith('draft-row:'));
    const scroll = row?.parents.filter(p => p.type === 'Scroll').at(-1); assert.ok(scroll, 'observed own draft-list Scroll');
    const [l, t, r, b] = scroll.bounds; f.d.run('swipe', String(r - 10), String(b - 70), String(r - 10), String(t + 70));
  }
  throw Error('own raw draft restore control is not visible; no guessed click');
}
function seed() {
  assert.ok(!state().stages.seed, 'seed attempt must never replay');
  assert.ok(!f.id('draft-title') && !f.id('card-detail'), 'workspace required'); f.dismissNotice();
  assert.ok(f.text('最近的念头'), 'root must first observe unfiltered Overview'); f.search(name);
  assert.equal(f.cards().length, 0); assert.ok(f.text('没有找到匹配的想法'), 'completed zero-result query');
  const oldRows = draftRows(openDrafts());
  assert.ok(!oldRows.some(row => f.d.flatten([row]).some(x => (x.n.text || '').includes(name))), 'no same raw fixture');
  f.click(f.text('关闭'));
  const preflight = state(); preflight.preexisting_draft_ids = oldRows.map(row => row.id.slice('draft-row:'.length));
  preflight.fresh_preflight_utc = new Date().toISOString(); preflight.fresh_business_results = 0; write(preflight);
  return mutation('seed', () => {
    f.click(f.text('新建灵感')); assert.equal(editorSeek(n => n.id === 'draft-title', 'initial blank title', true).text || '', '');
    inputBody(); for (let index = 0; index < 4; index++) f.nodes();
    const snapshot = editorSnapshot(); assert.equal(snapshot.title, ''); assert.equal(snapshot.pins.length, 0);
    capture('seed-body-only'); return { preexisting_draft_ids: oldRows.map(row => row.id.slice('draft-row:'.length)) };
  });
}
function open(label = 'picker-open') {
  assert.equal(state().stages.seed?.phase, 'complete');
  const snapshot = editorSnapshot(); assert.equal(snapshot.title, ''); assert.equal(snapshot.pins.length, 0);
  const control = editorSeek(n => n.id === 'editor-import', 'observed import');
  return mutation('open', () => { f.click(control); assert.ok(capture(label).some(x => x.n.type === 'SheetPage'), 'actual picker observed'); });
}
function batchObserve(order, label = 'batch-three-confirmed') {
  const expected = order.map(value => Object.hasOwn(files, value) ? files[value] : value);
  assert.equal(expected.length, 3); assert.equal(new Set(expected).size, 3);
  assert.ok(expected.every(value => Object.values(files).includes(value)), 'root supplies observed actual provider order');
  const snapshot = editorSnapshot(); assert.equal(snapshot.title, expected[0]);
  assert.deepEqual(snapshot.pins.map(pin => pin.name), expected); assert.equal(snapshot.pins.length, 3);
  const progress = editorSeek(n => n.id === 'attachment-import-progress', 'batch progress', true);
  assert.ok(f.d.flatten([progress]).some(x => x.n.text === '3/3'), 'actual completed progress');
  assert.ok(!f.id('attachment-import-stop'), 'no pending batch'); capture(label);
  const value = state(); assert.ok(!value.stages['batch-observe']); value.stages['batch-observe'] = { phase: 'attempted' }; write(value);
  return complete('batch-observe', { expected_order: expected, expected_title: expected[0], pins: snapshot.pins });
}
function retain(label = 'retained-raw-identity') {
  assert.equal(state().stages['batch-observe']?.phase, 'complete');
  return mutation('retain', () => {
    f.click(f.text('保留草稿')); assert.ok(!f.id('draft-title'));
    const row = ownDraft(draftRows(openDrafts())), draftId = row.id.slice('draft-row:'.length);
    capture(label); f.click(f.text('关闭')); return { draft_id: draftId };
  });
}
function restore(label = 'restored-same-raw') {
  const prior = state(); assert.equal(prior.stages.retain?.phase, 'complete'); assert.ok(prior.draft_id);
  return mutation('restore', () => {
    f.restart(); const row = ownDraft(draftRows(openDrafts())); assert.equal(row.id, 'draft-row:' + prior.draft_id);
    f.click(seekDraftRestore(prior.draft_id));
    const snapshot = editorSnapshot(); assert.equal(snapshot.title, prior.expected_title); assert.deepEqual(snapshot.pins, prior.pins);
    capture(label); return {};
  });
}
function bindRawIdentity(cardId, draftId, evidence) {
  assert.match(cardId, /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i);
  const value = state(); assert.equal(draftId, value.draft_id);
  assert.ok(typeof evidence === 'string' && evidence.length > 0, 'independent observed raw-scope evidence required');
  assert.ok(!value.raw_card_id || value.raw_card_id === cardId);
  if (value.business_card_id) assert.equal(value.business_card_id, cardId);
  value.raw_card_id = cardId; value.raw_identity_evidence = evidence; write(value);
}
function businessSnapshot() {
  const prior = state(); assert.ok(!f.id('draft-title'), 'do not resubmit an uncertain save');
  const close = f.id('detail-close'); if (close) f.click(close);
  f.dismissNotice(); f.search(name);
  const cards = f.cards(); assert.equal(cards.length, 1, 'exactly one own business result');
  const cardId = cards[0].id.slice('workspace-card:'.length);
  if (prior.raw_card_id) assert.equal(cardId, prior.raw_card_id);
  if (prior.business_card_id) assert.equal(cardId, prior.business_card_id);
  const title = f.view().find(x => x.n.type === 'Text' && x.n.text === prior.expected_title &&
    x.parents.some(p => p.id === 'workspace-card:' + cardId)); assert.ok(title); f.click(title.n);
  assert.equal(f.id('detail-title')?.text, prior.expected_title);
  const assets = rowAssets('saved-asset:'); assert.deepEqual(assets, prior.pins);
  const detail = f.view().filter(x => x.parents.some(p => p.id === 'card-detail'));
  for (const paragraph of body.split('\n\n')) assert.ok(detail.some(x => (x.n.text || '').includes(paragraph)), 'exact own body paragraph remains readable');
  return { business_card_id: cardId };
}
function businessSave(label = 'business-save-once') {
  const value = state(); assert.ok(value.draft_id, 'record exact raw ID first');
  const snapshot = editorSnapshot(); assert.equal(snapshot.title, value.expected_title); assert.deepEqual(snapshot.pins, value.pins);
  return mutation('business-save', () => {
    const control = unique(f.view().filter(x => x.n.type === 'Button' && x.n.text === '保存灵感' && visible(x)));
    assert.equal(control.length, 1); f.click(control[0].n);
    for (let index = 0; index < 5 && f.id('draft-title'); index++) f.nodes();
    const result = businessSnapshot(); capture(label); return result;
  });
}
function readback(label = 'restarted-same-business-card') {
  const value = state(); assert.equal(value.stages['business-save']?.phase, 'complete'); assert.equal(value.business_save_attempts, 1);
  return mutation('readback', () => {
    f.restart(); const result = businessSnapshot(); assert.equal(result.business_card_id, value.business_card_id);
    capture(label); return result;
  });
}
function assertNoOwnDraft(label = 'no-own-journal-after-publication') {
  const value = state(); assert.ok(value.business_card_id && value.draft_id);
  assert.ok(!f.id('draft-title')); const close = f.id('detail-close'); if (close) f.click(close);
  const rows = draftRows(openDrafts());
  assert.ok(!rows.some(row => row.id === 'draft-row:' + value.draft_id ||
    f.d.flatten([row]).some(x => (x.n.text || '').includes(name))), 'exact own draft consumed, no replacement raw fixture');
  capture(label); f.click(f.text('关闭')); return { draft_id: value.draft_id, remaining: false };
}
// Read-only reconciliation after root has inspected a stopped attempt. Never
// calls seed/import/confirm/save again. Other stopped stages remain explicit.
function reconcile(stage, label) {
  const value = state(); assert.ok(value.stages[stage] && value.stages[stage].phase !== 'complete');
  let result = {};
  if (stage === 'seed') { const observed = editorSnapshot(); assert.equal(observed.title, ''); assert.equal(observed.pins.length, 0); }
  else if (stage === 'open') assert.ok(fullTree().entries.some(x => x.n.type === 'SheetPage'), 'same pending picker is now observed');
  else if (stage === 'business-save') result = businessSnapshot();
  else throw Error('capture current state; root must inspect this stage without mutation replay');
  capture(label); return complete(stage, result);
}
if (require.main === module) {
  const [stage, ...args] = process.argv.slice(2);
  if (stage === 'capture') capture(args[0]);
  else if (stage === 'seed') seed();
  else if (stage === 'open') open(args[0]);
  else if (stage === 'picker-click-label') pickerClickLabel(args[0], args[1]);
  else if (stage === 'click-own-file') clickOwnFile(args[0], args[1]);
  else if (stage === 'confirm') confirm(args[0], args[1], args[2]);
  else if (stage === 'batch-observe') batchObserve(args);
  else if (stage === 'retain') retain(args[0]);
  else if (stage === 'restore') restore(args[0]);
  else if (stage === 'business-save') businessSave(args[0]);
  else if (stage === 'readback') readback(args[0]);
  else if (stage === 'assert-no-own-draft') assertNoOwnDraft(args[0]);
  else if (stage === 'reconcile') reconcile(args[0], args[1]);
  else throw Error('explicit observed stage required; use module exports for independent raw identity binding');
}
module.exports = { f, a, name, body, files, state, capture, editorSeek, seed, open, pickerClickLabel,
  observePickerMaximum, clickOwnFile, confirm, batchObserve, retain, restore, bindRawIdentity, businessSave, readback, assertNoOwnDraft, reconcile };
