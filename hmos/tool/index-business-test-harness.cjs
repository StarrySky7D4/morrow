'use strict';
// Fresh, verbatim Index methods and actual ETS Session/Business/Draft/FieldPolicy
// and Workbench queue. Native replies, platform APIs, component readiness and
// lifecycle completion are controlled seams. This is not Store/SDK/device proof.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const { response: fieldReply, deferred } = require('./editor-field-test-harness.cjs');
const root = path.resolve(__dirname, '../entry/src/main/ets/model');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets');
const source = fs.readFileSync(sourcePath, 'utf8');
const modelSources = new Map(fs.readdirSync(root).filter(name => name.endsWith('.ets')).map(name =>
  [name.slice(0, -4), fs.readFileSync(path.join(root, name), 'utf8')]));
const usedModels = new Set();
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const u64 = value => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(value)); return b; };
const frame = value => { const b = Buffer.from(value); return Buffer.concat([u64(b.length), b]); };
const identityHash = (domain, first, second) => sha(Buffer.concat([Buffer.from(domain), frame(first), frame(second)]));
const publicationHash = (card, p) => sha(Buffer.concat([Buffer.from('morrow.hmos.editor-publication.v1\0'), frame(card),
  frame(p.draft_id), u64(p.generation), frame(p.save_operation), Buffer.from(p.request_sha256, 'hex')]));
const settle = async () => { for (let n = 0; n < 80; n++) await Promise.resolve(); };
function actualMethod(name, required = true) {
  const re = new RegExp('^  private (?:async )?' + name + '\\(', 'm'), match = re.exec(source);
  if (!match) { assert.ok(!required, 'actual Index method exists: ' + name); return ''; }
  // Actual helpers can be separated by field initializers. Select the method
  // node end so a helper extraction cannot construct unrelated controllers.
  const prefix = 'class Extracted {\n';
  const parsed = ts.createSourceFile('extracted.ts', prefix + source.slice(match.index) + '\n}', ts.ScriptTarget.ES2020, true, ts.ScriptKind.TS);
  const node = parsed.statements[0]?.members?.[0];
  assert.ok(node && ts.isMethodDeclaration(node) && node.name.getText(parsed) === name, 'actual Index method boundary: ' + name);
  const result = source.slice(match.index, match.index + node.end - prefix.length);
  assert.equal((result.match(re) || []).length, 1); return result;
}
const names = ['save', 'businessPending', 'businessImportsReady', 'businessCurrent', 'businessParentMatches',
  'businessExact', 'businessHooks', 'businessChanged', 'loadBusinessIntents', 'restoreBusinessIntent',
  'reconcileBusiness', 'resumeBusinessEditor', 'submit', 'retry', 'command', 'discardDraft',
  'discardDraftConfirmed', 'closeEditor', 'keepDraftAndClose', 'beginRawFork', 'retryRawFork',
  'canPaste', 'canImportAttachment', 'importBoundary', 'newCardTodos', 'current', 'ownsEditorView',
  'draftField', 'directOwner', 'directValue', 'captureDirectInput', 'adoptExternalInput',
  'retirementCapture', 'draftSelectionChanged', 'flushDraft'];
names.push('finishBusinessInput', 'continueBusinessHandoff', 'finishBusinessClose', 'refreshAfterBusiness', 'reconcileBusinessHandoff');
for (const name of ['editorIdleForRead', 'currentEditorCardComplete', 'recoverLinkedBusinessDraft', 'recoverCurrentBusinessChild',
  'prepareCurrentBusinessRecovery', 'installRecoveredBusinessChild']) if (!names.includes(name)) names.push(name);
// Shared current Save entrypoint for the existing field/todo integration
// suites. Every helper here is extracted verbatim from the same freeze.
for (const name of ['editorInputChanged', 'draftTextChanged', 'restoreSelection', 'fieldCountIndex', 'setFieldCount', 'fieldCountLabel',
  'refreshFieldCount', 'refreshFieldCounts', 'inputReadyFor', 'taskRenameChanged', 'mutate', 'renameTask', 'canEditTasks',
  'foregroundChanged', 'editorLeaseMounted', 'editorLeaseRevoked', 'leaseTextChanged', 'leaseSelectionChanged', 'leaseFocusChanged',
  'leaseUncaptured', 'revokeEditorView', 'remountEditorView', 'bindInputRules', 'retryInputRules', 'captureTodoRows',
  'closeSavedEditor', 'retireDraft', 'focusDraftField', 'blurDraftField', 'todoDragPosition', 'directInputChanged']) if (!names.includes(name)) names.push(name);
// Root may introduce another business predicate while wiring Index. Include
// its actual method rather than substituting a hand-written truthy helper.
for (const match of source.matchAll(/^  private (?:async )?(business\w+)\(/gm)) {
  if (!names.includes(match[1])) names.push(match[1]);
}
const methods = names.map(name => actualMethod(name)).join('\n');
const bindingStart = source.indexOf('class BusinessEditorBinding {'), bindingEnd = source.indexOf('\nclass ', bindingStart + 1);
assert.ok(bindingStart >= 0 && bindingEnd > bindingStart, 'actual binding class');
const bindingCode = source.slice(bindingStart, bindingEnd);
const submittedFields = source.split(/\r?\n/).filter(line => /^  private submitted\w+:/.test(line)).join('\n');
function compile(input, filename) {
  const r = ts.transpileModule(input, { fileName: filename, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.deepEqual((r.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), [], 'SDK TS transpilation');
  return r.outputText;
}
const pageCode = compile(bindingCode + '\nexport class ActualIndexBusiness {\n' + submittedFields + '\n' + methods + '\n}', sourcePath);
function harness(options = {}) {
  const cache = new Map(), events = [], calls = [], nativeCalls = [], finishes = [], timers = new Map();
  let timer = 0, uuid = 0, page, draft, intercept, rawIntercept, latestRaw;
  const native = { async request(wire) {
    nativeCalls.push(wire); const p = JSON.parse(wire); events.push({ kind: 'native', action: p.action, wire });
    const route = () => receiver(wire, p);
    return JSON.stringify(intercept ? await intercept(wire, p, route) : await route());
  } };
  function load(name) {
    if (name === 'libmorrow.so') return { default: native };
    if (name === '@kit.ArkTS') return { util: { TextEncoder: class { encodeInto(value) { return new TextEncoder().encode(value); } } } };
    if (name === '@kit.CryptoArchitectureKit') return { cryptoFramework: { createMd() {
      const h = crypto.createHash('sha256'); return { async update({ data }) { h.update(data); },
        async digest() { return { data: new Uint8Array(h.digest()) }; } };
    } } };
    name = name.replace(/^\.\//, ''); if (cache.has(name)) return cache.get(name);
    const exports = {}, file = path.join(root, name + '.ets'); cache.set(name, exports);
    assert.ok(modelSources.has(name), 'model existed in the run source freeze: ' + name); usedModels.add(name);
    vm.runInNewContext(compile(modelSources.get(name), file), { exports, require: load, Uint8Array,
      setTimeout: fn => { const id = ++timer; timers.set(id, fn); return id; }, clearTimeout: id => timers.delete(id),
      encodeURIComponent }, { filename: file }); return exports;
  }
  const m = { ...load('EditorDraft'), ...load('EditorFieldPolicy'), ...load('EditorBusiness'), ...load('EditorBusinessSession'),
    ...load('EditorDraftFork'), ...load('EditorPaste'), ...load('EditorBusinessHandoff'), ...load('EditorBusinessRecovery') };
  const wb = load('Workbench'), actualSend = wb.workbench.send.bind(wb.workbench);
  wb.workbench.send = wire => {
    const action = JSON.parse(wire).action; calls.push(wire); events.push({ kind: 'admit', action, wire }); return actualSend(wire);
  };
  const scope = new m.DraftScope(); Object.assign(scope, { card_id: 'actual-index-card', draft_id: 'actual-index-parent',
    source_kind: options.mode === 'edit' ? 0 : 1, source_revision: options.mode === 'edit' ? '7' : '0',
    source: options.mode === 'edit' ? '0a02aabb' : '' });
  const values = new m.Values(); values.title.text = '原完整标题'; values.description.text = 'S1 原文 汉字 🧪 é.';
  values.hypothesis.text = 'h'; values.conclusion.text = 'c'; values.todos.text = options.mode === 'edit' ? '' : 'one\ntwo';
  values.category = '灵感'; values.stage = '待整理';
  if (options.values) Object.assign(values, plain(options.values));
  let record = options.record ? plain(options.record) : undefined;
  if (record) { Object.assign(scope, record.scope); Object.assign(values, plain(record.values)); }
  const state = { phase: '', submission: '', publication: null, proof: null, save: '', inspect: '', close: '', issueOperation: '',
    handoff: '', retirement: '', disposition: '', generation: '', first: undefined, current: undefined, parentRetired: undefined };
  function metadataView(part = 'summary') {
    const p = JSON.parse(state.submission), revision = m.nextGeneration(state.publication.scope.source_revision);
    return { proof: plain(state.proof), card_id: p.business.id, business_operation: p.business.operation,
      expected_revision: revision, phase: state.phase, current_generation: state.generation || (state.phase === 'prepared' ? '1' : '2'),
      current_active: state.phase !== 'closed', repeated: false, part, request_json: part === 'submission' ? state.submission : '',
      publication: part === 'publication' ? plain(state.publication) : null, save_request_json: part === 'save' ? state.save : '',
      inspect_request_json: part === 'inspect' ? state.inspect : '', close_request_json: part === 'close' ? state.close : '',
      close_disposition: state.disposition || (state.phase === 'closed' ? 'cancel_prepared' : ''), issue_operation: state.issueOperation,
      handoff_request_json: part === 'handoff' ? state.handoff : '', retirement_request_json: part === 'retirement' ? state.retirement : '' };
  }
  const metadataReply = (part, mutation = false) => ({ ok: true, error: '', effect: mutation ? 'committed' : 'not_committed',
    receipt_revision: mutation ? metadataView().current_generation : '', editor_intents: [metadataView(part)], intent_next_after: '' });
  function businessReply() {
    const p = JSON.parse(state.submission), b = p.business, revision = m.nextGeneration(state.publication.scope.source_revision);
    const source = Buffer.from('controlled-full-source:' + b.operation + ':' + revision).toString('hex');
    return { ok: true, error: '', effect: 'committed', receipt_revision: revision, cards: [], drafts: [],
      editor_commit: { commit_status: 'committed', qualification: 'development_editor_wire_v1', card_id: b.id, operation: b.operation,
        source_revision: state.publication.scope.source_revision, revision, event_id: 'event-' + b.operation,
        command_sha256: 'd'.repeat(64), content_sha256: sha(Buffer.from(source, 'hex')), request_sha256: sha(state.submission),
        publication_sha256: publicationHash(b.id, p.publication), publication: plain(p.publication), live_matches: true, live_revision: revision,
        historical_card: { id: b.id, revision, source, title: b.title, description: b.description, hypothesis: b.hypothesis,
          conclusion: b.conclusion, category: b.category, stage: b.stage, favorite: false, deleted: false, deleted_at: '0',
          tasks: b.todos ? [...new Set(b.todos.split('\n').map(x => x.trim()).filter(Boolean))].map((text, n) => ({ id: 'task-' + n, text, completion: 0 })) : [],
          assets: state.publication.assets.map(pin => ({ id: pin.selection.asset_id, name: pin.display_name, kind: 'image',
            media_type: pin.media_type, byte_length: pin.byte_length, sha256: pin.sha256 })) } } };
  }
  function registerRetirement() {
    const proposal = JSON.parse(state.handoff);
    state.retirement = ' ' + JSON.stringify({ schema_version: 1, intent: proposal.intent, plan_operation: proposal.plan_operation,
      handoff_request_sha256: sha(state.handoff), parent: proposal.parent, child_draft_id: proposal.child.draft_id,
      child_operation: proposal.child.operation_id, child_request_sha256: sha(JSON.stringify(proposal.child)),
      operation_id: proposal.retirement_operation }) + '\n';
  }
  function businessLink() {
    const proposal = JSON.parse(state.handoff), commit = businessReply().editor_commit;
    return { schema_version: 1, parent: plain(proposal.parent), intent: plain(proposal.intent), plan_operation: proposal.plan_operation,
      handoff_request_sha256: sha(state.handoff), child_operation: proposal.child.operation_id, committed_operation: commit.operation,
      committed_revision: commit.revision, command_sha256: commit.command_sha256, content_sha256: commit.content_sha256,
      request_sha256: commit.request_sha256, publication_sha256: commit.publication_sha256 };
  }
  function receiver(wire, p) {
    if (p.action === 'draft_save') {
      const request = p.draft, next = m.nextGeneration(request.expected_generation);
      const child = state.current && state.current.scope.draft_id === request.draft_id;
      const raw = Object.assign(child ? m.copyRecord(state.current) : new m.DraftRecord(), { scope: child ? plain(state.current.scope) : plain(scope), values: Object.assign(new m.Values(), plain(request.values),
        { assets: plain(request.assets) }), assets: request.assets.map(selection => {
          const old = (child ? state.current.assets : (latestRaw || record)?.assets || []).find(pin => pin.selection.asset_id === selection.asset_id);
          assert.ok(old, 'controlled raw receiver requires original confirmed pin'); return { ...plain(old), selection: plain(selection) };
        }), generation: next, current_generation: next, active: true, current_active: true,
        operation_id: request.operation_id, request_sha256: sha(JSON.stringify(request)) });
      if (child) state.current = plain(raw); else latestRaw = plain(raw);
      return { ok: true, error: '', effect: 'committed', receipt_revision: next, drafts: [raw], cards: [] };
    }
    if (p.action === 'editor_intent_prepare') {
      const request = JSON.parse(p.editor_intent.request_json); state.phase = 'prepared'; state.submission = p.editor_intent.request_json;
      state.publication = plain(state.current?.scope.draft_id === request.publication.draft_id ? state.current : latestRaw || record);
      state.generation = ''; state.disposition = ''; state.close = ''; state.handoff = ''; state.retirement = '';
      assert.ok(state.publication, 'actual raw receipt exists before intent prepare');
      state.proof = { intent_id: 'morrow-host-editor-intent-' + identityHash('morrow.hmos.editor-intent.v1\0', request.business.id, request.business.operation),
        prepare_operation: p.editor_intent.operation_id, generation: '1', prepared_record_sha256: 'f'.repeat(64), request_sha256: sha(state.submission) };
      state.issueOperation = 'morrow-host-intent-issue-' + identityHash('morrow.hmos.editor-intent-issue.v1\0', state.proof.intent_id, state.proof.prepare_operation);
      return metadataReply('summary', true);
    }
    if (p.action === 'editor_intent_issue') {
      assert.deepEqual(p.editor_intent_issue, { intent: state.proof, expected_generation: '1' }); state.phase = 'issued';
      state.save = ' {"editor_save":' + JSON.stringify({ intent: state.proof, request_json: state.submission }) + ',"action":"editor_save"}\n';
      state.inspect = ' {"editor_commit":' + JSON.stringify({ expected_revision: metadataView().expected_revision, request_json: state.submission }) +
        ',"action":"editor_commit_inspect"}\n'; return metadataReply('summary', true);
    }
    if (p.action === 'editor_intent_read') return metadataReply(p.editor_intent_ref.part);
    if (p.action === 'editor_intent_list') return { ok: true, error: '', effect: 'not_committed', receipt_revision: '',
      editor_intents: state.proof ? [metadataView()] : [], intent_next_after: '' };
    if (p.action === 'draft_continue_business') {
      const literal = p.business_handoff.request_json; if (state.handoff) assert.equal(literal, state.handoff); else state.handoff = literal;
      const proposal = JSON.parse(literal), child = proposal.child, commit = businessReply().editor_commit;
      state.phase = 'handoff_planned'; state.generation = '3'; registerRetirement();
      if (!state.first) state.first = Object.assign(m.copyRecord(state.publication), { scope: { card_id: child.card_id, draft_id: child.draft_id,
        source_kind: 0, source_revision: commit.revision, source: commit.historical_card.source }, generation: '1', current_generation: '1',
        active: true, current_active: true, operation_id: child.operation_id, request_sha256: sha(JSON.stringify(child)),
        values: { ...plain(child.values), assets: plain(child.assets) }, assets: child.assets.map(selection => {
          const old = state.publication.assets.find(pin => pin.selection.asset_id === selection.asset_id);
          assert.ok(old, 'controlled child receiver requires original pin'); return { ...plain(old), selection: plain(selection) };
        }), consumed_imports: [], fork_link: null, fork_retirement: null,
        business_link: businessLink(), business_retirement: null });
      if (!state.current) state.current = plain(state.first);
      return { ...metadataReply('summary', true), receipt_revision: '1', drafts: [plain(state.first)] };
    }
    if (p.action === 'draft_continue_business_retire') {
      assert.equal(p.business_retirement.request_json, state.retirement); const proposal = JSON.parse(state.handoff);
      if (!state.parentRetired) state.parentRetired = Object.assign(m.copyRecord(state.publication), { active: false, current_active: false,
        generation: m.nextGeneration(state.publication.generation), current_generation: m.nextGeneration(state.publication.generation),
        business_retirement: { schema_version: 1, child_draft_id: proposal.child.draft_id, operation_id: proposal.retirement_operation,
          business_link: businessLink() } });
      return { ...metadataReply('summary', true), receipt_revision: state.parentRetired.generation, drafts: [plain(state.parentRetired)] };
    }
    if (p.action === 'editor_intent_close') { state.phase = 'closed'; state.close = p.editor_intent_close.request_json;
      const close = JSON.parse(state.close); state.disposition = close.disposition;
      state.generation = close.disposition === 'saved_exact' ? '4' : close.disposition === 'handoff_retired' ? '5' : '3';
      return metadataReply('summary', true); }
    if (p.action === 'editor_save') { assert.equal(wire, state.save, 'literal registered save'); return businessReply(); }
    if (p.action === 'editor_commit_inspect') { assert.equal(wire, state.inspect, 'literal registered inspection'); return businessReply(); }
    return { ok: true, error: '', effect: 'committed', receipt_revision: '8', cards: plain(page.cards), drafts: [] };
  }
  const rawSender = async wire => {
    if (rawIntercept) await rawIntercept(wire); const reply = await wb.workbench.send(wire);
    if (!reply.ok) throw new m.DraftSaveFailure(reply.effect === 'not_committed', reply.error);
    return reply.drafts[0];
  };
  draft = new m.EditorDraftCoordinator(scope, values, rawSender, () => events.push({ kind: 'raw-changed' }), () => 'raw-op-' + (++uuid), record);
  const pause = draft.pauseWrites.bind(draft), resume = draft.resumeWrites.bind(draft);
  draft.pauseWrites = () => { events.push({ kind: 'pause' }); return pause(); };
  draft.resumeWrites = () => { events.push({ kind: 'resume' }); return resume(); };
  const context = vm.createContext({ exports: {}, ...m, DraftValues: m.Values, DraftTextValue: m.TextValue, Command: wb.Command,
    workbench: wb.workbench, util: { generateRandomUUID: () => 'index-op-' + (++uuid) }, setTimeout: fn => { const id = ++timer; timers.set(id, fn); return id; },
    clearTimeout: id => timers.delete(id) }); vm.runInContext(pageCode, context, { filename: sourcePath }); page = new context.exports.ActualIndexBusiness();
  const fieldPolicy = new m.EditorFieldPolicy(async wire => { const request = JSON.parse(wire); events.push({ kind: 'field', request });
    if (options.fieldIntercept) await options.fieldIntercept(request, page, draft); return JSON.stringify(fieldReply(request.field, request.text)); });
  Object.assign(page, { ready: true, pageAlive: true, foreground: true, editorOpen: true, editorDraft: draft, editorValues: m.copyValues(values), fieldPolicy,
    editorInputEpoch: 0, attachmentEditorIdentity: 'index-editor-owner', editorViewOwner: 'index-editor-owner', editorViewVisible: true, editorViewRevoked: false,
    editorBoundary: '', draftCaptureIncomplete: false, draftCaptureBlocked: new Set(), draftFocusedFields: new Set(['title', 'description']),
    draftSelectionPending: new Set(), draftRestoreInput: false, draftRetiredIdentity: '', retirementInput: new Map(), inputRevisions: new Map(),
    busy: false, pending: '', businessWorking: false, businessStatus: '', businessIntents: [], businessListError: '', businessNext: '', businessRecoveryIdentity: '',
    fieldValidationWorking: false, fieldCountEpochs: new Map(), fieldCountLabels: [], draftWorking: false, draftRetiring: false, draftRetirementUnknown: false, draftConflict: false,
    inputPolicy: { stop: () => {} }, inputRulesRevision: 0, inputRulesMessage: '', todoDragTimer: -1, editorFocusIntent: 0,
    todoInputValue: m.copyText(values.todos), draftStatus: '', draftRetirement: '', rawForkRetirement: '',
    attachmentWorking: false, pasteWorking: false, attachmentPending: '', pendingSpools: [], importRecords: [], taskEditId: '', taskRenameText: '',
    taskRenameValue: new m.TextValue(), taskText: values.todos.text, todoBusinessReady: true, todoFormatPending: false, todoInputRevision: 0,
    title: values.title.text, description: values.description.text, hypothesis: values.hypothesis.text, conclusion: values.conclusion.text, category: values.category,
    selected: scope.source_kind === 1 ? '' : scope.card_id, dirty: false, message: '', draftRecords: record ? [plain(record)] : [], draftSource: scope.source,
    // These legacy source fixtures exercise their original edit/create intent
    // contracts; they do not claim the new fresh native current_v2 read grant.
    editorBusinessMode: options.mode === 'edit' ? 'edit' : 'create',
    cards: scope.source_kind === 1 ? [] : [{ id: scope.card_id, source: scope.source, revision: scope.source_revision, title: values.title.text,
      description: values.description.text, hypothesis: values.hypothesis.text, conclusion: values.conclusion.text, category: values.category, stage: values.stage,
      favorite: false, deleted: false, deleted_at: '0', tasks: [], assets: [] }],
    directInput: { capture: () => true, bind: () => {}, canConfirm: () => true, stop: () => {}, view: () => undefined, unbind: () => {}, retry: () => {} }, inputReadyFor: () => true,
    refreshFieldCount: () => {}, draftChanged: () => {}, refreshPreview: () => {}, loadDrafts: async () => {},
    // Deliberate boundary: these tests observe business completion admission,
    // not Root's separate native handoff/close/view lifecycle implementation.
    finishBusinessInput: async (session, binding) => { finishes.push({ session, binding, receipt: plain(session.confirmed) }); },
    workspaceConditionsChanged: () => {}, bindInputRules: () => {}, refreshFieldCounts: () => {}, updateMarkdown: () => {},
    mediaPlayback: { pauseForBackground: () => events.push({ kind: 'media-pause' }) }, exitMediaFullscreen: () => {}, fileOpen: undefined,
    revokeEditorView: async parent => { events.push({ kind: 'controlled-view-detach' });
      page.editorBoundaryDraft = parent; page.editorViewRevoked = true; page.editorViewVisible = false; return true; },
    getUIContext: () => ({ showAlertDialog: () => events.push({ kind: 'alert' }) }) });
  return { page, draft, m, state, calls, nativeCalls, events, finishes, metadataView, metadataReply, businessReply,
    setIntercept(fn) { intercept = fn; }, setRawIntercept(fn) { rawIntercept = fn; },
    dispose() { if (!draft.disposed) draft.dispose(); if (page.editorDraft && !page.editorDraft.disposed) page.editorDraft.dispose(); timers.clear(); },
    enableActualLifecycle() { delete page.finishBusinessInput; }, registerRetirement,
    enableActualFields() { for (const name of ['refreshFieldCount', 'refreshFieldCounts', 'inputReadyFor', 'bindInputRules',
      'revokeEditorView', 'remountEditorView']) delete page[name]; },
    adopt(name, text) { const next = m.copyText(page.editorValues[name]); next.text = text; next.selection_base = text.length; next.selection_extent = text.length;
      page.adoptExternalInput(name, next); },
    async issued() { await page.save(); assert.equal(page.businessSession?.phase, 'issued'); return page.businessSession; },
    loadFixture(fixture) { state.phase = 'issued'; state.submission = fixture.request_json;
      state.publication = plain(fixture.parts.publication.editor_intents[0].publication); state.proof = plain(fixture.prepare_reply.editor_intents[0].proof);
      state.save = fixture.parts.save.editor_intents[0].save_request_json; state.inspect = fixture.parts.inspect.editor_intents[0].inspect_request_json;
      state.issueOperation = fixture.issue_reply.editor_intents[0].issue_operation;
      intercept = (wire, p) => { if (p.action === 'editor_intent_read') return plain(fixture.parts[p.editor_intent_ref.part]);
        if (p.action === 'editor_intent_list') return { ok: true, error: '', effect: 'not_committed', receipt_revision: '',
          editor_intents: [plain(fixture.issue_reply.editor_intents[0])], intent_next_after: '' };
        if (p.action === 'editor_save') { assert.equal(wire, state.save); return plain(fixture.business_reply); }
        if (p.action === 'editor_commit_inspect') { assert.equal(wire, state.inspect); return plain(fixture.inspect_reply); }
        throw new Error('Unexpected actual-Store DTO replay action: ' + p.action); };
    } };
}
function assertSourceUnchanged() {
  assert.equal(fs.readFileSync(sourcePath, 'utf8'), source, 'Index did not change during the run');
  for (const name of usedModels) assert.equal(fs.readFileSync(path.join(root, name + '.ets'), 'utf8'), modelSources.get(name),
    'actual loaded model did not change during the run: ' + name);
}
// Compatibility of test inputs only: old suites use the current strict Save
// and actual Workbench tail. No legacy create/edit command is synthesized.
function editorHarness(options = {}, profile = 'field') {
  const field = profile === 'field', events = [], text = value => ({ text: value, selection_base: -1, selection_extent: -1,
    affinity: 0, directional: false, composing_start: -1, composing_end: -1 });
  const existing = field ? !options.create : !!options.existing, pinId = field ? 'confirmed-pin-original' : 'confirmed-create-pin';
  const values = { title: text(field ? 'Title' : 'Original title'), description: text(field ? 'Body' : 'Original body'),
    hypothesis: text('Hypothesis'), conclusion: text('Conclusion'), todos: text(options.todos ?? ''), category: '实验', stage: '待验证',
    assets: [{ origin: 3, asset_id: pinId, aliases: ['original.png'] }] };
  const scope = { card_id: field ? 'card-field-fixture' : 'card-create-fixture', draft_id: field ? 'draft-field-fixture' : 'draft-create-fixture',
    source_kind: existing ? 0 : 1, source_revision: existing ? (field ? '1' : '7') : '0', source: existing ? '0a02aabb' : '' };
  const initialRecord = { scope, values, generation: '1', current_generation: '1', active: true, current_active: true, repeated: false,
    operation_id: 'original-fixture-raw', request_sha256: 'b'.repeat(64), consumed_imports: [], fork_link: null, fork_retirement: null,
    business_link: null, business_retirement: null, assets: [{ selection: plain(values.assets[0]), pin_id: 'draft-asset-0',
      display_name: 'original.png', media_type: 'image/png', byte_length: '8', sha256: 'a'.repeat(64) }] };
  let base; const ready = harness({ mode: existing ? 'edit' : 'create', record: initialRecord,
    fieldIntercept: async request => { events.push({ kind: 'field-check', request: plain(request) });
      if (options.fieldEffect) await options.fieldEffect({ request, page: base.page, draft: base.draft, events });
      if (options.fieldUnknown) throw Error('controlled field worker Unknown'); } }); base = ready;
  const { page, draft, m } = base, owned = [draft]; base.enableActualFields();
  page.directInput.canConfirm = () => options.directReady !== false;
  page.todoBusinessReady = options.todosReady !== false;
  page.draftFocusedFields = new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']);
  if (existing) page.cards[0].tasks = [{ id: field ? 'task-fixture' : 'original-v2-task-id', text: field ? 'Task' : 'Existing task', completion: 0 }];
  // Current actual builder callback, with its original guard behavior.
  const cancel = '.onClick(() => { this.taskEditId = \'\'; this.taskRenameText = \'\'; })';
  assert.ok(source.includes(cancel)); page.cancelRenameFromActualBuilder = function () { this.taskEditId = ''; this.taskRenameText = ''; };
  const revoke = page.revokeEditorView.bind(page), remount = page.remountEditorView.bind(page);
  page.revokeEditorView = parent => { const run = revoke(parent); page.editorLeaseRevoked(page.editorViewOwner); return run; };
  page.remountEditorView = parent => { const run = remount(parent); page.editorLeaseMounted(page.editorViewOwner); return run; };
  if (!field) base.enableActualLifecycle();
  base.setIntercept(async (wire, p, route) => {
    if (p.action === 'draft_save') {
      events.push({ kind: field ? 'draft-save' : 'raw-save', request: plain(p.draft), serialized: wire });
      const effect = options.draftEffect || options.rawEffect;
      if (effect) await effect({ request: p.draft, page, draft, events });
      if (options.draftUnknown || options.rawUnknown) throw Error('controlled raw journal Unknown'); return route();
    }
    if (p.action === 'editor_intent_close') {
      const command = JSON.parse(p.editor_intent_close.request_json), count = events.filter(e => e.kind === 'retirement').length + 1;
      events.push({ kind: 'retirement', command: plain(command), serialized: wire });
      if (options.retireEffect) await options.retireEffect({ command, count, page, draft, events });
      if (options.retireUnknown) throw Error('controlled exact close Unknown');
      if (options.retireResult) return options.retireResult({ command, count, page, draft, events }); return route();
    }
    const strict = p.action === 'editor_save', ordinary = !p.action.startsWith('editor_') && !p.action.startsWith('draft_') && p.action !== 'list';
    if (strict || ordinary) {
      const submission = strict ? JSON.parse(base.state.submission) : undefined, command = strict ? submission.business : p;
      const kind = field ? 'business-send' : 'business', count = events.filter(e => e.kind === kind).length + 1;
      events.push({ kind, command: plain(command), publication: submission && plain(submission.publication), serialized: wire, strict });
      if (options.businessEffect) await options.businessEffect({ command, count, page, draft, events });
      if (options.businessUnknown) throw Error('controlled business Unknown');
      if (options.businessResult) return typeof options.businessResult === 'function' ?
        options.businessResult({ command, count, page, draft, events }) : options.businessResult;
    }
    return route();
  });
  const running = new Set(), errors = [];
  for (const name of ['save', 'submit', 'retry', 'flushDraft', 'retireDraft', 'reconcileBusiness', 'reconcileBusinessHandoff']) {
    const actual = page[name].bind(page); page[name] = (...args) => {
      const run = Promise.resolve(actual(...args)); running.add(run);
      run.then(() => running.delete(run), error => { running.delete(run); errors.push(error); }); return run;
    };
  }
  function edit(value, name = 'todos') {
    const next = typeof value === 'string' ? Object.assign(new m.TextValue(), { text: value }) : Object.assign(new m.TextValue(), value);
    page.editorValues[name] = m.copyText(next); page.editorInputEpoch++;
    page[name === 'todos' ? 'taskText' : name] = next.text; page.editorDraft.update(m.copyValues(page.editorValues)); page.dirty = true;
  }
  function replaceEditor(todoText = page.editorValues.todos.text) {
    const replacementRecord = m.copyRecord(initialRecord); replacementRecord.scope.card_id = 'replacement-editor-card';
    replacementRecord.scope.draft_id = 'replacement-editor-draft'; replacementRecord.scope.source_revision = '12'; replacementRecord.scope.source_kind = 0;
    replacementRecord.values = m.copyValues(page.editorValues); replacementRecord.values.todos.text = todoText;
    const replacement = new m.EditorDraftCoordinator(replacementRecord.scope, replacementRecord.values,
      async () => { throw Error('old response must not write replacement'); }, () => {}, () => 'replacement-new-raw', replacementRecord);
    owned.push(replacement); page.editorDraft = replacement; page.editorValues = replacement.current;
    page.attachmentEditorIdentity = 'replacement-editor-owner'; page.editorViewOwner = 'replacement-editor-owner'; page.editorViewRevoked = false;
    page.editorViewVisible = true; page.editorBoundary = ''; page.editorInputEpoch++; page.selected = replacement.scope.card_id;
    page.draftSource = replacement.scope.source; page.taskText = todoText; page.editorOpen = true; return replacement;
  }
  async function finish() { while (running.size) await Promise.all([...running]); await settle(); assert.deepEqual(errors, [], 'no late helper error'); }
  return { ...base, events, draftApi: m, edit, replaceEditor, finish, success: () => base.businessReply(),
    close: async () => { try { await finish(); } finally { for (const d of owned) if (!d.disposed) d.dispose(); base.dispose(); } },
    business: () => events.filter(e => e.kind === (field ? 'business-send' : 'business')),
    raw: () => events.filter(e => e.kind === (field ? 'draft-save' : 'raw-save')),
    retirements: () => events.filter(e => e.kind === 'retirement'),
    save: async () => { const run = page.save(); for (let n = 0; n < 12; n++) await settle(); return { run }; } };
}
module.exports = { harness, editorHarness, plain, sha, settle, deferred, actualMethod, sourcePath, source, assertSourceUnchanged,
  sourceSHA: sha(source), modelPaths: ['EditorDraft', 'EditorBusiness', 'EditorBusinessSession', 'EditorFieldPolicy', 'Workbench'].map(name => path.join(root, name + '.ets')),
  modelIdentities: () => [...usedModels].sort().map(name => ({ path: path.join(root, name + '.ets'), sha256: sha(modelSources.get(name)) })) };
