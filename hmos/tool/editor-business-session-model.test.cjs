'use strict';
// Actual ETS Session + unchanged Business/Draft/FieldPolicy methods. Node
// providers and the intent receiver below are controlled protocol fixtures;
// they do not prove Store persistence, SDK/runtime, Index or device behavior.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), { test } = require('node:test'), crypto = require('node:crypto');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modelRoot = path.resolve(__dirname, '../entry/src/main/ets/model');
const plain = x => x === undefined ? undefined : JSON.parse(JSON.stringify(x));
const sha = x => crypto.createHash('sha256').update(x).digest('hex');
const u64 = x => { const result = Buffer.alloc(8); result.writeBigUInt64LE(BigInt(x)); return result; };
const frame = x => { const b = Buffer.from(x); return Buffer.concat([u64(b.length), b]); };
function framedHash(domain, first, second) { return sha(Buffer.concat([Buffer.from(domain), frame(first), frame(second)])); }
function publicationHash(card, p) { return sha(Buffer.concat([Buffer.from('morrow.hmos.editor-publication.v1\0'), frame(card),
  frame(p.draft_id), u64(p.generation), frame(p.save_operation), Buffer.from(p.request_sha256, 'hex')])); }
function model() {
  const cache = new Map();
  function load(name) {
    if (name === '@kit.ArkTS') return { util: { TextEncoder: class { encodeInto(value) { return new TextEncoder().encode(value); } } } };
    if (name === '@kit.CryptoArchitectureKit') return { cryptoFramework: { createMd() {
      const h = crypto.createHash('sha256'); return { async update({ data }) { h.update(data); }, async digest() { return { data: new Uint8Array(h.digest()) }; } };
    } } };
    name = name.replace(/^\.\//, ''); if (cache.has(name)) return cache.get(name);
    const file = path.join(modelRoot, name + '.ets'), exports = {};
    const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), { fileName: file, reportDiagnostics: true,
      compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
    assert.equal((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
    cache.set(name, exports); vm.runInNewContext(compiled.outputText, { exports, require: load, Uint8Array }); return exports;
  }
  return { ...load('EditorDraft'), ...load('EditorFieldPolicy'), ...load('EditorBusiness'), ...load('EditorBusinessSession') };
}
function harness({ mode = 'create', revision = '0', assets = ['A', 'B'] } = {}) {
  const m = model(), record = new m.DraftRecord(); Object.assign(record.scope, { card_id: '业务-card-😀', draft_id: 'parent-1',
    source_kind: mode === 'create' ? 1 : 0, source_revision: revision, source: mode === 'create' ? '' : '0a02aabb' });
  Object.assign(record, { operation_id: 'publication-save', generation: '9007199254740993', current_generation: '9007199254740993',
    active: true, current_active: true, request_sha256: 'a'.repeat(64) });
  record.values.title.text = '完整😀标题'; record.values.title.selection_base = 4; record.values.title.selection_extent = 1;
  record.values.title.affinity = 1; record.values.title.directional = true; record.values.description.text = '完整\n"\\原文';
  record.values.hypothesis.text = '假设'; record.values.conclusion.text = '结论'; record.values.todos.text = mode === 'create' ? '甲\n甲\n乙' : '';
  record.values.assets = assets.map(id => Object.assign(new m.AssetSelection(), { asset_id: id, aliases: ['asset:' + id, id + '.png'] }));
  record.assets = record.values.assets.map((selection, index) => Object.assign(new m.StoredAsset(), { selection: plain(selection), pin_id: 'draft-asset-' + index,
    display_name: selection.asset_id + '.png', media_type: 'image/png', byte_length: '3', sha256: (index ? 'b' : 'c').repeat(64) }));
  const business = new m.EditorBusinessFields(); Object.assign(business, { action: mode === 'create' ? 'create' : 'edit', id: record.scope.card_id,
    source: record.scope.source, operation: 'business-1', title: record.values.title.text, description: record.values.description.text,
    hypothesis: record.values.hypothesis.text, conclusion: record.values.conclusion.text, todos: record.values.todos.text,
    category: record.values.category, stage: record.values.stage });
  const owner = { current: true, exact: true, parentReady: true }, calls = [], fields = new m.EditorFieldPolicy(async wire => {
    const p = JSON.parse(wire), count = Array.from(p.text).length, limit = m.editorFieldLimit(p.field);
    return JSON.stringify({ ok: count <= limit, error: count <= limit ? '' : 'EditorFieldGraphemeLimit', field: p.field,
      grapheme_count: count, utf16_length: p.text.length, utf8_length: Buffer.byteLength(p.text), limit, unicode_version: '16.0.0' });
  });
  const state = { phase: '', submission: '', publication: plain(record), proof: null, save: '', inspect: '', close: '', issueOperation: '' };
  let intercept, session, changed = 0;
  function view(part = 'summary') {
    return { proof: plain(state.proof), card_id: business.id, business_operation: business.operation,
      expected_revision: m.nextGeneration(record.scope.source_revision), phase: state.phase,
      current_generation: state.phase === 'prepared' ? '1' : '2', current_active: state.phase !== 'closed', repeated: false, part,
      request_json: part === 'submission' ? state.submission : '', publication: part === 'publication' ? plain(state.publication) : null,
      save_request_json: part === 'save' ? state.save : '', inspect_request_json: part === 'inspect' ? state.inspect : '',
      close_request_json: part === 'close' ? state.close : '', close_disposition: state.phase === 'closed' ? 'cancel_prepared' : '',
      issue_operation: state.issueOperation };
  }
  function metadata(part, mutation = false) {
    return { ok: true, error: '', effect: mutation ? 'committed' : 'not_committed', receipt_revision: mutation ? view().current_generation : '',
      editor_intents: [view(part)], intent_next_after: '' };
  }
  function businessReply() {
    const p = JSON.parse(state.submission), b = p.business, r = m.nextGeneration(state.publication.scope.source_revision);
    const source = Buffer.from('synthetic full card\0' + b.operation + '\0' + r).toString('hex');
    const c = { commit_status: 'committed', qualification: 'development_editor_wire_v1', card_id: b.id, operation: b.operation,
      source_revision: state.publication.scope.source_revision, revision: r, event_id: 'event-' + b.operation, command_sha256: 'd'.repeat(64),
      content_sha256: sha(Buffer.from(source, 'hex')), request_sha256: sha(state.submission), publication_sha256: publicationHash(b.id, p.publication),
      publication: plain(p.publication), historical_card: { id: b.id, source, revision: r, title: b.title, description: b.description,
        hypothesis: b.hypothesis, conclusion: b.conclusion, category: b.category, stage: b.stage, favorite: false, deleted: false, deleted_at: '0',
        tasks: b.todos ? [{ id: 'task-A', text: '甲', completion: 0 }, { id: 'task-B', text: '乙', completion: 1 }] : [],
        assets: state.publication.assets.map(a => ({ id: a.selection.asset_id, name: a.display_name, kind: 'image', byte_length: a.byte_length,
          sha256: a.sha256, media_type: a.media_type })) }, live_matches: true, live_revision: r };
    return { ok: true, error: '', effect: 'committed', receipt_revision: r, editor_commit: c,
      cards: [{ id: b.id, source: 'not-history', revision: '999999' }] };
  }
  function route(wire) {
    const p = JSON.parse(wire);
    if (p.action === 'editor_intent_prepare') {
      const original = p.editor_intent.request_json, op = p.editor_intent.operation_id;
      if (!state.phase) {
        state.phase = 'prepared'; state.submission = original;
        state.proof = { intent_id: 'morrow-host-editor-intent-' + framedHash('morrow.hmos.editor-intent.v1\0', business.id, business.operation),
          prepare_operation: op, generation: '1', prepared_record_sha256: 'f'.repeat(64), request_sha256: sha(original) };
        state.issueOperation = 'morrow-host-intent-issue-' + framedHash('morrow.hmos.editor-intent-issue.v1\0',
          state.proof.intent_id, state.proof.prepare_operation);
      } else assert.equal(original, state.submission);
      return metadata('summary', true);
    }
    if (p.action === 'editor_intent_issue') {
      assert.deepEqual(p.editor_intent_issue, { intent: state.proof, expected_generation: '1' });
      state.phase = 'issued'; state.issueOperation = 'morrow-host-intent-issue-' + framedHash('morrow.hmos.editor-intent-issue.v1\0',
        state.proof.intent_id, state.proof.prepare_operation);
      // Deliberate order/spacing differs from the old model. Session must emit
      // these exact returned bytes rather than regenerate equivalent JSON.
      state.save = ' {"editor_save":' + JSON.stringify({ intent: state.proof, request_json: state.submission }) + ',"action":"editor_save"}\n';
      state.inspect = ' {"editor_commit":' + JSON.stringify({ expected_revision: view().expected_revision, request_json: state.submission }) +
        ',"action":"editor_commit_inspect"}\n'; return metadata('summary', true);
    }
    if (p.action === 'editor_intent_read') return metadata(p.editor_intent_ref.part);
    if (p.action === 'editor_intent_list') return metadata('summary');
    if (p.action === 'editor_intent_close') {
      state.close = p.editor_intent_close.request_json; state.phase = 'closed'; return metadata('summary', true);
    }
    if (p.action === 'editor_save') { assert.equal(wire, state.save); return businessReply(); }
    if (p.action === 'editor_commit_inspect') { assert.equal(wire, state.inspect); return businessReply(); }
    throw new Error('Unexpected receiver action');
  }
  const hooks = { fields, changed: () => { changed++; }, isCurrent: () => owner.current, isExact: () => owner.exact, parentReady: () => owner.parentReady,
    async send(wire) { const p = JSON.parse(wire), index = calls.length; calls.push(wire);
      return intercept ? intercept(wire, p, index, () => route(wire)) : route(wire); } };
  const freeze = async () => session = await m.EditorBusinessSessionCoordinator.prepare(mode, business, record, 'prepare-1', hooks);
  const ready = async () => { const s = session || await freeze(); await s.prepareIntent(); await s.issueIntent(); await s.loadTransport(); return s; };
  return { m, record, business, owner, calls, hooks, state, view, metadata, businessReply, freeze, ready,
    setIntercept(fn) { intercept = fn; }, get session() { return session; }, get changed() { return changed; } };
}
const action = wire => JSON.parse(wire).action;

test('actual Business admission freezes raw/publication before native prepare; intent is not a business receipt', async () => {
  const h = harness(), frozen = h.freeze(); h.business.title = 'late caller'; h.record.values.title.selection_extent = 0;
  const s = await frozen; assert.equal(h.calls.length, 0); assert.equal(s.publication.values.title.selection_extent, 1);
  assert.equal(JSON.parse(s.originalRequest).business.title, '完整😀标题');
  await s.prepareIntent(); assert.equal(s.phase, 'prepared'); assert.equal(s.committed, false); assert.equal(s.qualified, false);
  const proof = s.proof; proof.request_sha256 = 'x'; assert.equal(s.proof.request_sha256, sha(s.originalRequest));
  assert.equal(h.state.submission, s.originalRequest); assert.equal(h.calls.length, 1);
});

test('actual field/composition/full raw admission refuses a proposal before any durable mutation', async () => {
  for (const mutate of [h => h.record.values.title.composing_start = 0, h => h.business.title += 'other',
    h => h.record.current_active = false, h => h.record.request_sha256 = '', h => h.owner.exact = false]) {
    const h = harness(); mutate(h); await assert.rejects(h.freeze()); assert.equal(h.calls.length, 0);
  }
});

test('prepare Unknown retains original operation and full bytes; explicit retry after owner replacement uses the same request', async () => {
  const h = harness(), s = await h.freeze(); h.setIntercept((wire, p, n, route) => { const result = route(); if (n === 0) throw new Error('lost prepare ACK'); return result; });
  await assert.rejects(s.prepareIntent(), /lost prepare/); assert.equal(s.pendingNative, 'prepare');
  assert.equal(s.phase, 'unprepared'); await assert.rejects(s.issueIntent()); h.owner.current = h.owner.exact = false;
  await s.retryPrepare(); assert.equal(h.calls[1], h.calls[0]); assert.equal(s.phase, 'prepared'); assert.equal(s.committed, false);
});

test('a first no-write prepare failure stays known, while a no-write uncertain retry cannot erase uncertainty', async () => {
  const h = harness(), s = await h.freeze(); h.setIntercept(() => ({ ok: false, error: 'quota', effect: 'not_committed', receipt_revision: '' }));
  await assert.rejects(s.prepareIntent(), /quota/); assert.equal(s.pendingNative, '');
  const k = harness(), u = await k.freeze(); k.setIntercept(() => { throw new Error('unknown'); }); await assert.rejects(u.prepareIntent());
  k.setIntercept(() => ({ ok: false, error: 'quota', effect: 'not_committed', receipt_revision: '' }));
  await assert.rejects(u.retryPrepare()); assert.equal(u.pendingNative, 'prepare');
});

test('metadata proof/ID/domain/current generation and business tuple mismatches never admit native issue', async () => {
  const mutations = [v => v.proof.intent_id += 'x', v => v.proof.prepare_operation = 'foreign', v => v.proof.generation = '2',
    v => v.proof.request_sha256 = 'b'.repeat(64), v => v.proof.prepared_record_sha256 = 'A'.repeat(64), v => v.card_id = 'foreign',
    v => v.business_operation = 'foreign', v => v.expected_revision = '01', v => v.current_generation = '2', v => v.current_active = false,
    v => v.request_json = 'partial', v => v.publication = {}, v => v.proof.unexpected = true];
  for (const mutate of mutations) {
    const h = harness(), s = await h.freeze(); h.setIntercept((w, p, n, route) => { const r = route(); mutate(r.editor_intents[0]); return r; });
    await assert.rejects(s.prepareIntent()); assert.equal(s.pendingNative, 'prepare'); assert.equal(s.phase, 'unprepared');
  }
});

test('issue freezes only immutable proof/gen1, requires explicit ACK and literal reads before business', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent();
  assert.throws(() => s.save(), /transport/); await s.issueIntent(); assert.equal(s.phase, 'issued'); assert.equal(s.committed, false);
  assert.deepEqual(Object.keys(JSON.parse(s.originalIssue).editor_intent_issue).sort(), ['expected_generation', 'intent']);
  assert.throws(() => s.save(), /transport/); await s.readPart('save'); assert.throws(() => s.save(), /transport/);
  await s.readPart('inspect'); const result = await s.save(); assert.equal(result.card_id, h.business.id);
  assert.equal(h.calls.at(-1), h.state.save); assert.equal(s.originalSave, h.state.save); assert.equal(s.qualified, true);
});

test('issue Unknown reuses the exact CAS request and never presumes business submission', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent(); h.setIntercept((w, p, n, route) => {
    const r = route(); if (n === 1) throw new Error('lost issue'); return r;
  });
  await assert.rejects(s.issueIntent()); assert.equal(s.pendingNative, 'issue'); assert.throws(() => s.save());
  h.owner.exact = false; await s.retryIssue(); assert.equal(h.calls[2], h.calls[1]); assert.equal(s.phase, 'issued');
  assert.equal(s.committed, false); await s.loadTransport(); assert.throws(() => s.save(), /owner/);
});

test('issued before first business send with late S2/owner revocation settles original S1 only after explicit five-part restore and retry', async () => {
  for (const revokeOwner of [false, true]) {
    const h = harness(), s = await h.freeze(); await s.prepareIntent(); let release;
    h.setIntercept((w, p, n, route) => {
      const r = route(); if (p.action === 'editor_intent_issue') return new Promise(resolve => { release = () => resolve(r); }); return r;
    });
    const issued = s.issueIntent(), originalIssue = s.originalIssue; assert.equal(h.state.phase, 'issued');
    assert.equal(s.phase, 'prepared'); assert.equal(s.committed, false); assert.equal(h.calls.filter(w => action(w) === 'editor_save').length, 0);
    const latest = plain(s.publication.values); latest.todos.text = '晚S2 汉字🧪'; latest.todos.selection_base = 2;
    latest.todos.selection_extent = 1; latest.todos.composing_start = 0; latest.todos.composing_end = 1;
    h.record.values = plain(latest); h.owner.current = !revokeOwner; h.owner.exact = h.owner.parentReady = false;
    release(); await issued; assert.equal(s.phase, 'issued'); assert.equal(s.originalIssue, originalIssue);
    await s.loadTransport(); const registered = s.originalSave; assert.equal(registered, h.state.save);
    assert.throws(() => s.save(), /owner/); await assert.rejects(s.cancelPrepared('cancel-after-issue'), /prepared/);
    assert.equal(h.calls.filter(w => action(w) === 'editor_save').length, 0); assert.equal(s.committed, false);
    const before = h.calls.length, restored = await h.m.EditorBusinessSessionCoordinator.restore(s.proof, h.hooks);
    assert.deepEqual(h.calls.slice(before).map(w => JSON.parse(w).editor_intent_ref.part), ['submission', 'publication', 'save', 'inspect', 'close']);
    assert.equal(restored.committed, false); assert.equal(restored.originalSave, registered);
    assert.equal(h.calls.filter(w => action(w) === 'editor_save').length, 0);
    await restored.retrySave(); assert.equal(h.calls.at(-1), registered); assert.equal(restored.qualified, true);
    assert.equal(restored.confirmed.historical_card.title, '完整😀标题'); assert.equal(restored.mayConsume(latest), false);
    assert.deepEqual(plain(h.record.values), latest); assert.equal(h.owner.parentReady, false);
  }
});

test('late owner/epoch after prepare blocks fresh issue and late raw after issue blocks fresh save', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent(); h.owner.exact = false;
  assert.throws(() => s.issueIntent(), /owner/); assert.equal(h.calls.length, 1);
  h.owner.exact = true; await s.issueIntent(); await s.loadTransport(); h.owner.current = false;
  assert.throws(() => s.save(), /owner/); assert.equal(h.calls.filter(w => action(w) === 'editor_save').length, 0);
});

test('parent queue must be paused for the exact publication before fresh preparation/issue/save, while historical retry never resumes it', async () => {
  const blocked = harness(); blocked.owner.parentReady = false; await assert.rejects(blocked.freeze(), /parent/); assert.equal(blocked.calls.length, 0);
  const h = harness(), s = await h.freeze(); h.owner.parentReady = false; assert.throws(() => s.prepareIntent(), /parent/);
  h.owner.parentReady = true; await s.prepareIntent(); h.owner.parentReady = false; assert.throws(() => s.issueIntent(), /parent/);
  h.owner.parentReady = true; await s.issueIntent(); await s.loadTransport(); h.owner.parentReady = false; assert.throws(() => s.save(), /parent/);
  h.owner.parentReady = true; h.setIntercept((w, p, n, route) => { if (p.action === 'editor_save') throw new Error('Unknown'); return route(); });
  await assert.rejects(s.save()); h.owner.parentReady = false; h.setIntercept(undefined); await s.retrySave();
  assert.equal(s.qualified, true); assert.equal(h.owner.parentReady, false);
});

test('an in-flight metadata request excludes another action and late raw remains live while the original ACK is retained', async () => {
  const h = harness(), s = await h.freeze(); let release;
  h.setIntercept((w, p, n, route) => { const r = route(); return new Promise(resolve => { release = () => resolve(r); }); });
  const preparing = s.prepareIntent(); assert.equal(s.saving, true); assert.throws(() => s.issueIntent(), /in flight/);
  assert.throws(() => s.cancelPrepared('cancel-1'), /in flight/); assert.equal(h.calls.length, 1);
  h.record.values.todos.text = 'latest S2'; h.owner.current = h.owner.exact = h.owner.parentReady = false;
  release(); await preparing; assert.equal(s.phase, 'prepared'); assert.equal(h.record.values.todos.text, 'latest S2');
  assert.equal(s.publication.values.todos.text, '甲\n甲\n乙'); assert.throws(() => s.issueIntent(), /owner/);
});

test('missing/wrong/extra native save envelope cannot be sent and explicit read retry retains one exact part request', async () => {
  for (const mutate of [wire => '', wire => JSON.stringify({ ...JSON.parse(wire), extra: true }),
    wire => { const p = JSON.parse(wire); p.editor_save.request_json += ' '; return JSON.stringify(p); },
    wire => { const p = JSON.parse(wire); p.editor_save.intent.prepared_record_sha256 = 'a'.repeat(64); return JSON.stringify(p); }]) {
    const h = harness(), s = await h.freeze(); await s.prepareIntent(); await s.issueIntent();
    h.setIntercept((w, p, n, route) => { const r = route(); if (p.editor_intent_ref?.part === 'save') r.editor_intents[0].save_request_json = mutate(h.state.save); return r; });
    await assert.rejects(s.loadTransport()); assert.ok(s.pendingRead); assert.throws(() => s.save());
    const pending = s.pendingRead; h.setIntercept(undefined); await s.retryRead(); assert.equal(h.calls.at(-1), pending);
    await s.readPart('inspect'); await s.save(); assert.equal(h.calls.at(-1), h.state.save);
  }
});

test('native inspect literal is checked independently, mapped once and sent byte-for-byte even with different key order', async () => {
  const h = harness(), s = await h.ready(); const native = s.originalInspect;
  assert.notEqual(native, JSON.stringify({ action: 'editor_commit_inspect', editor_commit: { request_json: s.originalRequest, expected_revision: '1' } }));
  await s.inspect(); assert.equal(h.calls.at(-1), native); assert.equal(s.qualified, true); assert.equal(s.confirmed.historical_card.source, h.businessReply().editor_commit.historical_card.source);
  assert.notEqual(s.confirmed.historical_card.source, 'not-history');
});

test('business Unknown retries actual registered outer bytes after late selection/composition and view owner replacement', async () => {
  const h = harness(), s = await h.ready(); h.setIntercept((w, p, n, route) => { if (p.action === 'editor_save') throw new Error('lost business'); return route(); });
  await assert.rejects(s.save()); assert.equal(s.businessUnknown, true); const original = h.calls.at(-1);
  h.owner.current = h.owner.exact = false; const late = s.publication.values; late.todos.selection_extent = 2; late.todos.composing_start = 0; late.todos.composing_end = 1;
  h.setIntercept(undefined); await s.retrySave(); assert.equal(h.calls.at(-1), original); assert.equal(s.committed, true);
  assert.equal(s.mayConsume(late), false); assert.equal(s.mayConsume(s.publication.values), false);
});

test('inspection Unknown keeps its native original wire; later not_committed cannot wash a known committed effect', async () => {
  const h = harness(), s = await h.ready(); await s.save();
  h.setIntercept((w, p, n, route) => { if (p.action === 'editor_commit_inspect') throw new Error('lost inspect'); return route(); });
  await assert.rejects(s.inspect()); const original = h.calls.at(-1); assert.equal(s.committed, true);
  h.setIntercept((w, p, n, route) => p.action === 'editor_commit_inspect' ? { ok: false, error: 'not found', effect: 'not_committed', receipt_revision: '' } : route());
  await assert.rejects(s.retryInspect()); assert.equal(h.calls.at(-1), original); assert.equal(s.committed, true); assert.equal(s.qualified, true);
});

test('malformed committed business DTO retains commitment hint without qualifying or consuming the result', async () => {
  const h = harness(), s = await h.ready(); h.setIntercept((w, p, n, route) => {
    const r = route(); if (p.action === 'editor_save') r.editor_commit.historical_card.source += 'ff'; return r;
  });
  await assert.rejects(s.save()); assert.equal(s.committed, true); assert.equal(s.qualified, false); assert.equal(s.businessUnknown, true);
  assert.equal(s.mayConsume(s.publication.values), false); await assert.rejects(s.cancelPrepared('cancel-1')); assert.equal(s.committed, true);
});

test('restart reads all five parts without a mutation and preserves immutable historical publication despite later retirement', async () => {
  const h = harness(), s = await h.ready(); h.state.publication.current_active = false; h.state.publication.current_generation = '9007199254740994';
  const start = h.calls.length, restored = await h.m.EditorBusinessSessionCoordinator.restore(s.proof, h.hooks);
  assert.deepEqual(h.calls.slice(start).map(w => JSON.parse(w).editor_intent_ref.part), ['submission', 'publication', 'save', 'inspect', 'close']);
  assert.equal(restored.originalRequest, s.originalRequest); assert.equal(restored.originalSave, h.state.save);
  assert.equal(restored.businessUnknown, true); assert.equal(restored.committed, false); await restored.inspect(); assert.equal(restored.qualified, true);
  assert.equal(h.calls.at(-1), h.state.inspect);
});

test('restart rejects changed literal/hash/publication text/inconsistent asset aliases/order and mixed-phase snapshots', async () => {
  const mutations = [v => v.request_json += ' ', v => v.proof.request_sha256 = 'b'.repeat(64),
    v => v.publication.values.title.text = 'foreign', v => v.publication.values.assets[0].aliases.reverse(),
    v => v.publication.assets.reverse(),
    v => { v.phase = 'prepared'; v.current_generation = '1'; }];
  for (let i = 0; i < mutations.length; i++) {
    const h = harness(), s = await h.ready(); h.setIntercept((w, p, n, route) => {
      const r = route(), part = p.editor_intent_ref?.part;
      if (part === (i < 2 ? 'submission' : 'publication')) mutations[i](r.editor_intents[0]); return r;
    });
    await assert.rejects(h.m.EditorBusinessSessionCoordinator.restore(s.proof, h.hooks));
  }
});

test('restored asset metadata must match actual historical business assets before receipt qualification', async () => {
  const h = harness(), s = await h.ready(); h.setIntercept((w, p, n, route) => {
    const r = route(); if (p.editor_intent_ref?.part === 'publication') r.editor_intents[0].publication.assets[0].sha256 = 'e'.repeat(64); return r;
  });
  // A self-consistent native publication View is a trusted producer input.
  // ETS cannot reconstruct prepared protobuf SHA from JSON. Historical Card
  // inventory is nevertheless checked against its complete retained pins.
  const restored = await h.m.EditorBusinessSessionCoordinator.restore(s.proof, h.hooks);
  await assert.rejects(restored.inspect()); assert.equal(restored.qualified, false); assert.equal(restored.committed, true);
});

test('subsequent publication part requires full original selection, pin metadata and aliases against the frozen publication', async () => {
  const changes = [v => v.publication.values.title.selection_base = 0, v => v.publication.values.assets[0].aliases.reverse(),
    v => { v.publication.assets[0].sha256 = 'e'.repeat(64); }, v => { v.publication.assets.reverse(); v.publication.values.assets.reverse(); }];
  for (const mutate of changes) {
    const h = harness(), s = await h.ready(); h.setIntercept((w, p, n, route) => { const r = route(); mutate(r.editor_intents[0]); return r; });
    await assert.rejects(s.readPart('publication')); assert.ok(s.pendingRead); assert.equal(s.committed, false);
  }
});

test('prepared restart does not invent save/inspect plans; empty read payload is not cancellation or business absence', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent();
  const restored = await h.m.EditorBusinessSessionCoordinator.restore(s.proof, h.hooks); assert.equal(restored.phase, 'prepared');
  assert.equal(restored.originalSave, ''); assert.equal(restored.committed, false); assert.throws(() => restored.retrySave(), /transport/);
  await restored.issueIntent(); await restored.loadTransport(); await restored.retrySave(); assert.equal(restored.qualified, true);
});

test('native cancel prepared CAS has a fixed independent literal and unknown cancellation retries exact bytes', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent(); h.setIntercept((w, p, n, route) => {
    const result = route(); if (p.action === 'editor_intent_close' && n === 1) throw new Error('lost cancel'); return result;
  });
  await assert.rejects(s.cancelPrepared('cancel-1')); const original = s.originalCancel;
  assert.equal(s.pendingNative, 'cancel'); assert.equal(s.committed, false); await s.retryCancel(); assert.equal(h.calls.at(-1), original);
  assert.equal(s.phase, 'closed'); await s.readPart('close'); assert.throws(() => s.save(), /transport/);
  const restored = await h.m.EditorBusinessSessionCoordinator.restore(s.proof, h.hooks); assert.equal(restored.phase, 'closed');
  assert.equal(restored.originalCancel, original); assert.equal(restored.committed, false);
});

test('prepared historical retry returning current issued state does not grant prepared cancel rights', async () => {
  const h = harness(), s = await h.freeze(); h.setIntercept((w, p, n, route) => {
    const result = route(); if (n === 0) throw new Error('lost prepare'); return result;
  });
  await assert.rejects(s.prepareIntent()); const nativeIssue = JSON.stringify({ action: 'editor_intent_issue', editor_intent_issue: { intent: h.state.proof, expected_generation: '1' } });
  await h.hooks.send(nativeIssue); await s.retryPrepare(); assert.equal(s.phase, 'issued');
  await assert.rejects(s.cancelPrepared('cancel-1')); assert.equal(s.committed, false); await s.loadTransport(); await s.save(); assert.equal(s.qualified, true);
});

test('intent discovery is read-only and rejects duplicate IDs, foreign card, mixed parts and invalid pagination cursor', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent(); const page = await h.m.EditorBusinessSessionCoordinator.discover('', 16, h.business.id, h.hooks);
  assert.equal(page.intents[0].proof.intent_id, s.proof.intent_id); assert.equal(page.nextAfter, '');
  for (const mutate of [r => r.editor_intents.push(plain(r.editor_intents[0])), r => r.editor_intents[0].card_id = 'foreign',
    r => r.editor_intents[0].save_request_json = 'partial', r => r.intent_next_after = 'other', r => r.effect = 'committed']) {
    h.setIntercept((w, p, n, route) => { const r = route(); mutate(r); return r; });
    await assert.rejects(h.m.EditorBusinessSessionCoordinator.discover('', 16, h.business.id, h.hooks));
  }
});

test('complete native list DTO is copied before async SHA so caller mutation cannot replace its visible identity', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent(); let returned, release, stalled = false;
  h.setIntercept((w, p, n, route) => { returned = route(); return returned; });
  h.hooks.hash = bytes => { if (stalled) return Promise.resolve(sha(bytes)); stalled = true;
    return new Promise(resolve => { release = () => resolve(sha(bytes)); }); };
  const page = h.m.EditorBusinessSessionCoordinator.discover('', 16, h.business.id, h.hooks);
  for (let n = 0; n < 10 && !release; n++) await Promise.resolve(); assert.ok(release);
  returned.editor_intents[0].card_id = 'foreign-after-check'; returned.editor_intents[0].proof.request_sha256 = 'b'.repeat(64);
  release(); const result = await page; assert.equal(result.intents[0].card_id, h.business.id);
  assert.equal(result.intents[0].proof.request_sha256, sha(s.originalRequest)); assert.equal(h.calls.at(-1) && action(h.calls.at(-1)), 'editor_intent_list');
});

test('full UTF8 reply/transport budgets fail closed before sending and never truncate original bytes', async () => {
  const h = harness(), s = await h.freeze(); await s.prepareIntent(); await s.issueIntent();
  h.setIntercept((w, p, n, route) => { const r = route(); r.extra = '🧪'.repeat(131073); return r; });
  await assert.rejects(s.loadTransport(), /bytes limit/); assert.ok(s.pendingRead); assert.throws(() => s.save());
  h.setIntercept(undefined); await s.retryRead(); await s.readPart('inspect'); const original = s.originalSave;
  h.setIntercept((w, p, n, route) => { const r = route(); if (p.action === 'editor_save') r.extra = '🧪'.repeat(131073); return r; });
  await assert.rejects(s.save(), /bytes limit/); assert.equal(s.committed, true); assert.equal(s.qualified, false); assert.equal(s.originalSave, original);
});

test('continued_todos needs an own qualified historical baseline, constant root and a real source0 publication', async () => {
  const h = harness(), s = await h.ready(); await s.save(); const original = s.originalRequest, receipt = s.confirmed;
  const publication = s.publication; Object.assign(publication.scope, { source_kind: 0, source_revision: receipt.revision,
    source: receipt.historical_card.source, draft_id: 'successor' });
  Object.assign(publication, { operation_id: 'successor-save', generation: '1', current_generation: '1', request_sha256: 'e'.repeat(64) });
  publication.values.todos.text = '乙\n丙'; const business = JSON.parse(s.originalRequest).business;
  Object.assign(business, { action: 'edit', source: receipt.historical_card.source, operation: 'business-2', todos: publication.values.todos.text });
  const next = await s.continueTodos(business, publication, 'prepare-2', h.hooks), submission = JSON.parse(next.originalRequest);
  assert.equal(submission.mode, 'continued_todos'); assert.equal(submission.continuation.root_request_json, original);
  assert.equal(submission.continuation.baseline.content_sha256, receipt.content_sha256);
  publication.scope.source_kind = 1; await assert.rejects(s.continueTodos(business, publication, 'prepare-3', h.hooks));
});

test('complete u64 expected revision is retained without Number conversion across issued restore and inspect', async () => {
  const h = harness({ mode: 'edit', revision: '18446744073709551614', assets: [] }), s = await h.ready();
  assert.equal(s.view.expected_revision, '18446744073709551615'); const restored = await h.m.EditorBusinessSessionCoordinator.restore(s.proof, h.hooks);
  await restored.inspect(); assert.equal(restored.confirmed.revision, '18446744073709551615');
  assert.equal(JSON.parse(h.calls.at(-1)).editor_commit.expected_revision, '18446744073709551615');
});

test('fresh actual Store complete intent parts restore the exact native registered save/inspect literals and qualify historical Card bytes', async () => {
  const fixture = JSON.parse(fs.readFileSync(process.env.HMOS_EDITOR_INTENT_FIXTURE ||
    path.resolve(__dirname, '../reports/ui-source/v24/editor-intent-store-fixture.json'), 'utf8'));
  assert.equal(fixture.producer, 'fresh actual Store'); const proof = fixture.prepare_reply.editor_intents[0].proof;
  assert.equal(fixture.prepare_reply.effect, 'committed'); assert.equal(fixture.issue_reply.effect, 'committed');
  assert.deepEqual(fixture.issue_request.editor_intent_issue, { intent: proof, expected_generation: '1' });
  assert.equal(proof.request_sha256, sha(fixture.request_json));
  const submission = JSON.parse(fixture.request_json);
  assert.equal(proof.intent_id, 'morrow-host-editor-intent-' + framedHash('morrow.hmos.editor-intent.v1\0',
    submission.business.id, submission.business.operation));
  const save = fixture.parts.save.editor_intents[0].save_request_json, inspect = fixture.parts.inspect.editor_intents[0].inspect_request_json;
  const h = harness(), calls = [];
  const hooks = { ...h.hooks, async send(wire) {
    calls.push(wire); const p = JSON.parse(wire);
    if (p.action === 'editor_intent_read') { assert.equal(p.editor_intent_ref.intent_id, proof.intent_id);
      assert.equal(p.editor_intent_ref.prepare_operation, proof.prepare_operation); return plain(fixture.parts[p.editor_intent_ref.part]); }
    if (p.action === 'editor_save') { assert.equal(wire, save); return plain(fixture.business_reply); }
    assert.equal(p.action, 'editor_commit_inspect'); assert.equal(wire, inspect); return plain(fixture.inspect_reply);
  } };
  const s = await h.m.EditorBusinessSessionCoordinator.restore(proof, hooks);
  assert.equal(calls.length, 5); assert.equal(s.originalRequest, fixture.request_json); assert.equal(s.originalSave, save); assert.equal(s.originalInspect, inspect);
  assert.equal(s.committed, false); const actual = await s.inspect(); assert.equal(s.qualified, true);
  assert.deepEqual(plain(actual), fixture.inspect_reply.editor_commit);
  assert.equal(actual.content_sha256, sha(Buffer.from(actual.historical_card.source, 'hex')));
  assert.equal(actual.publication_sha256, publicationHash(actual.card_id, actual.publication));
  const retry = await h.m.EditorBusinessSessionCoordinator.restore(proof, hooks); await retry.retrySave();
  assert.equal(calls.at(-1), save); assert.equal(retry.qualified, true); assert.deepEqual(plain(retry.confirmed), fixture.business_reply.editor_commit);
});

test('fresh actual Store cancel_prepared complete five-part closed context restores its original Close without inventing business plans', async () => {
  const fixture = JSON.parse(fs.readFileSync(process.env.HMOS_EDITOR_INTENT_FIXTURE ||
    path.resolve(__dirname, '../reports/ui-source/v24/editor-intent-store-fixture.json'), 'utf8')), h = harness(), row = fixture.cancel;
  const proof = row.prepare_reply.editor_intents[0].proof, calls = [];
  const hooks = { ...h.hooks, async send(wire) { calls.push(wire); const p = JSON.parse(wire); assert.equal(p.action, 'editor_intent_read');
    assert.equal(p.editor_intent_ref.intent_id, proof.intent_id); return plain(row.parts[p.editor_intent_ref.part]); } };
  const s = await h.m.EditorBusinessSessionCoordinator.restore(proof, hooks);
  assert.equal(s.phase, 'closed'); assert.equal(s.originalRequest, row.request_json); assert.equal(s.originalSave, ''); assert.equal(s.originalInspect, '');
  assert.equal(JSON.parse(s.originalCancel).editor_intent_close.request_json, row.close_request.editor_intent_close.request_json);
  assert.equal(s.committed, false); assert.equal(s.qualified, false); assert.throws(() => s.retrySave(), /transport/);
  assert.equal(calls.length, 5); assert.equal(row.close_reply.effect, 'committed');
  assert.equal(row.parts.close.effect, 'not_committed'); assert.equal(row.parts.close.receipt_revision, '');
});
