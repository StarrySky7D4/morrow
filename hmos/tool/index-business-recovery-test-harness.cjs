'use strict';
// Verbatim current Index methods, actual ETS coordinators and the actual one
// Workbench queue. Only native responses, SDK event delivery and rendering are
// controlled. This is source integration evidence, not native/device proof.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict'), crypto = require('node:crypto');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modelRoot = path.resolve(__dirname, '../entry/src/main/ets/model');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets');
const source = fs.readFileSync(sourcePath, 'utf8');
const modelSources = new Map(fs.readdirSync(modelRoot).filter(f => f.endsWith('.ets')).map(f =>
  [f.slice(0, -4), fs.readFileSync(path.join(modelRoot, f), 'utf8')]));
const usedModels = new Set();
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const plain = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
function deferred() { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
async function settle() { for (let i = 0; i < 100; i++) await Promise.resolve(); }
function actualMethod(name) {
  const match = new RegExp('^  private (?:async )?' + name + '\\(', 'm').exec(source);
  assert.ok(match, 'actual current Index method exists: ' + name);
  // Some real methods precede later field initializers. A next-method regex
  // would accidentally include those initializers; use the TS method node end.
  const prefix = 'class Extracted {\n', input = prefix + source.slice(match.index) + '\n}';
  const parsed = ts.createSourceFile('extracted.ts', input, ts.ScriptTarget.ES2020, true, ts.ScriptKind.TS);
  const node = parsed.statements[0]?.members?.[0];
  assert.ok(node && ts.isMethodDeclaration(node) && node.name.getText(parsed) === name, 'actual current Index method boundary: ' + name);
  return source.slice(match.index, match.index + node.end - prefix.length);
}
function compile(input, filename) {
  const output = ts.transpileModule(input, { fileName: filename, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.deepEqual((output.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), [], 'TS source extraction parses');
  return output.outputText;
}
const names = [...source.matchAll(/^  private (?:async )?(\w+)\(/gm)].map(m => m[1]);
const first = source.indexOf('class BusinessEditorBinding {'), last = source.indexOf('\n@Component', first);
assert.ok(first >= 0 && last > first, 'current actual binding declarations');
const metadataStart = source.indexOf('.enabled(this.editorMetadataReady(owner)');
assert.ok(metadataStart > 0, 'actual metadata builder qualification');
const callbackStart = source.indexOf('.onSelect(', metadataStart) + '.onSelect('.length;
const callbackPrefix = 'const callback = ', callbackSource = callbackPrefix + source.slice(callbackStart);
const callbackParsed = ts.createSourceFile('callback.ts', callbackSource, ts.ScriptTarget.ES2020, true, ts.ScriptKind.TS);
const categoryCallback = callbackParsed.statements[0].declarationList.declarations[0].initializer;
assert.ok(ts.isArrowFunction(categoryCallback), 'actual editor category callback');
const categoryBody = categoryCallback.body.getText(callbackParsed);
const pageCode = compile(source.slice(first, last) + '\nexport class ActualIndexRecovery {\n' +
  names.map(actualMethod).join('\n') + '\nactualEditorCategorySelect(owner: string, _: number, value: string): void ' + categoryBody + '\n}', sourcePath);
const readonly = value => ({ ok: true, error: '', effect: 'not_committed', receipt_revision: '', cards: [], drafts: [], imports: [],
  editor_intents: [], intent_next_after: '', ...plain(value) });

function harness(options = {}) {
  const cache = new Map(), timers = new Map(), calls = [], nativeCalls = [], events = [], writers = new Set();
  let timer = 0, uuid = 0, intercept, route, page;
  const native = { async request(wire) {
    nativeCalls.push(wire); const request = JSON.parse(wire); events.push({ kind: 'native', wire, action: request.action });
    const next = () => route(wire, request);
    return JSON.stringify(intercept ? await intercept(wire, request, next) : await next());
  } };
  const timerAPI = { setTimeout: fn => { const id = ++timer; timers.set(id, fn); return id; }, clearTimeout: id => timers.delete(id) };
  function load(name) {
    if (name === 'libmorrow.so') return { default: native };
    if (name === '@kit.ArkTS') return { util: { TextEncoder: class { encodeInto(value) { return new TextEncoder().encode(value); } } } };
    if (name === '@kit.CryptoArchitectureKit') return { cryptoFramework: { createMd() {
      const h = crypto.createHash('sha256'); return { async update({ data }) { h.update(data); }, async digest() { return { data: new Uint8Array(h.digest()) }; } };
    } } };
    name = name.replace(/^\.\//, ''); if (cache.has(name)) return cache.get(name);
    assert.ok(modelSources.has(name), 'actual model exists in source freeze: ' + name); usedModels.add(name);
    const exports = {}; cache.set(name, exports);
    const file = path.join(modelRoot, name + '.ets');
    vm.runInNewContext(compile(modelSources.get(name), file), { exports, require: load, Uint8Array, encodeURIComponent, ...timerAPI }, { filename: file });
    return exports;
  }
  const m = {};
  for (const name of ['EditorDraft', 'EditorFieldPolicy', 'EditorInputPolicy', 'EditorDirectInput', 'EditorTodos', 'EditorBusiness',
    'EditorBusinessSession', 'EditorBusinessHandoff', 'EditorBusinessRecovery', 'EditorDraftFork', 'EditorPaste']) Object.assign(m, load(name));
  const wb = load('Workbench'), originalSend = wb.workbench.send.bind(wb.workbench);
  wb.workbench.send = wire => { calls.push(wire); events.push({ kind: 'admit', wire, action: JSON.parse(wire).action }); return originalSend(wire); };
  const context = vm.createContext({ exports: {}, ...m, DraftValues: m.Values, DraftTextValue: m.TextValue, Command: wb.Command,
    workbench: wb.workbench, util: { generateRandomUUID: () => 'actual-recovery-owner-' + (++uuid) }, ...timerAPI });
  vm.runInContext(pageCode, context, { filename: sourcePath }); page = new context.exports.ActualIndexRecovery();
  Object.assign(page, { cards: [], selected: '', detailId: '', title: '', description: '', hypothesis: '', conclusion: '', category: '灵感',
    taskText: '', taskEditId: '', taskRenameText: '', taskRenameOwner: '', taskRenameValue: new m.TextValue(),
    pageAlive: true, foreground: true, ready: true, editorOpen: false, dirty: false, busy: false, pending: '',
    editorValues: new m.Values(), editorInputEpoch: 0, editorOpenIdentity: '', editorBusinessMode: 'edit',
    businessWorking: false, businessStatus: '', businessIntents: [], businessNext: '', businessListError: '', businessRecoveryIdentity: '',
    businessHandoffCloseOperation: '', businessCloseExact: false, attachmentEditorIdentity: '', attachmentEditingValues: '', attachmentEditEpoch: 0,
    editorViewOwner: '', editorViewRevoked: true, editorViewVisible: true, editorBoundary: '', editorFocusIntent: 0,
    draftSource: '', draftRecords: [], draftsOpen: false, draftStatus: '', draftWorking: false, draftConflict: false, draftRestoreInput: false,
    draftRetiring: false, draftRetirementUnknown: false, draftRetiredIdentity: '', retirementInput: new Map(), draftRetirement: '',
    draftCaptureIncomplete: false, draftCaptureBlocked: new Set(), draftFocusedFields: new Set(), draftSelectionPending: new Set(),
    inputRevisions: new Map(), inputRulesRevision: 0, inputRulesMessage: '', fieldCountEpochs: new Map(), fieldCountLabels: ['', '', '', '', '', ''],
    fieldValidationWorking: false, todoInputValue: new m.TextValue(), todoInputRevision: 0, todoBusinessReady: false, todoFormatPending: false,
    todoDragTimer: -1, rawForkWorking: false, rawForkStatus: '', rawForkAction: '', rawForkRetirement: '', rawForkRetirementUnknown: false,
    attachmentWorking: false, pasteWorking: false, attachmentPending: '', pendingSpools: [], importRecords: [], attachmentDiagnostics: [], editorAssets: [],
    attachmentSelectionState: { phase: 'idle', busy: false, total: 0, started: 0, selected: 0, retained: 0, error: '' },
    previewBody: false, message: '', section: '概览', filter: '全部', sort: '默认顺序',
    // Rendering/framework seams do not grant input, save, recovery or lease qualification.
    updateMarkdown: () => {}, updateInlineImages: () => {}, refreshPreview: () => {}, workspaceConditionsChanged: () => {},
    loadImports: async () => {}, getUIContext: () => ({ showAlertDialog: () => events.push({ kind: 'alert' }) }) });
  const segmenter = new Intl.Segmenter('und', { granularity: 'grapheme' });
  page.fieldPolicy = new m.EditorFieldPolicy(async wire => {
    const p = JSON.parse(wire), count = [...segmenter.segment(p.text)].length, limit = m.editorFieldLimit(p.field);
    events.push({ kind: 'field-worker', request: p });
    return JSON.stringify({ ok: count <= limit, error: count <= limit ? '' : 'EditorFieldGraphemeLimit', field: p.field,
      grapheme_count: count, utf16_length: p.text.length, utf8_length: Buffer.byteLength(p.text), limit, unicode_version: '16.0.0' });
  });
  page.inputPolicy = new m.EditorInputPolicy(async () => { throw new Error('Test must explicitly deliver native input formatting'); }, async wire => sha(wire));
  page.directInput = new m.EditorDirectInput({ isCurrent: (...args) => page.directInputCurrent(...args),
    format: (...args) => page.inputPolicy.format(...args), apply: (...args) => page.applyDirectInput(...args), changed: () => page.directInputChanged() });
  route = (wire, request) => {
    if (request.action === 'list') return readonly({ cards: page.cards });
    if (request.action === 'draft_list') return readonly({ drafts: page.draftRecords });
    if (request.action === 'editor_intent_list') return readonly({ editor_intents: [] });
    throw new Error('No controlled native response: ' + request.action);
  };
  return { page, m, calls, nativeCalls, events, workbench: wb.workbench,
    setRoute(fn) { route = fn; }, setIntercept(fn) { intercept = fn; },
    mount() { page.editorLeaseMounted(page.editorViewOwner); },
    adopt(name, text) { const value = m.copyText(page.editorValues[name]); value.text = text; value.selection_base = text.length;
      value.selection_extent = text.length; page.adoptExternalInput(name, value); },
    track(writer) { writers.add(writer); return writer; },
    dispose() { writers.add(page.editorDraft); writers.add(page.businessRecovery?.ready?.writer); for (const writer of writers) writer?.dispose();
      page.businessRecovery?.dispose(); page.directInput.stop(); page.inputPolicy.stop(); page.fieldPolicy.stop(); timers.clear(); },
    async runTimers() { const batch = [...timers.values()]; timers.clear(); for (const fn of batch) fn(); await settle(); } };
}
function assertSourceUnchanged() {
  assert.equal(fs.readFileSync(sourcePath, 'utf8'), source, 'Index source remained frozen during integration run');
  for (const name of usedModels) assert.equal(fs.readFileSync(path.join(modelRoot, name + '.ets'), 'utf8'), modelSources.get(name), 'actual model remained frozen: ' + name);
}
function identities() { return [{ path: sourcePath, sha256: sha(source), bytes: Buffer.byteLength(source) },
  ...[...usedModels].sort().map(name => ({ path: path.join(modelRoot, name + '.ets'), sha256: sha(modelSources.get(name)), bytes: Buffer.byteLength(modelSources.get(name)) }))]; }
module.exports = { harness, actualMethod, source, sourcePath, plain, sha, readonly, deferred, settle, identities, assertSourceUnchanged };
