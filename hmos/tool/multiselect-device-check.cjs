// Root-owned, explicitly observed dev.16 stages. Loading this module operates
// no device. A mutation is journaled BEFORE injection and is never replayed on
// an interrupted observation; use capture/reconcile instead.
'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process'), crypto = require('node:crypto');
const assert = require('node:assert/strict');
process.env.HMOS_DEVICE = process.env.HMOS_DEVICE || '127.0.0.1:5555';
process.env.HMOS_REPORT_DIR = process.env.HMOS_REPORT_DIR || path.resolve(__dirname, '../reports/ui-source/v16/multiselect-device');
const f = require('./card-workflow-device-check.cjs'), a = require('./attachment-device-check.cjs');
const name = 'HMOS-multi-20261007-A';
const body = name + '\n\nPublic sequential multi-file import fixture. No business card until the explicit save stage.';
const files = { audio: 'HMOS-dev14-tone.wav', video: 'HMOS-dev14-bars.mp4', pdf: 'HMOS-dev14-generic.pdf' };
const journal = path.join(f.out, 'progress-multiselect.json');
const hdc = 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const unique = entries => [...new Map(entries.map(x => [JSON.stringify([x.n.id, x.n.type, x.n.text, x.n.bounds]), x])).values()];
const expectedVersion = process.env.HMOS_EXPECTED_VERSION || '0.1.0-hmos-dev.16';
assert.match(expectedVersion, /^0\.1\.0-hmos-dev\.[0-9]+$/);
const devNumber = Number(expectedVersion.split('.').at(-1)), expectedVersionCode = 1000000 + devNumber;
const expectedSha = (process.env.HMOS_EXPECTED_HAP_SHA256 || process.env.HMOS_EXPECTED_ZIP_SHA ||
  process.env.HMOS_HAP_SHA256 || process.env.ZIP_SHA || 'A971227AC2D39730C2228972B513DBAAB4049E1C9E9AAB49397517BFB0208C7C').toUpperCase();
assert.match(expectedSha, /^[A-F0-9]{64}$/, 'mandatory expected package SHA256');
const hapPath = path.resolve(process.env.HMOS_INSTALL_HAP || path.join(__dirname, '../.build/artifacts/dev' + devNumber + '/entry-default-unsigned.hap'));
const installLogPath = path.resolve(process.env.HMOS_INSTALL_LOG || path.join(__dirname, '../reports/ui-source/v' + devNumber + '/device-final-install.log'));
const installedBundlePath = path.resolve(process.env.HMOS_INSTALL_BUNDLE_JSON || path.join(__dirname, '../reports/ui-source/v' + devNumber + '/bundle-final.json'));
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
let activeStage, journalSerial = 0;
function journalIO(operation) {
  for (let attempt = 0; ; attempt++) {
    try { return operation(); }
    catch (error) {
      if (attempt >= 3 || !['UNKNOWN', 'EBUSY', 'EACCES', 'EPERM'].includes(error.code)) throw error;
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 50 * (attempt + 1));
    }
  }
}
function bundleIdentity(raw) {
  const start = raw.indexOf('{'); assert.ok(start >= 0, 'actual bm JSON'); const bundle = JSON.parse(raw.slice(start));
  assert.equal(bundle.name || bundle.applicationInfo?.name, 'dev.morrow.hmos');
  assert.equal(bundle.versionName, expectedVersion, 'exact installed version name');
  assert.equal(bundle.versionCode, expectedVersionCode, 'exact installed version code');
  return { name: 'dev.morrow.hmos', versionName: bundle.versionName, versionCode: bundle.versionCode };
}
function installBinding() {
  // The environment supplies constraints and paths, never proof. Hash the
  // actual archive and bind it to the exact successful installation path and
  // saved bm readback. bm does not expose the installed archive's byte hash.
  const archive = fs.readFileSync(hapPath), archiveHash = hash(archive);
  assert.equal(archiveHash, expectedSha, 'actual installation archive matches mandatory expected SHA256');
  const installLog = fs.readFileSync(installLogPath, 'utf8');
  const installedPaths = [...installLog.matchAll(/App install path:(.*?) msg:install bundle successfully\./g)].map(m => path.resolve(m[1].trim()));
  assert.ok(installedPaths.some(p => p.toLowerCase() === hapPath.toLowerCase()), 'successful install log names this exact archive');
  const bundleRaw = fs.readFileSync(installedBundlePath, 'utf8'), identity = bundleIdentity(bundleRaw);
  return { hap_path: hapPath, hap_sha256: archiveHash, hap_bytes: archive.length,
    install_log_path: installLogPath, install_log_sha256: hash(installLog),
    saved_bundle_path: installedBundlePath, saved_bundle_sha256: hash(bundleRaw), ...identity,
    device: process.env.HMOS_DEVICE,
    proof_scope: 'Host archive hash + exact path in successful install log + saved/fresh bm version; install log has no device serial and bm exposes no installed-byte hash' };
}

function state() {
  const binding = installBinding();
  const value = fs.existsSync(journal) ? JSON.parse(journalIO(() => fs.readFileSync(journal, 'utf8'))) : {
    fixture: name, body, device: process.env.HMOS_DEVICE, expected_version: expectedVersion,
    expected_version_code: expectedVersionCode, expected_package_sha256: expectedSha, installation_evidence: binding,
    stages: {}, picker_click_order: [], expected_order: [], draft_id: null, raw_card_id: null,
    business_card_id: null, business_save_attempts: 0, pins: [],
  };
  assert.equal(value.fixture, name); assert.equal(value.body, body); assert.equal(value.device, process.env.HMOS_DEVICE);
  assert.equal(value.expected_version, expectedVersion, 'journal version is immutable');
  assert.equal(value.expected_version_code, expectedVersionCode);
  assert.equal(value.expected_package_sha256, expectedSha, 'journal package hash is immutable');
  assert.deepEqual(value.installation_evidence, binding, 'installation evidence cannot drift between stages');
  return value;
}
function write(value) {
  assert.equal(value.expected_package_sha256, expectedSha); assert.equal(value.expected_version, expectedVersion);
  assert.deepEqual(value.installation_evidence, installBinding());
  if (fs.existsSync(journal)) {
    const previous = JSON.parse(journalIO(() => fs.readFileSync(journal, 'utf8')));
    if (previous.raw_card_id) {
      assert.equal(value.raw_card_id, previous.raw_card_id, 'raw card identity cannot be overwritten');
      assert.deepEqual(value.raw_identity_evidence, previous.raw_identity_evidence, 'independent raw identity proof is immutable');
    }
    if (previous.business_card_id) assert.equal(value.business_card_id, previous.business_card_id, 'published business ID cannot change');
  }
  const pending = journal + '.' + process.pid + '-' + (++journalSerial) + '.pending';
  try {
    journalIO(() => fs.writeFileSync(pending, JSON.stringify(value, null, 2) + '\n'));
    journalIO(() => fs.renameSync(pending, journal));
  } catch (error) { error.message += '; prior journal retained; pending local evidence: ' + pending; throw error; }
}
function freshVersion(stage, boundary) {
  const raw = cp.execFileSync(hdc, ['-t', process.env.HMOS_DEVICE, 'shell', 'bm', 'dump', '-n', 'dev.morrow.hmos'],
    { encoding: 'utf8', timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
  const label = stage.replace(/[^a-z0-9-]/gi, '-') + '-version-' + boundary + '-' + Date.now();
  const evidence = path.join(f.out, label + '.log'); assert.ok(!fs.existsSync(evidence), 'fresh bm evidence'); fs.writeFileSync(evidence, raw);
  let identity; try { identity = bundleIdentity(raw); } catch (error) { error.message += '; actual bm evidence: ' + evidence; throw error; }
  return { ...identity, device: process.env.HMOS_DEVICE, observed_utc: new Date().toISOString(), evidence, sha256: hash(raw) };
}
function begin(stage) {
  const value = state(); assert.ok(!value.stages[stage], 'attempt exists; inspect and reconcile, never replay ' + stage);
  value.stages[stage] = { phase: 'attempted', attempted_utc: new Date().toISOString() }; write(value);
  const version = freshVersion(stage, 'begin'), checked = state(); checked.stages[stage].version_begin = version; write(checked);
}
function complete(stage, details = {}) {
  const value = state(); assert.ok(value.stages[stage], 'a recorded attempt is required');
  assert.notEqual(value.stages[stage].phase, 'complete', 'completed stage cannot replay');
  const version = freshVersion(stage, 'complete');
  Object.assign(value, details);
  const qualification = ['business-save', 'readback'].includes(stage) && details.raw_identity_status === 'VERIFIED' ? 'PASS_FULL_IDENTITY' : 'PASS_SCOPED';
  Object.assign(value.stages[stage], { phase: 'complete', qualification, version_complete: version, completed_utc: new Date().toISOString() });
  write(value); console.log(qualification, stage, details); return value;
}
function mutation(stage, work) {
  assert.ok(!activeStage, 'stages may not nest');
  assert.ok(!state().stages[stage], 'attempt exists; inspect and reconcile, never replay ' + stage);
  try {
    begin(stage); activeStage = stage;
    if (stage === 'business-save') { const value = state(); assert.equal(value.business_save_attempts, 0); value.business_save_attempts = 1; write(value); }
    return complete(stage, work() || {});
  }
  catch (error) {
    const observed = state(); if (observed.stages[stage]) { Object.assign(observed.stages[stage], { phase: 'unknown', observation_error: String(error) }); write(observed); } throw error;
  } finally { activeStage = undefined; }
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
  if (!activeStage) { let entries; mutation('capture:' + label, () => { entries = capture(label); return { observation_only: true, evidence: label }; }); return entries; }
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
  return mutation('picker-nav:' + label, () => { const control = pickerTarget(n => n.text === text, text); f.click(control); capture(label); });
}
function observePickerMaximum(observedText, label) {
  assert.ok(typeof observedText === 'string' && /20/.test(observedText), 'root supplies the actually observed maximum/count text');
  return mutation('picker-maximum:' + label, () => {
    const control = pickerTarget(n => n.text === observedText, observedText, false); capture(label);
    return { picker_maximum_observation: { text: control.text, maximum: 20, evidence: label } };
  });
}
function fileName(kind) { assert.ok(Object.hasOwn(files, kind), 'own public fixture kind'); return files[kind]; }
function clickOwnFile(kind, label) {
  const filename = fileName(kind), value = state(); assert.ok(value.stages.open?.phase === 'complete');
  assert.ok(!value.picker_click_order.includes(filename), 'same file is never toggled/reselected automatically');
  return mutation('picker-file:' + kind, () => {
    const control = pickerTarget(n => n.text === filename, filename);
    f.click(control); const observed = state(); observed.picker_click_order.push(filename); write(observed);
    capture(label); return {};
  });
}
// Root supplies the exact observed confirmation ID/text. No Done button is
// assumed, and no confirmation is injected after an automatic provider return.
function confirm(observedId, label, observedText = undefined, requireSheet = true) {
  assert.ok(observedId || observedText, 'explicit observed confirmation identity');
  const value = state(); assert.equal(value.picker_click_order.length, 3, 'three own file clicks observed');
  return mutation('confirm', () => {
    const control = pickerTarget(n => (!observedId || n.id === observedId) && (!observedText || n.text === observedText),
      observedId || observedText, requireSheet);
    f.click(control); capture(label); return { confirmation_control: { id: control.id, text: control.text } };
  });
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
  return mutation('seed', () => {
    assert.ok(!f.id('draft-title') && !f.id('card-detail'), 'workspace required'); f.dismissNotice();
    assert.ok(f.text('最近的念头'), 'root must first observe unfiltered Overview'); f.search(name);
    assert.equal(f.cards().length, 0); assert.ok(f.text('没有找到匹配的想法'), 'completed zero-result query');
    const oldRows = draftRows(openDrafts());
    assert.ok(!oldRows.some(row => f.d.flatten([row]).some(x => (x.n.text || '').includes(name))), 'no same raw fixture');
    f.click(f.text('关闭'));
    const preflight = state(); preflight.preexisting_draft_ids = oldRows.map(row => row.id.slice('draft-row:'.length));
    preflight.fresh_preflight_utc = new Date().toISOString(); preflight.fresh_business_results = 0; write(preflight);
    f.click(f.text('新建灵感')); assert.equal(editorSeek(n => n.id === 'draft-title', 'initial blank title', true).text || '', '');
    inputBody(); for (let index = 0; index < 4; index++) f.nodes();
    const snapshot = editorSnapshot(); assert.equal(snapshot.title, ''); assert.equal(snapshot.pins.length, 0);
    capture('seed-body-only'); return { preexisting_draft_ids: oldRows.map(row => row.id.slice('draft-row:'.length)) };
  });
}
function open(label = 'picker-open') {
  assert.equal(state().stages.seed?.phase, 'complete');
  return mutation('open', () => {
    const snapshot = editorSnapshot(); assert.equal(snapshot.title, ''); assert.equal(snapshot.pins.length, 0);
    const control = editorSeek(n => n.id === 'editor-import', 'observed import');
    f.click(control); assert.ok(capture(label).some(x => x.n.type === 'SheetPage'), 'actual picker observed');
  });
}
function batchObserve(order, label = 'batch-three-confirmed') {
  const expected = order.map(value => Object.hasOwn(files, value) ? files[value] : value);
  assert.equal(expected.length, 3); assert.equal(new Set(expected).size, 3);
  assert.ok(expected.every(value => Object.values(files).includes(value)), 'root supplies observed actual provider order');
  return mutation('batch-observe', () => {
    const snapshot = editorSnapshot(); assert.equal(snapshot.title, expected[0]);
    assert.deepEqual(snapshot.pins.map(pin => pin.name), expected); assert.equal(snapshot.pins.length, 3);
    const progress = editorSeek(n => n.id === 'attachment-import-progress', 'batch progress', true);
    assert.ok(f.d.flatten([progress]).some(x => x.n.text === '3/3'), 'actual completed progress');
    assert.ok(!f.id('attachment-import-stop'), 'no pending batch'); capture(label);
    return { expected_order: expected, expected_title: expected[0], pins: snapshot.pins };
  });
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
function bindRawIdentity(draftId, cardId, evidencePath = process.env.HMOS_RAW_IDENTITY_EVIDENCE) {
  assert.match(cardId, /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i);
  const value = state(); assert.equal(draftId, value.draft_id);
  assert.equal(value.stages.retain?.phase, 'complete', 'exact retained draft has been independently observed');
  assert.equal(value.business_save_attempts, 0, 'raw identity must be bound BEFORE any business save attempt');
  assert.ok(!value.business_card_id, 'published UI cannot provide independent pre-save raw identity');
  assert.ok(evidencePath, 'actual independent native draft-list/read artifact required; UI does not expose raw card ID');
  const absolute = path.resolve(evidencePath), raw = fs.readFileSync(absolute, 'utf8'), evidence = JSON.parse(raw);
  assert.ok(['native.draft_list', 'native.draft_read'].includes(evidence.source), 'independent native reply, not business-card UI or a caller note');
  assert.equal(evidence.device, value.device); assert.equal(evidence.versionName, expectedVersion);
  assert.equal(evidence.package_sha256?.toUpperCase(), expectedSha);
  assert.ok(Number.isFinite(Date.parse(evidence.observed_utc)) && Date.parse(evidence.observed_utc) >= Date.parse(value.stages.retain.completed_utc),
    'native scope was read after this retained draft observation');
  assert.ok(Date.parse(evidence.observed_utc) <= Date.now(), 'native observation timestamp cannot be in the future');
  assert.equal(evidence.reply?.ok, true, 'acknowledged native draft read'); assert.equal(evidence.reply.error, '');
  const records = evidence.reply?.drafts || [];
  const matches = records.filter(record => record.scope?.draft_id === draftId);
  assert.equal(matches.length, 1, 'one independently read exact draft scope');
  assert.equal(matches[0].scope.card_id, cardId, 'actual native raw card ID');
  assert.equal(matches[0].values?.description?.text, body, 'native scope belongs to the exact fixture body');
  assert.equal(matches[0].values?.title?.text, value.expected_title);
  assert.deepEqual((matches[0].assets || []).map(asset => ({ asset_id: asset.selection?.asset_id, name: asset.display_name })), value.pins);
  assert.ok(!value.raw_card_id || value.raw_card_id === cardId, 'raw card binding is immutable');
  return mutation('bind-raw-identity', () => ({ raw_card_id: cardId, raw_identity_status: 'VERIFIED',
    raw_identity_evidence: { path: absolute, sha256: hash(raw), source: evidence.source, observed_utc: evidence.observed_utc,
      draft_id: draftId, card_id: cardId, independent_before_business_save: true } }));
}
function requireRawIdentity(value) {
  assert.ok(value.raw_card_id && value.raw_identity_evidence?.independent_before_business_save,
    'bindRawIdentity(draft_id, raw_card_id) with independent native evidence first; use explicit scoped stage when UI cannot expose it');
  const evidence = value.raw_identity_evidence;
  assert.equal(evidence.draft_id, value.draft_id); assert.equal(evidence.card_id, value.raw_card_id);
  assert.equal(hash(fs.readFileSync(evidence.path)), evidence.sha256, 'independent raw-scope artifact remains unchanged');
}
function businessSnapshot(identityScope = 'FULL') {
  const prior = state(); assert.ok(!f.id('draft-title'), 'do not resubmit an uncertain save');
  if (identityScope === 'FULL') requireRawIdentity(prior);
  else assert.equal(identityScope, 'SCOPED_UNKNOWN');
  const close = f.id('detail-close'); if (close) f.click(close);
  f.dismissNotice(); f.search(name);
  const cards = f.cards(); assert.equal(cards.length, 1, 'exactly one own business result');
  const cardId = cards[0].id.slice('workspace-card:'.length);
  if (identityScope === 'FULL' || prior.raw_card_id) assert.equal(cardId, prior.raw_card_id, 'published business ID equals independently bound raw card ID');
  if (prior.business_card_id) assert.equal(cardId, prior.business_card_id);
  const title = f.view().find(x => x.n.type === 'Text' && x.n.text === prior.expected_title &&
    x.parents.some(p => p.id === 'workspace-card:' + cardId)); assert.ok(title); f.click(title.n);
  assert.equal(f.id('detail-title')?.text, prior.expected_title);
  const assets = rowAssets('saved-asset:'); assert.deepEqual(assets, prior.pins);
  const detail = f.view().filter(x => x.parents.some(p => p.id === 'card-detail'));
  for (const paragraph of body.split('\n\n')) assert.ok(detail.some(x => (x.n.text || '').includes(paragraph)), 'exact own body paragraph remains readable');
  return { business_card_id: cardId, raw_identity_status: identityScope === 'FULL' ? 'VERIFIED' : 'UNKNOWN',
    identity_scope: identityScope, draft_id: prior.draft_id,
    ...(identityScope === 'FULL' ? { raw_card_id: prior.raw_card_id } : {}),
    identity_limit: identityScope === 'FULL' ? 'Independent pre-save native scope and published ID agree' :
      'Draft UI exposes draft_id; raw card_id is not exposed. Published business ID, body and pins only; no complete raw/business identity PASS' };
}
function saveWithScope(label, identityScope) {
  const value = state(); assert.ok(value.draft_id, 'record exact raw draft ID first');
  if (identityScope === 'FULL') requireRawIdentity(value);
  // Both entry points use the SAME journal stage and save-attempt counter. An
  // unknown full/scoped save can never be replayed by switching entry points.
  return mutation('business-save', () => {
    const attempt = state(); attempt.business_save_identity_scope = identityScope; write(attempt);
    const snapshot = editorSnapshot(); assert.equal(snapshot.title, value.expected_title); assert.deepEqual(snapshot.pins, value.pins);
    const control = unique(f.view().filter(x => x.n.type === 'Button' && x.n.text === '保存灵感' && visible(x)));
    assert.equal(control.length, 1); f.click(control[0].n);
    for (let index = 0; index < 5 && f.id('draft-title'); index++) f.nodes();
    const result = businessSnapshot(identityScope); capture(label); return result;
  });
}
function businessSave(label = 'business-save-once') { return saveWithScope(label, 'FULL'); }
function businessSaveScoped(label = 'business-save-scoped-once') { return saveWithScope(label, 'SCOPED_UNKNOWN'); }
function readbackWithScope(label, identityScope) {
  const value = state(); assert.equal(value.stages['business-save']?.phase, 'complete'); assert.equal(value.business_save_attempts, 1);
  if (identityScope === 'FULL') { requireRawIdentity(value); assert.equal(value.business_save_identity_scope, 'FULL'); }
  return mutation('readback', () => {
    f.restart(); const result = businessSnapshot(identityScope); assert.equal(result.business_card_id, value.business_card_id);
    capture(label); return result;
  });
}
function readback(label = 'restarted-same-business-card') { return readbackWithScope(label, 'FULL'); }
function readbackScoped(label = 'restarted-same-business-card-scoped') { return readbackWithScope(label, 'SCOPED_UNKNOWN'); }
function assertNoOwnDraft(label = 'no-own-journal-after-publication') {
  const value = state(); assert.ok(value.business_card_id && value.draft_id);
  return mutation('assert-no-own-draft', () => {
    assert.ok(!f.id('draft-title')); const close = f.id('detail-close'); if (close) f.click(close);
    const rows = draftRows(openDrafts());
    assert.ok(!rows.some(row => row.id === 'draft-row:' + value.draft_id ||
      f.d.flatten([row]).some(x => (x.n.text || '').includes(name))), 'exact own draft consumed, no replacement raw fixture');
    capture(label); f.click(f.text('关闭')); return { draft_id: value.draft_id, remaining: false, raw_identity_status: value.raw_identity_status || 'UNKNOWN' };
  });
}
// Read-only reconciliation after root has inspected a stopped attempt. Never
// calls seed/import/confirm/save again. Other stopped stages remain explicit.
function reconcile(stage, label) {
  const value = state(); assert.ok(value.stages[stage] && value.stages[stage].phase !== 'complete');
  assert.ok(!activeStage); activeStage = 'reconcile:' + label;
  try {
    const beginObservation = freshVersion(activeStage, 'begin'), updated = state();
    updated.stages[stage].reconcile_begin = beginObservation; write(updated);
    let result = {};
    if (stage === 'seed') { const observed = editorSnapshot(); assert.equal(observed.title, ''); assert.equal(observed.pins.length, 0); }
    else if (stage === 'open') assert.ok(fullTree().entries.some(x => x.n.type === 'SheetPage'), 'same pending picker is now observed');
    else if (stage === 'business-save') result = businessSnapshot(value.business_save_identity_scope || 'SCOPED_UNKNOWN');
    else throw Error('capture current state; root must inspect this stage without mutation replay');
    capture(label); return complete(stage, { ...result, reconciled_without_mutation_replay: true });
  } finally { activeStage = undefined; }
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
  else if (stage === 'business-save-scoped') businessSaveScoped(args[0]);
  else if (stage === 'readback') readback(args[0]);
  else if (stage === 'readback-scoped') readbackScoped(args[0]);
  else if (stage === 'bind-raw-identity') bindRawIdentity(args[0], args[1], args[2]);
  else if (stage === 'observe-picker-maximum') observePickerMaximum(args[0], args[1]);
  else if (stage === 'assert-no-own-draft') assertNoOwnDraft(args[0]);
  else if (stage === 'reconcile') reconcile(args[0], args[1]);
  else throw Error('explicit observed stage required; use module exports for independent raw identity binding');
}
module.exports = { f, a, name, body, files, state, capture, editorSeek, seed, open, pickerClickLabel,
  observePickerMaximum, clickOwnFile, confirm, batchObserve, retain, restore, bindRawIdentity, businessSave, businessSaveScoped,
  readback, readbackScoped, assertNoOwnDraft, reconcile, bundleIdentity, installBinding, expectedVersion, expectedSha };
