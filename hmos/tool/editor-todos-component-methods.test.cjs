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
function text(value, patch = {}) { return Object.assign(new draft.TextValue(), { text: value }, patch); }
function fixture(initial = 'a\nb', options = {}) {
  const view = new EditorTodos(), policy = createPolicy().policy, captures = [], statuses = [], focuses = [], reveals = [], requestedFocus = [];
  let owner = 'owner-A', revision = 0, raw = text(initial);
  view.ownerKey = owner; view.revision = revision; view.source = draft.copyText(raw);
  view.onCapture = (value, capturedOwner) => {
    if (capturedOwner !== owner) return undefined;
    raw = draft.copyText(value); captures.push(raw); view.revision = ++revision; view.source = draft.copyText(raw); return revision;
  };
  view.isCurrent = (capturedOwner, capturedRevision, value) => owner === capturedOwner && revision === capturedRevision && JSON.stringify(raw) === JSON.stringify(value);
  view.count = async (value, guard) => (await policy.checkField('todos', value, guard)).grapheme_count;
  view.format = options.format || (async (_, value) => ({ action: 'accepted', value: draft.copyText(value) }));
  view.onStatus = (...args) => statuses.push(args); view.onRowFocused = (...args) => focuses.push(args); view.onRevealRow = (...args) => reveals.push(args);
  view.getUIContext = () => ({ getFocusController: () => ({ requestFocus: element => requestedFocus.push(element) }) });
  view.aboutToAppear();
  return { view, captures, statuses, focuses, reveals, requestedFocus, get raw() { return raw; },
    ticket(index = 0) { return view.ticket(view.state.rows[index]); },
    echo() { view.sourceChanged(); },
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
  const f = fixture('a'); await f.view.addRow(); await tick();
  assert.equal(f.raw.text, 'a\n'); assert.equal(f.reveals.length, 1); assert.equal(f.requestedFocus[0], f.reveals[0][2]);
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

test('parent state exposes pending/incomplete before row formatter and complete after exact reply', async () => {
  const wait = deferred(), f = fixture('a', { format: () => wait.promise });
  const running = f.view.inputFor(f.ticket(), 'candidate'); await tick();
  assert.ok(f.statuses.some(([, complete, pending]) => !complete && pending));
  wait.resolve({ action: 'accepted', value: text('candidate') }); await running;
  assert.equal(f.statuses.at(-1)[1], true); assert.equal(f.statuses.at(-1)[2], false);
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
