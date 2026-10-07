'use strict';
// Verbatim fresh Index business methods plus actual ETS draft/policy/Command
// models. Editing values, worker replies and service receipts are synthetic.
// This does not execute ArkUI builders, IME, Rust persistence or a device.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto'), assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const { response: fieldReply, deferred } = require('./editor-field-test-harness.cjs');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets');
const source = fs.readFileSync(sourcePath, 'utf8'), modelRoot = path.resolve(__dirname, '../entry/src/main/ets/model');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 100; i++) await Promise.resolve(); };
function section(from, to) {
  const start = source.indexOf(from), end = source.indexOf(to, start + from.length);
  assert.ok(start >= 0 && end > start, 'fresh actual Index anchors: ' + from);
  assert.equal(source.indexOf(from, start + 1), -1, 'unique actual Index anchor: ' + from);
  return source.slice(start, end);
}
function actualMethod(name) {
  const pattern = new RegExp('^  private (?:async )?' + name + '\\(', 'm'), match = pattern.exec(source);
  assert.ok(match, 'actual Index method: ' + name);
  const end = /^  }\r?$/mg; end.lastIndex = match.index;
  const closing = end.exec(source); assert.ok(closing, 'actual method closing: ' + name);
  return source.slice(match.index, closing.index + closing[0].length);
}
const methods = [
  section('  private draftField(', '  private updateMarkdown('),
  section('  private draftTextChanged(', '  private restoreSelection('),
  section('  private async flushDraft(', '  private refreshEditorAssets('),
  section('  private command(', '  private moveTask('),
  section('  private current():', '  private visibleCards('),
  actualMethod('editorInputChanged'),
  actualMethod('retireDraft'), actualMethod('closeSavedEditor'),
  section('  private ownsEditorView(', '  private attachDraft(')
].join('\n');
const submittedFields = source.split(/\r?\n/).filter(line => /^  private submitted\w+:/.test(line)).join('\n');
assert.ok(submittedFields.includes('submittedRaw'), 'actual frozen submission fields');
function compile(text, filename) {
  const result = ts.transpileModule(text, { fileName: filename, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  return result.outputText;
}
const code = compile('export class ActualIndexTodosBusiness {\n' + submittedFields + '\n' + methods + '\n}', sourcePath);
function harness(options = {}) {
  const modules = new Map(), events = [], timers = new Map(); let timer = 0, operation = 0, page;
  function load(name) {
    if (modules.has(name)) return modules.get(name);
    const api = {}; modules.set(name, api); const filename = path.join(modelRoot, name + '.ets');
    vm.runInNewContext(compile(fs.readFileSync(filename, 'utf8'), filename), { exports: api,
      require: id => id === 'libmorrow.so' ? { default: {} } : load(id.replace(/^\.\//, '')),
      setTimeout: fn => { const id = ++timer; timers.set(id, fn); return id; }, clearTimeout: id => timers.delete(id), encodeURIComponent }, { filename });
    return api;
  }
  const draftApi = load('EditorDraft'), fieldApi = load('EditorFieldPolicy'), values = new draftApi.Values();
  const scope = Object.assign(new draftApi.DraftScope(), { card_id: 'card-create-fixture', draft_id: 'draft-create-fixture',
    source_kind: options.existing ? 0 : 1, source_revision: options.existing ? '7' : '0', source: options.existing ? 'original-v2-source' : '' });
  values.title.text = 'Original title'; values.description.text = 'Original body'; values.hypothesis.text = 'Hypothesis';
  values.conclusion.text = 'Conclusion'; values.todos.text = options.todos ?? ''; values.category = '实验'; values.stage = '待验证';
  values.assets = [Object.assign(new draftApi.AssetSelection(), { origin: 3, asset_id: 'confirmed-create-pin', aliases: ['original.png'] })];
  const stored = selection => Object.assign(new draftApi.StoredAsset(), { selection: plain(selection), pin_id: 'draft-asset-0',
    display_name: 'original.png', media_type: 'image/png', byte_length: '8', sha256: 'a'.repeat(64) });
  const initialRecord = Object.assign(new draftApi.DraftRecord(), { scope, values: draftApi.copyValues(values), generation: '1',
    current_generation: '1', active: true, current_active: true, operation_id: 'original-raw-intent', assets: values.assets.map(stored) });
  const draft = new draftApi.EditorDraftCoordinator(scope, values, async serialized => {
    const request = JSON.parse(serialized).draft; events.push({ kind: 'raw-save', request: plain(request), serialized });
    if (options.rawEffect) await options.rawEffect({ request, page, draft, events });
    if (options.rawUnknown) throw Error('synthetic raw journal Unknown');
    return Object.assign(new draftApi.DraftRecord(), { scope: Object.assign(new draftApi.DraftScope(), scope),
      values: Object.assign(new draftApi.Values(), request.values, { assets: plain(request.assets) }), assets: request.assets.map(stored),
      operation_id: request.operation_id, generation: draftApi.nextGeneration(request.expected_generation),
      current_generation: draftApi.nextGeneration(request.expected_generation), active: true, current_active: true });
  }, () => {}, () => 'raw-operation-' + (++operation), initialRecord);
  const ownedDrafts = [draft];
  const workbench = { async send(serialized) {
    const command = JSON.parse(serialized), retirement = command.action === 'draft_discard';
    events.push({ kind: retirement ? 'retirement' : 'business', command: plain(command), serialized });
    const count = events.filter(e => e.kind === (retirement ? 'retirement' : 'business')).length;
    if (retirement) {
      if (options.retireEffect) await options.retireEffect({ command, count, page, draft, events });
      if (options.retireUnknown) throw Error('synthetic discard Unknown');
      if (options.retireResult) return options.retireResult({ command, count, page, draft, events });
      const generation = draftApi.nextGeneration(command.generation);
      return { ok: true, drafts: [Object.assign(new draftApi.DraftRecord(), { scope: Object.assign(new draftApi.DraftScope(), scope),
        generation, current_generation: generation, active: false, current_active: false })] };
    }
    if (options.businessEffect) await options.businessEffect({ command, count, page, draft, events });
    if (options.businessResult) return options.businessResult({ command, count, page, draft, events });
    return success(command);
  } };
  function success(command) {
    const prior = page.cards.find(card => card.id === command.id);
    const card = { ...plain(prior), id: command.id, source: 'synthetic-receipt-' + command.operation, revision: options.existing ? '8' : '1',
      title: command.title, description: command.description, hypothesis: command.hypothesis, conclusion: command.conclusion,
      category: command.category, stage: command.stage, favorite: false, deleted: false,
      tasks: options.existing ? plain(prior.tasks) : [{ id: 'synthetic-backend-task', text: 'fixture task', completion: 0 }], assets: [] };
    return { ok: true, error: '', effect: 'committed', cards: [...plain(page.cards.filter(value => value.id !== card.id)), card],
      drafts: [], receipt_revision: options.existing ? '8' : '1' };
  }
  const context = vm.createContext({ exports: {}, workbench, util: { generateRandomUUID: () => 'business-operation-' + (++operation) },
    ...draftApi, ...fieldApi, ...load('EditorPaste'), ...load('EditorDirectInput'), ...load('EditorDraftFork'), DraftTextValue: draftApi.TextValue,
    Command: load('Workbench').Command });
  vm.runInContext(code, context, { filename: sourcePath }); page = new context.exports.ActualIndexTodosBusiness();
  const fieldPolicy = new fieldApi.EditorFieldPolicy(async serialized => {
    const request = JSON.parse(serialized); events.push({ kind: 'field-check', request: plain(request) });
    if (options.fieldEffect) await options.fieldEffect({ request, page, draft, events });
    if (options.fieldUnknown) throw Error('synthetic field worker Unknown');
    return JSON.stringify(fieldReply(request.field, request.text));
  });
  Object.assign(page, { ready: true, pageAlive: true, foreground: true, editorOpen: true, editorDraft: draft,
    editorValues: draftApi.copyValues(values), fieldPolicy, editorInputEpoch: 0, fieldCountEpochs: new Map(), fieldCountLabels: [],
    fieldValidationWorking: false, selected: options.existing ? scope.card_id : '', attachmentEditorIdentity: 'create-editor-owner',
    // Readiness is a controlled platform contract here; the actual coordinator
    // and native algorithms have separate tests. Business guards remain actual.
    directInput: { canConfirm: () => options.directReady !== false, capture: () => true, bind: () => {}, stop: () => {}, view: () => undefined, unbind: () => {}, retry: () => {} },
    inputPolicy: { stop: () => {} }, inputRevisions: new Map(), inputRulesRevision: 0, inputRulesMessage: '',
    todoBusinessReady: options.todosReady !== false, todoFormatPending: false, todoInputRevision: 0, todoInputValue: draftApi.copyText(values.todos), todoDragTimer: -1,
    attachmentWorking: false, pasteWorking: false, draftRestoreInput: false, busy: false, pending: '', draftWorking: false,
    draftRetiring: false, draftRetirement: '', draftRetirementUnknown: false, draftCaptureIncomplete: false, draftConflict: false,
    draftRetiredIdentity: '', retirementInput: new Map(),
    editorViewOwner: 'create-editor-owner', editorViewVisible: true, editorViewRevoked: false, editorBoundary: '',
    importRecords: [], pendingSpools: [], attachmentPending: '', rawForkRetirement: '',
    title: values.title.text, description: values.description.text, hypothesis: values.hypothesis.text, conclusion: values.conclusion.text,
    category: values.category, taskText: values.todos.text, taskEditId: '', taskRenameText: '', taskRenameValue: new draftApi.TextValue(),
    draftFocusedFields: new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']), draftSelectionPending: new Set(),
    draftCaptureBlocked: new Set(), dirty: true, message: '', draftSource: scope.source, draftStatus: '', draftRecords: [initialRecord],
    directInputWorking: false, todosWorking: false, todosPending: false, todosComplete: true,
    cards: options.existing ? [{ id: scope.card_id, source: scope.source, revision: scope.source_revision, title: values.title.text,
      description: values.description.text, hypothesis: values.hypothesis.text, conclusion: values.conclusion.text,
      category: values.category, stage: values.stage, favorite: false, deleted: false,
      tasks: [{ id: 'original-v2-task-id', text: 'Existing task', completion: 1 }], assets: [] }] : [],
    draftChanged: () => {}, refreshPreview: () => {}, loadDrafts: async () => {},
    attachDraft: () => { throw Error('unexpected draft handoff in create/edit test'); } });
  // Controlled framework lifecycle delivery; the freshly extracted methods
  // above still enforce all admission and cleanup conditions.
  const revokeView = page.revokeEditorView.bind(page), remountView = page.remountEditorView.bind(page);
  page.revokeEditorView = draft => { const result = revokeView(draft); page.editorLeaseRevoked(page.editorViewOwner); return result; };
  page.remountEditorView = draft => { const result = remountView(draft); page.editorLeaseMounted(page.editorViewOwner); return result; };
  function edit(value, field = 'todos') {
    const next = typeof value === 'string' ? Object.assign(new draftApi.TextValue(), { text: value }) : Object.assign(new draftApi.TextValue(), value);
    page.editorValues[field] = draftApi.copyText(next); page.editorInputEpoch++;
    const display = field === 'todos' ? 'taskText' : field; page[display] = next.text;
    draft.update(draftApi.copyValues(page.editorValues)); page.dirty = true;
  }
  function replaceEditor(text = page.editorValues.todos.text) {
    const replacementValues = draftApi.copyValues(page.editorValues); replacementValues.todos.text = text;
    const replacementScope = Object.assign(new draftApi.DraftScope(), { card_id: 'replacement-editor-card', draft_id: 'replacement-editor-draft',
      source_kind: 0, source_revision: '12', source: 'replacement-card-source' });
    const replacementRecord = Object.assign(new draftApi.DraftRecord(), { scope: replacementScope, values: replacementValues,
      generation: '1', current_generation: '1', active: true, current_active: true,
      operation_id: 'replacement-original-raw', assets: replacementValues.assets.map(stored) });
    const replacement = new draftApi.EditorDraftCoordinator(replacementScope, replacementValues,
      async () => { throw Error('old response must not save the replacement editor'); }, () => {}, () => 'replacement-new-raw', replacementRecord);
    ownedDrafts.push(replacement); page.editorDraft = replacement; page.editorValues = draftApi.copyValues(replacementValues);
    page.attachmentEditorIdentity = 'replacement-editor-owner'; page.editorInputEpoch++; page.selected = replacementScope.card_id;
    page.draftSource = replacementScope.source; page.taskText = text; page.editorOpen = true;
    page.cards.push({ id: replacementScope.card_id, revision: '12', source: replacementScope.source, title: replacementValues.title.text,
      description: replacementValues.description.text, hypothesis: replacementValues.hypothesis.text, conclusion: replacementValues.conclusion.text,
      category: replacementValues.category, stage: replacementValues.stage, tasks: [], assets: [], favorite: false, deleted: false });
    return replacement;
  }
  // Track actual business work even when save launches submit without awaiting
  // it. Gate release must settle these methods before a test passes/disposes.
  const running = new Set(), methodErrors = [];
  for (const name of ['save', 'submit', 'retry', 'flushDraft', 'retireDraft']) {
    const actual = page[name].bind(page);
    page[name] = (...args) => {
      const run = Promise.resolve(actual(...args)); running.add(run);
      run.then(() => running.delete(run), error => { running.delete(run); methodErrors.push(error); }); return run;
    };
  }
  async function finish() {
    let timeout;
    try {
      await Promise.race([new Promise((_, reject) => { timeout = setTimeout(() => reject(Error('actual Index methods did not finish after gate release')), 4000); }),
        (async () => { while (running.size) await Promise.all([...running]); await settle(); })()]);
      assert.deepEqual(methodErrors, [], 'actual Index methods returned no late platform/helper error');
    } finally { clearTimeout(timeout); }
  }
  return { page, draft, events, draftApi, success, edit, replaceEditor, finish,
    close: async () => { try { await finish(); } finally { for (const owned of ownedDrafts) owned.dispose(); } },
    business: () => events.filter(e => e.kind === 'business'), raw: () => events.filter(e => e.kind === 'raw-save'),
    retirements: () => events.filter(e => e.kind === 'retirement'), save: async () => {
      page.save().catch(() => {}); await settle();
      assert.deepEqual(methodErrors, [], 'actual save returned no unexpected platform/helper error');
    } };
}
test('actual Index create sends one immutable operation with complete raw todos and confirmed publication', async () => {
  const gate = deferred(), raw = '  e\u0301😀  \n\nsecond\r\n  second  \n', h = harness({ todos: raw, businessEffect: () => gate.promise });
  try {
    h.edit('  Exact title  ', 'title'); await h.save(); assert.equal(h.business().length, 1);
    const sent = h.business()[0], record = h.draft.confirmed;
    assert.equal(sent.command.action, 'create'); assert.equal(sent.command.todos, raw); assert.equal(sent.command.title, '  Exact title  ');
    assert.equal(sent.command.id, h.draft.scope.card_id); assert.equal(sent.command.draft_id, record.scope.draft_id);
    assert.equal(sent.command.generation, record.generation); assert.equal(sent.command.draft_operation, record.operation_id);
    assert.equal(h.raw().at(-1).request.values.todos.text, raw); assert.equal(h.raw().at(-1).request.assets[0].asset_id, 'confirmed-create-pin');
    assert.equal(h.page.pending, sent.serialized); assert.equal(h.page.submittedRaw.todos.text, raw);
    gate.resolve(); await h.finish(); assert.deepEqual(h.business().map(e => e.command.action), ['create']);
  } finally { gate.resolve(); await h.close(); }
});
test('new-card todos composition is preserved in confirmed raw but never admitted as create', async () => {
  const h = harness(); try {
    h.edit({ text: 'first\n候选😀', selection_base: 10, selection_extent: 6, affinity: 1, directional: true, composing_start: 6, composing_end: 10 });
    await h.save(); assert.equal(h.business().length, 0); assert.equal(h.retirements().length, 0); assert.equal(h.page.editorOpen, true);
    assert.deepEqual(plain(h.draft.confirmed.values.todos), plain(h.draft.current.todos));
    assert.equal(h.raw().at(-1).request.values.todos.composing_start, 6); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { await h.close(); }
});
test('complete grapheme overflow or 101 original blank rows stay raw without a partial create', async () => {
  for (const raw of ['😀'.repeat(1001), '\n'.repeat(100)]) {
    const h = harness({ todos: 'before' }); try {
      h.edit(raw); await h.save(); assert.equal(h.business().length, 0); assert.equal(h.retirements().length, 0);
      assert.equal(h.draft.current.todos.text, raw); assert.equal(h.draft.confirmed.values.todos.text, raw);
      assert.equal(h.raw().at(-1).request.values.todos.text, raw); assert.equal(h.page.pending, '');
    } finally { await h.close(); }
  }
});
test('unready direct-input or row formatting blocks business while exact raw remains retainable', async () => {
  for (const dependency of ['direct', 'rows']) {
    const options = { directReady: dependency !== 'direct', todosReady: dependency !== 'rows' }, h = harness(options);
    try {
      h.edit('pending\nfull raw'); await h.save(); assert.equal(h.business().length, 0, dependency);
      assert.equal(h.page.pending, '', dependency); assert.equal(h.retirements().length, 0, dependency);
      assert.equal(h.draft.confirmed.values.todos.text, 'pending\nfull raw', dependency);
      assert.equal(h.draft.confirmed.assets.length, 1, dependency); assert.equal(h.raw().length, 1, dependency);
      options.directReady = true; h.page.todoBusinessReady = true; await h.save(); await h.finish();
      assert.equal(h.business().length, 1, dependency); assert.equal(h.business()[0].command.todos, 'pending\nfull raw', dependency);
      assert.equal(h.raw().length, 1, dependency);
    } finally { await h.close(); }
  }
});
test('backend known rejection preserves full todos/pins and requires an explicit new save', async () => {
  const raw = '  first  \n\nsecond  ', h = harness({ todos: raw,
    businessResult: () => ({ ok: false, error: 'TaskTextByteLimit', effect: 'not_committed', cards: [], drafts: [] }) });
  try {
    await h.save(); assert.equal(h.business().length, 1); assert.equal(h.page.pending, ''); assert.equal(h.page.submittedRaw, undefined);
    assert.equal(h.page.editorOpen, true); assert.equal(h.draft.current.todos.text, raw); assert.equal(h.draft.confirmed.assets.length, 1);
    assert.equal(h.retirements().length, 0); await settle(); assert.equal(h.business().length, 1);
    h.edit('corrected'); await h.save(); assert.equal(h.business().length, 2);
    assert.notEqual(h.business()[1].command.operation, h.business()[0].command.operation);
    assert.equal(h.business()[1].command.todos, 'corrected');
  } finally { await h.close(); }
});
test('Unknown create retains exact wire and explicit retry does not borrow later todos', async () => {
  let h; h = harness({ todos: '  original\nraw  ', businessResult: ({ command, count }) => {
    if (count === 1) throw Error('synthetic lost reply'); return h.success(command);
  } });
  try {
    await h.save(); const original = h.business()[0].serialized;
    assert.equal(h.page.pending, original); assert.equal(h.page.submittedRaw.todos.text, '  original\nraw  ');
    h.edit('later\nnew row'); await settle(); assert.equal(h.business().length, 1);
    await h.page.retry(); await h.finish(); assert.equal(h.business().length, 2);
    assert.equal(h.business()[1].serialized, original); assert.equal(h.page.pending, '');
    assert.equal(h.draft.current.todos.text, 'later\nnew row'); assert.equal(h.page.editorOpen, true);
    assert.equal(h.retirements().length, 0); assert.equal(h.draft.disposed, false);
  } finally { await h.close(); }
});
test('raw journal Unknown reconciles only its original todos; later rows need a fresh confirmed publication', async () => {
  const options = { rawUnknown: true }, h = harness(options); try {
    h.edit('first\nuncertain'); await h.save(); assert.equal(h.business().length, 0); assert.equal(h.draft.unknown, true);
    const original = h.raw()[0].serialized; assert.equal(h.draft.pending, original);
    h.edit('later\nraw'); options.rawUnknown = false; await h.draft.retry();
    assert.equal(h.raw().length, 2); assert.equal(h.raw()[1].serialized, original); assert.equal(h.draft.unknown, false);
    assert.equal(h.draft.confirmed.values.todos.text, 'first\nuncertain'); assert.equal(h.draft.current.todos.text, 'later\nraw');
    assert.equal(h.draft.dirty, true); assert.equal(h.business().length, 0); assert.equal(h.draft.confirmed.assets.length, 1);
    await h.save(); await h.finish(); assert.equal(h.raw().length, 3); assert.equal(h.raw()[2].request.values.todos.text, 'later\nraw');
    assert.equal(h.business().length, 1); assert.equal(h.business()[0].command.todos, 'later\nraw');
    assert.equal(h.business()[0].command.generation, h.draft.confirmed.generation);
    assert.equal(h.business()[0].command.draft_operation, h.raw()[2].request.operation_id);
  } finally { await h.close(); }
});
test('structured unconfirmed or historical committed error preserves the original create wire', async () => {
  for (const effect of ['unknown', 'committed']) {
    const h = harness({ todos: 'first\nsecond', businessResult: () => ({ ok: false, error: 'OriginalOutcomeNeedsReconciliation', effect, cards: [], drafts: [] }) });
    try {
      await h.save(); const original = h.business()[0].serialized; assert.equal(h.page.pending, original, effect);
      assert.equal(h.page.submittedRaw.todos.text, 'first\nsecond', effect); assert.equal(h.retirements().length, 0, effect);
      await settle(); assert.equal(h.business().length, 1, effect); await h.page.retry();
      assert.equal(h.business()[1].serialized, original, effect); assert.equal(h.page.pending, original, effect);
    } finally { await h.close(); }
  }
});
test('late todos text/range/composition/edit-away-back revokes create input consumption', async () => {
  for (const mutation of ['text', 'selection', 'composition', 'away-back']) {
    const gate = deferred(), h = harness({ todos: 'original\nraw', businessEffect: () => gate.promise });
    try {
      await h.save(); assert.equal(h.business().length, 1, mutation);
      if (mutation === 'text') h.edit('later\nraw');
      if (mutation === 'selection') h.edit({ ...plain(h.draft.current.todos), selection_base: 8, selection_extent: 2, affinity: 1, directional: true });
      if (mutation === 'composition') h.edit({ ...plain(h.draft.current.todos), composing_start: 0, composing_end: 8 });
      if (mutation === 'away-back') { h.edit('away'); h.edit('original\nraw'); }
      const later = plain(h.draft.current.todos); gate.resolve(); await h.finish();
      assert.deepEqual(plain(h.draft.current.todos), later, mutation); assert.equal(h.retirements().length, 0, mutation);
      assert.equal(h.page.editorOpen, true, mutation); assert.equal(h.draft.disposed, false, mutation);
      assert.equal(h.business()[0].command.todos, 'original\nraw', mutation);
    } finally { gate.resolve(); await h.close(); }
  }
});
test('a different editor owner with identical raw values cannot be consumed by an old create receipt', async () => {
  const gate = deferred(), h = harness({ todos: 'same\nraw', businessEffect: () => gate.promise });
  try {
    await h.save(); const replacement = h.replaceEditor();
    gate.resolve(); await h.finish(); assert.equal(h.retirements().length, 0);
    assert.equal(h.page.editorOpen, true); assert.equal(h.page.editorDraft, replacement); assert.equal(replacement.disposed, false);
    assert.equal(replacement.current.todos.text, 'same\nraw'); assert.equal(h.page.selected, replacement.scope.card_id);
  } finally { gate.resolve(); await h.close(); }
});
test('successful exact create retires its full raw generation without emptying/normalizing it first', async () => {
  const raw = '  first\n\nfirst  \n second  ', h = harness({ todos: 'before' }); try {
    h.edit(raw); await h.save(); assert.equal(h.business().length, 1); assert.equal(h.retirements().length, 1);
    const create = h.business()[0].command, discard = h.retirements()[0].command;
    assert.equal(discard.id, create.id); assert.equal(discard.draft_id, create.draft_id); assert.equal(discard.generation, create.generation);
    assert.deepEqual(h.raw().map(e => e.request.values.todos.text), [raw]);
    assert.equal(h.draft.confirmed.values.todos.text, raw); assert.equal(h.draft.disposed, true);
    assert.equal(h.page.editorDraft, undefined); assert.equal(h.page.editorOpen, false); assert.equal(h.page.pending, '');
  } finally { await h.close(); }
});
test('late input/range/roundtrip/actual SDK callback or owner change during discard never closes the live editor', async () => {
  for (const mutation of ['input', 'selection', 'away-back', 'callback', 'owner']) {
    const gate = deferred(), h = harness({ todos: 'published\nraw', retireEffect: () => gate.promise });
    try {
      await h.save(); assert.equal(h.retirements().length, 1, mutation);
      const replacement = mutation === 'owner' ? h.replaceEditor('replacement\nretained') : undefined;
      if (mutation === 'input') h.edit('late\nretained');
      if (mutation === 'selection') h.edit({ ...plain(h.draft.current.todos), selection_base: 9, selection_extent: 2, affinity: 1, directional: true });
      if (mutation === 'away-back') { h.edit('away'); h.edit('published\nraw'); }
      if (mutation === 'callback') {
        const epoch = h.page.editorInputEpoch, before = plain(h.draft.current), preview = { value: '候选😀', offset: 2 };
        h.page.editorInputChanged('title', '晚到SDK原文', preview);
        assert.ok(h.page.editorInputEpoch > epoch); assert.equal(h.page.draftCaptureIncomplete, true);
        assert.deepEqual(JSON.parse(h.page.retirementInput.get('title:text')), { value: '晚到SDK原文', preview });
        assert.equal(h.page.editorValues.title.text, '晚到候选😀SDK原文');
        assert.equal(h.page.editorValues.title.composing_start, 2); assert.equal(h.page.editorValues.title.composing_end, 6);
        assert.deepEqual(plain(h.draft.current), before, 'retiring callback cannot update the old journal');
      }
      const expected = plain(h.page.editorDraft.current.todos);
      gate.resolve(); await h.finish(); assert.equal(h.page.editorOpen, true, mutation);
      assert.ok(h.page.editorDraft && !h.page.editorDraft.disposed, mutation);
      if (replacement) assert.equal(h.page.editorDraft, replacement, mutation);
      assert.equal(h.retirements().length, 1, mutation); assert.deepEqual(plain(h.page.editorDraft.current.todos), expected, mutation);
      if (mutation !== 'owner') {
        assert.equal(h.page.draftRetiredIdentity, h.draft.scope.draft_id, mutation); const rawCount = h.raw().length;
        assert.equal(await h.page.flushDraft(), false, mutation); assert.equal(await h.page.retireDraft(), false, mutation);
        assert.equal(h.raw().length, rawCount, mutation); assert.equal(h.retirements().length, 1, mutation);
        assert.equal(h.page.inputReadyFor('create'), false, mutation); assert.equal(h.page.editorOpen, true, mutation);
      }
    } finally { gate.resolve(); await h.close(); }
  }
});
test('invalid discard receipt retains its original retirement request and raw editor', async () => {
  const h = harness({ todos: 'published\nraw', retireResult: ({ command }) => ({ ok: true, drafts: [
    { scope: { card_id: command.id, draft_id: 'different-draft' }, generation: '99', current_generation: '99', active: false, current_active: false }
  ] }) });
  try {
    await h.save(); assert.equal(h.retirements().length, 1); assert.equal(h.page.draftRetirement, h.retirements()[0].serialized);
    assert.equal(h.page.draftRetirementUnknown, true); assert.equal(h.page.editorOpen, true); assert.equal(h.draft.disposed, false);
    assert.equal(h.draft.current.todos.text, 'published\nraw'); const original = plain(h.draft.current), epoch = h.page.editorInputEpoch;
    h.page.editorInputChanged('description', '清理Unknown完整SDK输入', { value: '候选', offset: 2 });
    assert.ok(h.page.editorInputEpoch > epoch); assert.equal(h.page.draftCaptureIncomplete, true);
    assert.equal(JSON.parse(h.page.retirementInput.get('description:text')).value, '清理Unknown完整SDK输入');
    assert.deepEqual(plain(h.draft.current), original); assert.equal(await h.page.flushDraft(), false);
    assert.equal(h.page.draftRetirement, h.retirements()[0].serialized); await settle(); assert.equal(h.retirements().length, 1);
  } finally { await h.close(); }
});
test('existing V2 edit leaves single pending task input separate and sends no legacy todos or task command', async () => {
  const raw = 'pending single task', h = harness({ existing: true, todos: raw }); try {
    h.edit('edited existing body', 'description'); await h.save(); assert.equal(h.business().length, 1);
    const command = h.business()[0].command; assert.equal(command.action, 'edit'); assert.equal(command.todos, '');
    assert.equal(command.source, 'original-v2-source'); assert.equal(h.draft.current.todos.text, raw);
    assert.equal(h.page.cards[0].tasks[0].id, 'original-v2-task-id'); assert.equal(h.retirements().length, 0);
    assert.equal(h.page.editorOpen, true); assert.equal(h.draft.disposed, false);
  } finally { await h.close(); }
});
test('actual create admission rejects a command borrowing different raw todos', async () => {
  const h = harness({ todos: 'authoritative\nraw' }); try {
    const command = h.page.command('create'), raw = h.draft.current;
    Object.assign(command, { title: raw.title.text, description: raw.description.text, hypothesis: raw.hypothesis.text,
      conclusion: raw.conclusion.text, category: raw.category, stage: raw.stage, todos: 'different\nraw' });
    await h.page.submit(command, raw); await settle(); assert.equal(h.business().length, 0); assert.equal(h.page.pending, '');
    assert.equal(h.draft.current.todos.text, 'authoritative\nraw'); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { await h.close(); }
});
test('emit fresh source identity for the actual Index business boundary', () => {
  console.log('Index.ets SHA256=' + crypto.createHash('sha256').update(source).digest('hex') + '; SDK TypeScript=' + ts.version);
});
