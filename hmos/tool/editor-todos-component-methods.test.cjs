'use strict';
const { test } = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), crypto = require('node:crypto');
const ts = require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const { load, createPolicy, deferred } = require('./editor-field-test-harness.cjs');
const draft = load('EditorDraft'), model = load('EditorTodos');
const sourcePath = path.resolve(__dirname, '../entry/src/main/ets/pages/EditorTodos.ets');
const source = fs.readFileSync(sourcePath, 'utf8').replace(/\r\n/g, '\n');
const childStart = source.indexOf('\n@Component\nstruct EditorTodoRowInput');
const parentEnd = source.indexOf('\n  @Builder\n');
const childEnd = source.indexOf('\n  @Builder\n', childStart);
// Execute actual component fields and methods. ArkUI decorators/struct spelling
// and declarative builders are removed, not owner/IME/controller implementation.
const methods = (source.slice(0, parentEnd) + '\n}\n' + source.slice(childStart, childEnd) + '\n}\nexport { EditorTodoRowInput };')
  .replace(/@Component\n/g, '').replace('export struct ', 'export class ').replace('struct EditorTodoRowInput', 'class EditorTodoRowInput')
  .replace(/@(?:Prop|State)(?:\s+@Watch\('[^']+'\))?\s*/g, '');
class Controller {
  constructor() { this.selections = []; this.stopped = 0; }
  setTextSelection(base, extent) { this.selections.push([base, extent]); }
  stopEditing() { this.stopped++; }
}
const compiled = ts.transpileModule(methods, { reportDiagnostics: true,
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
assert.deepEqual((compiled.diagnostics || []).filter(x => x.category === ts.DiagnosticCategory.Error), []);
const exportsObject = {};
vm.runInNewContext(compiled.outputText, { exports: exportsObject, setTimeout, clearTimeout, encodeURIComponent,
  TextAreaController: Controller, require(name) {
    if (name === '../model/EditorDraft') return draft;
    if (name === '../model/EditorTodos') return model;
    if (name === '../model/UiStrings') return { uiText: text => text };
    throw new Error('Unexpected dependency: ' + name);
  } });
const { EditorTodos, EditorTodoRowInput } = exportsObject;
const tick = () => new Promise(resolve => setTimeout(resolve, 5));
const waitForFocus = async condition => {
  for (let n = 0; n < 120 && !condition(); n++) await tick();
  assert.ok(condition(), 'bounded focus completion');
};
function text(value, patch = {}) { return Object.assign(new draft.TextValue(), { text: value }, patch); }
function fixture(initial = 'a\nb', options = {}) {
  const view = new EditorTodos(), policy = createPolicy().policy, captures = [], statuses = [], focuses = [], reveals = [], requestedFocus = [];
  let owner = 'owner-A', revision = 0, raw = text(initial);
  view.ownerKey = owner; view.revision = revision; view.source = draft.copyText(raw);
  view.onCapture = (value, capturedOwner) => {
    if (capturedOwner !== owner) return undefined;
    raw = draft.copyText(value); captures.push(raw); revision++;
    if (!options.deferProps) { view.revision = revision; view.source = draft.copyText(raw); }
    return revision;
  };
  view.isCurrent = (capturedOwner, capturedRevision, value) => owner === capturedOwner && revision === capturedRevision && JSON.stringify(raw) === JSON.stringify(value);
  view.count = async (value, guard) => (await policy.checkField('todos', value, guard)).grapheme_count;
  view.format = options.format || (async (_, value) => ({ action: 'accepted', value: draft.copyText(value) }));
  view.onStatus = status => statuses.push(JSON.parse(JSON.stringify(status)));  view.onRowFocused = (...args) => focuses.push(args); view.onRevealRow = (...args) => reveals.push(args);
  view.getUIContext = () => ({ getFocusController: () => ({ requestFocus: element => requestedFocus.push(element) }) });
  view.aboutToAppear();
  return { view, captures, statuses, focuses, reveals, requestedFocus, get raw() { return raw; }, get revision() { return revision; }, get owner() { return owner; },
    ticket(index = 0) { return view.ticket(view.state.rows[index]); },
    echo() { view.sourceChanged(); },
    deliver(prop) {
      if (prop === 'ownerKey') view.ownerKey = owner;
      else if (prop === 'revision') view.revision = revision;
      else if (prop === 'source') view.source = draft.copyText(raw);
      else throw new Error('Unsupported prop delivery: ' + prop);
      view.sourceChanged();
    },
    external(value, newOwner = owner) { owner = newOwner; raw = draft.copyText(value); revision++; },
    switchOwner(newOwner = 'owner-B', value = text('x\ny')) {
      owner = newOwner; raw = value; view.ownerKey = owner; view.source = draft.copyText(raw); view.revision = ++revision; view.sourceChanged();
    }
  };
}
function rowFixture() {
  const view = new EditorTodoRowInput(), inputs = [], selections = [], focuses = [];
  view.owner = 'owner-A'; view.row = Object.assign(new model.EditorTodoRow(), { id: 'row_A', incarnation: 1, display_text: 'abc', value: text('abc') });
  view.onInput = (...args) => { inputs.push(args); return Promise.resolve(); };
  view.onSelection = (...args) => { selections.push(args); return Promise.resolve(); };
  view.onRowFocus = ticket => focuses.push(ticket); view.aboutToAppear();
  return { view, inputs, selections, focuses, ticket: view.session };
}

test('actual component methods source identity; builders retain full rows and no platform maxLength', t => {
  t.diagnostic('pages/EditorTodos.ets normalized SHA256 ' + crypto.createHash('sha256').update(source).digest('hex'));
  assert.equal(typeof EditorTodos, 'function'); assert.equal(typeof EditorTodoRowInput, 'function');
  assert.match(source, /ForEach\(this\.state\.rows/); assert.doesNotMatch(source, /\.maxLength\(/);
  assert.match(source, /\.enablePreviewText\(true\)/); assert.doesNotMatch(source, /pendingtask_add|tasksAdd\(/);
});

test('actual parent captures whole raw field and maps per-row selection', async () => {
  const f = fixture('😀\nnext'); await f.view.inputFor(f.ticket(1), 'new');
  assert.equal(f.raw.text, '😀\nnew'); await f.view.selectionFor(f.ticket(1), 2, 2);
  assert.equal(f.raw.selection_base, 5); f.view.focusFor(f.ticket(1)); assert.equal(f.focuses[0][1], f.ticket(1).id);
});

test('preview text is forwarded unchanged to model and raw composing offsets', async () => {
  const f = fixture('a\nb'); await f.view.inputFor(f.ticket(1), 'b', { value: '候😀', offset: 0 });
  assert.equal(f.raw.text, 'a\n候😀b'); assert.equal(f.raw.composing_start, 2); assert.equal(f.raw.composing_end, 5);
});

test('successful Add reveals the exact owned element before its guarded focus callback', async () => {
  const f = fixture('a'); await f.view.addRow();
  const ticket = f.ticket(1); f.view.areaFor(ticket, { globalPosition: { y: 100 }, height: 80 });
  await waitForFocus(() => f.requestedFocus.length > 0);
  assert.equal(f.raw.text, 'a\n'); assert.equal(f.reveals.length, 1); assert.equal(f.requestedFocus[0], f.reveals[0][2]);
});

test('manual row focus supersedes a pending Add focus without losing the added row', async () => {
  const f = fixture('a'); await f.view.addRow(); const added = f.ticket(1);
  f.view.areaFor(added, { globalPosition: { y: 100 }, height: 80 });
  f.view.focusFor(f.ticket()); f.view.focusAddedRow(); await tick();
  assert.equal(f.requestedFocus.length, 0); assert.equal(f.view.focusTicket, undefined);
  assert.equal(f.raw.text, 'a\n'); assert.equal(f.view.state.focused_id, f.ticket().id);
});

test('external field focus revokes pending Add even before its prop watcher runs', async () => {
  const f = fixture('a'); await f.view.addRow(); const added = f.ticket(1);
  f.view.areaFor(added, { globalPosition: { y: 100 }, height: 80 });
  f.view.focusIntent++; f.view.focusAddedRow(); await tick();
  assert.equal(f.requestedFocus.length, 0); assert.equal(f.view.focusTicket, undefined);
  assert.equal(f.raw.text, 'a\n');
});

test('user focus during awaited Add prevents a late focus request while retaining the row', async () => {
  const f = fixture('a'), wait = deferred(); await tick(); f.view.model.countCache.clear();
  f.view.count = async () => wait.promise;
  const adding = f.view.addRow(); await tick(); f.view.focusFor(f.ticket());
  wait.resolve(1); await adding;
  assert.equal(f.view.state.rows.length, 2); assert.equal(f.raw.text, 'a\n');
  assert.equal(f.view.focusTicket, undefined); assert.equal(f.requestedFocus.length, 0);
});

test('old row incarnation area notification cannot delete the current measured area', async () => {
  const f = fixture('a'), old = f.ticket();
  f.view.editingEnabled = false; f.view.availabilityChanged();
  f.view.editingEnabled = true; f.view.availabilityChanged();
  const current = f.ticket(); assert.notEqual(current.incarnation, old.incarnation);
  f.view.areaFor(current, { globalPosition: { y: 100 }, height: 80 });
  f.view.areaFor(old, { globalPosition: { y: 200 }, height: 0 });
  assert.equal(f.view.areas.get(current.id).incarnation, current.incarnation);
  assert.equal(f.view.areas.get(current.id).height, 80);
});

test('new empty row waits for its measured incarnation before reveal/focus and retains the raw echo', async () => {
  const f = fixture(''); await f.view.addRow(); f.echo();
  f.view.focusAddedRow(); assert.equal(f.reveals.length, 0); assert.equal(f.requestedFocus.length, 0);
  assert.equal(f.raw.text, ''); assert.equal(f.view.state.rows.length, 1);
  const ticket = f.ticket(); f.view.areaFor(ticket, { globalPosition: { y: 200 }, height: 80 });
  await waitForFocus(() => f.requestedFocus.length > 0);
  assert.equal(f.requestedFocus[0], f.view.element(ticket.id, ticket.owner));
});

test('API26 invisible focus error is contained and the same mounted row can become focusable later', async () => {
  const f = fixture('a'); let attempts = 0;
  f.view.getUIContext = () => ({ getFocusController: () => ({ requestFocus(element) {
    if (++attempts <= 2) throw Object.assign(new Error('invisible'), { code: 150003 });
    f.requestedFocus.push(element);
  } }) });
  await f.view.addRow(); const ticket = f.ticket(1);
  f.view.areaFor(ticket, { globalPosition: { y: 200 }, height: 80 });
  await waitForFocus(() => f.requestedFocus.length === 1);
  assert.equal(attempts, 3); assert.equal(f.raw.text, 'a\n'); assert.equal(f.view.state.rows.length, 2);
  assert.equal(f.view.focusTicket, undefined);
});

test('permanently unavailable focus remains bounded without losing or deleting the added row', async () => {
  const f = fixture('a'); let attempts = 0;
  f.view.getUIContext = () => ({ getFocusController: () => ({ requestFocus() { attempts++; throw new Error('disabled'); } }) });
  await f.view.addRow(); const ticket = f.ticket(1); f.view.areaFor(ticket, { globalPosition: { y: 200 }, height: 80 });
  for (let n = 0; n < 15; n++) f.view.focusAddedRow();
  assert.equal(f.view.focusTicket, undefined); assert.ok(attempts > 0 && attempts <= 12);
  const finished = attempts; await tick(); assert.equal(attempts, finished);
  assert.equal(f.raw.text, 'a\n'); assert.equal(f.view.state.rows.length, 2);
});

test('disable and disappear revoke pending mounted-row focus before an old callback can request it', async () => {
  for (const stop of ['disable', 'disappear']) {
    const f = fixture('a'); await f.view.addRow(); const ticket = f.ticket(1);
    f.view.areaFor(ticket, { globalPosition: { y: 200 }, height: 80 });
    if (stop === 'disable') { f.view.editingEnabled = false; f.view.availabilityChanged(); }
    else f.view.aboutToDisappear();
    f.view.focusAddedRow(); await tick(); assert.equal(f.requestedFocus.length, 0);
    assert.equal(f.view.focusTicket, undefined); assert.equal(f.raw.text, 'a\n');
  }
});

test('owner switch between Add and queued focus cancels old-card focus', async () => {
  const f = fixture('a'); await f.view.addRow(); f.switchOwner(); await tick();
  assert.equal(f.requestedFocus.length, 0); assert.equal(f.raw.text, 'x\ny');
});

test('parent disable/disappear blocks editing and late row callbacks without borrowing a new owner', async () => {
  const f = fixture(), ticket = f.ticket(); f.view.editingEnabled = false; f.view.availabilityChanged();
  await f.view.inputFor(ticket, 'late'); assert.equal(f.raw.text, 'a\nb');
  f.view.editingEnabled = true; f.view.availabilityChanged(); f.view.aboutToDisappear();
  await f.view.inputFor(ticket, 'late-again'); assert.equal(f.raw.text, 'a\nb');
});

test('parent status distinguishes complete raw capture and pending formatting before exact reply', async () => {
  const wait = deferred(), f = fixture('a', { format: () => wait.promise });
  const running = f.view.inputFor(f.ticket(), 'candidate'); await tick();
  assert.ok(f.statuses.some(status => status.raw_capture_complete && !status.business_ready && status.format_pending));
  wait.resolve({ action: 'accepted', value: text('candidate') }); await running;
  assert.equal(f.statuses.at(-1).raw_capture_complete, true); assert.equal(f.statuses.at(-1).business_ready, true); assert.equal(f.statuses.at(-1).format_pending, false);
});

test('readonly count failure keeps full rows visible with unconfirmed counter', async () => {
  const f = fixture(); f.view.count = async () => { throw new Error('count unknown'); };
  f.switchOwner('new', text('full\nsource')); await tick();
  assert.equal(f.view.state.rows.length, 2); assert.equal(f.view.state.count, -1); assert.equal(f.raw.text, 'full\nsource');
});

test('drop uses matching private drag token and measured window geometry to insert after target', () => {
  const f = fixture('a\nb\nc'), first = f.ticket(), target = f.ticket(2);
  f.view.areaFor(target, { globalPosition: { y: 100 }, height: 80 }); f.view.startDrag(first);
  f.view.dropFor(target, { getWindowY: () => 145 }); assert.equal(f.raw.text, 'b\nc\na');
});

test('drop before midpoint preserves moved row identity', () => {
  const f = fixture('a\nb\nc'), sourceTicket = f.ticket(2), target = f.ticket();
  f.view.areaFor(target, { globalPosition: { y: 100 }, height: 80 }); f.view.startDrag(sourceTicket);
  f.view.dropFor(target, { getWindowY: () => 110 }); assert.equal(f.raw.text, 'c\na\nb');
  assert.equal(f.view.state.rows[0].id, sourceTicket.id);
});

test('foreign drop, malformed geometry and drag-end leave raw unchanged', () => {
  const f = fixture(), target = f.ticket(1);
  f.view.areaFor(target, { globalPosition: { y: 100 }, height: 80 }); f.view.dropFor(target, { getWindowY: () => 200 });
  assert.equal(f.raw.text, 'a\nb'); f.view.startDrag(f.ticket());
  f.view.areaFor(target, { globalPosition: { y: '100vp' }, height: 80 }); f.view.dropFor(target, { getWindowY: () => 200 });
  assert.equal(f.raw.text, 'a\nb'); f.view.startDrag(f.ticket()); f.view.finishDrag();
  f.view.dropFor(target, { getWindowY: () => 200 }); assert.equal(f.raw.text, 'a\nb');
});

test('drag frozen before selection change cannot reorder on late drop', async () => {
  const f = fixture(), target = f.ticket(1); f.view.areaFor(target, { globalPosition: { y: 100 }, height: 80 });
  f.view.startDrag(f.ticket()); await f.view.selectionFor(f.ticket(), 1, 1); f.view.dropFor(target, { getWindowY: () => 200 });
  assert.equal(f.raw.text, 'a\nb');
});

test('drag-position callback forwards only a current drag and cancels a stale revision', async () => {
  const f = fixture(), positions = []; f.view.onDragPosition = (...args) => positions.push(args);
  f.view.startDrag(f.ticket()); f.view.dragMoved({ getWindowY: () => 120 });
  assert.equal(positions[0][1], 120); assert.equal(positions[0][2], true);
  await f.view.selectionFor(f.ticket(), 1, 1); f.view.dragMoved({ getWindowY: () => 200 });
  assert.equal(positions.at(-1)[2], false); assert.equal(positions.filter(x => x[2]).length, 1);
});

test('actual child forwards committed string plus exact SDK preview separately', () => {
  const f = rowFixture(); f.view.changeFor(f.ticket, 'ab', { value: '候😀', offset: 1 });
  assert.equal(f.inputs.length, 1); assert.equal(f.inputs[0][1], 'ab'); assert.equal(f.inputs[0][2].value, '候😀');
  assert.equal(f.view.text, 'ab');
});

test('child selection only captures while focused and preserves reverse UTF16 order', () => {
  const f = rowFixture(); f.view.selectionFor(f.ticket, 2, 0); assert.equal(f.selections.length, 0);
  f.view.focusFor(f.ticket, true); f.view.selectionFor(f.ticket, 2, 0);
  assert.equal(f.selections[0][1], 2); assert.equal(f.selections[0][2], 0);
});

test('child controllers are stable through refresh and restore confirmed selection after a frame', async () => {
  const f = rowFixture(), controller = f.view.controller; f.view.focusFor(f.ticket, true);
  f.view.row.value = text('abc', { selection_base: 2, selection_extent: 0 }); f.view.rowChanged(); await tick();
  assert.equal(f.view.controller, controller); assert.deepEqual(controller.selections, [[2, 0]]);
  f.view.selectionFor(f.view.session, 2, 0); assert.equal(f.selections.length, 0);
});

test('child does not attempt arbitrary composition restore or confirmed cursor during pending', async () => {
  const f = rowFixture(); f.view.focusFor(f.ticket, true);
  f.view.row.value = text('a候bc', { selection_base: 2, selection_extent: 2, composing_start: 1, composing_end: 2 });
  f.view.rowChanged(); await tick(); assert.equal(f.view.controller.selections.length, 0);
  f.view.row.pending = true; f.view.row.value = text('abc', { selection_base: 1, selection_extent: 1 });
  f.view.rowChanged(); await tick(); assert.equal(f.view.controller.selections.length, 0);
});

test('child old incarnation callbacks and destroyed controller queued selection are revoked', async () => {
  const f = rowFixture(); f.view.row.incarnation = 2; f.view.rowChanged();
  f.view.changeFor(f.ticket, 'old'); assert.equal(f.inputs.length, 0);
  f.view.focusFor(f.view.session, true); f.view.row.value = text('abc', { selection_base: 1, selection_extent: 1 });
  f.view.rowChanged(); f.view.aboutToDisappear(); await tick();
  assert.equal(f.view.controller.selections.length, 0); assert.equal(f.view.controller.stopped, 1);
});

test('same local row ID on another owner cannot admit old callbacks', () => {
  const f = rowFixture(); f.view.owner = 'owner-B'; f.view.rowChanged(); f.view.changeFor(f.ticket, 'borrowed');
  assert.equal(f.inputs.length, 0);
});

test('actual builder uses SDK adaptive1..3 SCROLL lines with full source and Flutter row geometry', () => {
  assert.match(source, /\.minLines\(1\)\.maxLines\(3,\s*\{\s*overflowMode:\s*MaxLinesMode\.SCROLL\s*\}\)\.textOverflow\(TextOverflow\.None\)/);
  assert.match(source, /\.lineHeight\('23\.1fp'\)/); assert.doesNotMatch(source, /\.height\(76\)|onContentSizeChange|\.maxLength\(/);
  assert.match(source, /fontSize\(11\).*fontWeight\(600\)/);
  assert.match(source, /\.width\(17\)\.height\(17\)/); assert.match(source, /\.height\(40\)\.width\(40\)/);
  assert.match(source, /\.width\(19\)\.height\(19\)/); assert.match(source, /\.height\(40\)\.width\(36\)/);
  assert.match(source, /fill\(this\.controlInk \|\| this\.muted\)/); assert.match(source, /fill\(this\.muted\)/);
  assert.match(source, /borderWidth\(\{ bottom: this\.focused \? 1 : 0 \}\)/);
});

test('parent Unknown status keeps complete raw retain permission and separately blocks business readiness', async () => {
  const f = fixture('a', { format: async () => { throw new Error('Unknown'); } });
  await f.view.inputFor(f.ticket(), 'full raw 😀'); const status = f.statuses.at(-1);
  assert.equal(status.owner, f.view.ownerKey); assert.equal(status.revision, f.view.revision);
  assert.equal(JSON.stringify(status.value), JSON.stringify(f.raw)); assert.equal(status.raw_capture_complete, true);
  assert.equal(status.business_ready, false); assert.equal(status.format_pending, false);
  assert.equal(status.capture_error, ''); assert.match(status.format_error, /Unknown/);
});

test('valid preview reports complete captured composition rather than formatter failure', async () => {
  const f = fixture('a'); await f.view.inputFor(f.ticket(), 'a', { value: '候😀', offset: 1 });
  const status = f.statuses.at(-1); assert.equal(status.value.text, 'a候😀');
  assert.equal(status.value.composing_start, 1); assert.equal(status.value.composing_end, 4);
  assert.equal(status.raw_capture_complete, true); assert.equal(status.business_ready, false);
  assert.equal(status.format_pending, false); assert.equal(status.format_error, '');
});

test('unlocatable preview reports capture incomplete and preserves exact old raw identity', async () => {
  const f = fixture('old'); await f.view.inputFor(f.ticket(), 'new', { value: '候', offset: 99 });
  const status = f.statuses.at(-1); assert.equal(status.raw_capture_complete, false); assert.equal(status.business_ready, false);
  assert.equal(status.value.text, 'old'); assert.match(status.capture_error, /位置/);
  assert.equal(JSON.stringify(status.value), JSON.stringify(f.raw));
});

test('invalid new-owner source status does not borrow previous owner row state', () => {
  const f = fixture('old'); f.switchOwner('new-owner', text('new raw', { selection_base: 99 }));
  const status = f.statuses.at(-1); assert.equal(status.owner, 'new-owner'); assert.equal(status.revision, f.view.revision);
  assert.equal(status.value.text, 'new raw'); assert.equal(status.raw_capture_complete, false);
  assert.equal(status.business_ready, false); assert.match(status.capture_error, /位置/);
});

test('failed nonmutating Add count does not misreport the current full raw as capture-incomplete', async () => {
  const f = fixture('a'); await tick(); f.view.model.countCache.clear();
  f.view.count = async () => { throw new Error('AddCountUnknown'); };
  await f.view.addRow(); assert.equal(f.raw.text, 'a'); assert.equal(f.view.state.rows.length, 1);
  assert.equal(f.view.model.status().raw_capture_complete, true); assert.equal(f.view.model.status().business_ready, true);
  assert.equal(f.statuses.at(-1).raw_capture_complete, true); assert.match(f.view.state.error, /AddCountUnknown/);
});

test('disable pending preserves raw retain permission but cannot turn canceled format into a confirmed receipt', async () => {
  const wait = deferred(), f = fixture('a', { format: () => wait.promise });
  const running = f.view.inputFor(f.ticket(), 'complete pending raw'); await tick();
  f.view.editingEnabled = false; f.view.availabilityChanged();
  assert.equal(f.statuses.at(-1).raw_capture_complete, true); assert.equal(f.statuses.at(-1).business_ready, false);
  f.view.editingEnabled = true; f.view.availabilityChanged();
  wait.resolve({ action: 'accepted', value: text('late') }); await running;
  assert.equal(f.raw.text, 'complete pending raw'); assert.equal(f.statuses.at(-1).business_ready, false);
  assert.equal(f.statuses.at(-1).value.text, 'complete pending raw');
});

test('disabled same-owner parent input and selection notify complete events without raw mutation or formatting', async () => {
  const formats = [], notifications = [], f = fixture('a\nb', { format: async (...args) => { formats.push(args); return { action: 'accepted', value: args[1] }; } });
  const ticket = f.ticket(1), original = JSON.stringify(f.raw), preview = { value: '候😀\r\n', offset: 9 };
  f.view.onUncaptured = (...args) => notifications.push(args); f.view.editingEnabled = false; f.view.availabilityChanged();
  await f.view.inputFor(ticket, 'whole\nraw😀', preview); await f.view.selectionFor(ticket, 7, 2);
  assert.equal(notifications.length, 2); assert.equal(notifications[0][0], 'owner-A'); assert.equal(notifications[0][1], ticket.id);
  assert.deepEqual(JSON.parse(notifications[0][2]), { kind: 'input', text: 'whole\nraw😀', preview });
  assert.deepEqual(JSON.parse(notifications[1][2]), { kind: 'selection', base: 7, extent: 2 });
  assert.equal(JSON.stringify(f.raw), original); assert.equal(f.captures.length, 0); assert.equal(formats.length, 0);
});

test('disabled current parent ticket can notify after epoch preflight is invalidated but old identity never does', async () => {
  const notifications = [], f = fixture('a'); const ticket = f.ticket();
  f.view.onUncaptured = (...args) => notifications.push(args); f.view.editingEnabled = false; f.view.availabilityChanged();
  const current = f.view.isCurrent;
  f.view.isCurrent = () => false;
  await f.view.inputFor(ticket, 'unconfirmed'); assert.equal(notifications.length, 1);
  f.view.isCurrent = current;
  f.view.editingEnabled = true; f.view.availabilityChanged(); f.view.editingEnabled = false; f.view.availabilityChanged();
  await f.view.selectionFor(ticket, 0, 0); assert.equal(notifications.length, 1);
  f.switchOwner(); await f.view.inputFor(ticket, 'foreign'); await f.view.selectionFor(ticket, 1, 1);
  assert.equal(notifications.length, 1); f.view.aboutToDisappear();
  await f.view.inputFor(f.ticket(), 'destroyed'); assert.equal(notifications.length, 1);
});

test('disabled actual child input/selection forward raw events without changing its text or invoking normal capture', () => {
  const notifications = [], f = rowFixture(); f.view.onUncaptured = (...args) => notifications.push(args);
  f.view.focusFor(f.ticket, true); f.view.editingEnabled = false;
  const original = f.view.text, preview = { value: '候😀', offset: 1 };
  f.view.changeFor(f.ticket, 'unconfirmed raw', preview); f.view.selectionFor(f.ticket, 2, 0);
  assert.equal(notifications.length, 2); assert.equal(notifications[0][0], f.ticket);
  assert.deepEqual(JSON.parse(notifications[0][1]), { kind: 'input', text: 'unconfirmed raw', preview });
  assert.deepEqual(JSON.parse(notifications[1][1]), { kind: 'selection', base: 2, extent: 0 });
  assert.equal(f.view.text, original); assert.equal(f.inputs.length, 0); assert.equal(f.selections.length, 0);
});

test('disabled child refuses old owner/incarnation/disposed callbacks and current child notification reaches parent', () => {
  const notifications = [], parent = fixture('a'), child = new EditorTodoRowInput();
  child.owner = parent.view.ownerKey; child.row = parent.view.state.rows[0]; child.editingEnabled = false;
  parent.view.editingEnabled = false; parent.view.availabilityChanged();
  parent.view.onUncaptured = (...args) => notifications.push(args);
  child.onUncaptured = (ticket, event) => parent.view.uncapturedFor(ticket, event); child.aboutToAppear();
  const ticket = child.session; child.changeFor(ticket, 'same owner'); assert.equal(notifications.length, 1);
  child.row = Object.assign(new model.EditorTodoRow(), child.row, { incarnation: ticket.incarnation + 1 }); child.rowChanged();
  child.changeFor(ticket, 'old incarnation'); child.selectionFor(ticket, 1, 1); assert.equal(notifications.length, 1);
  child.owner = 'foreign-owner'; child.rowChanged(); child.changeFor(ticket, 'old owner'); assert.equal(notifications.length, 1);
  child.aboutToDisappear(); child.changeFor(child.session, 'destroyed'); assert.equal(notifications.length, 1);
  assert.equal(parent.captures.length, 0); assert.equal(parent.raw.text, 'a');
});

test('restored disabled unfocused row initialization does not report a missing raw selection', async () => {
  const parent = fixture('first 汉字 🧪 é.'), child = new EditorTodoRowInput(), notifications = [];
  const original = JSON.stringify(parent.raw);
  parent.view.onUncaptured = (...args) => notifications.push(args);
  parent.view.editingEnabled = false; parent.view.availabilityChanged();
  child.owner = parent.view.ownerKey; child.row = parent.view.state.rows[0]; child.editingEnabled = false;
  child.onUncaptured = (ticket, event) => parent.view.uncapturedFor(ticket, event); child.aboutToAppear();
  child.selectionFor(child.session, 0, 0); child.selectionFor(child.session, -1, -1);
  assert.equal(notifications.length, 0); assert.equal(parent.captures.length, 0);
  assert.equal(JSON.stringify(parent.raw), original); assert.equal(parent.statuses.at(-1).raw_capture_complete, true);
  parent.view.editingEnabled = true; parent.view.availabilityChanged();
  child.row = parent.view.state.rows[0]; child.editingEnabled = true; child.rowChanged();
  child.selectionFor(child.session, 0, 0); await tick();
  assert.equal(notifications.length, 0); assert.equal(parent.captures.length, 0);
  assert.equal(JSON.stringify(parent.raw), original); assert.equal(parent.statuses.at(-1).raw_capture_complete, true);
  parent.view.aboutToDisappear(); child.aboutToDisappear();
});

test('already issued controller restoration echo remains identifiable after a focused row is disabled', async () => {
  const f = rowFixture(), notifications = []; f.view.onUncaptured = (...args) => notifications.push(args);
  f.view.focusFor(f.ticket, true); f.view.row.value = text('abc', { selection_base: 2, selection_extent: 0 });
  f.view.rowChanged(); await tick(); assert.deepEqual(f.view.controller.selections, [[2, 0]]);
  f.view.editingEnabled = false; f.view.selectionFor(f.view.session, 2, 0);
  assert.equal(notifications.length, 0); assert.equal(f.selections.length, 0);
  f.view.selectionFor(f.view.session, 0, 2);
  assert.equal(notifications.length, 1); assert.deepEqual(JSON.parse(notifications[0][1]), { kind: 'selection', base: 0, extent: 2 });
  assert.equal(f.view.text, 'abc'); assert.equal(f.inputs.length, 0); assert.equal(f.selections.length, 0);
});

test('disabled blur revokes current focus after a delivered genuine selection, while all text input remains observable', () => {
  const f = rowFixture(), notifications = []; f.view.onUncaptured = (...args) => notifications.push(args);
  f.view.focusFor(f.ticket, true); f.view.editingEnabled = false; f.view.selectionFor(f.ticket, 2, 0);
  assert.equal(notifications.length, 1); assert.equal(f.view.focused, true);
  f.view.focusFor(f.ticket, false); assert.equal(f.view.focused, false);
  f.view.selectionFor(f.ticket, 0, 0); assert.equal(notifications.length, 1);
  const preview = { value: '候😀', offset: 99 }; f.view.changeFor(f.ticket, 'unconfirmed raw', preview);
  assert.equal(notifications.length, 2); assert.deepEqual(JSON.parse(notifications[1][1]), { kind: 'input', text: 'unconfirmed raw', preview });
  assert.equal(f.view.text, 'abc'); assert.equal(f.inputs.length, 0); assert.equal(f.selections.length, 0);
});

test('disabled focused invalid or absent-selection positions are retained, never mistaken for an unissued restoration', () => {
  const f = rowFixture(), notifications = []; f.view.onUncaptured = (...args) => notifications.push(args);
  f.view.focusFor(f.ticket, true); f.view.editingEnabled = false;
  f.view.selectionFor(f.ticket, -1, -1); f.view.selectionFor(f.ticket, 99, -2);
  assert.deepEqual(notifications.map(row => JSON.parse(row[1])), [
    { kind: 'selection', base: -1, extent: -1 }, { kind: 'selection', base: 99, extent: -2 }
  ]);
  assert.equal(f.view.controller.selections.length, 0); assert.equal(f.inputs.length, 0); assert.equal(f.selections.length, 0);
});

test('old row identity cannot consume a new controller echo or blur the new incarnation', async () => {
  const f = rowFixture(), notifications = []; f.view.onUncaptured = (...args) => notifications.push(args);
  f.view.focusFor(f.ticket, true); f.view.row.incarnation = 2;
  f.view.row.value = text('abc', { selection_base: 2, selection_extent: 0 }); f.view.rowChanged(); await tick();
  f.view.editingEnabled = false; f.view.selectionFor(f.ticket, 2, 0); f.view.focusFor(f.ticket, false);
  assert.equal(notifications.length, 0); assert.equal(f.view.focused, true); assert.equal(f.view.expectedBase, 2);
  f.view.selectionFor(f.view.session, 2, 0); assert.equal(f.view.expectedBase, -1); assert.equal(notifications.length, 0);
  f.view.selectionFor(f.view.session, 0, 2); assert.equal(notifications.length, 1);
});

test('refreshing a pending row invalidates the previous incarnation controller restoration marker', async () => {
  const f = rowFixture(), notifications = []; f.view.onUncaptured = (...args) => notifications.push(args);
  f.view.focusFor(f.ticket, true); f.view.row.value = text('abc', { selection_base: 2, selection_extent: 0 });
  f.view.rowChanged(); await tick(); assert.equal(f.view.expectedBase, 2);
  f.view.row.incarnation = 2; f.view.row.pending = true; f.view.rowChanged();
  assert.equal(f.view.expectedBase, -1); f.view.editingEnabled = false; f.view.selectionFor(f.view.session, 2, 0);
  assert.equal(notifications.length, 1); assert.deepEqual(JSON.parse(notifications[0][1]), { kind: 'selection', base: 2, extent: 0 });
  assert.equal(f.view.controller.selections.length, 1); assert.equal(f.selections.length, 0);
});

for (const order of [['revision', 'source'], ['source', 'revision']]) {
  test('actual per-prop ' + order.join('-first-') + ' self echo preserves changed-text pending formatter and exact ACK', async () => {
    const wait = deferred(), formats = [], f = fixture('old', { deferProps: true, format: (oldValue, newValue, remaining, owned) => {
      formats.push({ oldValue: draft.copyText(oldValue), newValue: draft.copyText(newValue), remaining, owned }); return wait.promise;
    } });
    const original = f.ticket(), running = f.view.inputFor(original, 'whole candidate 😀'); await tick();
    assert.equal(formats.length, 1); assert.equal(f.view.revision, 0); assert.equal(f.view.source.text, 'old');
    assert.equal(f.view.state.value.text, 'whole candidate 😀'); const statusCount = f.statuses.length;
    f.deliver(order[0]);
    assert.equal(f.statuses.length, statusCount); assert.equal(f.ticket().incarnation, original.incarnation);
    assert.equal(f.view.model.status().format_pending, true); assert.equal(f.view.model.status().business_ready, false);
    assert.equal(formats[0].owned(), true); assert.equal(f.view.model.status().format_error, '');
    f.deliver(order[1]);
    assert.equal(f.ticket().incarnation, original.incarnation); assert.equal(f.view.model.status().format_pending, true);
    assert.equal(f.view.model.status().business_ready, false); assert.equal(formats[0].owned(), true);
    assert.equal(f.captures.length, 1); assert.equal(JSON.stringify(f.view.state.value), JSON.stringify(f.raw));
    wait.resolve({ action: 'accepted', value: draft.copyText(formats[0].newValue) }); await running;
    assert.equal(f.captures.length, 2); assert.equal(f.raw.text, 'whole candidate 😀');
    assert.equal(f.view.model.status().format_pending, false); assert.equal(f.view.model.status().business_ready, true);
    assert.equal(f.view.model.status().format_error, ''); f.view.aboutToDisappear();
  });
}

test('actual per-prop self selection echo preserves latest same-text formatter, reverse positions and stale-reply fence', async () => {
  const waits = [deferred(), deferred()], formats = [], f = fixture('abc', { deferProps: true, format: (oldValue, newValue, remaining, owned) => {
    const index = formats.length; formats.push({ oldValue: draft.copyText(oldValue), newValue: draft.copyText(newValue), remaining, owned }); return waits[index].promise;
  } });
  f.external(text('abc', { selection_base: 0, selection_extent: 0 })); f.deliver('revision'); f.deliver('source');
  const original = f.ticket(), input = f.view.inputFor(original, 'abc'); await tick();
  const selecting = f.view.selectionFor(original, 2, 0); await tick();
  assert.equal(formats.length, 2); assert.equal(f.raw.text, f.view.source.text);
  assert.equal(f.view.source.selection_base, 0); assert.equal(f.raw.selection_base, 2); assert.equal(f.raw.selection_extent, 0);
  const statusCount = f.statuses.length; f.deliver('revision');
  assert.equal(f.statuses.length, statusCount); assert.equal(f.ticket().incarnation, original.incarnation);
  assert.equal(f.view.model.status().format_pending, true); assert.equal(f.view.model.status().business_ready, false);
  assert.equal(f.view.model.status().format_error, ''); f.deliver('source');
  assert.equal(f.ticket().incarnation, original.incarnation); assert.equal(formats[1].owned(), true); assert.equal(formats[0].owned(), false);
  waits[0].resolve({ action: 'accepted', value: draft.copyText(formats[0].newValue) }); await input;
  assert.equal(f.captures.length, 2); assert.equal(f.view.model.status().format_pending, true);
  assert.equal(f.view.model.status().business_ready, false); assert.equal(f.raw.selection_base, 2);
  waits[1].resolve({ action: 'accepted', value: draft.copyText(formats[1].newValue) }); await selecting;
  assert.equal(f.captures.length, 3); assert.equal(f.raw.selection_base, 2); assert.equal(f.raw.selection_extent, 0);
  assert.equal(f.view.model.status().business_ready, true); assert.equal(f.view.model.status().format_error, ''); f.view.aboutToDisappear();
});

test('actual per-prop self composition-commit echo keeps same-text full raw and confirms only the exact formatter ACK', async () => {
  const wait = deferred(), formats = [], f = fixture('abc', { deferProps: true, format: (oldValue, newValue, remaining, owned) => {
    formats.push({ oldValue: draft.copyText(oldValue), newValue: draft.copyText(newValue), remaining, owned }); return wait.promise;
  } });
  f.external(text('abc', { selection_base: 2, selection_extent: 2, composing_start: 0, composing_end: 2 }));
  f.deliver('revision'); f.deliver('source');
  const original = f.ticket(), running = f.view.inputFor(original, 'abc'); await tick();
  assert.equal(formats.length, 1); assert.equal(f.raw.text, f.view.source.text);
  assert.equal(f.view.source.composing_start, 0); assert.equal(f.raw.composing_start, -1);
  const statusCount = f.statuses.length; f.deliver('revision');
  assert.equal(f.statuses.length, statusCount); assert.equal(f.ticket().incarnation, original.incarnation);
  assert.equal(f.view.model.status().format_error, ''); assert.equal(f.view.model.status().business_ready, false);
  f.deliver('source'); assert.equal(f.ticket().incarnation, original.incarnation);
  assert.equal(f.view.model.status().format_pending, true); assert.equal(formats[0].owned(), true);
  wait.resolve({ action: 'accepted', value: draft.copyText(formats[0].newValue) }); await running;
  assert.equal(f.captures.length, 2); assert.equal(f.raw.composing_start, -1); assert.equal(f.raw.composing_end, -1);
  assert.equal(f.raw.selection_base, 2); assert.equal(f.view.model.status().business_ready, true);
  assert.equal(f.view.model.status().format_error, ''); f.view.aboutToDisappear();
});

test('actual full external replacement still cancels the older formatter after mixed props are deferred', async () => {
  const wait = deferred(), formats = [], f = fixture('old', { deferProps: true, format: (_old, value, _remaining, owned) => {
    formats.push({ value: draft.copyText(value), owned }); return wait.promise;
  } });
  const original = f.ticket(), running = f.view.inputFor(original, 'complete pending'); await tick();
  f.deliver('revision'); f.deliver('source'); assert.equal(f.view.model.status().format_pending, true);
  f.external(text('external complete value', { selection_base: 3, selection_extent: 1, directional: true }));
  const statusCount = f.statuses.length; f.deliver('revision');
  assert.equal(f.statuses.length, statusCount); assert.equal(f.ticket().incarnation, original.incarnation);
  assert.equal(f.view.model.status().format_pending, true); assert.equal(formats[0].owned(), false);
  f.deliver('source'); assert.notEqual(f.ticket().incarnation, original.incarnation);
  assert.equal(f.view.model.status().format_pending, false); assert.equal(JSON.stringify(f.view.state.value), JSON.stringify(f.raw));
  wait.resolve({ action: 'accepted', value: draft.copyText(formats[0].value) }); await running;
  assert.equal(f.captures.length, 1); assert.equal(f.raw.text, 'external complete value'); assert.equal(f.raw.selection_base, 3);
  await f.view.inputFor(original, 'borrowed old row'); assert.equal(f.captures.length, 1); f.view.aboutToDisappear();
});

test('actual new-owner prop delivery cannot clear old-owner rows or areas until its complete parent snapshot arrives', async () => {
  const f = fixture('old', { deferProps: true }), original = f.ticket();
  f.view.areaFor(original, { globalPosition: { y: 90 }, height: 40 });
  const focusEpoch = f.view.focusIntentEpoch, statusCount = f.statuses.length;
  f.external(text('new owner raw'), 'owner-B'); f.deliver('ownerKey');
  assert.equal(f.view.state.owner, 'owner-A'); assert.equal(f.view.areas.size, 1);
  assert.equal(f.view.focusIntentEpoch, focusEpoch); assert.equal(f.statuses.length, statusCount);
  await f.view.inputFor(original, 'old callback'); assert.equal(f.captures.length, 0);
  f.deliver('revision'); assert.equal(f.view.state.owner, 'owner-A'); assert.equal(f.view.areas.size, 1);
  f.deliver('source'); assert.equal(f.view.state.owner, 'owner-B'); assert.equal(f.view.areas.size, 0);
  assert.ok(f.view.focusIntentEpoch > focusEpoch); assert.equal(f.view.state.value.text, 'new owner raw');
  await f.view.selectionFor(original, 1, 0); assert.equal(f.captures.length, 0); f.view.aboutToDisappear();
});

test('actual disabled restore admits its complete parent snapshot without allowing editing or reusing an old row ticket', async () => {
  const notifications = [], f = fixture('old', { deferProps: true }), original = f.ticket();
  f.view.onUncaptured = (...args) => notifications.push(args);
  f.view.editingEnabled = false; f.view.availabilityChanged();
  const restored = text('restored 汉字 🧪 é.', { selection_base: 3, selection_extent: 1, directional: true });
  f.external(restored); const statusCount = f.statuses.length; f.deliver('revision');
  assert.equal(f.statuses.length, statusCount); assert.equal(f.view.state.value.text, 'old');
  f.deliver('source'); assert.equal(JSON.stringify(f.view.state.value), JSON.stringify(restored));
  assert.equal(f.view.model.status().raw_capture_complete, true); assert.equal(f.view.model.status().format_pending, false);
  assert.equal(f.view.model.active, false); assert.notEqual(f.ticket().incarnation, original.incarnation);
  await f.view.inputFor(original, 'old callback'); assert.equal(notifications.length, 0);
  await f.view.inputFor(f.ticket(), 'real disabled input'); assert.equal(notifications.length, 1);
  assert.deepEqual(JSON.parse(notifications[0][2]), { kind: 'input', text: 'real disabled input' });
  assert.equal(f.captures.length, 0); assert.equal(JSON.stringify(f.raw), JSON.stringify(restored));
  f.view.editingEnabled = true; f.view.availabilityChanged();
  assert.equal(f.view.model.active, true); assert.equal(JSON.stringify(f.view.state.value), JSON.stringify(restored));
  assert.equal(f.captures.length, 0); f.view.aboutToDisappear();
});
