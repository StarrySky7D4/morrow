'use strict';
// Executes verbatim, freshly extracted Index methods with the actual ETS
// ClipboardPaste, EditorDraft, ClipboardInput and AttachmentFiles models.
// Synthetic SDK/FS/native replies qualify integration only, never a device.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto'), assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const { createClipboardHarness, createFilesHarness, sha, deferred } = require('./clipboard-test-harness.cjs');
const { response: fieldReply } = require('./editor-field-test-harness.cjs');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets');
const source = fs.readFileSync(sourcePath, 'utf8'), modelRoot = path.resolve(__dirname, '../entry/src/main/ets/model');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const settle = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
function section(from, to) {
  const begin = source.indexOf(from), end = source.indexOf(to, begin + from.length);
  assert.ok(begin >= 0 && end > begin, 'actual Index method anchors ' + from);
  assert.equal(source.indexOf(from, begin + 1), -1, 'one actual method anchor');
  return source.slice(begin, end);
}
const methods = [
  section('  private businessPending():', '  private businessImportsReady():'),
  section('  private pasteTargetName():', '  private async openMarkdownLink('),
  section('  private current():', '  private refreshPreview('),
  section('  private draftField(', '  private updateMarkdown('),
  section('  private draftTextChanged(', '  private draftSelectionChanged('),
  section('  private draftSelectionChanged(', '  private restoreSelection('),
  section('  private editorInputChanged(', '  private select('),
  section('  private async loadImports():', '  private importBoundary('),
  section('  private async acceptPreparedImport(', '  private async retryAttachment('),
  section('  private async addImportedAsset(', '  private async selectPendingImport('),
  section('  private foregroundChanged():', '  private async switchMediaFullscreen(')
].join('\n');
function compile(text, filename) {
  const compiled = ts.transpileModule(text, { fileName: filename, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  const errors = (compiled.diagnostics || []).filter(item => item.category === ts.DiagnosticCategory.Error);
  assert.equal(errors.length, 0, ts.formatDiagnosticsWithColorAndContext(errors, {
    getCanonicalFileName: name => name, getCurrentDirectory: () => process.cwd(), getNewLine: () => '\n' }));
  return compiled.outputText;
}
const integrationCode = compile('export class IndexPasteIntegration {\n' + methods + '\n}', sourcePath);
function integrationModels(workbench) {
  const modules = new Map(), timers = new Map(); let timer = 0, operation = 0;
  const context = vm.createContext({ exports: {}, console, encodeURIComponent, Buffer,
    util: { generateRandomUUID: () => 'operation-' + (++operation) }, workbench,
    setTimeout: callback => { const id = ++timer; timers.set(id, callback); return id; }, clearTimeout: id => timers.delete(id) });
  function load(name) {
    if (modules.has(name)) return modules.get(name); const api = {}; modules.set(name, api);
    const filename = path.join(modelRoot, name + '.ets');
    const moduleContext = vm.createContext({ exports: api, encodeURIComponent,
      setTimeout: context.setTimeout, clearTimeout: context.clearTimeout, require: id => id === 'libmorrow.so' ? { default: {} } : load(id.replace(/^\.\//, '')) });
    vm.runInContext(compile(fs.readFileSync(filename, 'utf8'), filename), moduleContext, { filename }); return api;
  }
  const draft = load('EditorDraft'), attachments = load('Attachments');
  const fieldPolicy = load('EditorFieldPolicy'), directInput = load('EditorDirectInput');
  Object.assign(context, draft, attachments, fieldPolicy, directInput, load('EditorPaste'), load('ClipboardPaste'), load('AttachmentImportSelection'),
    { DraftTextValue: draft.TextValue, Command: load('Workbench').Command });
  vm.runInContext(integrationCode, context, { filename: sourcePath });
  return { draft, attachments, fieldPolicy, directInput, Index: context.exports.IndexPasteIntegration };
}
function conversionReply(bytes, text, images = []) {
  const count = fieldReply('description', text);
  return { ok: true, error: '', paste_text: text, paste_grapheme_count: count.grapheme_count,
    paste_utf16_length: count.utf16_length, paste_utf8_length: count.utf8_length, unicode_version: '16.0.0',
    warnings: [], images, source_sha256: sha(bytes), source_byte_length: String(bytes.length) };
}
function harness(records, options = {}) {
  const clipboard = createClipboardHarness(records, options.clipboard || {}), files = createFilesHarness(clipboard);
  const events = [], imported = [], importedMetadata = new Map(); let draft, writes = 0;
  const workbench = {
    async send(serialized) { const command = JSON.parse(serialized); assert.equal(command.action, 'import_list'); return { ok: true, error: '', imports: plain(imported) }; },
    async importFd(serialized, fd) {
      const request = JSON.parse(serialized).import_request, bytes = files.read(files.fdPath(fd));
      assert.equal(request.sha256, sha(bytes)); assert.equal(request.byte_length, String(bytes.length));
      events.push({ kind: 'native-import', request: plain(request) });
      if (options.importEffect) await options.importEffect({ request, events, draft, page });
      if (options.unknownAt === events.filter(item => item.kind === 'native-import').length) throw Error('synthetic unknown native reply');
      const assetId = 'asset-draft-import-' + sha(Buffer.from(request.sha256 + request.operation_id));
      const record = { request: plain(request), asset_id: assetId, phase: 'ready', current_active: true, bytes_retained: true, staging_revision: '1', repeated: false };
      imported.push(record); importedMetadata.set(assetId, request); return JSON.stringify({ ok: true, error: '', imports: [record] });
    }
  };
  const models = integrationModels(workbench), api = models.draft, values = new api.Values(), scope = new api.DraftScope();
  scope.card_id = 'card-clipboard'; scope.draft_id = 'draft-clipboard'; scope.source_kind = 1;
  values.category = '实验'; values.title.text = 'Fixture';
  for (const field of ['description', 'hypothesis', 'conclusion', 'todos']) {
    values[field].text = 'before after'; values[field].selection_base = 7; values[field].selection_extent = 12;
  }
  if (options.target === 'title') { values.title.text = 'before after'; values.title.selection_base = 7; values.title.selection_extent = 12; }
  for (let i = 0; i < (options.selectedCount || 0); i++) values.assets.push(Object.assign(new api.AssetSelection(), { asset_id: 'existing-' + i }));
  const stored = (selection, index) => { const meta = importedMetadata.get(selection.asset_id); return {
    selection: plain(selection), pin_id: 'draft-asset-' + index, display_name: meta?.name || 'existing.png',
    media_type: 'application/octet-stream', byte_length: meta?.byte_length || '1', sha256: meta?.sha256 || 'a'.repeat(64) }; };
  const restored = Object.assign(new api.DraftRecord(), { scope, values: api.copyValues(values), generation: '1', current_generation: '1',
    operation_id: 'restore-1', active: true, current_active: true, assets: values.assets.map(stored) });
  draft = new api.EditorDraftCoordinator(scope, values, async serialized => {
    const request = JSON.parse(serialized).draft; events.push({ kind: 'draft-save', request: plain(request) });
    const reply = new api.DraftRecord(); reply.scope = Object.assign(new api.DraftScope(), {
      card_id: request.card_id, draft_id: request.draft_id, source_kind: request.source_kind, source_revision: request.source_revision });
    reply.values = Object.assign(new api.Values(), request.values, { assets: plain(request.assets) });
    reply.assets = request.assets.map(stored); reply.operation_id = request.operation_id;
    reply.generation = api.nextGeneration(request.expected_generation); reply.current_generation = reply.generation;
    reply.active = true; reply.current_active = true; return reply;
  }, () => {}, () => 'draft-write-' + (++writes), restored);
  const page = new models.Index();
  const fieldPolicy = new models.fieldPolicy.EditorFieldPolicy(async serialized => {
    const request = JSON.parse(serialized); events.push({ kind: 'field-count', request: plain(request) });
    if (options.fieldEffect) await options.fieldEffect({ request, page, draft, events });
    return JSON.stringify(fieldReply(request.field, request.text));
  });
  Object.assign(page, { ready: true, pageAlive: true, foreground: true, editorOpen: true, editorDraft: draft, editorValues: api.copyValues(values),
    editorViewVisible: true, editorViewRevoked: false, editorBoundary: '',
    fieldPolicy, editorInputEpoch: 0, fieldCountEpochs: new Map(), fieldCountLabels: [], fieldValidationWorking: false,
    // Preserve existing paste assertions; only the new direct-input platform
    // readiness dependency is synthetic in this integration harness.
    directInput: { canConfirm: () => true, capture: () => true, bind: () => {}, stop: () => {}, view: () => undefined, unbind: () => {}, retry: () => {} },
    inputPolicy: { stop: () => {} }, inputRevisions: new Map(), inputRulesRevision: 0, inputRulesMessage: '',
    todoBusinessReady: true, todoFormatPending: false, todoInputRevision: 0, todoInputValue: api.copyText(values.todos), todoDragTimer: -1,
    taskEditId: '', taskRenameValue: new api.TextValue(), draftFocusedFields: new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']),
    attachmentEditorIdentity: 'editor-clipboard', attachmentFiles: files.files, clipboardInput: clipboard.input,
    attachmentWorking: false, pasteWorking: false, draftRestoreInput: false, busy: false, draftWorking: false,
    draftRetiring: false, draftRetirementUnknown: false, draftCaptureIncomplete: false, draftConflict: false,
    attachmentPending: '', pending: '', importRecords: [], cards: [], selected: '', lastDraftField: options.target || 'description',
    title: values.title.text, description: values.description.text, hypothesis: values.hypothesis.text, conclusion: values.conclusion.text,
    category: values.category, taskText: values.todos.text, viewportWidth: 1000, previewBody: false, message: '',
    draftCaptureBlocked: new Set(), draftSelectionPending: new Set(),
    draftChanged: () => {}, refreshEditorAssets: () => {}, updateMarkdown: () => {}, restoreSelection: () => {},
    mediaPlayback: { pauseForBackground: () => events.push({ kind: 'background-media-pause' }) }, exitMediaFullscreen: () => {}, fileOpen: undefined });
  if (options.realDirectInput) {
    page.directInput = new models.directInput.EditorDirectInput({
      isCurrent: (owner, key, revision, value) => page.directInputCurrent(owner, key, revision, value),
      // Synthetic accepted formatter proposal. Actual sequencing/ownership and
      // Root's full external-value handoff execute without a readiness mock.
      format: async (field, _old, next) => {
        const count = fieldReply(field, next.text);
        return { value: api.copyText(next), action: 'accepted', format_applied: true,
          grapheme_count: count.grapheme_count, limit: count.limit };
      },
      apply: (proposal, owned) => page.applyDirectInput(proposal, owned), changed: () => page.directInputChanged()
    });
    page.bindInputRules();
    // Row component readiness is independent; no row edit in these direct-input
    // cases is claimed to execute EditorTodos or its native row formatter.
    page.todoBusinessReady = true;
  }
  if (options.importCount) imported.push(...Array.from({ length: options.importCount }, (_, i) => ({ request: { operation_id: 'old-' + i } })));
  if (options.convert) files.setConvert(options.convert(files));
  else files.setConvert(async (serialized, fd) => {
    const request = JSON.parse(serialized), bytes = files.read(files.fdPath(fd));
    return JSON.stringify(conversionReply(bytes, request.format === 'plain' ? bytes.toString('utf8') : 'converted'));
  });
  if (options.image) files.setImage(options.image(files));
  return { page, draft, files, clipboard, events, models, imported, importedMetadata,
    async paste() { await page.pasteContent(); return { values: plain(draft.current), message: page.message, events }; },
    close() { draft.dispose(); } };
}
const imageBytes = Uint8Array.from([137, 80, 78, 71, 13, 10, 26, 10]);
const rich = { 'text/plain': 'plain fallback', 'text/html': '<p>public fixture</p>' };
function imageConversion(h, imageCount = 1) {
  return async (serialized, fd) => {
    const request = JSON.parse(serialized), bytes = h.read(h.fdPath(fd));
    if (request.format === 'plain') return JSON.stringify(conversionReply(bytes, bytes.toString('utf8')));
    const images = Array.from({ length: imageCount }, (_, i) => ({ local_id: 'image-' + (i + 1),
      name: 'clipboard-' + request.expected_sha256 + '-image-' + (i + 1) + '.png', byte_length: String(imageBytes.length), sha256: sha(imageBytes) }));
    return JSON.stringify(conversionReply(bytes, 'rich\n\n' + images.map(item => '![image](attachment:' + item.name + ')').join('\n'), images));
  };
}
test('actual Index methods + SDK compiler are used, with source identity emitted', () => {
  assert.ok(methods.includes('this.importClipboardAttachment') && methods.includes('this.acceptPreparedImport'));
  console.log('Index.ets SHA256=' + crypto.createHash('sha256').update(source).digest('hex') + '; SDK TypeScript=' + ts.version);
});
test('text selection insertion uses actual Index + draft methods for every editable field', async () => {
  for (const target of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) {
    const h = harness([{ 'text/plain': '粘贴😀' }], { target });
    try { const result = await h.paste(); assert.equal(result.values[target].text, 'before 粘贴😀');
      assert.equal(result.values[target].selection_base, result.values[target].text.length); assert.match(result.message, /内容已保留/);
      assert.equal(h.events.filter(item => item.kind === 'native-import').length, 0);
    } finally { h.close(); }
  }
});
test('actual DirectInput remains current after paste adopts complete selection metadata', async () => {
  const h = harness([{ 'text/plain': '粘贴😀' }], { realDirectInput: true }); try {
    assert.equal(h.page.canPaste(), true); const result = await h.paste(); await settle();
    const value = h.draft.current.description, state = h.page.directInput.view('description');
    assert.equal(result.values.description.text, 'before 粘贴😀'); assert.equal(value.selection_base, value.text.length);
    assert.equal(value.selection_extent, value.text.length); assert.deepEqual(plain(state.value), plain(value));
    assert.equal(state.revision, h.page.inputRevisions.get('description')); assert.equal(state.pending, false);
    assert.equal(h.page.directInput.canConfirm('description'), true); assert.equal(h.page.inputReadyFor('create'), true);
    assert.equal(h.page.inputReadyFor('edit'), true); assert.equal(h.page.canPaste(), true);
    h.page.foreground = false; h.page.foregroundChanged(); assert.equal(h.page.directInput.canConfirm('description'), false);
    h.page.foreground = true; h.page.foregroundChanged(); await settle();
    for (const key of ['title', 'description', 'hypothesis', 'conclusion']) {
      assert.equal(h.page.directInput.canConfirm(key), true, 'each stopped field resumes: ' + key);
      assert.deepEqual(plain(h.page.directInput.view(key).value), plain(h.draft.current[key]), key);
    }
    // Background reset requires the independent row formatter to qualify its
    // resumed revision too. Ordinary DirectInput completion cannot stand in for it.
    assert.equal(h.page.inputReadyFor('edit'), false); assert.equal(h.page.canPaste(), true);
    h.page.todoBusinessReady = true; // Controlled row readiness, as at initial harness mounting.
    assert.equal(h.page.inputReadyFor('edit'), true);
  } finally { h.page.directInput.stop(); h.close(); }
});
test('paste and selected-import automatic titles hand complete raw/selection to actual DirectInput', async () => {
  for (const mode of ['paste', 'selected-import']) {
    const h = harness([rich], { realDirectInput: true, convert: files => imageConversion(files), image: files => async (_request, _fd, destination) => {
      files.writeFd(destination, Buffer.from(imageBytes)); return JSON.stringify(files.stream(Buffer.from(imageBytes))); } });
    try {
      const values = h.draft.current; values.title.text = ''; values.title.selection_base = -1; values.title.selection_extent = -1;
      h.page.editorValues = h.models.draft.copyValues(values); h.page.title = ''; h.draft.update(values); await h.draft.flush();
      h.page.bindInputRules(); h.page.todoBusinessReady = true;
      if (mode === 'paste') { const result = await h.paste(); assert.match(result.message, /内容已保留/); }
      else {
        const request = Object.assign(new h.models.attachments.ImportRequest(), { card_id: h.draft.scope.card_id, draft_id: h.draft.scope.draft_id,
          operation_id: 'synthetic-selected-title', expected_generation: h.draft.confirmed.generation, name: '用户附件 🧪.txt',
          kind: 'file', byte_length: '8', sha256: 'b'.repeat(64) });
        const record = Object.assign(new h.models.attachments.ImportRecord(), { request, asset_id: 'selected-title-asset', phase: 'ready',
          current_active: true, bytes_retained: true, staging_revision: '1' });
        h.imported.push(record); h.importedMetadata.set(record.asset_id, request);
        assert.equal(await h.page.addImportedAsset(record, true, h.draft), true);
      }
      await settle(); const title = h.draft.current.title; assert.ok(title.text.length > 0, 'actual automatic title: ' + mode);
      assert.equal(title.selection_base, title.text.length, mode); assert.equal(title.selection_extent, title.text.length, mode);
      assert.deepEqual(plain(h.page.directInput.view('title').value), plain(title), mode);
      assert.equal(h.page.directInput.canConfirm('title'), true, mode); assert.equal(h.page.directInput.canConfirm('description'), true, mode);
      assert.equal(h.page.inputReadyFor('create'), true, mode); assert.equal(h.page.inputReadyFor('edit'), true, mode); assert.equal(h.page.canPaste(), true, mode);
      assert.equal(h.draft.confirmed.values.title.text, title.text, mode); assert.ok(h.draft.confirmed.assets.length > 0, mode);
    } finally { h.page.directInput.stop(); h.close(); }
  }
});
test('rich source and exact image pins precede portable body save', async () => {
  const h = harness([rich], { convert: files => imageConversion(files), image: files => async (_request, _fd, destination) => {
    files.writeFd(destination, Buffer.from(imageBytes)); return JSON.stringify(files.stream(Buffer.from(imageBytes))); } });
  try { const result = await h.paste(), requests = h.events.filter(item => item.kind === 'native-import');
    assert.equal(requests.length, 2); assert.equal(result.values.assets.length, 2);
    assert.match(result.values.description.text, /attachment:asset-draft-import-[a-f0-9]{64}/); assert.doesNotMatch(result.values.description.text, /attachment:clipboard-/);
    const textSave = h.events.findIndex(item => item.kind === 'draft-save' && item.request.values.description.text.includes('rich'));
    assert.ok(textSave > h.events.findLastIndex(item => item.kind === 'native-import'));
  } finally { h.close(); }
});
test('image extraction failure preserves source and uses plain fallback without dangling refs', async () => {
  const h = harness([rich], { convert: files => imageConversion(files), image: () => async () => { throw Error('synthetic image extraction failed'); } });
  try { const result = await h.paste(); assert.equal(result.values.description.text, 'before plain fallback');
    assert.equal(result.values.assets.length, 1); assert.doesNotMatch(result.values.description.text, /attachment:clipboard-/);
    assert.match(result.message, /原件.*转换未完成/);
  } finally { h.close(); }
});
test('expanded 20-slot or import-record capacity stops before any native import or text save', async () => {
  for (const setup of [{ selectedCount: 19 }, { importCount: 19 }]) {
    const h = harness([rich], { ...setup, convert: files => imageConversion(files), image: files => async (_request, _fd, destination) => {
      files.writeFd(destination, Buffer.from(imageBytes)); return JSON.stringify(files.stream(Buffer.from(imageBytes))); } });
    try { const result = await h.paste(); assert.equal(h.events.filter(item => item.kind === 'native-import').length, 0);
      assert.equal(result.values.description.text, 'before after'); assert.match(result.message, /剩余位置不足/);
      assert.equal((await h.files.files.recover()).length, 0, 'unissued spools cleaned');
    } finally { h.close(); }
  }
});
test('unknown second import preserves first confirmed pin and exact original request/spool, no body insertion', async () => {
  const h = harness([{ 'application/pdf': Uint8Array.from([1, 2, 3]).buffer }, { 'text/plain': 'second', 'application/zip': Uint8Array.from([4, 5]).buffer }], { unknownAt: 2 });
  try { const result = await h.paste(); assert.equal(result.values.assets.length, 1); assert.equal(result.values.description.text, 'before after');
    assert.equal(h.events.filter(item => item.kind === 'native-import').length, 2); assert.match(result.message, /已确认附件 1\/2/);
    const spools = await h.files.files.recover(); assert.equal(spools.length, 1); assert.ok(spools[0].original_request);
    assert.deepEqual(JSON.parse(spools[0].original_request).import_request, h.events.filter(item => item.kind === 'native-import')[1].request);
  } finally { h.close(); }
});
test('selection, IME, other field, clipboard or foreground changing during read prevents all imports/text', async () => {
  for (const change of ['selection', 'ime', 'other', 'clipboard', 'foreground']) {
    const gate = deferred(), h = harness([{ 'text/plain': 'must not insert' }], { clipboard: { read: () => gate.promise } });
    try { const run = h.page.pasteContent(); await settle();
      assert.equal(h.page.pasteWorking, true); assert.equal(h.page.canPaste(), false);
      if (change === 'selection') h.page.editorValues.description.selection_base = 0;
      else if (change === 'ime') h.page.editorValues.description.composing_start = 0;
      else if (change === 'other') { const v = h.draft.current; v.hypothesis.text = 'newer'; v.hypothesis.selection_base = 5; v.hypothesis.selection_extent = 5; h.draft.update(v); }
      else if (change === 'clipboard') h.clipboard.state.count++;
      else { h.page.foreground = false; h.page.foregroundChanged(); }
      gate.resolve(); await run; assert.equal(h.events.filter(item => item.kind === 'native-import').length, 0);
      assert.equal(h.draft.current.description.text, 'before after'); assert.equal(h.page.pasteWorking, false);
    } finally { h.close(); }
  }
});
test('IME candidate present before paste never reads clipboard', async () => {
  const h = harness([{ 'text/plain': 'must not insert' }]);
  try { h.page.editorValues.description.composing_start = 0; h.page.editorValues.description.composing_end = 1;
    await h.paste(); assert.equal(h.clipboard.logs.some(item => item[0] === 'getData'), false); assert.match(h.page.message, /输入法候选/);
  } finally { h.close(); }
});
test('foreground loss during admitted native import retains its confirmed pin and admits no later import/body', async () => {
  const h = harness([{ 'application/pdf': Uint8Array.from([1, 2, 3]).buffer },
    { 'application/zip': Uint8Array.from([4, 5]).buffer }], { importEffect: async ({ page }) => {
      page.foreground = false; page.foregroundChanged();
    } });
  try { const result = await h.paste(); assert.equal(result.values.assets.length, 1);
    assert.equal(result.values.description.text, 'before after'); assert.equal(h.events.filter(item => item.kind === 'native-import').length, 1);
    assert.match(result.message, /粘贴已停止 · 已确认附件 1\/2/); assert.equal(h.page.pasteWorking, false);
    console.log('late foreground loss: actual confirmed pin count=' + result.values.assets.length + '; UI=' + result.message.replace(/\n/g, ' '));
  } finally { h.close(); }
});
test('plain byte flavor inserts converted text into all non-body fields', async () => {
  for (const target of ['title', 'hypothesis', 'conclusion', 'todos']) {
    const h = harness([{ 'text/plain': Uint8Array.from(Buffer.from('bytes plain')).buffer }], { target,
      convert: files => async (_serialized, fd) => { const bytes = files.read(files.fdPath(fd)); return JSON.stringify(conversionReply(bytes, 'bytes plain')); } });
    try { await h.paste(); assert.equal(h.draft.current[target].text, 'before bytes plain', 'actual converted plain fallback for ' + target); }
    finally { h.close(); }
  }
});

for (const change of ['text roundtrip', 'selection roundtrip']) {
  test('actual Index input epoch rejects ' + change + ' during a delayed complete future-field count', async () => {
    const gate = deferred(), entered = deferred(), h = harness([rich], { fieldEffect: async ({ request }) => {
      if (request.text === 'before converted') { entered.resolve(); await gate.promise; }
    } });
    try {
      const original = plain(h.draft.current), pending = h.page.pasteContent(); await entered.promise;
      if (change === 'text roundtrip') {
        h.page.editorInputChanged('description', 'before afterx'); h.page.editorInputChanged('description', 'before after');
      } else {
        h.page.draftSelectionChanged('description', 0, 0); h.page.draftSelectionChanged('description', 7, 12);
      }
      assert.deepEqual(plain(h.draft.current), original); assert.equal(h.page.editorInputEpoch, 2);
      gate.resolve(); await pending; assert.equal(h.events.filter(item => item.kind === 'native-import').length, 0);
      assert.equal(h.draft.current.description.text, 'before after'); assert.match(h.page.message, /变化/);
      assert.equal((await h.files.files.recover()).length, 0);
    } finally { gate.resolve(); h.close(); }
  });
}
test('actual Index rejects a complete future over-limit title before any native import', async () => {
  const h = harness([{ 'text/plain': 'x'.repeat(61), 'application/pdf': Uint8Array.from([1, 2, 3]).buffer }], { target: 'title' });
  try {
    const result = await h.paste(); assert.equal(h.events.filter(item => item.kind === 'native-import').length, 0);
    assert.equal(result.values.title.text, 'before after'); assert.match(result.message, /60.*超限/);
    assert.equal((await h.files.files.recover()).length, 0);
  } finally { h.close(); }
});
