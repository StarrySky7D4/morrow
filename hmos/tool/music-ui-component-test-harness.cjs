'use strict';
// Verbatim component fields/methods with real MusicUi/MusicLibrary/MusicPlayback
// and UiStrings. ArkUI layout, measured areas and timers are controlled seams.
// This neither renders ArkUI nor proves device pointer/AVPlayer behavior.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const modelRoot = path.resolve(__dirname, '../entry/src/main/ets/model');
const pageRoot = path.resolve(__dirname, '../entry/src/main/ets/pages');
const usedInputs = new Set([__filename]);
function compile(source, filename) {
  const result = ts.transpileModule(source, { fileName: filename, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.deepEqual((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), [], 'actual method TS transpilation');
  return result.outputText;
}
function source(name) { const filename = path.join(pageRoot, name + '.ets'); usedInputs.add(filename); return fs.readFileSync(filename, 'utf8'); }
function actualComponent(name, content, end) {
  const exported = 'export struct ' + name + ' {', marker = content.includes(exported) ? exported : 'struct ' + name + ' {';
  const start = content.indexOf(marker);
  assert.ok(start >= 0, 'actual component exists: ' + name);
  const bodyStart = start + marker.length, bodyEnd = content.indexOf(end, bodyStart);
  assert.ok(bodyEnd > bodyStart, 'actual component method boundary: ' + name);
  const body = content.slice(bodyStart, bodyEnd).replace(/@(Prop|State|Watch\('[^']*'\))\s*/g, '');
  return 'export class ' + name + ' {\n' + body + '\n}';
}
function harness() {
  const cache = new Map(), calls = [], reorders = [], timers = new Map(); let timer = 0;
  const globals = { exports: {}, require: load, setInterval: callback => { const id = ++timer; timers.set(id, callback); return id; },
    clearInterval: id => timers.delete(id), setTimeout, clearTimeout, encodeURIComponent,
    Scroller: class { constructor() { this.moves = []; this.fail = false; } scrollBy(x, y) { if (this.fail) throw new Error('ScrollerUnavailable'); this.moves.push([x, y]); } } };
  function load(request) {
    const name = request.replace(/^\.\//, '').replace(/^\.\.\/model\//, '');
    if (cache.has(name)) return cache.get(name);
    const filename = path.join(modelRoot, name + '.ets'); usedInputs.add(filename);
    const exports = {}; cache.set(name, exports);
    vm.runInNewContext(compile(fs.readFileSync(filename, 'utf8'), filename), { ...globals, exports }, { filename }); return exports;
  }
  const ui = load('MusicUi'), strings = load('UiStrings');
  const panelSource = source('MusicPanel'), footerSource = source('MusicFooter');
  const helperStart = panelSource.indexOf('class MusicDragTicket {'), helperEnd = panelSource.indexOf('@Component', helperStart);
  assert.ok(helperStart >= 0 && helperEnd > helperStart, 'actual drag helper classes');
  const components = panelSource.slice(helperStart, helperEnd) + '\n' + actualComponent('MusicPanelContent', panelSource, '\n  @Builder') + '\n' +
    actualComponent('MusicFooterContent', footerSource, '\n  build()') + '\n' + actualComponent('LyricsDialogContent', footerSource, '\n  build()');
  vm.runInNewContext(compile(components, path.join(pageRoot, 'MusicPanel.ets')), { ...globals, MusicUiView: ui.MusicUiView, uiText: strings.uiText });
  const page = new globals.exports.MusicPanelContent(), footer = new globals.exports.MusicFooterContent(), dialog = new globals.exports.LyricsDialogContent();
  const callbacks = { action: (...args) => calls.push(args), reorder: ids => reorders.push([...ids]) };
  const view = new ui.MusicUiView(); Object.assign(view, { controlsEnabled: true, canWrite: true });
  Object.assign(view.library, { library_revision: '7', loaded: true, selected_track_id: 'track-b', order: ['track-a', 'track-b', 'track-c'] });
  view.library.tracks = view.library.order.map((id, index) => ({ track_id: id, library_revision: '7', import_operation: 'import-' + id,
    sha256: String(index + 1).repeat(64), title: 'Title ' + id, name: id + '.wav', phase: 'Ready' }));
  view.library.records = [...view.library.tracks]; Object.assign(view.playback, { trackId: 'track-b', phase: 'prepared', durationMs: 120000, positionMs: 1000 });
  for (const component of [page, footer, dialog]) { component.view = view; component.callbacks = callbacks; }
  page.aboutToAppear();
  return { page, footer, dialog, view, calls, reorders, timers, panelSource, footerSource,
    tick: () => { for (const callback of [...timers.values()]) callback(); },
    area: (top, height = 40) => ({ globalPosition: { x: 0, y: top }, height }),
    event: y => ({ fingerList: [{ globalY: y }] }) };
}
// Record the SDK Canvas boundary while executing verbatim production geometry,
// colors and paint calls. This does not rasterize blur or prove rendered pixels.
function insetHarness(props = {}) {
  class ControlledPath {
    constructor() { this.commands = []; }
    moveTo(...args) { this.commands.push(['moveTo', ...args]); }
    lineTo(...args) { this.commands.push(['lineTo', ...args]); }
    arcTo(...args) { this.commands.push(['arcTo', ...args]); }
    closePath() { this.commands.push(['closePath']); }
    rect(...args) { this.commands.push(['rect', ...args]); }
    addPath(path) { this.commands.push(['addPath', path.commands.map(command => [...command])]); }
  }
  class ControlledCanvas {
    constructor() {
      this.width = 220; this.height = 64; this.commands = []; this.stack = []; this.filter = 'none'; this.currentClip = null;
    }
    clearRect(...args) { this.commands.push({ op: 'clearRect', args }); }
    save() { this.stack.push({ filter: this.filter, currentClip: this.currentClip, fillStyle: this.fillStyle,
      strokeStyle: this.strokeStyle, lineWidth: this.lineWidth }); this.commands.push({ op: 'save' }); }
    restore() { assert.ok(this.stack.length, 'Canvas state has an owner'); Object.assign(this, this.stack.pop()); this.commands.push({ op: 'restore' }); }
    clip(path) { this.currentClip = path.commands; this.commands.push({ op: 'clip', path: path.commands }); }
    createLinearGradient(...args) { return { args, stops: [], addColorStop(offset, color) { this.stops.push([offset, color]); } }; }
    stroke(path) { this.commands.push({ op: 'stroke', path: path.commands, style: this.strokeStyle, width: this.lineWidth, filter: this.filter }); }
    fill(path, rule) { this.commands.push({ op: 'fill', path: path.commands, rule, color: this.fillStyle, filter: this.filter, clip: this.currentClip }); }
  }
  const filename = path.join(modelRoot, 'Appearance.ets'); usedInputs.add(filename);
  const appearance = {};
  vm.runInNewContext(compile(fs.readFileSync(filename, 'utf8'), filename), { exports: appearance }, { filename });
  const panelSource = source('MusicPanel'), reliefSource = source('RecessedGlassRelief'), timers = new Map(); let timer = 0;
  const globals = { exports: {}, mix: appearance.mix, Path2D: ControlledPath, CanvasRenderingContext2D: ControlledCanvas,
    RenderingContextSettings: class {}, setTimeout: callback => { const id = ++timer; timers.set(id, callback); return id; },
    clearTimeout: id => timers.delete(id) };
  vm.runInNewContext(compile(actualComponent('RecessedGlassRelief', reliefSource, '\n  build()'), path.join(pageRoot, 'RecessedGlassRelief.ets')), globals);
  const inset = new globals.exports.RecessedGlassRelief(); Object.assign(inset, { depthMultiplier: -.8 }, props);
  return { inset, canvas: inset.ctx, timers, panelSource, reliefSource,
    tick: () => { const pending = [...timers.values()]; timers.clear(); for (const callback of pending) callback(); } };
}
module.exports = { harness, insetHarness, usedInputs, source };
