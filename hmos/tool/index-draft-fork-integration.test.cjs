'use strict';
// Actual freshly extracted Index methods, actual ETS draft/fork models and
// actual ViewLease lifecycle methods. Framework delivery and native Store
// receipts are controlled; this is not an IME queue, device or persistence test.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const crypto = require('node:crypto'), assert = require('node:assert/strict'), { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const root = path.resolve(__dirname, '../entry/src/main/ets'), indexPath = path.join(root, 'pages/Index.ets');
const leasePath = path.join(root, 'pages/EditorViewLease.ets'), source = fs.readFileSync(indexPath, 'utf8');
const clone = value => value === undefined ? undefined : JSON.parse(JSON.stringify(value));
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const tick = async () => { for (let i = 0; i < 100; ++i) await Promise.resolve(); };
function gate() { let resolve, reject; const promise = new Promise((r, j) => { resolve = r; reject = j; }); return { promise, resolve, reject }; }
function section(from, to) {
  const start = source.indexOf(from), end = source.indexOf(to, start + from.length);
  assert.ok(start >= 0 && end > start, 'fresh Index anchors ' + from); return source.slice(start, end);
}
function method(name) {
  const match = new RegExp('^  private (?:async )?' + name + '\\(', 'm').exec(source);
  assert.ok(match, 'fresh actual method ' + name);
  const oneLine = source.slice(match.index).split(/\r?\n/, 1)[0];
  if (/ \{.*\} *$/.test(oneLine)) return oneLine;
  const ending = /^  }\r?$/mg; ending.lastIndex = match.index; const close = ending.exec(source);
  assert.ok(close); return source.slice(match.index, close.index + close[0].length);
}
function compile(text, file) {
  const result = ts.transpileModule(text, { fileName: file, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  const errors = (result.diagnostics || []).filter(item => item.category === ts.DiagnosticCategory.Error);
  assert.equal(errors.length, 0, errors.map(e => ts.flattenDiagnosticMessageText(e.messageText, '\n')).join('\n'));
  return result.outputText;
}
const methods = [section('  private draftField(', '  private updateMarkdown('),
  section('  private draftTextChanged(', '  private restoreSelection('),
  section('  private ownsEditorView(', '  private attachDraft('),
  method('editorInputChanged'), method('flushDraft'), method('keepDraftAndClose'), method('closeSavedEditor'),
  method('retireDraft'), method('discardDraftConfirmed'), method('retryDraftRetirement'),
  method('focusDraftField'), method('blurDraftField'), method('restoreSelection'), method('current')].join('\n');
const fields = source.split(/\r?\n/).filter(line => /^  (?:@State )?(?:private )?(?:editorView\w+|editorFocusIntent|editor(?:Detach|Mount)Resolve|editorBoundary\w*|rawFork\w*):/.test(line))
  .map(line => line.replace('@State ', '')).join('\n');
assert.ok(fields.includes('editorViewRevoked') && fields.includes('rawForkParent'), 'actual state initializers');
const indexCode = compile('export class ActualIndex {\n' + fields + '\n' + methods + '\n}', indexPath);
const leaseSource = fs.readFileSync(leasePath, 'utf8');
const leaseCode = compile(leaseSource.replace('@Component\n', '').replace('export struct', 'export class')
  .replace(/@Prop /g, '').replace(/@BuilderParam /g, ''), leasePath);

function harness(options = {}) {
  let op = 0, timer = 0, page, lease; const modules = new Map(), timers = new Map(), events = [], slots = new Map(), history = new Map();
  function load(name) {
    if (modules.has(name)) return modules.get(name); const api = {}; modules.set(name, api);
    const file = path.join(root, 'model', name + '.ets');
    vm.runInNewContext(compile(fs.readFileSync(file, 'utf8'), file), { exports: api,
      require: id => id === 'libmorrow.so' ? { default: {} } : load(id.replace(/^\.\//, '')),
      setTimeout: callback => { const id = ++timer; timers.set(id, callback); return id; }, clearTimeout: id => timers.delete(id), encodeURIComponent }, { filename: file });
    return api;
  }
  const api = load('EditorDraft'), fork = load('EditorDraftFork'), scope = new api.DraftScope(), values = new api.Values();
  Object.assign(scope, { card_id: 'card-lease', draft_id: 'parent-lease', source_kind: options.kind ?? 0, source_revision: options.kind === 1 ? '0' : '7', source: options.kind === 1 ? '' : 'a1b2' });
  values.title.text = 'Original title'; values.description.text = 'Original body'; values.todos.text = 'row one\n row two ';
  values.category = '实验'; values.stage = '待验证';
  values.assets = [Object.assign(new api.AssetSelection(), { asset_id: 'parent-pin', origin: 3, aliases: ['picture.png', 'original.png'] })];
  const stored = selection => Object.assign(new api.StoredAsset(), { selection: clone(selection), pin_id: 'draft-asset-0', display_name: 'picture.png',
    media_type: 'image/png', byte_length: '8', sha256: 'c'.repeat(64) });
  const initial = Object.assign(new api.DraftRecord(), { scope, values: api.copyValues(values), assets: values.assets.map(stored),
    operation_id: 'parent-save', generation: '1', current_generation: '1', active: true, current_active: true, request_sha256: 'a'.repeat(64) });
  slots.set(scope.draft_id, clone(initial));
  function activeView(id) { return clone(slots.get(id)); }
  async function send(serialized) {
    const command = JSON.parse(serialized); events.push({ action: command.action, serialized, command: clone(command), writer: page?.editorDraft?.scope.draft_id });
    if (options.effect) await options.effect({ command, serialized, page, parent, events, slots, activeView });
    let record;
    const operation = command.action === 'draft_fork' ? command.fork.child.operation_id : command.action === 'draft_save' ? command.draft.operation_id :
      command.action === 'draft_fork_retire' ? command.fork_retirement.operation_id : command.operation;
    if (history.has(operation)) {
      const prior = history.get(operation); assert.equal(serialized, prior.serialized, 'same operation cannot borrow later wire'); record = clone(prior.record);
      const current = slots.get(record.scope.draft_id); record.current_generation = current.generation; record.current_active = current.active;
    } else if (command.action === 'draft_save' || command.action === 'draft_fork') {
      const raw = command.action === 'draft_save' ? command.draft : command.fork.child;
      const prior = slots.get(raw.draft_id), base = command.action === 'draft_fork' ? slots.get(command.fork.parent_draft_id) : prior;
      if (command.action === 'draft_save') assert.equal(raw.expected_generation, prior.generation);
      else {
        assert.equal(raw.expected_generation, '0'); assert.equal(command.fork.parent_generation, base.generation);
        assert.equal(command.fork.parent_save_operation, base.operation_id); assert.equal(command.fork.parent_request_sha256, base.request_sha256);
      }
      record = Object.assign(new api.DraftRecord(), { scope: Object.assign(new api.DraftScope(), base.scope, { draft_id: raw.draft_id }),
        values: Object.assign(new api.Values(), clone(raw.values), { assets: clone(raw.assets) }), assets: raw.assets.map(stored), operation_id: raw.operation_id,
        generation: api.nextGeneration(raw.expected_generation), current_generation: api.nextGeneration(raw.expected_generation), active: true, current_active: true,
        request_sha256: sha(serialized), fork_link: command.action === 'draft_save' ? prior.fork_link : Object.assign(new api.DraftForkLink(), {
          parent_draft_id: command.fork.parent_draft_id, parent_generation: command.fork.parent_generation,
          parent_save_operation: command.fork.parent_save_operation, parent_request_sha256: command.fork.parent_request_sha256, child_operation: raw.operation_id }) });
      slots.set(raw.draft_id, clone(record)); history.set(operation, { serialized, record: clone(record) });
    } else if (command.action === 'draft_fork_retire' || command.action === 'draft_discard') {
      const identity = command.fork_retirement, id = identity ? identity.parent_draft_id : command.draft_id, prior = slots.get(id);
      assert.equal(identity ? identity.parent_generation : command.generation, prior.generation);
      record = clone(prior); record.active = false; record.current_active = false;
      record.generation = api.nextGeneration(prior.generation); record.current_generation = record.generation;
      if (identity) {
        const child = slots.get(identity.child_draft_id); assert.equal(child.active, true);
        assert.equal(identity.child_operation, child.fork_link.child_operation);
        record.fork_retirement = Object.assign(new api.DraftForkRetirement(), { child_draft_id: identity.child_draft_id, operation_id: identity.operation_id,
          fork_link: Object.assign(new api.DraftForkLink(), { parent_draft_id: identity.parent_draft_id, parent_generation: identity.parent_generation,
            parent_save_operation: identity.parent_save_operation, parent_request_sha256: identity.parent_request_sha256, child_operation: identity.child_operation }) });
      }
      slots.set(id, clone(record)); history.set(operation, { serialized, record: clone(record) });
    } else if (command.action === 'draft_list') return { ok: true, drafts: [...slots.values()].filter(r => r.active), cards: [] };
    else throw Error('Unexpected controlled action ' + command.action);
    if (options.after) await options.after({ command, record, page, parent, events, slots });
    if (options.reply) record = options.reply({ command, record: clone(record), page, events }) || record;
    return { ok: true, drafts: [record], cards: [], error: '', effect: 'committed' };
  }
  const parent = new api.EditorDraftCoordinator(scope, values, async serialized => {
    const reply = await send(serialized); return reply.drafts[0];
  }, () => {}, () => 'raw-parent-' + (++op), initial);
  const context = vm.createContext({ exports: {}, ...api, ...fork, ...load('EditorPaste'), DraftTextValue: api.TextValue,
    Command: load('Workbench').Command, workbench: { send }, util: { generateRandomUUID: () => 'operation-' + (++op) },
    setTimeout: callback => { const id = ++timer; timers.set(id, callback); return id; }, clearTimeout: id => timers.delete(id) });
  vm.runInContext(indexCode, context, { filename: indexPath }); page = new context.exports.ActualIndex();
  Object.assign(page, { pageAlive: true, foreground: true, ready: true, editorOpen: true, editorDraft: parent, editorValues: api.copyValues(values),
    attachmentEditorIdentity: 'initial-view-owner', editorViewOwner: 'initial-view-owner', editorViewVisible: true,
    editorInputEpoch: 0, attachmentWorking: false, pasteWorking: false, attachmentPending: '', pendingSpools: [], importRecords: [],
    busy: false, pending: '', draftWorking: false, fieldValidationWorking: false, draftRetiring: false, draftRetirementUnknown: false,
    draftRetirement: '', draftRetiredIdentity: '', retirementInput: new Map(), draftCaptureIncomplete: false, draftRestoreInput: false, draftConflict: false,
    draftSource: scope.source, selected: options.kind === 1 ? '' : scope.card_id, cards: [], draftRecords: [initial],
    title: values.title.text, description: values.description.text, hypothesis: '', conclusion: '', taskText: values.todos.text, category: values.category,
    dirty: true, message: '', draftStatus: '', taskEditId: '', taskRenameText: '', taskRenameValue: new api.TextValue(),
    draftFocusedFields: new Set(['title', 'description', 'hypothesis', 'conclusion', 'todos']), draftSelectionPending: new Set(), draftCaptureBlocked: new Set(),
    fieldCountEpochs: new Map(), fieldCountLabels: [], inputRevisions: new Map(), todoInputValue: api.copyText(values.todos), todoInputRevision: 0, todoBusinessReady: true,
    directInput: { stop() {}, capture() {}, bind() {}, unbind() {}, view() {}, canConfirm() { return true; } }, inputPolicy: { stop() {} }, fieldPolicy: { stop() {} },
    draftChanged() {}, refreshFieldCount() {}, refreshFieldCounts() {}, setFieldCount() {}, todoDragPosition() {},
    loadDrafts: async () => {}, taskRenameChanged: (text, preview) => { events.push({ action: 'rename-capture', text, preview }); },
    titleController: { caretPosition: () => events.push({ action: 'restore-caret' }) }, taskController: { caretPosition() {} } });
  const leaseExports = {}; vm.runInNewContext(leaseCode, { exports: leaseExports }, { filename: leasePath });
  function mount() {
    lease = new leaseExports.EditorViewLease(); lease.owner = page.editorViewOwner;
    lease.onMounted = owner => page.editorLeaseMounted(owner); lease.onRevoked = owner => page.editorLeaseRevoked(owner); lease.aboutToAppear(); return lease;
  }
  mount();
  return { page, parent, api, events, slots, timers, mount, revoke: () => lease.aboutToDisappear(),
    actions: action => events.filter(e => e.action === action),
    close() { parent.dispose(); page.editorDraft?.dispose(); timers.clear(); },
    input(text, name = 'description', preview) { page.leaseTextChanged(page.editorViewOwner, name, text, preview); },
    selection(base, extent, name = 'description') { page.draftFocusedFields.add(name); page.leaseSelectionChanged(page.editorViewOwner, name, base, extent); } };
}

test('actual Index/lease and installed compiler identities are emitted', () => {
  console.log(JSON.stringify({ index_sha256: sha(source), lease_sha256: sha(leaseSource), compiler: ts.version, scope: 'controlled actual methods; no SDK queue/Store/device' }));
  assert.ok(source.includes('EditorViewLease({ owner: owner'));
  for (const field of ['title', 'description', 'hypothesis', 'conclusion', 'todos']) assert.ok(source.includes("this.leaseTextChanged(owner, '" + field + "'"));
  assert.ok(source.includes('this.leaseTaskRenameChanged(owner, task.id, renameOwner'));
  assert.ok(source.includes('focusIntent: this.editorFocusIntent'));
});
test('first fixed ACK switches writer before complete latest child flush, then retires only parent proof', async () => {
  const first = gate(), h = harness({ effect: ({ command }) => command.action === 'draft_fork' ? first.promise : undefined });
  try {
    const run = h.page.beginRawFork(); await tick(); const wire = h.actions('draft_fork')[0].serialized;
    assert.equal(h.parent.writesPaused, true); h.input('before after', 'description', { offset: 7, value: '候选' }); h.selection(2, 9);
    const late = clone(h.page.editorValues.description); first.resolve(); await run;
    const child = h.page.editorDraft; assert.notEqual(child, h.parent); assert.equal(h.parent.disposed, true);
    assert.equal(child.scope.source, 'a1b2'); assert.equal(child.scope.source_revision, '7');
    assert.deepEqual(clone(child.confirmed.values.description), late); assert.equal(h.actions('draft_fork')[0].serialized, wire);
    const childSave = h.actions('draft_save').find(e => e.command.draft.draft_id === child.scope.draft_id);
    assert.equal(childSave.writer, child.scope.draft_id); assert.equal(childSave.command.draft.assets[0].origin, 3);
    assert.equal(h.actions('draft_fork')[0].command.fork.child.assets[0].origin, 4);
    assert.equal(h.actions('draft_fork_retire').length, 1); assert.equal(h.actions('draft_discard').length, 0); assert.equal(h.page.editorOpen, true);
  } finally { first.resolve(); h.close(); }
});
test('first Unknown retains original wire while later full raw requires explicit same request retry', async () => {
  let fail = true; const h = harness({ after: ({ command }) => { if (command.action === 'draft_fork' && fail) { fail = false; throw Error('controlled lost ACK'); } } });
  try {
    await h.page.beginRawFork(); const original = h.actions('draft_fork')[0].serialized;
    assert.equal(h.page.rawFork.unknown, true); assert.equal(h.page.editorDraft, h.parent); assert.equal(h.actions('draft_fork_retire').length, 0);
    h.input('later complete raw'); await tick(); assert.equal(h.actions('draft_fork').length, 1);
    await h.page.retryRawFork(); assert.equal(h.actions('draft_fork')[1].serialized, original);
    assert.equal(h.page.editorDraft.confirmed.values.description.text, 'later complete raw'); assert.equal(h.parent.disposed, true);
  } finally { h.close(); }
});
test('child Unknown blocks parent cleanup; exact child retry precedes latest fresh child generation', async () => {
  let fail = true; const h = harness({ after: ({ command }) => { if (command.action === 'draft_save' && command.draft.draft_id !== 'parent-lease' && fail) { fail = false; throw Error('controlled child lost ACK'); } } });
  try {
    await h.page.beginRawFork(); const child = h.page.editorDraft, firstSave = h.actions('draft_save')[0].serialized;
    assert.equal(child.unknown, true); assert.equal(h.actions('draft_fork_retire').length, 0); assert.equal(h.parent.disposed, false);
    h.input('after unknown child'); await h.page.retryRawFork();
    assert.equal(h.actions('draft_save')[1].serialized, firstSave);
    assert.equal(child.confirmed.values.description.text, 'after unknown child'); assert.equal(h.parent.disposed, true);
  } finally { h.close(); }
});
test('parent cleanup Unknown uses original retirement wire and preserves every later child capture', async () => {
  let fail = true; const h = harness({ after: ({ command }) => { if (command.action === 'draft_fork_retire' && fail) { fail = false; throw Error('controlled retirement lost ACK'); } } });
  try {
    await h.page.beginRawFork(); const wire = h.actions('draft_fork_retire')[0].serialized, child = h.page.editorDraft;
    assert.equal(h.page.rawForkRetirementUnknown, true); h.input('after parent unknown'); await tick();
    assert.equal(h.actions('draft_fork_retire').length, 1); await h.page.retryRawFork();
    assert.equal(h.actions('draft_fork_retire')[1].serialized, wire); assert.equal(child.confirmed.values.description.text, 'after parent unknown');
    assert.equal(h.parent.disposed, true); assert.equal(h.page.editorDraft, child);
  } finally { h.close(); }
});
test('wrong first scope/fullraw/pin aliases and wrong retirement proof never authorize writer/cleanup', async () => {
  for (const change of ['scope', 'raw', 'aliases', 'retirement']) {
    const h = harness({ reply: ({ command, record }) => {
      if (change === 'retirement' && command.action === 'draft_fork_retire') record.fork_retirement.fork_link.parent_request_sha256 = 'd'.repeat(64);
      if (command.action === 'draft_fork') {
        if (change === 'scope') record.scope.source = 'a2';
        if (change === 'raw') record.values.description.text += '!';
        if (change === 'aliases') record.assets[0].selection.aliases.reverse();
      }
      return record;
    } });
    try {
      await h.page.beginRawFork(); assert.equal(h.parent.disposed, false, change);
      if (change === 'retirement') assert.equal(h.page.rawForkRetirementUnknown, true);
      else { assert.equal(h.page.editorDraft, h.parent, change); assert.equal(h.page.rawFork.unknown, true); assert.equal(h.actions('draft_fork_retire').length, 0); }
    } finally { h.close(); }
  }
});
test('same raw with a different live owner cannot adopt first ACK or issue parent cleanup', async () => {
  const delayed = gate(), h = harness({ effect: ({ command }) => command.action === 'draft_fork' ? delayed.promise : undefined });
  try {
    const run = h.page.beginRawFork(); await tick(); h.page.attachmentEditorIdentity = 'replacement-owner'; h.page.editorViewOwner = 'replacement-owner';
    delayed.resolve(); await run; assert.equal(h.page.editorDraft, h.parent); assert.equal(h.parent.disposed, false); assert.equal(h.actions('draft_fork_retire').length, 0);
    await h.page.retryRawFork(); assert.equal(h.actions('draft_fork').length, 1);
  } finally { delayed.resolve(); h.close(); }
});
test('an unselected Ready/Unknown import or pending spool cannot be silently left behind by fork', async () => {
  for (const kind of ['ready', 'unknown', 'spool']) {
    const h = harness();
    try { if (kind === 'spool') h.page.pendingSpools = [{}]; else h.page.importRecords = [{ phase: kind, asset_id: 'not-selected' }];
      await h.page.beginRawFork(); assert.equal(h.actions('draft_fork').length, 0); assert.equal(h.parent.writesPaused, false); assert.equal(h.parent.disposed, false);
    } finally { h.close(); }
  }
});
test('capture incomplete during first ACK preserves both slots and never retires parent', async () => {
  const delayed = gate(), h = harness({ effect: ({ command }) => command.action === 'draft_fork' ? delayed.promise : undefined });
  try {
    const run = h.page.beginRawFork(); await tick(); h.page.leaseUncaptured(h.page.editorViewOwner, 'todos', '{"preview":"unresolved"}');
    delayed.resolve(); await run; assert.equal(h.page.draftCaptureIncomplete, true); assert.equal(h.parent.disposed, false);
    assert.equal(h.page.editorDraft, h.parent); assert.equal(h.actions('draft_fork_retire').length, 0);
  } finally { delayed.resolve(); h.close(); }
});
test('manual discard waits for actual lease revoke; old callbacks cannot populate a new owner', async () => {
  const h = harness();
  try {
    const oldOwner = h.page.editorViewOwner, run = h.page.discardDraftConfirmed(); await tick();
    assert.equal(h.page.editorViewVisible, false); assert.equal(h.page.editorViewRevoked, false); assert.equal(h.actions('draft_discard').length, 0);
    h.input('accepted while detaching'); h.revoke(); await run;
    assert.equal(h.actions('draft_discard').length, 1); assert.equal(h.page.editorOpen, false); assert.equal(h.parent.disposed, true);
    const before = clone(h.page.editorValues); h.page.editorOpen = true; h.page.editorViewOwner = 'next-view'; h.page.attachmentEditorIdentity = 'next-view'; h.page.editorViewRevoked = false;
    h.page.leaseTextChanged(oldOwner, 'description', 'old queued text'); h.page.leaseSelectionChanged(oldOwner, 'description', 0, 2);
    assert.deepEqual(clone(h.page.editorValues), before); assert.equal(h.actions('draft_fork').length, 0);
  } finally { h.close(); }
});
test('second manual discard of the live child also requires revoke and never forks or revives it', async () => {
  const h = harness();
  try {
    await h.page.beginRawFork(); const child = h.page.editorDraft, oldOwner = h.page.editorViewOwner;
    const run = h.page.discardDraftConfirmed(); await tick(); assert.equal(h.actions('draft_discard').length, 0);
    h.revoke(); await run; assert.equal(h.actions('draft_discard').length, 1); assert.equal(child.disposed, true); assert.equal(h.page.editorDraft, undefined);
    h.page.leaseTextChanged(oldOwner, 'description', 'late child text'); await h.page.discardDraftConfirmed();
    assert.equal(h.actions('draft_discard').length, 1); assert.equal(h.actions('draft_fork').length, 1);
  } finally { h.close(); }
});
test('incomplete detach capture stops deletion and remounts full values under a fresh lease', async () => {
  const h = harness();
  try {
    const old = h.page.editorViewOwner, run = h.page.discardDraftConfirmed(); await tick();
    h.input('完整候选', 'description', { value: '保留', offset: 2 }); const value = clone(h.page.editorValues.description);
    h.page.leaseUncaptured(old, 'todos', '{"unknown_row":"raw event"}'); h.revoke(); await run;
    assert.equal(h.actions('draft_discard').length, 0); assert.equal(h.page.editorOpen, true); assert.notEqual(h.page.editorViewOwner, old);
    assert.deepEqual(clone(h.page.editorValues.description), value); assert.equal(h.page.draftCaptureIncomplete, true);
    assert.equal(h.page.retirementInput.get('todos:text'), '{"unknown_row":"raw event"}'); assert.equal(h.parent.disposed, false);
  } finally { h.close(); }
});
test('exact business close waits for lease and late accepted same-value epoch creates durable fork', async () => {
  for (const late of [false, true]) {
    const h = harness();
    try {
      const expected = h.parent.current, run = h.page.closeSavedEditor(expected, h.page.editorInputEpoch); await tick();
      assert.equal(h.actions('draft_discard').length, 0);
      if (late) h.input(expected.description.text);
      h.revoke(); if (late) { await tick(); h.mount(); }
      const closed = await run;
      if (late) { assert.equal(closed, false); assert.equal(h.page.editorOpen, true); assert.equal(h.actions('draft_discard').length, 0); assert.equal(h.actions('draft_fork').length, 1); }
      else { assert.equal(closed, true); assert.equal(h.actions('draft_discard').length, 1); assert.equal(h.page.editorDraft, undefined); }
    } finally { h.close(); }
  }
});
test('discard Unknown stays detached and only explicit retry resends its original wire', async () => {
  let fail = true; const h = harness({ after: ({ command }) => { if (command.action === 'draft_discard' && fail) { fail = false; throw Error('controlled discard lost ACK'); } } });
  try {
    const run = h.page.discardDraftConfirmed(); await tick(); h.revoke(); await run;
    const original = h.actions('draft_discard')[0].serialized; assert.equal(h.page.draftRetirementUnknown, true); assert.equal(h.page.editorViewVisible, false);
    h.input('revoked SDK text'); await tick(); assert.equal(h.actions('draft_discard').length, 1);
    await h.page.retryDraftRetirement(); assert.equal(h.actions('draft_discard')[1].serialized, original); assert.equal(h.page.editorOpen, false);
  } finally { h.close(); }
});
test('rename incarnation and selection restoration timer cannot borrow a replacement lease', async () => {
  const h = harness();
  try {
    const old = h.page.editorViewOwner; h.page.taskEditId = 'task-1'; h.page.taskRenameOwner = 'rename-new';
    h.page.leaseFocusChanged('wrong-owner', 'title', true); assert.equal(h.page.editorFocusIntent, 0);
    h.page.leaseFocusChanged(old, 'title', true); assert.equal(h.page.editorFocusIntent, 1);
    h.page.leaseTaskRenameFocus(old, 'task-1', 'rename-old'); assert.equal(h.page.editorFocusIntent, 1);
    h.page.leaseTaskRenameFocus(old, 'task-1', 'rename-new'); assert.equal(h.page.editorFocusIntent, 2);
    h.page.leaseTaskRenameChanged(old, 'task-1', 'rename-old', 'stale'); h.page.leaseTaskRenameChanged(old, 'task-2', 'rename-new', 'stale');
    assert.equal(h.actions('rename-capture').length, 0); h.page.leaseTaskRenameChanged(old, 'task-1', 'rename-new', 'accepted'); assert.equal(h.actions('rename-capture').length, 1);
    h.page.editorValues.title.selection_base = 1; h.page.editorValues.title.selection_extent = 1; h.page.draftSelectionPending.add('title'); h.page.restoreSelection('title');
    h.revoke(); h.page.remountEditorView(h.parent); h.mount(); for (const callback of h.timers.values()) callback();
    assert.equal(h.actions('restore-caret').length, 0);
  } finally { h.close(); }
});
