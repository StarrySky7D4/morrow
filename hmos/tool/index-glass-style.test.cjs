'use strict';
// Verbatim Index math and Builder guards. Stack children are captured at an
// ArkUI boundary; this does not measure, draw, blur or rasterize a device UI.
const test = require('node:test'), assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const file = path.resolve(__dirname, '../entry/src/main/ets/pages/Index.ets'), source = fs.readFileSync(file, 'utf8');
const modelFile = path.resolve(__dirname, '../entry/src/main/ets/model/Appearance.ets');
const plain = value => JSON.parse(JSON.stringify(value));
function compile(value, filename) {
  const result = ts.transpileModule(value, { fileName: filename, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  assert.deepEqual((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), []); return result.outputText;
}
function method(name) {
  const found = new RegExp('^  (?:private )?' + name + '\\(', 'm').exec(source); assert.ok(found, 'actual Index method: ' + name);
  const prefix = 'class Actual {\n', parsed = ts.createSourceFile(file, prefix + source.slice(found.index) + '\n}', ts.ScriptTarget.ES2020, true, ts.ScriptKind.TS);
  const node = parsed.statements[0]?.members[0]; assert.ok(ts.isMethodDeclaration(node)); assert.equal(node.name.getText(parsed), name);
  return source.slice(found.index, found.index + node.end - prefix.length);
}
function builder(name) {
  const start = source.indexOf('  ' + name + '('); assert.ok(start >= 0);
  const end = source.indexOf('\n  }', start); assert.ok(end > start);
  const actual = source.slice(start, end + '\n  }'.length);
  if (name === 'MusicPanel') return actual; // Already an expression with attributes.
  assert.equal((actual.match(/Stack\(\) \{/g) || []).length, 1, 'single actual outer Stack');
  // Retain every actual guard, child call and attribute. Only ArkUI's outer
  // Stack-child syntax becomes a JS block so it can execute at a call seam.
  return actual.replace('Stack() {', 'const actualStack = Stack(); {').replace('}.width(', '}\n    actualStack.width(');
}
const appearance = {}; vm.runInNewContext(compile(fs.readFileSync(modelFile, 'utf8'), modelFile), { exports: appearance });
const methods = ['surfaceShadow', 'depthFor', 'material', 'modeFor', 'round', 'surface', 'tone', 'baseColor', 'ink',
  'muted', 'accent', 'line', 'styleEdgeWidth', 'materialBlur', 'gradient', 'materialGradient'];
const code = compile('export class ActualGlass {\n' + methods.map(method).join('\n') + '\n' + ['Rim', 'DraftRim', 'MusicPanel'].map(builder).join('\n') + '\n}', file);
function page(density = 1) {
  const nodes = [], pxCalls = [], exports = {};
  function component(type, props) {
    const node = { type, props, attributes: {} }; nodes.push(node);
    return new Proxy({}, { get: (_, name) => (...args) => { node.attributes[name] = args; return new Proxy({}, handler(node)); } });
  }
  function handler(node) { return { get: (_, name) => (...args) => { node.attributes[name] = args; return new Proxy({}, handler(node)); } }; }
  vm.runInNewContext(code, { exports, ...appearance, Color: { Transparent: 'transparent' }, HitTestMode: { None: 'none' }, GradientDirection: { RightBottom: 'right-bottom' },
    Stack: () => component('Stack'), RecessedGlassRelief: props => component('RecessedGlassRelief', props),
    GlassRim: props => component('GlassRim', props), MusicPanelContent: props => component('MusicPanelContent', props) }, { filename: file });
  const p = new exports.ActualGlass(); Object.assign(p, { visualStyle: 'flat', styleDepth: 1, mode: '磨砂', dark: false,
    radius: 20, theme: '白色', themeColor: '', toneGray: 0, lightness: 88.5, materials: [], glassOpacity: 76,
    fontFamily: '', locale: 'zh', musicView: {}, musicCallbacks: {}, surfaceAreas: { 'music:22': [220, 400], 'search:12': [400, 42] },
    getUIContext: () => ({ vp2px: value => { pxCalls.push(value); return value * density; } }),
    rememberSurface() {}, previewMode: () => p.mode, previewRadius: () => p.round(20) });
  return { p, nodes, pxCalls, material: (id, values) => Object.assign(new appearance.MaterialChoice(), { id, enabled: true }, values) };
}

test('actual Glass primary cast converts Flutter logical radius and offsets to SDK physical px', () => {
  const expected = { neumorphism: [5, 1.5, 1.5], paper: [1, 0, 2], clay: [4, 1, 2], fluent: [4, 0, 1.5], brutalist: [0, 2.5, 2.5], industrial: [1, 0, 2] };
  for (const density of [1, 2, 3]) for (const [style, [radius, x, y]] of Object.entries(expected)) {
    const { p } = page(density); const result = p.surfaceShadow('music', style, .5);
    assert.equal(result.radius, radius * density); assert.equal(result.offsetX || 0, x * .5 * density); assert.equal(result.offsetY, y * .5 * density);
    assert.equal(result.fill, false, 'cast does not fill a translucent Glass center');
  }
});

test('actual Glass primary alpha saturates at full depth without removing its designed geometry', () => {
  const { p } = page(3); const half = p.surfaceShadow('music', 'neumorphism', .5), full = p.surfaceShadow('music', 'neumorphism', 1), deep = p.surfaceShadow('music', 'neumorphism', 2);
  assert.equal(half.color, '#0f8A8299'); assert.equal(full.color, '#1f8A8299'); assert.equal(deep.color, full.color);
  assert.equal(deep.offsetX, full.offsetX * 2); assert.equal(deep.radius, full.radius);
  for (const style of ['neumorphism', 'paper', 'clay', 'fluent', 'brutalist', 'industrial']) {
    assert.equal(p.surfaceShadow('music', style, 0).color.slice(1, 3), '00');
  }
});

test('actual flat frosted, clear and liquid preserve their distinct inherited Glass shadows', () => {
  const { p } = page(3); const frosted = p.surfaceShadow('music'); assert.equal(frosted.radius, 54); assert.equal(frosted.offsetY, 24); assert.equal(frosted.color, '#09716386');
  p.mode = '超透'; const clear = p.surfaceShadow('music'); assert.equal(clear.radius, 42); assert.equal(clear.offsetY, 12); assert.equal(clear.color, '#06716386');
  p.dark = true; assert.equal(p.surfaceShadow('music').color, '#06000000');
  p.mode = '液体玻璃'; assert.deepEqual(plain(p.surfaceShadow('music')), { radius: 0, color: 'transparent' });
  assert.equal(p.surfaceShadow('music', 'fluent').color, '#38000000', 'an explicit visual style still retains its cast in liquid mode');
});

test('actual recessed search and default Footer suppress only their specified outer casts', () => {
  const h = page(3); for (const style of ['neumorphism', 'paper', 'clay', 'fluent', 'brutalist', 'industrial']) {
    assert.deepEqual(plain(h.p.surfaceShadow('search', style)), { radius: 0, color: 'transparent' });
    assert.deepEqual(plain(h.p.surfaceShadow('footer', style)), { radius: 0, color: 'transparent' });
  }
  assert.equal(h.p.surfaceShadow('search', 'flat').radius, 54, 'Flutter flat returns inherited material before recessed style changes');
  h.p.materials = [h.material('footer', {})]; assert.equal(h.p.surfaceShadow('footer', 'clay').radius, 12);
});

test('actual normal Glass has no extra relief Canvas; search retains its real shared inset', () => {
  for (const style of appearance.visualStyles) {
    const h = page(); h.p.visualStyle = style;
    for (const id of ['navigation', 'hero', 'appearance', 'daily', 'music', 'quick', 'summary', 'card:one', '']) {
      h.nodes.length = 0; h.p.Rim(id); assert.deepEqual(h.nodes.map(n => n.type), ['Stack'], `${id} ${style}: ordinary Glass has no positive relief`);
    }
    h.nodes.length = 0; h.p.materials = [h.material('search', { radius: 6, depth: .4 })]; h.p.Rim('search', 12);
    assert.deepEqual(h.nodes.map(n => n.type), ['Stack', 'RecessedGlassRelief']);
    const inset = h.nodes[1].props; assert.equal(inset.depthMultiplier, -1); assert.equal(inset.depth, .4); assert.equal(inset.radius, 3.6); assert.equal(inset.surface, h.p.surface());
  }
});

test('actual Liquid rim remains optical and Footer/preview keep their separate material guards', () => {
  const h = page(); h.p.mode = '液体玻璃'; h.p.Rim('music', 22); assert.deepEqual(h.nodes.map(n => n.type), ['Stack', 'GlassRim']);
  h.nodes.length = 0; h.p.Rim('footer', 14); assert.deepEqual(h.nodes.map(n => n.type), ['Stack']);
  h.p.materials = [h.material('footer', {})]; h.nodes.length = 0; h.p.Rim('footer', 14); assert.deepEqual(h.nodes.map(n => n.type), ['Stack', 'GlassRim']);
  h.nodes.length = 0; h.p.DraftRim(); assert.deepEqual(h.nodes.map(n => n.type), ['Stack', 'GlassRim']);
  h.p.mode = '磨砂'; h.nodes.length = 0; h.p.DraftRim(); assert.deepEqual(h.nodes.map(n => n.type), ['Stack']);
});

test('actual music inner uses global Appearance while outer material keeps local radius and depth', () => {
  const h = page(); h.p.visualStyle = 'clay'; h.p.styleDepth = .4; h.p.radius = 20;
  h.p.materials = [h.material('music', { radius: 2, depth: 2, mode: '超透' })]; h.p.MusicPanel();
  const music = h.nodes.find(n => n.type === 'MusicPanelContent'); assert.ok(music);
  assert.equal(music.props.depth, .4); assert.equal(music.props.radiusScale, 1.3);
  assert.equal(music.attributes.borderRadius[0], 2.2); assert.equal(music.props.surface, h.p.surface());
  assert.equal(h.p.depthFor('music'), 2, 'local outer depth is preserved'); assert.equal(music.attributes.shadow[0].offsetY, 4);
});

test('actual candidate material preview passes its own mode instead of the saved global mode', () => {
  const { p } = page(3); assert.equal(p.mode, '磨砂');
  assert.deepEqual(plain(p.surfaceShadow('', 'flat', 1, '液体玻璃')), { radius: 0, color: 'transparent' });
  assert.equal(p.surfaceShadow('', 'flat', 1, '超透').radius, 42);
  assert.equal(p.surfaceShadow('', 'flat', 1, '磨砂').radius, 54); assert.equal(p.mode, '磨砂');
  assert.match(source, /\.shadow\(this\.surfaceShadow\('', this\.visualStyle, this\.previewDepth\(\), this\.previewMode\(\)\)\)/,
    'actual preview paint call binds the mode before the preview is applied');
});
