'use strict';
// Execute fresh, verbatim Index methods and actual ETS policy/draft models.
// The worker and business service replies are synthetic. This is integration
// evidence, not native Unicode segmentation, ArkUI IME or device acceptance.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto'), assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const { response: fieldReply, deferred } = require('./editor-field-test-harness.cjs');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets');
const source = fs.readFileSync(sourcePath, 'utf8'), root = path.resolve(__dirname, '../entry/src/main/ets/model');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 40; i++) await Promise.resolve(); };
function section(from, to) {
  const start = source.indexOf(from), end = source.indexOf(to, start + from.length);
  assert.ok(start >= 0 && end > start, 'fresh Index method anchors: ' + from);
  assert.equal(source.indexOf(from, start + 1), -1, 'unique Index method anchor');
  return source.slice(start, end);
}
const methods = [
  section('  private draftField(', '  private updateMarkdown('),
  section('  private draftTextChanged(', '  private restoreSelection('),
  section('  private editorInputChanged(', '  private select('),
  section('  private async flushDraft(', '  private refreshEditorAssets('),
  section('  private command(', '  private moveTask('),
  section('  private foregroundChanged():', '  private async switchMediaFullscreen('),
  section('  private current():', '  private visibleCards('),
  section('  private async keepDraftAndClose():', '  private async retireDraft('),
  section('  private closeEditor():', '  private favoriteCard(')
].join('\n');
// These new submission fields use their actual declarations/initializers.
const submittedRenameFields = ['submittedRename', 'submittedRenameEpoch', 'submittedRenameEditor'].map(name => {
  const line = source.split(/\r?\n/).find(value => value.startsWith('  private ' + name + ':'));
  assert.ok(line, 'actual submitted rename field: ' + name); return line;
}).join('\n');
const cancelRenameAnchor = '.onClick(() => { this.taskEditId = \'\'; this.taskRenameText = \'\'; })';
assert.ok(source.includes(cancelRenameAnchor), 'actual rename cancel callback');
const cancelRenameMethod = 'cancelRenameFromActualBuilder(): void { ' + cancelRenameAnchor.slice(cancelRenameAnchor.indexOf('{') + 1, cancelRenameAnchor.lastIndexOf('}')) + ' }';
function compile(text, filename) {
  const result = ts.transpileModule(text, { fileName: filename, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  return result.outputText;
}
const code = compile('export class ActualIndexFields {\n' + submittedRenameFields + '\n' + methods + '\n' + cancelRenameMethod + '\n}', sourcePath);
function harness(options = {}) {
  const modules = new Map(), events = [], timers = new Map(); let timer = 0, operation = 0, page;
  function load(name) {
    if (modules.has(name)) return modules.get(name);
    const api = {}; modules.set(name, api); const filename = path.join(root, name + '.ets');
    vm.runInNewContext(compile(fs.readFileSync(filename, 'utf8'), filename), { exports: api,
      require: id => id === 'libmorrow.so' ? { default: {} } : load(id.replace(/^\.\//, '')),
      setTimeout: fn => { const id = ++timer; timers.set(id, fn); return id; }, clearTimeout: id => timers.delete(id), encodeURIComponent }, { filename });
    return api;
  }
  const draftApi = load('EditorDraft'), policy = load('EditorFieldPolicy');
  const values = new draftApi.Values(), scope = new draftApi.DraftScope();
  scope.card_id = 'card-field-fixture'; scope.draft_id = 'draft-field-fixture'; scope.source_kind = 0; scope.source_revision = '1';
  values.title.text = 'Title'; values.description.text = 'Body'; values.hypothesis.text = 'Hypothesis';
  values.conclusion.text = 'Conclusion'; values.todos.text = options.todos || ''; values.category = '实验'; values.stage = '待验证';
  values.assets = [Object.assign(new draftApi.AssetSelection(), { origin: 3, asset_id: 'confirmed-pin-original', aliases: ['original.png'] })];
  const stored = selected => Object.assign(new draftApi.StoredAsset(), { selection: selected, pin_id: 'draft-asset-0',
    display_name: 'original.png', media_type: 'image/png', byte_length: '8', sha256: 'a'.repeat(64) });
  const record = Object.assign(new draftApi.DraftRecord(), { scope, values: draftApi.copyValues(values), generation: '1',
    current_generation: '1', active: true, current_active: true, operation_id: 'restored-fixture', assets: values.assets.map(stored) });
  const draft = new draftApi.EditorDraftCoordinator(scope, values, async serialized => {
    const request = JSON.parse(serialized).draft; events.push({ kind: 'draft-save', request: plain(request) });
    if (options.draftEffect) await options.draftEffect({ request, page, draft, events });
    if (options.draftUnknown) throw Error('synthetic uncertain raw journal');
    return Object.assign(new draftApi.DraftRecord(), { scope: Object.assign(new draftApi.DraftScope(), scope),
      values: Object.assign(new draftApi.Values(), request.values, { assets: plain(request.assets) }),
      assets: request.assets.map(stored), operation_id: request.operation_id,
      generation: draftApi.nextGeneration(request.expected_generation), current_generation: draftApi.nextGeneration(request.expected_generation),
      active: true, current_active: true });
  }, () => {}, () => 'raw-operation-' + (++operation), record);
  const workbench = { async send(serialized) {
    const command = JSON.parse(serialized); events.push({ kind: 'business-send', command, serialized });
    if (options.businessEffect) await options.businessEffect({ command, page, draft, events });
    if (options.businessUnknown) throw Error('synthetic uncertain business reply');
    if (options.businessResult) return options.businessResult;
    return { ok: true, error: '', cards: plain(page.cards), receipt_revision: '2' };
  } };
  const context = vm.createContext({ exports: {}, workbench, util: { generateRandomUUID: () => 'operation-' + (++operation) },
    ...draftApi, ...policy, ...load('EditorPaste'), DraftTextValue: draftApi.TextValue, Command: load('Workbench').Command });
  vm.runInContext(code, context, { filename: sourcePath }); page = new context.exports.ActualIndexFields();
  const fieldPolicy = new policy.EditorFieldPolicy(async serialized => {
    const request = JSON.parse(serialized); events.push({ kind: 'field-check', request });
    if (options.fieldEffect) await options.fieldEffect({ request, page, draft, events });
    if (options.fieldUnknown) throw Error('synthetic worker failure');
    return JSON.stringify(fieldReply(request.field, request.text));
  });
  Object.assign(page, { ready: true, pageAlive: true, foreground: true, editorOpen: true, editorDraft: draft,
    editorValues: draftApi.copyValues(values), fieldPolicy, editorInputEpoch: 0, fieldCountEpochs: new Map(),
    fieldCountLabels: [], fieldValidationWorking: false, selected: scope.card_id, attachmentEditorIdentity: 'editor-field-owner',
    attachmentWorking: false, pasteWorking: false, draftRestoreInput: false, busy: false, pending: '', submittedRaw: undefined,
    draftWorking: false, draftRetiring: false, draftRetirementUnknown: false, draftCaptureIncomplete: false, draftConflict: false,
    title: values.title.text, description: values.description.text, hypothesis: values.hypothesis.text, conclusion: values.conclusion.text,
    category: values.category, taskText: values.todos.text, taskEditId: '', taskRenameText: '', taskRenameValue: new draftApi.TextValue(),
    draftFocusedFields: new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']), draftSelectionPending: new Set(),
    draftCaptureBlocked: new Set(), dirty: false, message: '', draftSource: 'frozen-card-source',
    cards: [{ id: scope.card_id, source: 'frozen-card-source', revision: '1', title: values.title.text, description: values.description.text,
      hypothesis: values.hypothesis.text, conclusion: values.conclusion.text, category: values.category, stage: values.stage,
      favorite: false, deleted: false, tasks: [{ id: 'task-fixture', text: 'Task', completion: 0 }], assets: [] }],
    draftChanged: () => {}, refreshPreview: () => {}, loadDrafts: async () => {}, retireDraft: async () => { events.push({ kind: 'retire-attempt' }); return false; },
    mediaPlayback: { pauseForBackground: () => events.push({ kind: 'media-pause' }) }, exitMediaFullscreen: () => {}, fileOpen: undefined });
  return { page, draft, events, draftApi, close: () => draft.dispose(),
    business: () => events.filter(e => e.kind === 'business-send'), raw: () => events.filter(e => e.kind === 'draft-save') };
}
function editCommand(h) {
  const cmd = h.page.command('edit'), values = h.draft.current;
  cmd.title = values.title.text; cmd.description = values.description.text; cmd.hypothesis = values.hypothesis.text;
  cmd.conclusion = values.conclusion.text; cmd.category = values.category; cmd.stage = values.stage; return cmd;
}
test('fresh actual Index methods use installed SDK compiler; emit source hash', () => {
  assert.ok(methods.includes('JSON.stringify(cmd) === original') && methods.includes('draft.flush()'));
  console.log('Index.ets SHA256=' + crypto.createHash('sha256').update(source).digest('hex') + '; SDK TypeScript=' + ts.version);
});
test('all five fields retain original graphemes and UTF16 range in actual raw journal', async () => {
  const h = harness(); try {
    const text = '  e\u0301👩‍👩‍👧‍👦🇨🇳\r\n  ';
    for (const name of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
      h.page.editorInputChanged(name, text); h.page.draftSelectionChanged(name, text.length, 3);
    }
    assert.equal(await h.page.flushDraft(), true);
    for (const name of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
      const value = h.raw().at(-1).request.values[name]; assert.equal(value.text, text);
      assert.equal(value.selection_base, text.length); assert.equal(value.selection_extent, 3);
    }
    assert.equal(h.raw().at(-1).request.assets[0].asset_id, 'confirmed-pin-original');
  } finally { h.close(); }
});
test('IME candidate is journaled losslessly in all fields while business admission is blocked', async () => {
  for (const name of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
    const h = harness(); try {
      h.page.editorInputChanged(name, 'A😀B', { value: '候选e\u0301', offset: 3 });
      assert.equal(await h.page.flushDraft(), true); const raw = h.raw().at(-1).request.values[name];
      assert.equal(raw.text, 'A😀候选e\u0301B'); assert.equal(raw.composing_start, 3); assert.equal(raw.composing_end, 7);
      if (name === 'todos') h.page.mutate('task_add'); else await h.page.submit(editCommand(h), h.draft.current);
      await settle(); assert.equal(h.business().length, 0); assert.equal(h.draft.current[name].text, raw.text);
    } finally { h.close(); }
  }
});
test('invalid IME offset blocks retention and leaves previous confirmed raw values intact', async () => {
  const h = harness(); try {
    h.page.editorInputChanged('description', 'new', { value: '候选', offset: 9 });
    assert.equal(h.page.draftCaptureIncomplete, true); assert.equal(await h.page.flushDraft(), false);
    await h.page.submit(editCommand(h), h.draft.current); assert.equal(h.business().length, 0);
    assert.equal(h.draft.current.description.text, 'Body'); assert.equal(h.draft.confirmed.assets[0].selection.asset_id, 'confirmed-pin-original');
  } finally { h.close(); }
});
test('older count cannot replace a newer edit or a newer UTF16 selection count', async () => {
  const gates = [], h = harness({ fieldEffect: () => { const gate = deferred(); gates.push(gate); return gate.promise; } });
  try {
    h.page.editorInputChanged('title', 'old'); h.page.editorInputChanged('title', 'e\u0301😀');
    await settle(); assert.equal(gates.length, 2); gates[1].resolve(); await settle(); assert.equal(h.page.fieldCountLabel('title'), '2 / 60');
    gates[0].resolve(); await settle(); assert.equal(h.page.fieldCountLabel('title'), '2 / 60');
    h.page.draftSelectionChanged('title', 4, 2); await settle(); gates[2].resolve(); await settle();
    assert.equal(h.draft.current.title.selection_base, 4); assert.equal(h.draft.current.title.selection_extent, 2);
  } finally { h.close(); }
});
test('business validation revokes edits, selection, category, owner, command and foreground changes', async () => {
  for (const change of ['text', 'selection', 'category', 'owner', 'command', 'foreground', 'closed', 'disposed']) {
    const gate = deferred(), h = harness({ fieldEffect: ({ request }) => request.field === 'title' ? gate.promise : undefined });
    try {
      const cmd = editCommand(h), run = h.page.submit(cmd, h.draft.current); await settle();
      assert.equal(h.page.fieldValidationWorking, true);
      if (change === 'text') h.page.editorInputChanged('description', 'later');
      if (change === 'selection') h.page.draftSelectionChanged('title', 1, 4);
      if (change === 'category') { const values = h.draft.current; values.category = '灵感'; h.draft.update(values); }
      if (change === 'owner') h.page.attachmentEditorIdentity = 'other-owner';
      if (change === 'command') cmd.category = '进行中';
      if (change === 'foreground') { h.page.foreground = false; h.page.foregroundChanged(); }
      if (change === 'closed') { h.page.closeEditor(); await settle(); assert.equal(h.page.editorOpen, false); }
      if (change === 'disposed') h.draft.dispose();
      gate.resolve(); await run; assert.equal(h.business().length, 0, change); assert.equal(h.page.pending, '', change);
      assert.equal(h.page.fieldValidationWorking, false, change);
    } finally { h.close(); }
  }
});
test('later input during final raw flush prevents issuing a frozen business command', async () => {
  const gate = deferred(), h = harness({ draftEffect: () => gate.promise }); try {
    h.page.editorInputChanged('description', 'first'); const run = h.page.submit(editCommand(h), h.draft.current);
    await settle(); assert.equal(h.raw().length, 1); h.page.editorInputChanged('description', 'later');
    gate.resolve(); await run; assert.equal(h.business().length, 0); assert.equal(h.draft.current.description.text, 'later');
    assert.equal(h.draft.current.assets[0].asset_id, 'confirmed-pin-original');
  } finally { h.close(); }
});
test('over-limit title keeps every character and confirmed pins without business mutation', async () => {
  const h = harness(); try {
    const text = '👩‍👩‍👧‍👦'.repeat(61); h.page.editorInputChanged('title', text);
    await h.page.submit(editCommand(h), h.draft.current); assert.equal(h.business().length, 0);
    assert.equal(h.draft.current.title.text, text); assert.equal(h.draft.confirmed.assets.length, 1);
    assert.equal(await h.page.flushDraft(), true); assert.equal(h.raw().at(-1).request.values.title.text, text);
  } finally { h.close(); }
});
test('count worker failure never authorizes business or normalizes original input', async () => {
  const h = harness({ fieldUnknown: true }); try {
    h.page.editorInputChanged('description', '  e\u0301\n'); await h.page.submit(editCommand(h), h.draft.current);
    assert.equal(h.business().length, 0); assert.equal(h.draft.current.description.text, '  e\u0301\n');
    assert.equal(await h.page.flushDraft(), true); assert.equal(h.raw().at(-1).request.values.description.text, '  e\u0301\n');
  } finally { h.close(); }
});
test('unknown raw journal preserves exact pending request and confirmed pins, no business issue', async () => {
  const h = harness({ draftUnknown: true }); try {
    h.page.editorInputChanged('description', 'raw unconfirmed'); await h.page.submit(editCommand(h), h.draft.current);
    assert.equal(h.business().length, 0); assert.equal(h.draft.unknown, true);
    assert.equal(JSON.parse(h.draft.pending).draft.values.description.text, 'raw unconfirmed');
    assert.equal(h.draft.confirmed.values.description.text, 'Body'); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { h.close(); }
});
test('unknown business reply retains original serialized command and raw snapshot with confirmed pins', async () => {
  const h = harness({ businessUnknown: true }); try {
    h.page.editorInputChanged('title', ' e\u0301 '); await h.page.submit(editCommand(h), h.draft.current);
    assert.equal(h.business().length, 1); assert.equal(h.page.pending, h.business()[0].serialized);
    assert.equal(JSON.parse(h.page.pending).title, ' e\u0301 '); assert.equal(h.page.submittedRaw.assets[0].asset_id, 'confirmed-pin-original');
    assert.equal(h.draft.confirmed.values.title.text, ' e\u0301 '); assert.equal(h.draft.confirmed.assets.length, 1);
  } finally { h.close(); }
});
test('create/edit business send preserves exact source strings and binds confirmed generation', async () => {
  const h = harness({ todos: 'pending todo remains' }); try {
    h.page.editorInputChanged('title', ' e\u0301😀 '); h.page.editorInputChanged('description', '👩‍👩‍👧‍👦\r\n');
    await h.page.submit(editCommand(h), h.draft.current); assert.equal(h.business().length, 1);
    const cmd = h.business()[0].command; assert.equal(cmd.title, ' e\u0301😀 '); assert.equal(cmd.description, '👩‍👩‍👧‍👦\r\n');
    assert.equal(cmd.draft_id, h.draft.scope.draft_id); assert.equal(cmd.generation, h.draft.confirmed.generation);
    assert.equal(cmd.draft_operation, h.draft.confirmed.operation_id); assert.equal(h.draft.current.todos.text, 'pending todo remains');
    assert.equal(h.draft.confirmed.assets.length, 1); assert.equal(h.events.some(e => e.kind === 'retire-attempt'), false);
  } finally { h.close(); }
});
test('task_add validates full raw text and does not consume later text or IME', async () => {
  const gate = deferred(), h = harness({ todos: 'Task😀', businessEffect: () => gate.promise }); try {
    h.page.mutate('task_add'); await settle(); assert.equal(h.business().length, 1);
    h.page.editorInputChanged('todos', 'Task😀', { value: '候选', offset: 6 }); gate.resolve(); await settle();
    assert.equal(h.business()[0].command.text, 'Task😀'); assert.equal(h.draft.current.todos.text, 'Task😀候选');
    assert.equal(h.draft.current.todos.composing_start, 6); assert.equal(h.page.taskText, 'Task😀');
  } finally { h.close(); }
});
test('task rename validates its untrimmed original while command retains established trimming', async () => {
  const h = harness({ todos: 'independent pending todo' }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged(' e\u0301😀 '); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); assert.equal(h.business()[0].command.text, 'e\u0301😀');
    assert.ok(h.events.some(e => e.kind === 'field-check' && e.request.field === 'todos' && e.request.text === ' e\u0301😀 '));
    assert.equal(h.draft.current.todos.text, 'independent pending todo');
    assert.equal(h.page.taskEditId, ''); assert.equal(h.page.taskRenameText, ''); assert.equal(h.page.submittedRename, undefined);
  } finally { h.close(); }
});
test('same-value rename roundtrip after issue does not consume newer input epoch', async () => {
  const gate = deferred(), h = harness({ todos: 'independent todo', businessEffect: () => gate.promise }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Task'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1);
    h.page.taskRenameChanged('Later'); h.page.taskRenameChanged('Task'); gate.resolve(); await settle();
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, 'Task');
    assert.equal(h.page.taskRenameText, 'Task'); assert.equal(h.page.pending, '');
  } finally { h.close(); }
});
test('actual cancel callback invalidates pending validation and later reopen is not consumed', async () => {
  const gate = deferred(), h = harness({ todos: 'independent todo', fieldEffect: ({ request }) => request.field === 'todos' ? gate.promise : undefined }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('New name'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.page.fieldValidationWorking, true);
    h.page.cancelRenameFromActualBuilder(); h.page.renameTask(h.page.current().tasks[0]);
    assert.equal(h.page.taskEditId, '', 'busy validation rejects reopen until the prior check is revoked');
    gate.resolve(); await settle(); assert.equal(h.business().length, 0);
    h.page.renameTask(h.page.current().tasks[0]); await settle();
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameText, 'Task');
  } finally { h.close(); }
});
test('unknown rename preserves original request and frozen raw identity through explicit reconciliation', async () => {
  const options = { todos: 'independent todo', businessUnknown: true }, h = harness(options); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged(' Task '); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); const original = h.page.pending, snapshot = plain(h.page.submittedRename);
    const epoch = h.page.submittedRenameEpoch; assert.equal(snapshot.text, ' Task '); assert.equal(JSON.parse(original).text, 'Task');
    h.page.taskRenameChanged(' Task ', { value: '候选', offset: 6 });
    assert.equal(h.page.pending, original); assert.deepEqual(plain(h.page.submittedRename), snapshot); assert.equal(h.page.submittedRenameEpoch, epoch);
    options.businessUnknown = false; await h.page.retry();
    assert.equal(h.business().length, 2); assert.equal(h.business()[1].serialized, original);
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, ' Task 候选');
    assert.equal(h.draft.current.assets[0].asset_id, 'confirmed-pin-original');
  } finally { h.close(); }
});
test('confirmed not-committed rename clears only frozen request, preserving user text', async () => {
  const h = harness({ todos: 'independent todo', businessResult: { ok: false, effect: 'not_committed', error: 'synthetic rejection' } }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Original rename'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); assert.equal(h.page.pending, ''); assert.equal(h.page.submittedRename, undefined);
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, 'Original rename');
    assert.equal(h.page.taskRenameText, 'Original rename'); assert.equal(h.draft.current.assets[0].asset_id, 'confirmed-pin-original');
  } finally { h.close(); }
});
test('foreground loss after issued rename revokes successful input consumption', async () => {
  const gate = deferred(), h = harness({ todos: 'independent todo', businessEffect: () => gate.promise }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Task'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); h.page.foreground = false; h.page.foregroundChanged();
    gate.resolve(); await settle(); assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameText, 'Task');
  } finally { h.close(); }
});
test('late IME rename callback during issued business request retains newer candidate', async () => {
  const gate = deferred(), h = harness({ todos: 'independent pending todo', businessEffect: () => gate.promise }); try {
    h.page.renameTask(h.page.current().tasks[0]); h.page.taskRenameChanged('Task'); h.page.mutate('task_rename', h.page.current().tasks[0]);
    await settle(); assert.equal(h.business().length, 1); assert.equal(h.page.busy, true);
    h.page.taskRenameChanged('Task', { value: '候选', offset: 4 }); gate.resolve(); await settle();
    assert.equal(h.page.taskEditId, 'task-fixture'); assert.equal(h.page.taskRenameValue.text, 'Task候选');
    assert.equal(h.page.taskRenameValue.composing_start, 4); assert.equal(h.page.taskRenameText, 'Task');
  } finally { h.close(); }
});
