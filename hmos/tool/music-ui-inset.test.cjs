'use strict';
const test = require('node:test'), assert = require('node:assert/strict');
const { insetHarness } = require('./music-ui-component-test-harness.cjs');
const close = (actual, expected) => assert.ok(Math.abs(actual - expected) < 1e-10, `${actual} matches ${expected}`);
const rgba = value => { const m = /^rgba\((\d+),(\d+),(\d+),([\d.eE+-]+)\)$/.exec(value); assert.ok(m, 'numeric Canvas RGBA'); return m.slice(1).map(Number); };
function color(value, channels, opacity) { const numbers = rgba(value); assert.deepEqual(numbers.slice(0, 3), channels); close(numbers[3], opacity); }
function paint(style, dark = false, props = {}) { const h = insetHarness({ style, dark, ...props }); h.inset.draw(); return h; }
const strokes = h => h.canvas.commands.filter(command => command.op === 'stroke');
const fills = h => h.canvas.commands.filter(command => command.op === 'fill');
const ops = h => h.canvas.commands.map(command => command.op);

test('actual inset painter covers all Flutter styles with their distinct paint operations', () => {
  const expected = { flat: [], neumorphism: ['save', 'clip', 'fill', 'fill', 'restore'], paper: ['stroke', 'stroke'],
    clay: ['save', 'clip', 'fill', 'fill', 'restore', 'stroke'], fluent: ['stroke', 'stroke'],
    brutalist: ['stroke', 'stroke'], industrial: ['stroke', 'stroke', 'stroke'] };
  for (const dark of [false, true]) for (const [style, commands] of Object.entries(expected)) {
    const h = paint(style, dark); assert.deepEqual(ops(h), ['clearRect', ...commands], `${style} ${dark ? 'dark' : 'light'}`);
    assert.equal(h.canvas.filter, 'none', 'filter does not escape inner wall'); assert.equal(h.canvas.stack.length, 0);
  }
});

test('actual repaint clears old frames for flat, zero depth, unknown style and invalid geometry', () => {
  for (const props of [{ style: 'flat' }, { style: 'unknown' }, { depth: 0 }, { depth: -1 }, { depth: NaN }, { depth: Infinity }]) {
    const h = paint('industrial'); h.canvas.commands = []; Object.assign(h.inset, props); h.inset.draw(); assert.deepEqual(ops(h), ['clearRect']);
  }
  for (const [field, value] of [['width', 0], ['height', 7], ['width', NaN], ['height', Infinity]]) {
    const h = insetHarness({ style: 'clay' }); h.canvas[field] = value; h.inset.draw(); assert.deepEqual(ops(h), ['clearRect']);
  }
});

test('actual paper lines preserve Flutter light/dark alpha and edge orientation', () => {
  for (const dark of [false, true]) {
    const h = paint('paper', dark), [outline, edge] = strokes(h), rgb = dark ? [0, 0, 0] : [55, 50, 61];
    color(outline.style, rgb, (dark ? .4 : .28) * .8); assert.equal(outline.width, 1); assert.deepEqual(outline.path[0], ['moveTo', 14, 0]);
    assert.equal(edge.width, 1); assert.deepEqual(edge.style.args, [1, 1, 219, 63]);
    color(edge.style.stops[0][1], [255, 255, 255], (dark ? .34 : .82) * .8);
    color(edge.style.stops[2][1], rgb, (dark ? .72 : .52) * .8); assert.deepEqual(edge.style.stops[1], [.5, 'rgba(0,0,0,0)']);
  }
});

test('actual fluent inset reverses the edge to shadow then light without filling the child', () => {
  for (const dark of [false, true]) {
    const h = paint('fluent', dark), [outline, edge] = strokes(h);
    color(outline.style, dark ? [0, 0, 0] : [55, 50, 61], .35 * .8); assert.equal(outline.width, 1);
    color(edge.style.stops[0][1], [0, 0, 0], (dark ? .48 : .18) * .8);
    color(edge.style.stops[2][1], [255, 255, 255], (dark ? .34 : .82) * .8);
    assert.equal(edge.width, .8); assert.deepEqual(edge.style.args, [1, 1, 219, 63]); assert.deepEqual(fills(h), []);
  }
});

test('actual brutalist inset uses the requested theme ink and transparent lower edge', () => {
  for (const dark of [false, true]) {
    const h = paint('brutalist', dark), [outline, edge] = strokes(h), rgb = dark ? [255, 255, 255] : [33, 30, 38];
    color(outline.style, rgb, .8); assert.equal(outline.width, 1.8);
    color(edge.style.stops[0][1], rgb, .45 * .8); color(edge.style.stops[2][1], [0, 0, 0], 0);
    assert.equal(edge.width, 1); assert.deepEqual(edge.style.args, [1.4, 1.4, 218.6, 62.6]); assert.deepEqual(fills(h), []);
  }
});

test('actual industrial inset retains all three contours, scaled wall width and theme tones', () => {
  for (const dark of [false, true]) {
    const h = paint('industrial', dark), [outline, edge, inner] = strokes(h);
    assert.equal(outline.width, 1.6); color(outline.style, dark ? [0, 0, 0] : [55, 50, 61], (dark ? .72 : .52) * .8);
    assert.equal(edge.width, 1.5); assert.deepEqual(edge.style.args, [1.5, 1.5, 218.5, 62.5]);
    color(edge.style.stops[0][1], [0, 0, 0], (dark ? .48 : .18) * .8);
    color(edge.style.stops[2][1], [255, 255, 255], (dark ? .34 : .82) * .8);
    assert.equal(inner.width, .7); color(inner.style, [255, 255, 255], .2 * .8); assert.deepEqual(inner.path[0], ['moveTo', 14, 3]);
  }
});

test('actual clay inset uses blurred complement walls and the separate highlight contour', () => {
  for (const dark of [false, true]) {
    const h = paint('clay', dark), walls = fills(h), [highlight] = strokes(h);
    color(walls[0].color, [0, 0, 0], (dark ? .48 : .18) * .8);
    color(walls[1].color, [255, 255, 255], (dark ? .34 : .82) * .8);
    for (const wall of walls) { assert.equal(wall.rule, 'evenodd'); assert.equal(wall.filter, 'blur(4vp)'); assert.deepEqual(wall.clip[0], ['moveTo', 14, 0]); }
    close(walls[0].path[1][1][0][1], 15.2); close(walls[1].path[1][1][0][1], 12.8);
    close(walls[0].path[0][1], -23.2); assert.equal(highlight.width, 1); color(highlight.style, [255, 255, 255], .3 * .8);
    assert.equal(highlight.filter, 'none', 'highlight is painted after the wall state restores');
  }
});

test('actual neumorphic inset derives both casts from the real palette surface', () => {
  const light = paint('neumorphism', false, { surface: '#E1E3E9' }), lightWalls = fills(light);
  color(lightWalls[0].color, [56, 57, 58], .24 * .8); color(lightWalls[1].color, [249, 249, 251], .82 * .8);
  const dark = paint('neumorphism', true, { surface: '#292634' }), darkWalls = fills(dark);
  color(darkWalls[0].color, [6, 6, 8], .52 * .8); color(darkWalls[1].color, [101, 99, 109], .20 * .8);
  for (const h of [light, dark]) for (const wall of fills(h)) { assert.equal(wall.filter, 'blur(2vp)'); assert.equal(wall.rule, 'evenodd'); }
  assert.deepEqual(fills(paint('neumorphism', false, { surface: 'invalid' })).map(w => w.color), fills(paint('neumorphism')).map(w => w.color));
});

test('actual inset wall is the rounded opening complement with opposing offsets and exact unshifted clip', () => {
  const h = paint('neumorphism'), walls = fills(h);
  for (let index = 0; index < 2; index++) {
    const wall = walls[index], offset = index === 0 ? 1 : -1;
    assert.deepEqual(wall.path[0], ['rect', -13, -13, 246, 90]);
    assert.deepEqual(wall.path[1][1][0], ['moveTo', 14 + offset, offset]);
    assert.deepEqual(wall.path[1][1][2], ['arcTo', 220 + offset, offset, 220 + offset, 14 + offset, 14]);
    assert.deepEqual(wall.clip[0], ['moveTo', 14, 0]); assert.equal(wall.path.length, 2, 'only exterior and opening are filled with evenodd');
  }
  assert.deepEqual(ops(h), ['clearRect', 'save', 'clip', 'fill', 'fill', 'restore']);
});

test('actual depth uses -.8 times the material depth while opacity saturates separately from geometry', () => {
  for (const [depth, strength, fade] of [[.5, .4, .4], [1, .8, .8], [1.25, 1, 1], [2, 1.6, 1], [5, 2, 1]]) {
    const industrial = paint('industrial', false, { depth }), neumorphic = paint('neumorphism', false, { depth });
    close(strokes(industrial)[0].width, 2 * strength); color(strokes(industrial)[0].style, [55, 50, 61], .52 * fade);
    close(fills(neumorphic)[0].path[1][1][0][1], 14 + 1.25 * strength);
    color(fills(neumorphic)[0].color, [64, 64, 64], .24 * fade);
  }
});

test('actual rounded geometry clamps to available size and preserves the parent radius', () => {
  const h = insetHarness({ style: 'industrial', radius: 100 }); h.canvas.width = 60; h.canvas.height = 40; h.inset.draw();
  const [outer, edge, inner] = strokes(h); assert.deepEqual(outer.path[0], ['moveTo', 20, 0]);
  assert.deepEqual(edge.path[2], ['arcTo', 58.5, 1.5, 58.5, 20, 18.5]);
  assert.deepEqual(inner.path[2], ['arcTo', 57, 3, 57, 20, 17]);
  h.inset.radius = NaN; h.canvas.commands = []; h.inset.draw(); assert.deepEqual(strokes(h)[0].path[0], ['moveTo', 0, 0]);
});

test('actual resize callbacks coalesce and disappearance cancels the pending paint', () => {
  const h = insetHarness({ style: 'paper' }); h.inset.resized(); h.inset.resized(); assert.equal(h.timers.size, 1); assert.deepEqual(h.canvas.commands, []);
  h.tick(); assert.equal(h.timers.size, 0); assert.equal(strokes(h).length, 2);
  h.canvas.commands = []; h.inset.resized(); h.inset.aboutToDisappear(); assert.equal(h.timers.size, 0); assert.equal(h.inset.resizeTimer, -1);
  h.tick(); assert.deepEqual(h.canvas.commands, []);
});

test('actual current and selected rows keep fill=false and relay the parent theme surface', () => {
  const h = insetHarness(), rows = h.panelSource.slice(h.panelSource.indexOf('private trackRow('), h.panelSource.indexOf('private playlist()'));
  const current = h.panelSource.slice(h.panelSource.indexOf("Text(this.t('随身听'))"), h.panelSource.indexOf("this.iconAction(0xf0193"));
  assert.match(rows, /borderRadius\(this\.round\(10\)\)\.backgroundColor\(Color\.Transparent\)/);
  assert.match(current, /borderRadius\(this\.round\(14\)\)\.backgroundColor\(Color\.Transparent\)/);
  for (const source of [rows, current]) assert.match(source, /MusicInsetRelief\(\{ style: this\.style, depth: this\.depth, dark: this\.dark, surface: this\.surface,/);
  const painter = h.panelSource.slice(h.panelSource.indexOf('struct MusicInsetRelief'), h.panelSource.indexOf('export struct MusicPanelContent'));
  assert.doesNotMatch(painter, /fillRect\(|\.backgroundColor\(|\.border\(|\.shadow\(/);
  assert.match(painter, /c\.fill\(wall, 'evenodd'\)/); assert.match(painter, /hitTestBehavior\(HitTestMode\.None\)/);
});
