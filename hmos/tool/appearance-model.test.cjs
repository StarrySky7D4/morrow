// Runs the actual pure ArkTS model through the SDK's TypeScript compiler.
// This validates policy/math, not ArkUI rendering or platform preferences.
const fs = require('node:fs'), vm = require('node:vm'), path = require('node:path');
const assert = require('node:assert/strict');
const { test } = require('node:test');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../entry/src/main/ets/model/Appearance.ets'), 'utf8');
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } }).outputText, { exports: exportsObject });
const { MaterialChoice, normalizeMaterials, resolveMaterial, canFollow, styleRadiusScale, visualStyles } = exportsObject;
const material = (id, values = {}) => Object.assign(new MaterialChoice(), { id }, values);
test('old material preferences retain values and inherit depth', () => {
  const old = { id:'hero', enabled:true, mode:'超透', blur:7, opacity:39, radius:13, color:'#AABBCC' };
  const [loaded] = normalizeMaterials([old]);
  assert.equal(loaded.follow, ''); assert.equal(loaded.depth, -1);
  for (const key of Object.keys(old)) assert.equal(loaded[key], old[key]);
});
test('follow resolves complete terminal material even through disabled links', () => {
  const target = material('b', { enabled:true, depth:0, radius:0, opacity:0, blur:0, color:'#123456' });
  const chain = normalizeMaterials([material('a', {follow:'b'}), target, material('c', {follow:'a'})]);
  for(const id of ['a','b','c']) {
    const v=resolveMaterial(chain,id); assert.equal(v.id,'b'); assert.equal(v.depth,0); assert.equal(v.radius,0); assert.equal(v.opacity,0); assert.equal(v.blur,0);
  }
  target.enabled=false;
  assert.equal(resolveMaterial([material('a',{follow:'b'}),target],'a'),undefined);
  assert.equal(resolveMaterial([material('a',{follow:'missing'})],'a'),undefined);
});
test('all cycles rejected including disabled components, missing targets allowed', () => {
  for(const values of [[material('a',{follow:'a'})],[material('a',{follow:'b'}),material('b',{follow:'a'})]]) {
    assert.throws(()=>normalizeMaterials(values),/cycle/); assert.throws(()=>resolveMaterial(values,'a'),/cycle/);
  }
  const chain=[material('a',{follow:'b'}),material('b',{follow:'c'})];
  assert.equal(canFollow(chain,'c','a'),false); assert.equal(canFollow(chain,'a','a'),false);
  assert.equal(canFollow(chain,'c','missing'),true); assert.equal(canFollow(chain,'a',''),true);
  assert.doesNotThrow(()=>normalizeMaterials(chain));
});
test('invalid material data fails before replacing accepted values', () => {
  for(const field of ['blur','opacity','radius','depth']) for(const value of [NaN, Infinity, -2, 300])
    assert.throws(()=>normalizeMaterials([material('a',{[field]:value})]));
  for(const invalid of [{id:''},{color:'bad'},{mode:'future'},{follow:4},{enabled:'yes'}])
    assert.throws(()=>normalizeMaterials([material('a',invalid)]));
  assert.throws(()=>normalizeMaterials([material('a'),material('a')]));
});
test('maximum chain is bounded and survives JSON restart without losing overrides', () => {
  const chain=Array.from({length:4096},(_,i)=>material('m'+i,i<4095?{follow:'m'+(i+1)}:{enabled:true,depth:2}));
  const saved=JSON.stringify(normalizeMaterials(chain)); const restored=normalizeMaterials(JSON.parse(saved));
  assert.equal(resolveMaterial(restored,'m0').depth,2);
  assert.throws(()=>normalizeMaterials([...chain,material('overflow')]));
});
test('seven style radius rules match Flutter and keep a square radius square', () => {
  const expected=[1,1,.24,1.3,.55,.18,.4];
  visualStyles.forEach((style,i)=>{assert.equal(styleRadiusScale(style),expected[i]);assert.equal(0*styleRadiusScale(style),0);});
});
