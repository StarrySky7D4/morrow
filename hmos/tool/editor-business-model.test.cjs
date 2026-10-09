'use strict';
// Fresh actual ETS module + field-policy contract. Platform crypto/UTF8 use
// controlled Node providers; receipts are synthetic exact-native DTO fixtures.
// This is not Store reconstruction, protected provenance, ArkTS SDK or device
// evidence. Native tests independently qualify the canonical command/result.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), { test } = require('node:test'), crypto = require('node:crypto');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const root = path.resolve(__dirname, '../entry/src/main/ets/model');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const u64 = value => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(value)); return b; };
const frame = value => { const b = Buffer.from(value, 'utf8'); return Buffer.concat([u64(b.length), b]); };
// Independently written from frozen Rust publication_hash, including its NUL
// domain, raw digest bytes and full u64. It is deliberately not JSON hashing.
function publicationHash(card, proof) {
  return sha(Buffer.concat([Buffer.from('morrow.hmos.editor-publication.v1\0'), frame(card), frame(proof.draft_id),
    u64(proof.generation), frame(proof.save_operation), Buffer.from(proof.request_sha256, 'hex')]));
}
function model() {
  const modules = new Map();
  function load(name) {
    if (name === '@kit.ArkTS') return { util: { TextEncoder: class { encodeInto(value) { return new TextEncoder().encode(value); } } } };
    if (name === '@kit.CryptoArchitectureKit') return { cryptoFramework: { createMd(algorithm) {
      assert.equal(algorithm, 'SHA256'); const h = crypto.createHash('sha256');
      return { async update({ data }) { h.update(data); }, async digest() { return { data: new Uint8Array(h.digest()) }; } };
    } } };
    name = name.replace(/^\.\//, ''); if (modules.has(name)) return modules.get(name);
    const file = path.join(root, name + '.ets'), output = {};
    const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), { fileName: file, reportDiagnostics: true,
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
    assert.equal((compiled.diagnostics || []).filter(x => x.category === ts.DiagnosticCategory.Error).length, 0);
    modules.set(name, output); vm.runInNewContext(compiled.outputText, { exports: output, require: load, Uint8Array }); return output;
  }
  return { ...load('EditorDraft'), ...load('EditorFieldPolicy'), ...load('EditorBusiness') };
}
function harness({ mode = 'create', revision = '0', assets = ['A', 'B'] } = {}) {
  const m = model(), scope = new m.DraftScope(); scope.card_id = '业务-card-😀'; scope.draft_id = 'publication-1';
  scope.source_kind = mode === 'create' ? 1 : 0; scope.source_revision = revision; scope.source = mode === 'create' ? '' : '0a02aabb';
  const values = new m.Values(); values.title.text = '  原始😀标题  '; values.title.selection_base = 4; values.title.selection_extent = 1;
  values.title.affinity = 1; values.title.directional = true; values.description.text = '原文\n"\\😀';
  values.hypothesis.text = '假设'; values.conclusion.text = '结论'; values.todos.text = mode === 'create' ? '甲\n\n 甲 \n乙\n' : '';
  values.assets = assets.map((id, i) => Object.assign(new m.AssetSelection(), { origin: mode === 'create' ? 2 : i ? 2 : 0,
    asset_id: id, aliases: [id + '.png', 'attachment:' + id + '/😀'] }));
  const record = Object.assign(new m.DraftRecord(), { scope, values, operation_id: 'publication-save-1', generation: '9007199254740993',
    current_generation: '9007199254740993', active: true, current_active: true, request_sha256: 'a'.repeat(64),
    assets: values.assets.map((selection, i) => Object.assign(new m.StoredAsset(), { selection: plain(selection), pin_id: 'draft-asset-' + i,
      display_name: selection.asset_id + '.png', media_type: 'image/png', byte_length: i ? '9007199254740993' : '3', sha256: (i ? 'b' : 'c').repeat(64) })) });
  const business = Object.assign(new m.EditorBusinessFields(), { action: mode === 'create' ? 'create' : 'edit', id: scope.card_id,
    operation: 'business-1', source: scope.source, title: values.title.text, description: values.description.text,
    hypothesis: values.hypothesis.text, conclusion: values.conclusion.text, todos: values.todos.text, category: values.category, stage: values.stage });
  const owner = { current: true, exact: true }, calls = [], fieldCalls = []; let changed = 0, fieldOverride;
  const fields = new m.EditorFieldPolicy(async wire => {
    const request = JSON.parse(wire); fieldCalls.push(request);
    if (fieldOverride) return fieldOverride(request);
    // Inputs in these fixtures contain single-codepoint graphemes. Complex
    // Unicode16 count qualification belongs to the actual native/Flutter suite.
    const count = Array.from(request.text).length, limit = m.editorFieldLimit(request.field);
    return JSON.stringify({ ok: count <= limit, error: count <= limit ? '' : 'EditorFieldGraphemeLimit', field: request.field,
      grapheme_count: count, utf16_length: request.text.length, utf8_length: Buffer.byteLength(request.text), limit, unicode_version: '16.0.0' });
  });
  const hooks = { fields, changed: () => { changed++; }, isCurrent: () => owner.current, isExact: () => owner.exact,
    send: wire => new Promise((resolve, reject) => calls.push({ wire, request: JSON.parse(wire), resolve, reject })) };
  const prepare = () => m.EditorBusinessCoordinator.prepare(mode, business, record, hooks);
  function reply(coordinator, changes = {}) {
    const submission = coordinator.submission, raw = coordinator.publication;
    const revision = m.nextGeneration(raw.scope.source_revision), field = submission.business;
    // Complete binary Card bytes are synthetic; their digest is nevertheless
    // computed independently over source bytes, never over source hex or text.
    const source = Buffer.from('binary-card\0' + field.id + '\0' + field.operation + '\0' + revision).toString('hex');
    const historical_card = { id: field.id, revision, source, title: field.title, description: field.description, hypothesis: field.hypothesis,
      conclusion: field.conclusion, category: field.category, stage: field.stage, favorite: false, deleted: false, deleted_at: '0',
      tasks: field.todos ? [{ id: 'actual-task-A', text: '甲', completion: 0 }, { id: 'actual-task-B', text: '乙', completion: 1 }] : [],
      assets: raw.assets.map(pin => ({ id: pin.selection.asset_id, name: pin.display_name, kind: 'image',
        byte_length: pin.byte_length, sha256: pin.sha256, media_type: pin.media_type })) };
    const commit = { commit_status: 'committed', qualification: 'development_editor_wire_v1', card_id: field.id, operation: field.operation,
      source_revision: raw.scope.source_revision, revision, event_id: 'event-' + field.operation, command_sha256: 'd'.repeat(64),
      content_sha256: sha(Buffer.from(source, 'hex')), request_sha256: sha(Buffer.from(coordinator.originalRequest)),
      publication_sha256: publicationHash(field.id, submission.publication), publication: plain(submission.publication), historical_card,
      live_matches: true, live_revision: revision };
    if (typeof changes === 'function') changes(commit); else Object.assign(commit, changes);
    return { ok: true, error: '', effect: 'committed', receipt_revision: commit.revision, editor_commit: commit,
      cards: [{ id: field.id, source: 'foreign-current-source', revision: '999999' }] };
  }
  function ack(coordinator, index, changes = {}) { const result = reply(coordinator, changes); calls[index].resolve(result); return result; }
  function successor(previous, operation = 'business-2') {
    const result = previous.confirmed, nextRecord = plain(previous.publication); nextRecord.scope.draft_id = 'successor-' + operation;
    nextRecord.scope.source_kind = 0; nextRecord.scope.source_revision = result.revision; nextRecord.scope.source = result.historical_card.source;
    nextRecord.operation_id = 'publication-' + operation; nextRecord.generation = '1'; nextRecord.current_generation = '1';
    nextRecord.request_sha256 = 'e'.repeat(64); nextRecord.values.title.text = '后继完整原文 ' + operation;
    nextRecord.values.todos.text = '乙\n丙\n乙\n'; nextRecord.values.assets.forEach(asset => asset.origin = 3);
    nextRecord.assets.forEach((pin, i) => pin.selection = plain(nextRecord.values.assets[i]));
    const next = plain(previous.submission.business); Object.assign(next, { action: 'edit', operation, source: result.historical_card.source,
      title: nextRecord.values.title.text, todos: nextRecord.values.todos.text });
    return { business: next, record: nextRecord };
  }
  return { m, record, business, owner, calls, fields, fieldCalls, hooks, prepare, reply, ack, successor,
    setFieldOverride(value) { fieldOverride = value; }, get changed() { return changed; } };
}

test('prepare freezes all raw strings, full editing publication and exact original wire before asynchronous checks', async () => {
  const h = harness(), work = h.prepare(); h.business.title = 'caller changed'; h.record.values.title.selection_extent = 0;
  h.record.values.assets[0].aliases[1] = 'caller changed'; const c = await work;
  assert.equal(c.submission.business.title, '  原始😀标题  '); assert.equal(c.publication.values.title.selection_extent, 1);
  assert.equal(c.publication.values.assets[0].aliases[1], 'attachment:A/😀'); assert.equal(h.fieldCalls.length, 5);
  const exposed = c.submission; exposed.publication.generation = '99'; c.publication.values.todos.text = 'getter changed';
  const issued = c.save(); assert.equal(h.calls.length, 1); assert.equal(h.calls[0].wire, c.originalSave);
  assert.equal(h.calls[0].request.editor_save.request_json, c.originalRequest); assert.equal(JSON.parse(c.originalRequest).business.todos, '甲\n\n 甲 \n乙\n');
  h.ack(c, 0); await issued; assert.equal(c.qualified, true); assert.equal(c.mayConsume(c.publication.values), true);
});

test('native field policy, composition, exact raw and active publication are separate mandatory admission gates', async () => {
  for (const mutate of [h => h.record.values.title.composing_start = 0, h => h.business.title += 'changed',
    h => h.record.current_generation = '9007199254740994', h => h.record.current_active = false, h => h.record.request_sha256 = '',
    h => h.record.scope.source_revision = '01', h => h.owner.exact = false]) {
    const h = harness(); mutate(h); await assert.rejects(h.prepare()); assert.equal(h.calls.length, 0);
  }
  const h = harness(); h.business.title = h.record.values.title.text = 'a'.repeat(61);
  await assert.rejects(h.prepare(), /最多输入 60/); assert.equal(h.calls.length, 0);
  const bad = harness(); bad.setFieldOverride(() => JSON.stringify({ ok: true }));
  await assert.rejects(bad.prepare(), /计数回执/); assert.equal(bad.calls.length, 0);
});

test('current_v2 freezes current full source and empty LF fields while parsing actual complete TaskId results without converting them', async () => {
  const h = harness({ mode: 'current_v2', revision: '9007199254740993' }), c = await h.prepare();
  assert.equal(c.submission.mode, 'current_v2'); assert.equal(c.submission.business.action, 'edit');
  assert.equal(c.submission.business.todos, ''); assert.equal(c.submission.continuation, null);
  assert.equal(c.publication.values.todos.text, ''); assert.equal(c.publication.scope.source_kind, 0);
  assert.equal(c.publication.scope.source_revision, '9007199254740993'); assert.equal(c.submission.business.source, h.record.scope.source);
  const original = c.originalRequest, work = c.save();
  const tasks = [{ id: 'task-stable-乙', text: '同名', completion: 2 }, { id: 'task-stable-甲', text: '同名', completion: 1 }];
  h.ack(c, 0, reply => reply.historical_card.tasks = plain(tasks)); await work;
  assert.equal(c.qualified, true); assert.deepEqual(plain(c.confirmed.historical_card.tasks), tasks);
  const next = h.successor(c); await assert.rejects(c.continueTodos(next.business, next.record), /root|baseline|continu/);
  assert.equal(c.originalRequest, original); assert.equal(h.calls.length, 1);
});

test('current_v2 cannot admit LF todos, source mismatch, stale publication, composition or revoked owner', async () => {
  const changes = [h => h.business.todos = h.record.values.todos.text = '真实任务不能以 LF 替换',
    h => h.business.source = '0a01', h => h.record.scope.source_kind = 1,
    h => h.record.current_generation = h.m.nextGeneration(h.record.generation),
    h => h.record.values.description.composing_start = 0, h => h.owner.exact = false];
  for (const change of changes) {
    const h = harness({ mode: 'current_v2', revision: '7' }); change(h);
    await assert.rejects(h.prepare()); assert.equal(h.calls.length, 0);
  }
});

test('owner or input epoch revoked during asynchronous validation cannot issue a proposal', async () => {
  const h = harness(); let release;
  h.setFieldOverride(request => new Promise(resolve => { release = () => resolve(JSON.stringify({ ok: true, error: '', field: request.field,
    grapheme_count: Array.from(request.text).length, utf16_length: request.text.length, utf8_length: Buffer.byteLength(request.text),
    limit: h.m.editorFieldLimit(request.field), unicode_version: '16.0.0' })); }));
  const preparing = h.prepare(); h.owner.exact = false; release(); await assert.rejects(preparing, /目标已变化/); assert.equal(h.calls.length, 0);
});

test('Unknown retains one complete original save wire; retry is explicit even after late raw and owner replacement', async () => {
  const h = harness(), c = await h.prepare(), issued = c.save(), frozen = c.originalSave;
  h.calls[0].reject(new Error('lost receipt')); await assert.rejects(issued, /lost receipt/); assert.equal(c.unknown, true);
  h.business.todos += '\nlate'; h.record.values.todos.text = h.business.todos; h.owner.current = false; h.owner.exact = false;
  await assert.rejects(c.save(), /frozen/); assert.equal(h.calls.length, 1);
  const retried = c.retrySave(); assert.equal(h.calls[1].wire, frozen); h.ack(c, 1); await retried;
  assert.equal(c.committed, true); assert.equal(c.unknown, false); assert.equal(c.mayConsume(c.publication.values), false);
  await assert.rejects(c.retrySave(), /No uncertain/); assert.equal(h.calls.length, 2);
});

test('late selection/composition or same-values away/back epoch cannot consume a valid historical result', async () => {
  const h = harness(), c = await h.prepare(), issued = c.save(); h.owner.exact = false;
  const latest = c.publication.values; latest.title.selection_base = 2; latest.title.composing_start = 1; latest.title.composing_end = 2;
  h.ack(c, 0); await issued; assert.equal(c.committed, true); assert.equal(c.mayConsume(latest), false);
  assert.equal(c.mayConsume(c.publication.values), false); h.owner.exact = true;
  assert.equal(c.mayConsume(latest), false); assert.equal(c.mayConsume(c.publication.values), true);
});

test('initial definitive no-write failure is rejected; an Unknown retry cannot be downgraded by not_committed', async () => {
  const h = harness(), c = await h.prepare(), issued = c.save(); h.calls[0].resolve({ ok: false, error: 'budget', effect: 'not_committed' });
  await assert.rejects(issued, /budget/); assert.equal(c.rejected, true); assert.equal(c.unknown, false);
  await assert.rejects(c.retrySave(), /No uncertain/);
  const k = harness(), u = await k.prepare(), first = u.save(); k.calls[0].reject(new Error('Unknown'));
  await assert.rejects(first); const retry = u.retrySave(); k.calls[1].resolve({ ok: false, error: 'changed proof', effect: 'not_committed' });
  await assert.rejects(retry, /changed proof/); assert.equal(u.unknown, true); assert.equal(u.originalSave, k.calls[0].wire);
});

test('full source digest and canonical framed publication independently bind strict history, including u64 over 2^53', async () => {
  const h = harness({ mode: 'edit', revision: '9007199254740993' }), c = await h.prepare(), issued = c.save();
  const response = h.ack(c, 0); await issued;
  assert.equal(c.confirmed.revision, '9007199254740994'); assert.equal(c.confirmed.source_revision, '9007199254740993');
  assert.equal(c.confirmed.publication_sha256, publicationHash(h.business.id, c.submission.publication));
  assert.notEqual(c.confirmed.content_sha256, sha(response.editor_commit.historical_card.source));
  c.confirmed.historical_card.source = 'ffff'; assert.equal(c.confirmed.historical_card.source, response.editor_commit.historical_card.source);
});

test('mismatched tuple/hash/revision/full source/common fields/duplicate identities never qualify or consume', async () => {
  const mutations = [commit => commit.operation = 'foreign', commit => commit.publication.generation = '9007199254740992',
    commit => commit.request_sha256 = 'f'.repeat(64), commit => commit.publication_sha256 = sha(JSON.stringify(commit.publication)),
    commit => commit.revision = '01', commit => commit.source_revision = '1', commit => commit.historical_card.source += 'ff',
    commit => commit.historical_card.title = 'partial', commit => commit.historical_card.category = 'foreign',
    commit => commit.historical_card.tasks[1].id = commit.historical_card.tasks[0].id,
    commit => commit.historical_card.tasks[0].text = '', commit => commit.historical_card.assets.reverse(),
    commit => commit.historical_card.assets[0].sha256 = 'f'.repeat(64), commit => delete commit.historical_card.assets[0],
    commit => commit.historical_card.assets[0] = null];
  for (const mutation of mutations) {
    const h = harness(), c = await h.prepare(), issued = c.save(); h.ack(c, 0, mutation); await assert.rejects(issued);
    assert.equal(c.unknown, true); assert.equal(c.committed, false); assert.equal(c.mayConsume(c.publication.values), false);
    assert.equal(h.calls.length, 1);
  }
});

test('inspect uses only exact historical DTO even when Reply.cards is newer, missing or deleted live state', async () => {
  for (const diagnostic of [{ live_matches: false, live_revision: '99' }, { live_matches: false, live_revision: '' }]) {
    const h = harness(), c = await h.prepare(), first = c.save(); h.ack(c, 0); await first;
    const inspected = c.inspect('1'); assert.equal(h.calls[1].request.action, 'editor_commit_inspect');
    assert.equal(h.calls[1].request.editor_commit.request_json, c.originalRequest); h.ack(c, 1, diagnostic); await inspected;
    assert.equal(c.confirmed.revision, '1'); assert.equal(c.confirmed.historical_card.revision, '1'); assert.equal(c.committed, true);
    const next = h.successor(c); await assert.rejects(c.continueTodos(next.business, next.record, h.hooks), /historical baseline/);
    assert.equal(h.calls.length, 2);
  }
});

test('snapshot absence does not resolve uncertain original save, generate a new operation or automatically retry', async () => {
  const h = harness(), c = await h.prepare(), issued = c.save(); h.calls[0].reject(new Error('lost'));
  await assert.rejects(issued); const inspected = c.inspect('1'), submission = c.submission;
  h.calls[1].resolve({ ok: true, error: '', effect: 'not_committed', receipt_revision: '', editor_commit: {
    commit_status: 'absent', qualification: 'absent', card_id: submission.business.id, operation: submission.business.operation,
    source_revision: '', revision: '', event_id: '', command_sha256: '', content_sha256: '', request_sha256: '', publication_sha256: '',
    publication: submission.publication, historical_card: null, live_matches: false, live_revision: '' } });
  assert.equal(await inspected, undefined); assert.equal(c.snapshotAbsent, true); assert.equal(c.unknown, true); assert.equal(h.calls.length, 2);
  const retried = c.retrySave(); assert.equal(h.calls[2].wire, h.calls[0].wire); h.ack(c, 2); await retried;
});

test('Unknown inspection retains its own same wire and cannot be confused with save transport', async () => {
  const h = harness(), c = await h.prepare(), first = c.save(); h.calls[0].reject(new Error('lost save')); await assert.rejects(first);
  const inspected = c.inspect('1'); await assert.rejects(c.retrySave(), /inspection.*flight/);
  h.calls[1].reject(new Error('lost inspect')); await assert.rejects(inspected); assert.equal(c.checkingUnknown, true);
  const original = c.pendingInspect; await assert.rejects(c.inspect('1'), /explicitly/);
  const retry = c.retryInspect(); assert.equal(h.calls[2].wire, original); h.ack(c, 2); await retry;
  assert.equal(c.qualified, true); assert.equal(c.checkingUnknown, false); assert.equal(c.unknown, false);
});

test('a qualified known commit remains committed on later damaged/oversize readback and cannot become absent', async () => {
  const h = harness(), c = await h.prepare(), first = c.save(); h.ack(c, 0); await first; const known = c.confirmed;
  const inspected = c.inspect('1'), response = h.reply(c); response.editor_commit.historical_card = null;
  h.calls[1].resolve(response); await assert.rejects(inspected, /proof/); assert.equal(c.committed, true);
  assert.deepEqual(plain(c.confirmed), plain(known)); assert.equal(c.checkingUnknown, true);
  const retry = c.retryInspect(); h.calls[2].resolve({ ...h.reply(c), padding: 'x'.repeat(512 * 1024) });
  await assert.rejects(retry, /JSON bytes/); assert.equal(c.committed, true); assert.equal(c.pendingInspect, h.calls[1].wire);
});

test('restart restore preserves noncanonical JSON spacing/key order and exact retired publication history', async () => {
  const h = harness(), c = await h.prepare();
  const original = ' \n' + JSON.stringify(c.submission, null, 2) + '\n '; const historical = c.publication;
  historical.current_active = false; historical.current_generation = '9007199254740994'; h.owner.current = false; h.owner.exact = false;
  const restored = await h.m.EditorBusinessCoordinator.restore(original, historical, h.hooks);
  assert.equal(restored.originalRequest, original); assert.equal(restored.unknown, true); assert.equal(h.fieldCalls.length, 5);
  const inspected = restored.inspect('1'); assert.equal(h.calls[0].request.editor_commit.request_json, original);
  h.ack(restored, 0, { live_matches: false, live_revision: '9' }); await inspected;
  assert.equal(restored.qualified, true); assert.equal(restored.mayConsume(restored.publication.values), false);
  assert.equal(restored.confirmed.request_sha256, sha(Buffer.from(original)));
});

test('legacy semantic-only history stays readonly and cannot acquire strict wire/publication or a continued root', async () => {
  const h = harness({ mode: 'edit', revision: '4' }), c = await h.prepare();
  const inspected = c.inspect('5'); h.ack(c, 0, commit => {
    commit.qualification = 'legacy_semantic_only'; commit.request_sha256 = ''; commit.publication_sha256 = '';
    commit.historical_card.category = '旧route保留'; commit.historical_card.stage = '旧route保留';
    commit.historical_card.tasks = [{ id: 'migrated-task', text: 'legacy ambiguous', completion: 2 }];
  }); await inspected; assert.equal(c.committed, true); assert.equal(c.qualified, false); assert.equal(c.mayConsume(c.publication.values), false);
  const next = h.successor(c); await assert.rejects(c.continueTodos(next.business, next.record, h.hooks), /own exact/);
  assert.equal(h.calls.length, 1);
});

test('continued todos derives constant original create plus latest six-field baseline through two own historical commits', async () => {
  const h = harness(), rootModel = await h.prepare(), first = rootModel.save(); h.ack(rootModel, 0); await first;
  h.owner.exact = false; const next = h.successor(rootModel); h.owner.exact = true;
  const second = await rootModel.continueTodos(next.business, next.record, h.hooks);
  const context = second.submission.continuation; assert.equal(context.root_request_json, rootModel.originalRequest);
  assert.deepEqual(plain(context.baseline), { operation: rootModel.confirmed.operation, revision: rootModel.confirmed.revision,
    command_sha256: rootModel.confirmed.command_sha256, content_sha256: rootModel.confirmed.content_sha256,
    request_sha256: rootModel.confirmed.request_sha256, publication_sha256: rootModel.confirmed.publication_sha256 });
  assert.equal(second.submission.business.source, rootModel.confirmed.historical_card.source);
  assert.equal(second.submission.business.todos, '乙\n丙\n乙\n'); const saved = second.save(); h.ack(second, 1); await saved;
  const thirdInput = h.successor(second, 'business-3'), third = await second.continueTodos(thirdInput.business, thirdInput.record, h.hooks);
  assert.equal(third.submission.continuation.root_request_json, rootModel.originalRequest);
  assert.equal(third.submission.continuation.baseline.operation, 'business-2');
  assert.equal(third.originalRequest.includes(second.originalRequest), false); assert.equal(h.calls.length, 2);
});

test('foreign source/operation/owner and kind1 raw fork cannot be promoted to a business continuation', async () => {
  const h = harness(), c = await h.prepare(), first = c.save(); h.ack(c, 0); await first;
  for (const mutate of [n => n.business.source = 'ffff', n => n.business.operation = c.confirmed.operation,
    n => n.business.id = 'foreign', n => n.record.scope.source_kind = 1, n => n.record.scope.source_revision = '2',
    n => n.record.current_active = false]) {
    const next = h.successor(c); mutate(next); await assert.rejects(c.continueTodos(next.business, next.record, h.hooks));
  }
  h.owner.current = false; const next = h.successor(c); await assert.rejects(c.continueTodos(next.business, next.record, h.hooks));
  assert.equal(h.calls.length, 1);
});

test('ordinary existing V2 edit rejects any nonempty legacy todos and cannot be used as an owned-root factory', async () => {
  const h = harness({ mode: 'edit', revision: '4' }); h.business.todos = h.record.values.todos.text = 'not empty';
  await assert.rejects(h.prepare(), /V2/); assert.equal(h.calls.length, 0);
  const k = harness({ mode: 'edit', revision: '4' }), c = await k.prepare(), issued = c.save(); k.ack(c, 0); await issued;
  const next = k.successor(c); await assert.rejects(c.continueTodos(next.business, next.record, k.hooks), /baseline/);
  await assert.rejects(k.m.EditorBusinessCoordinator.prepare('continued_todos', next.business, next.record, k.hooks), /own qualified/);
});

test('complete escaped outer wire budget, not UTF16 length or inner JSON size, gates all proposals', async () => {
  const h = harness(); h.business.description = h.record.values.description.text = '"\\\n'.repeat(60000);
  const inner = JSON.stringify({ schema_version: 1, mode: 'create', business: h.business, publication: { draft_id: h.record.scope.draft_id,
    generation: h.record.generation, save_operation: h.record.operation_id, request_sha256: h.record.request_sha256 }, continuation: null });
  assert.ok(Buffer.byteLength(inner) < 512 * 1024); assert.ok(Buffer.byteLength(JSON.stringify({ action: 'editor_save', editor_save: { request_json: inner } })) > 512 * 1024);
  await assert.rejects(h.prepare(), /JSON bytes/); assert.equal(h.calls.length, 0); assert.equal(h.fieldCalls.length, 0);
});

test('inspect revision overflow/noncanonical identity and incomplete Unicode fail before any transport', async () => {
  const h = harness(), c = await h.prepare(); for (const rev of ['01', '2', '9007199254740993', '18446744073709551616']) {
    await assert.rejects(c.inspect(rev), /revision/);
  }
  assert.equal(h.calls.length, 0);
  const bad = harness(); bad.business.title = bad.record.values.title.text = '\ud800'; await assert.rejects(bad.prepare(), /Unicode/);
  const foreign = harness(); foreign.business.operation = 'bad/op'; await assert.rejects(foreign.prepare(), /identity/);
  const exhausted = harness({ mode: 'edit', revision: '18446744073709551615' }); await assert.rejects(exhausted.prepare(), /exhausted/);
});

test('missing editor_commit and oversized initial reply remain Unknown with exact retry, never use current cards fallback', async () => {
  for (const missing of [true, false]) {
    const h = harness(), c = await h.prepare(), issued = c.save(), response = h.reply(c);
    if (missing) delete response.editor_commit; else response.padding = 'x'.repeat(512 * 1024);
    h.calls[0].resolve(response); await assert.rejects(issued); assert.equal(c.unknown, true); assert.equal(c.committed, false);
    const retry = c.retrySave(); assert.equal(h.calls[1].wire, h.calls[0].wire); h.ack(c, 1); await retry;
  }
});

test('full DTO is frozen before asynchronous source hashing, so caller mutation cannot change an admitted receipt', async () => {
  const h = harness(); let hashes = 0, release;
  h.hooks.hash = async bytes => {
    if (++hashes <= 2) return sha(bytes);
    return new Promise(resolve => { release = () => resolve(sha(bytes)); });
  };
  const c = await h.prepare(), issued = c.save(), response = h.ack(c, 0);
  for (let n = 0; n < 12 && !release; n++) await Promise.resolve(); assert.ok(release);
  response.editor_commit.operation = 'foreign-after-check'; response.editor_commit.historical_card.title = 'foreign-after-check';
  response.editor_commit.historical_card.source = 'ffff'; response.editor_commit.publication.generation = '99';
  release(); await issued; assert.equal(c.confirmed.operation, 'business-1'); assert.equal(c.confirmed.historical_card.title, '  原始😀标题  ');
  assert.equal(c.confirmed.publication.generation, '9007199254740993'); assert.equal(c.qualified, true);
});

test('canonical publication supports the complete u64 range and history effect/status inconsistency is rejected', async () => {
  const h = harness(); h.record.generation = h.record.current_generation = '18446744073709551615';
  const c = await h.prepare(), first = c.save(); h.ack(c, 0); await first;
  assert.equal(c.confirmed.publication_sha256, publicationHash(h.business.id, c.submission.publication));
  const inspected = c.inspect('1'), bad = h.reply(c); bad.effect = 'not_committed'; h.calls[1].resolve(bad);
  await assert.rejects(inspected, /incomplete/); assert.equal(c.committed, true); assert.equal(c.checkingUnknown, true);
});

test('fresh actual Store create and continued_todos DTOs pass original-wire/publication/full historical-source validation', async () => {
  const fixturePath = path.resolve(__dirname, '../reports/ui-source/v22/editor-business-store-fixture.json');
  const fixture = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));
  assert.equal(fixture.producer, 'fresh actual Store'); assert.deepEqual(fixture.cases.map(row => row.name), ['create', 'continued_todos']);
  for (const row of fixture.cases) {
    const h = harness(), c = await h.m.EditorBusinessCoordinator.restore(row.request_json, row.publication, h.hooks);
    const inspected = c.inspect(row.reply.editor_commit.revision); assert.equal(h.calls.length, 1);
    assert.equal(h.calls[0].request.editor_commit.request_json, row.request_json); h.calls[0].resolve(row.reply);
    const actual = await inspected; assert.equal(c.qualified, true); assert.equal(c.unknown, false);
    assert.equal(actual.request_sha256, sha(Buffer.from(row.request_json)));
    assert.equal(actual.publication_sha256, publicationHash(actual.card_id, actual.publication));
    assert.equal(actual.content_sha256, sha(Buffer.from(actual.historical_card.source, 'hex')));
    assert.deepEqual(plain(c.confirmed), row.reply.editor_commit); assert.equal(h.fieldCalls.length, 0);
  }
  // Replays captured native results through a controlled receiver. The
  // exporter test is the Store authority; no network/native/device call here.
});
