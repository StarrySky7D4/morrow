'use strict';
const assert = require('node:assert/strict'), { test } = require('node:test');
const { snapshot, material, plain, deferred, tick, fixture, decodeAppearancePreferences: decode,
  encodeAppearancePreferences: encode, validateAppearanceCandidate: validate, appearancePreferenceStatus: status,
  AppearancePreferences, appearancePreferenceName, appearancePreferenceKey } = require('./appearance-preferences-test-harness.cjs');

test('isolated UI namespace and existing key are explicit', () => {
  assert.equal(appearancePreferenceName, 'studio-appearance'); assert.equal(appearancePreferenceKey, 'appearance-v1');
});
test('positive absent key restores actual defaults without writes', async () => {
  const f = fixture({ raw: undefined }); assert.equal(f.model.view().state, 'not-loaded');
  assert.equal(await f.model.restore(), true); assert.deepEqual(plain(f.model.view().current), plain(snapshot()));
  assert.equal(f.model.view().originalRaw, undefined); assert.deepEqual(f.counts(), { reads: 1, puts: 0, flushes: 0 });
});
test('complete real snapshot and all supported styles round-trip', () => {
  for (const visualStyle of ['flat', 'neumorphism', 'paper', 'clay', 'fluent', 'brutalist', 'industrial']) {
    const value = snapshot({ theme: '自定义', mode: '液体玻璃', opacity: 20, radius: 0, accent: '#123456', grayscale: 100,
      lightness: 0, background: '透明', solid: '#FFFFFF', canvasLiquid: true, canvasBlur: 40, canvasOpacity: 100,
      font: 'HarmonyOS Sans', locale: 'ru', daily: ['给自己倒一杯水'], materials: [material('music', { depth: 2 })], visualStyle, styleDepth: 2 });
    assert.deepEqual(plain(decode(encode(value, undefined))), plain(value));
  }
});
test('legacy optional material/style defaults are narrow and preserve stored values', () => {
  const old = plain(snapshot({ opacity: 37, font: 'Old font' })); delete old.materials; delete old.visualStyle; delete old.styleDepth;
  const restored = decode(JSON.stringify(old)); assert.equal(restored.opacity, 37); assert.equal(restored.font, 'Old font');
  assert.deepEqual(plain(restored.materials), []); assert.equal(restored.visualStyle, 'flat'); assert.equal(restored.styleDepth, 1);
});
test('missing core fields and missing candidate fields cannot become defaults', () => {
  for (const key of Object.keys(snapshot()).filter(key => !['materials', 'visualStyle', 'styleDepth'].includes(key))) {
    const missing = plain(snapshot()); delete missing[key]; assert.throws(() => decode(JSON.stringify(missing)), /Incomplete/);
  }
  for (const key of Object.keys(snapshot())) {
    const missing = plain(snapshot()); delete missing[key]; assert.throws(() => validate(missing), /Incomplete/);
  }
});
test('all root known wrong types reject instead of coercion or clamping', () => {
  const invalid = { theme: true, mode: 0, opacity: '76', radius: null, accent: 3, grayscale: '1', lightness: false,
    background: [], solid: {}, canvasLiquid: 'false', canvasBlur: '0', canvasOpacity: null, font: 7, locale: true,
    daily: {}, materials: {}, visualStyle: null, styleDepth: '1' };
  for (const [key, value] of Object.entries(invalid)) assert.throws(() => decode(JSON.stringify(snapshot({ [key]: value }))), key);
});
test('bounded finite numbers reject negatives, overflow, nonfinite and incompatible styles', () => {
  for (const [key, min, max] of [['opacity',20,100],['radius',0,32],['grayscale',0,100],['lightness',0,100],['canvasBlur',0,40],['canvasOpacity',0,100],['styleDepth',0,2]]) {
    for (const bad of [min - .1, max + .1, NaN, Infinity, -Infinity]) assert.throws(() => validate(snapshot({ [key]: bad })), key);
    assert.doesNotThrow(() => validate(snapshot({ [key]: min }))); assert.doesNotThrow(() => validate(snapshot({ [key]: max })));
  }
  for (const [key, bad] of [['theme','future'],['mode','future'],['background','future'],['locale','xx'],['visualStyle','future']])
    assert.throws(() => decode(JSON.stringify(snapshot({ [key]: bad }))));
});
test('colors, text, daily records and material graph are actually validated', () => {
  for (const patch of [{ accent: '#FFF' }, { solid: 'transparent' }, { font: 'x'.repeat(513) }, { font: '\uD800' },
    { daily: [''] }, { daily: ['one','one'] }, { daily: [1] }, { daily: ['x'.repeat(4097)] }, { daily: Array(4097).fill('x') },
    { materials: [material('a',{follow:'b'}),material('b',{follow:'a'})] }, { materials: [material('a'),material('a')] },
    { materials: [material('a',{opacity:101})] }, { materials: [material('a',{enabled:'true'})] }]) assert.throws(() => validate(snapshot(patch)));
  const missingMaterial = plain(material('a')); delete missingMaterial.blur;
  assert.throws(() => validate(snapshot({ materials: [missingMaterial] })), /Invalid|Incomplete/);
});
test('corrupt JSON and non-object roots never produce a valid snapshot', () => {
  for (const raw of ['', '{', 'null', 'true', '3', '[]', '"text"', '{}', '\uD800']) assert.throws(() => decode(raw));
});
test('duplicate root and edited material keys reject ambiguous records', () => {
  const complete = JSON.stringify(snapshot()); assert.throws(() => decode(complete.replace('{','{"theme":"白色",')), /Duplicate/);
  const old = JSON.stringify(snapshot({materials:[material('a')]}));
  assert.throws(() => decode(old.replace('"id":"a"','"id":"a","id":"a"')), /Duplicate/);
  assert.throws(() => decode(old.replace('"id":"a"','"id":"a","f":0,"f":1')), /Duplicate/);
});
test('unknown top-level fields retain literal values including unsafe JS integers', () => {
  const raw = JSON.stringify(snapshot()).slice(0,-1) + ',"future":{"raw":9007199254740993123456789,"nested":[1e309,-0,"x,}\\\"y"]},"__proto__":{"ok":true}}';
  const changed = encode(snapshot({opacity:53}), raw); assert.ok(changed.includes('"future":{"raw":9007199254740993123456789,"nested":[1e309,-0,"x,}\\\"y"]}'));
  assert.ok(changed.includes('"__proto__":{"ok":true}')); assert.equal(decode(changed).opacity, 53); assert.equal({}.ok, undefined);
});
test('unknown material fields follow stable id across reorder and changed overrides', () => {
  const raw = JSON.stringify(snapshot({materials:[material('a'),material('b')]})).replace('"id":"a"','"id":"a","future":{"integer":9007199254740993}')
    .replace('"id":"b"','"id":"b","future":{"arr":["x",{"v":false}]}');
  const encoded = encode(snapshot({materials:[material('b',{opacity:27}),material('a',{depth:2}),material('c')]}),raw);
  const value = JSON.parse(encoded); assert.deepEqual(value.materials.map(x=>x.id),['b','a','c']);
  assert.deepEqual(value.materials[0].future,{arr:['x',{v:false}]}); assert.ok(encoded.includes('"integer":9007199254740993'));
  assert.equal(value.materials[0].opacity,27); assert.equal(value.materials[1].depth,2); assert.equal(value.materials[2].future,undefined);
});
test('removed material unknown fields do not attach to a new id and old raw remains untouched', () => {
  const raw = JSON.stringify(snapshot({materials:[material('a')]})).replace('"id":"a"','"id":"a","future":"only a"');
  const next = encode(snapshot({materials:[material('b')]}),raw); assert.ok(!next.includes('only a')); assert.ok(raw.includes('only a'));
});
test('JSON span scanner handles escaped keys and nested delimiters without dropping future values', () => {
  const raw = JSON.stringify(snapshot()).slice(0,-1) + ',"fu\\u0074ure":{"items":[{"text":"[,}]:\\\\\\\""}],"null":null},"  spaced ": [ true, false ] }';
  const next = encode(snapshot({radius:11}), raw); assert.ok(next.includes('"future":{"items":[{"text":"[,}]:\\\\\\\""}],"null":null}'));
  assert.ok(next.includes('"  spaced ":[ true, false ]')); assert.equal(decode(next).radius,11);
});
test('original corrupt or incomplete input cannot be encoded over', () => {
  for (const raw of ['{', '{}', JSON.stringify(snapshot({opacity:'bad'}))]) assert.throws(() => encode(snapshot(),raw));
});
test('unknown-field encoded budget is checked before writing', () => {
  const raw = JSON.stringify(snapshot()).slice(0,-1) + ',"future":"' + 'x'.repeat(4*1024*1024) + '"}';
  assert.throws(() => decode(raw), /budget/); assert.throws(() => encode(snapshot(),raw), /budget/);
  const multi = JSON.stringify(snapshot()).slice(0,-1) + ',"future":"' + '汉'.repeat(1500000) + '"}';
  assert.throws(() => decode(multi), /budget/);
});
test('full candidate rejects undeclared properties instead of installing a new unknown contract', () => {
  assert.throws(() => validate(snapshot({future:'new'})), /Incomplete/);
  assert.throws(() => validate(snapshot({materials:[material('a',{future:true})]})), /Incomplete/);
  for (const key of ['follow','depth']) {
    const old=plain(material('a'));delete old[key];
    assert.doesNotThrow(()=>decode(JSON.stringify(snapshot({materials:[old]}))));
    assert.throws(()=>validate(snapshot({materials:[old]})),/Incomplete/);
  }
});
test('save before first successful read only retains a complete local candidate', async () => {
  const f=fixture(); assert.equal(await f.model.save(snapshot({opacity:31})),false);
  assert.equal(f.model.view().candidate.opacity,31); assert.equal(f.model.view().dirty,true); assert.equal(f.model.view().current,undefined);
  assert.deepEqual(f.counts(),{reads:0,puts:0,flushes:0}); await f.model.restore();
  assert.equal(f.model.view().current.opacity,76); assert.equal(f.model.view().candidate.opacity,31); assert.equal(f.model.view().dirty,true);
});
test('read rejection blocks every later ordinary save without side effects', async () => {
  const f=fixture({read(){throw new Error('disk read failed');}}); assert.equal(await f.model.restore(),false);
  for(const opacity of [23,24,25]) assert.equal(await f.model.save(snapshot({opacity})),false);
  const view=f.model.view(); assert.equal(view.state,'read-failed'); assert.equal(view.candidate.opacity,25);
  assert.equal(view.originalRaw,undefined); assert.equal(view.current,undefined); assert.deepEqual(f.counts(),{reads:1,puts:0,flushes:0});
});
test('malformed original raw remains inspectable and cannot be replaced with defaults', async () => {
  for(const raw of ['{', '', JSON.stringify(snapshot({font:42}))]) {
    const f=fixture({raw}); await f.model.restore(); await f.model.save(snapshot());
    assert.equal(f.model.view().state,'read-failed'); assert.equal(f.model.view().observedRaw,raw);
    assert.equal(f.backend.cache,raw); assert.equal(f.backend.disk,raw); assert.equal(f.counts().puts,0);
  }
});
test('loading blocks save and candidate survives completed restore', async () => {
  const gate=deferred(), f=fixture({read(n,b){return n===1?gate.promise:b.cache;}}), loading=f.model.restore();
  assert.equal(f.model.view().state,'loading'); assert.equal(f.model.view().busy,true); assert.equal(await f.model.save(snapshot({opacity:34})),false);
  assert.equal(await f.model.restore(),false); gate.resolve(f.backend.cache); assert.equal(await loading,true);
  assert.equal(f.model.view().candidate.opacity,34); assert.equal(f.model.view().current.opacity,76); assert.equal(f.counts().puts,0);
});
test('durable put then flush then exact readback precede current promotion', async () => {
  const gate=deferred(), f=fixture({async flush(n,b){await gate.promise;b.disk=b.cache;}}); await f.model.restore();
  const saved=f.model.save(snapshot({opacity:29})); await tick(); assert.equal(f.model.view().current.opacity,76);
  assert.equal(f.model.view().candidate.opacity,29); assert.equal(f.model.view().busy,true); assert.equal(f.model.view().dirty,true);
  assert.equal(JSON.parse(f.backend.cache).opacity,29); assert.equal(JSON.parse(f.backend.disk).opacity,76);
  gate.resolve(); assert.equal(await saved,true); assert.equal(f.model.view().current.opacity,29); assert.equal(f.model.view().dirty,false);
  assert.deepEqual(f.calls.map(x=>x.kind),['read','read','put','flush','read']); assert.equal(f.model.view().unknownRaw,undefined);
});
test('serial queue preserves rapid candidate order and keeps latest candidate during older ACK', async () => {
  const first=deferred(), f=fixture({async flush(n,b){if(n===1)await first.promise;b.disk=b.cache;}}); await f.model.restore();
  const a=f.model.save(snapshot({opacity:30})), b=f.model.save(snapshot({opacity:40})), c=f.model.save(snapshot({opacity:50}));
  await tick(); assert.equal(f.counts().puts,1); assert.equal(f.model.view().candidate.opacity,50); assert.equal(f.model.view().current.opacity,76);
  first.resolve(); assert.deepEqual(await Promise.all([a,b,c]),[true,true,true]);
  assert.deepEqual(f.calls.filter(x=>x.kind==='put').map(x=>JSON.parse(x.raw).opacity),[30,40,50]);
  assert.ok(f.notices.some(x=>x.view.current?.opacity===30&&x.view.candidate?.opacity===50&&x.view.dirty));
  assert.equal(f.model.view().current.opacity,50); assert.equal(f.model.view().busy,false);
});
test('queue uses freshly confirmed unknown-field original instead of stale enqueue bytes', async () => {
  const raw=JSON.stringify(snapshot()).slice(0,-1)+',"future":{"v":9007199254740993123}}', f=fixture({raw}); await f.model.restore();
  assert.deepEqual(await Promise.all([f.model.save(snapshot({opacity:33})),f.model.save(snapshot({opacity:44}))]),[true,true]);
  assert.ok(f.backend.disk.includes('"v":9007199254740993123')); assert.equal(f.model.view().current.opacity,44);
});
test('put failure is unknown and never automatically flushes or replays queued candidates', async () => {
  const f=fixture({put(raw,n,b){b.cache=raw;throw new Error('lost put acknowledgement');}}); await f.model.restore();
  const original=f.model.view().originalRaw, a=f.model.save(snapshot({opacity:33})),b=f.model.save(snapshot({opacity:44}));
  assert.deepEqual(await Promise.all([a,b]),[false,false]); const view=f.model.view();
  assert.equal(view.state,'write-unknown'); assert.equal(view.originalRaw,original); assert.equal(view.current.opacity,76);
  assert.equal(view.candidate.opacity,44); assert.equal(JSON.parse(view.unknownRaw).opacity,33); assert.equal(f.counts().puts,1); assert.equal(f.counts().flushes,0);
  assert.equal(await f.model.save(snapshot({opacity:55})),false); assert.equal(f.counts().puts,1);
});
test('flush lost ACK keeps unknown bytes and known current even if storage actually committed', async () => {
  const f=fixture({flush(n,b){b.disk=b.cache;throw new Error('lost flush ACK');}}); await f.model.restore();
  const original=f.model.view().originalRaw; assert.equal(await f.model.save(snapshot({opacity:32})),false);
  assert.equal(JSON.parse(f.backend.disk).opacity,32); assert.equal(f.model.view().current.opacity,76); assert.equal(f.model.view().originalRaw,original);
  assert.equal(f.model.view().state,'write-unknown'); assert.equal(f.model.view().dirty,true);
});
test('write exact readback mismatch stays unknown and retains last confirmed source', async () => {
  const f=fixture({read(n,b){if(n===3)return JSON.stringify(snapshot({opacity:67}));return b.cache;}}); await f.model.restore();
  const original=f.model.view().originalRaw; assert.equal(await f.model.save(snapshot({opacity:28})),false);
  assert.equal(f.model.view().state,'write-unknown'); assert.equal(f.model.view().originalRaw,original); assert.equal(f.model.view().current.opacity,76);
  assert.equal(JSON.parse(f.model.view().observedRaw).opacity,67); assert.equal(JSON.parse(f.model.view().unknownRaw).opacity,28);
});
test('write readback rejection stays unknown after successful put and flush', async () => {
  const f=fixture({read(n,b){if(n===3)throw new Error('readback unavailable');return b.cache;}}); await f.model.restore();
  assert.equal(await f.model.save(snapshot({opacity:24})),false); assert.equal(f.model.view().state,'write-unknown'); assert.equal(f.model.view().current.opacity,76);
});
test('preflight read failure blocks unissued write and later queued writes', async () => {
  const f=fixture({read(n,b){if(n===2)throw new Error('preflight failed');return b.cache;}}); await f.model.restore();
  assert.deepEqual(await Promise.all([f.model.save(snapshot({opacity:26})),f.model.save(snapshot({opacity:36}))]),[false,false]);
  assert.equal(f.model.view().state,'read-failed'); assert.equal(f.counts().puts,0); assert.equal(f.model.view().unknownRaw,undefined);
});
test('external raw change blocks overwrite and preserves confirmed and observed originals', async () => {
  const f=fixture(); await f.model.restore(); const original=f.model.view().originalRaw;
  f.backend.cache=JSON.stringify(snapshot({opacity:63})); assert.equal(await f.model.save(snapshot({opacity:27})),false);
  assert.equal(f.model.view().state,'read-failed'); assert.equal(f.model.view().originalRaw,original);
  assert.equal(f.model.view().current.opacity,76); assert.equal(JSON.parse(f.model.view().observedRaw).opacity,63); assert.equal(f.counts().puts,0);
});
test('explicit reread recovers transient read failure without writing retained candidate', async () => {
  let fail=true; const f=fixture({read(n,b){if(fail)throw new Error('temporarily offline');return b.cache;}}); await f.model.restore();
  await f.model.save(snapshot({opacity:25})); fail=false; assert.equal(await f.model.recover(),true);
  assert.equal(f.model.view().state,'ready'); assert.equal(f.model.view().current.opacity,76); assert.equal(f.model.view().candidate.opacity,25);
  assert.deepEqual(f.counts(),{reads:2,puts:0,flushes:0}); assert.equal(f.model.view().dirty,true);
  assert.equal(await f.model.save(f.model.view().candidate),true); assert.equal(f.counts().puts,1);
});
test('explicit recovery cannot turn still-corrupt original into defaults', async () => {
  const f=fixture({raw:'{'}); await f.model.restore(); await f.model.save(snapshot());
  assert.equal(await f.model.recover(),false); assert.equal(f.model.view().state,'read-failed'); assert.equal(f.backend.disk,'{');
  assert.deepEqual(f.counts(),{reads:2,puts:0,flushes:0});
});
test('unknown recovery requires flush ACK plus exact reread and performs no put replay', async () => {
  const f=fixture({flush(n,b){if(n===1)throw new Error('uncertain first flush');b.disk=b.cache;}}); await f.model.restore();
  await f.model.save(snapshot({opacity:35})); assert.equal(f.model.view().state,'write-unknown'); assert.equal(JSON.parse(f.backend.disk).opacity,76);
  assert.equal(await f.model.recover(),true); assert.equal(f.model.view().state,'ready'); assert.equal(f.model.view().current.opacity,35);
  assert.equal(JSON.parse(f.backend.disk).opacity,35); assert.equal(f.counts().puts,1); assert.equal(f.counts().flushes,2);
  assert.equal(f.model.view().unknownRaw,undefined);
});
test('unknown recovery preserves newer unsaved candidate instead of silently saving it', async () => {
  const f=fixture({flush(n,b){if(n===1)throw new Error('lost');b.disk=b.cache;}}); await f.model.restore(); await f.model.save(snapshot({opacity:30}));
  await f.model.save(snapshot({opacity:45})); assert.equal(await f.model.recover(),true);
  assert.equal(f.model.view().current.opacity,30); assert.equal(f.model.view().candidate.opacity,45); assert.equal(f.model.view().dirty,true); assert.equal(f.counts().puts,1);
});
test('unknown recovery flush failure preserves original and remains blocked', async () => {
  const f=fixture({flush(){throw new Error('flush unavailable');}}); await f.model.restore(); const original=f.model.view().originalRaw;
  await f.model.save(snapshot({opacity:30})); assert.equal(await f.model.recover(),false);
  assert.equal(f.model.view().state,'write-unknown'); assert.equal(f.model.view().originalRaw,original); assert.equal(f.model.view().current.opacity,76);
});
test('recovery readback mismatch never confirms a different payload', async () => {
  const f=fixture({flush(n,b){if(n===1)throw new Error('lost');b.disk=b.cache;},read(n,b){if(n===4)return JSON.stringify(snapshot({opacity:65}));return b.cache;}});
  await f.model.restore(); await f.model.save(snapshot({opacity:30})); assert.equal(await f.model.recover(),false);
  assert.equal(f.model.view().state,'write-unknown'); assert.equal(f.model.view().current.opacity,76); assert.equal(f.counts().puts,1);
});
test('explicit recovery may observe old disk bytes without automatically rolling back or replacing them', async () => {
  const f=fixture({put(){throw new Error('put rejected before effect');}}); await f.model.restore(); const raw=f.model.view().originalRaw;
  await f.model.save(snapshot({opacity:22})); assert.equal(await f.model.recover(),true);
  assert.equal(f.model.view().originalRaw,raw); assert.equal(f.model.view().current.opacity,76); assert.equal(f.model.view().candidate.opacity,22);
  assert.equal(f.counts().puts,1); assert.equal(f.counts().flushes,1);
});
test('ordinary restore cannot bypass read-failed or unknown recovery action', async () => {
  const f=fixture({read(){throw new Error('read fail');}}); await f.model.restore(); assert.equal(await f.model.restore(),false); assert.equal(f.counts().reads,1);
  const u=fixture({flush(){throw new Error('write fail');}}); await u.model.restore(); await u.model.save(snapshot({opacity:20}));
  const reads=u.counts().reads; assert.equal(await u.model.restore(),false); assert.equal(u.counts().reads,reads);
});
test('stale restore response does not promote current or notify replacement foreground owner', async () => {
  const gate=deferred(), f=fixture({read(n,b){return n===1?gate.promise:b.cache;}}), restoring=f.model.restore();
  const notices=f.notices.length; f.owner.epoch='page-A:foreground-2';gate.resolve(f.backend.cache); assert.equal(await restoring,false);
  assert.equal(f.model.view().state,'not-loaded'); assert.equal(f.model.view().current,undefined); assert.equal(f.notices.length,notices);
  assert.equal(await f.model.restore(),true); assert.equal(f.model.view().current.opacity,76);
});
test('stale queued candidate is never issued after foreground epoch changes', async () => {
  const first=deferred(), f=fixture({async flush(n,b){if(n===1)await first.promise;b.disk=b.cache;}}); await f.model.restore();
  const a=f.model.save(snapshot({opacity:30})),b=f.model.save(snapshot({opacity:40}));await tick();const notices=f.notices.length;
  f.owner.epoch='page-A:foreground-2';first.resolve();assert.deepEqual(await Promise.all([a,b]),[false,false]);
  assert.equal(f.counts().puts,1);assert.equal(f.model.view().state,'write-unknown');assert.equal(f.model.view().current.opacity,76);assert.equal(f.notices.length,notices);
});
test('owner loss during put prevents flush and preserves unknown issued literal', async () => {
  const gate=deferred(), f=fixture({async put(raw,n,b){b.cache=raw;await gate.promise;}}); await f.model.restore(); const saved=f.model.save(snapshot({opacity:23}));await tick();
  f.owner.current=false;gate.resolve();assert.equal(await saved,false);assert.equal(f.counts().flushes,0);assert.equal(f.model.view().state,'write-unknown');
  assert.equal(JSON.parse(f.model.view().unknownRaw).opacity,23);assert.equal(f.model.view().current.opacity,76);
});
test('owner loss during write readback prevents current promotion after durable ACK', async () => {
  const gate=deferred(), f=fixture({read(n,b){return n===3?gate.promise:b.cache;}});await f.model.restore();const saved=f.model.save(snapshot({opacity:23}));await tick();
  f.owner.epoch='page-B:foreground-1';gate.resolve(f.backend.cache);assert.equal(await saved,false);assert.equal(f.model.view().state,'write-unknown');
  assert.equal(f.model.view().current.opacity,76);assert.equal(JSON.parse(f.backend.disk).opacity,23);
});
test('stale recovery cannot clear unknown write or install observed current', async () => {
  const gate=deferred(), f=fixture({flush(n,b){if(n===1)throw new Error('lost');return gate.promise.then(()=>{b.disk=b.cache;});}});await f.model.restore();await f.model.save(snapshot({opacity:24}));
  const recovering=f.model.recover();await tick();f.owner.epoch='page-A:foreground-2';gate.resolve();assert.equal(await recovering,false);
  assert.equal(f.model.view().state,'write-unknown');assert.equal(f.model.view().current.opacity,76);assert.ok(f.model.view().unknownRaw);
});
test('disposed model suppresses callbacks and cannot issue later reads or writes', async () => {
  const f=fixture();await f.model.restore();const notices=f.notices.length;const counts=f.counts();f.model.dispose();
  assert.equal(await f.model.save(snapshot({opacity:24})),false);assert.equal(await f.model.restore(),false);assert.equal(await f.model.recover(),false);
  assert.deepEqual(f.counts(),counts);assert.equal(f.notices.length,notices);
});
test('disposed issued write keeps original current and never continues into flush', async () => {
  const gate=deferred(),f=fixture({async put(raw,n,b){b.cache=raw;await gate.promise;}});await f.model.restore();const saved=f.model.save(snapshot({opacity:24}));await tick();
  const notices=f.notices.length;f.model.dispose();gate.resolve();assert.equal(await saved,false);assert.equal(f.counts().flushes,0);
  assert.equal(f.model.view().state,'write-unknown');assert.equal(f.model.view().current.opacity,76);assert.equal(f.notices.length,notices);
});
test('snapshot and input ownership prevent mutations outside the queued candidate', async () => {
  const gate=deferred(),f=fixture({async flush(n,b){await gate.promise;b.disk=b.cache;}});await f.model.restore();
  const input=snapshot({opacity:24,daily:['one'],materials:[material('music',{opacity:33})]}),saved=f.model.save(input);
  input.opacity=88;input.daily.push('two');input.materials[0].opacity=99;const view=f.model.view();view.candidate.opacity=92;view.candidate.daily.push('three');
  await tick();gate.resolve();assert.equal(await saved,true);const current=f.model.view().current;
  assert.equal(current.opacity,24);assert.deepEqual(plain(current.daily),['one']);assert.equal(current.materials[0].opacity,33);
});
test('same current candidate does not produce extra writes but still verifies original', async () => {
  const f=fixture();await f.model.restore();assert.equal(await f.model.save(snapshot()),true);assert.deepEqual(f.counts(),{reads:2,puts:0,flushes:0});
  f.backend.cache='corrupt';assert.equal(await f.model.save(snapshot()),false);assert.equal(f.model.view().state,'read-failed');assert.equal(f.counts().puts,0);
});
test('invalid candidate leaves previous queued candidate and original unmodified', async () => {
  const f=fixture();await f.model.restore();const view=plain(f.model.view());await assert.rejects(f.model.save(snapshot({opacity:NaN})),/Invalid/);
  assert.deepEqual(plain(f.model.view()),view);assert.equal(f.counts().puts,0);
});
test('status text cannot report saved while loading, blocked, uncertain, busy or dirty', async () => {
  const f=fixture();assert.ok(!status(f.model.view()).includes('已在本机确认'));await f.model.save(snapshot({opacity:24}));await f.model.restore();
  assert.match(status(f.model.view()),/尚未保存/);await f.model.save(snapshot({opacity:24}));assert.match(status(f.model.view()),/已在本机确认/);
  const blocked=fixture({raw:'{'});await blocked.model.restore();assert.match(status(blocked.model.view()),/不会覆盖/);
  const unknown=fixture({flush(){throw new Error('lost');}});await unknown.model.restore();await unknown.model.save(snapshot({opacity:23}));assert.match(status(unknown.model.view()),/尚未确认/);
});
test('non-string stored values cannot be treated as an absent key', async () => {
  for(const raw of [true,7,null,[],{}]) {
    const f=fixture({raw});assert.equal(await f.model.restore(),false);assert.equal(f.model.view().state,'read-failed');
    assert.equal(await f.model.save(snapshot()),false);assert.equal(f.counts().puts,0);assert.equal(f.backend.cache,raw);
  }
});
test('stale preflight read response never issues put or changes confirmed original', async () => {
  const gate=deferred(),f=fixture({read(n,b){return n===2?gate.promise:b.cache;}});await f.model.restore();const original=f.model.view().originalRaw;
  const saved=f.model.save(snapshot({opacity:25}));await tick();const notices=f.notices.length;f.owner.epoch='page-A:foreground-2';gate.resolve(f.backend.cache);
  assert.equal(await saved,false);assert.equal(f.counts().puts,0);assert.equal(f.model.view().originalRaw,original);assert.equal(f.model.view().current.opacity,76);
  assert.equal(f.model.view().candidate.opacity,25);assert.equal(f.notices.length,notices);
});
test('recovery is refused while an issued operation still has a live pending promise', async () => {
  const gate=deferred(),f=fixture({async flush(n,b){await gate.promise;b.disk=b.cache;}});await f.model.restore();const saved=f.model.save(snapshot({opacity:25}));await tick();
  assert.equal(f.model.view().busy,true);assert.ok(f.model.view().unknownRaw);const counts=f.counts();assert.equal(await f.model.recover(),false);assert.deepEqual(f.counts(),counts);
  gate.resolve();assert.equal(await saved,true);
});
test('unknown recovery read rejection or malformed observation never flushes over unreadable state', async () => {
  for(const mode of ['reject','corrupt']) {
    const f=fixture({flush(){throw new Error('first flush lost');},read(n,b){if(n===3){if(mode==='reject')throw new Error('recover read unavailable');return '{';}return b.cache;}});
    await f.model.restore();await f.model.save(snapshot({opacity:26}));const original=f.model.view().originalRaw;
    assert.equal(await f.model.recover(),false);assert.equal(f.model.view().state,'write-unknown');assert.equal(f.counts().flushes,1);
    assert.equal(f.model.view().originalRaw,original);assert.equal(f.model.view().current.opacity,76);assert.ok(f.model.view().unknownRaw);
  }
});
test('stale recovery readback cannot adopt a confirmed observed value for the replacement owner', async () => {
  const gate=deferred(),f=fixture({flush(n,b){if(n===1)throw new Error('lost');b.disk=b.cache;},read(n,b){return n===4?gate.promise:b.cache;}});
  await f.model.restore();await f.model.save(snapshot({opacity:26}));const recovering=f.model.recover();await tick();const notices=f.notices.length;
  f.owner.epoch='page-A:foreground-2';gate.resolve(f.backend.cache);assert.equal(await recovering,false);assert.equal(f.model.view().state,'write-unknown');
  assert.equal(f.model.view().current.opacity,76);assert.equal(f.notices.length,notices);assert.ok(f.model.view().unknownRaw);
});
test('candidate boundary cannot mutate returned confirmed material or pending source arrays', async () => {
  const f=fixture({raw:JSON.stringify(snapshot({materials:[material('a')],daily:['one']}))});await f.model.restore();const raw=f.model.view().originalRaw;
  const returned=f.model.view();returned.current.materials[0].opacity=0;returned.current.daily.push('two');
  assert.equal(f.model.view().current.materials[0].opacity,76);assert.deepEqual(plain(f.model.view().current.daily),['one']);assert.equal(f.model.view().originalRaw,raw);
});
test('semantically escaped duplicate keys also fail before any record overwrite', async () => {
  const raw=JSON.stringify(snapshot()).replace('"theme":"白色"','"theme":"白色","th\\u0065me":"深色"');
  const f=fixture({raw});assert.equal(await f.model.restore(),false);assert.equal(await f.model.save(snapshot()),false);
  assert.equal(f.backend.disk,raw);assert.equal(f.counts().puts,0);
});
function peer(port){let model;const notices=[];model=new AppearancePreferences(port,{owned:()=>true,owner:()=> 'replacement-page:foreground-1',
  changed(){notices.push(plain(model.view()));}});return{model,notices};}
test('same namespace old live flush blocks all replacement reads puts and flushes',async()=>{
  const gate=deferred(),f=fixture({async flush(n,b){const captured=b.cache;if(n===1)await gate.promise;b.disk=captured;}});await f.model.restore();
  const original=f.model.view().originalRaw,old=f.model.save(snapshot({opacity:30}));await tick();f.model.dispose();const replacement=peer(f.port),b=replacement.model;
  const before=f.counts();assert.equal(await b.restore(),false);assert.equal(await b.save(snapshot({opacity:50})),false);assert.equal(await b.recover(),false);
  assert.deepEqual(f.counts(),before);assert.equal(b.view().busy,true);assert.equal(b.view().state,'write-unknown');assert.equal(b.view().originalRaw,original);
  assert.equal(b.view().current.opacity,76);assert.equal(JSON.parse(b.view().unknownRaw).opacity,30);assert.equal(b.view().candidate.opacity,50);
  gate.resolve();assert.equal(await old,false);assert.equal(b.view().busy,false);assert.equal(b.view().state,'write-unknown');
  assert.equal(JSON.parse(f.backend.disk).opacity,30);assert.equal(b.view().current.opacity,76);assert.ok(!status(b.view()).includes('已在本机确认'));
});
test('replacement explicitly reconciles terminal unknown before separately saving latest preview',async()=>{
  const gate=deferred(),f=fixture({async flush(n,b){const captured=b.cache;if(n===1)await gate.promise;b.disk=captured;}});await f.model.restore();
  const old=f.model.save(snapshot({opacity:30}));await tick();f.model.dispose();const b=peer(f.port).model;
  await b.restore();await b.save(snapshot({opacity:50}));gate.resolve();await old;assert.equal(await b.restore(),false);
  assert.equal(await b.recover(),true);assert.equal(b.view().current.opacity,30);assert.equal(b.view().candidate.opacity,50);assert.equal(b.view().dirty,true);
  assert.equal(f.counts().puts,1);assert.equal(f.counts().flushes,2);assert.equal(await b.save(b.view().candidate),true);
  assert.equal(b.view().current.opacity,50);assert.equal(JSON.parse(f.backend.disk).opacity,50);assert.equal(f.counts().puts,2);assert.equal(f.counts().flushes,3);
});
test('replacement cannot treat still-live unissued read as completed namespace ownership',async()=>{
  const gate=deferred(),f=fixture({read(n,b){return n===1?gate.promise:b.cache;}}),old=f.model.restore();f.model.dispose();const b=peer(f.port).model;
  const counts=f.counts();assert.equal(await b.restore(),false);assert.equal(await b.recover(),false);assert.equal(await b.save(snapshot({opacity:23})),false);
  assert.deepEqual(f.counts(),counts);assert.equal(b.view().busy,true);gate.resolve(f.backend.cache);await old;
  assert.equal(b.view().state,'read-failed');assert.equal(b.view().busy,false);assert.equal(await b.recover(),true);assert.equal(f.counts().puts,0);
});
test('terminal old unknown literal survives page replacement without automatic restore clearing it',async()=>{
  const f=fixture({flush(){throw new Error('terminal lost ACK');}});await f.model.restore();const original=f.model.view().originalRaw;
  await f.model.save(snapshot({opacity:31}));f.model.dispose();const b=peer(f.port).model,counts=f.counts();
  assert.equal(await b.restore(),false);assert.deepEqual(f.counts(),counts);assert.equal(b.view().originalRaw,original);assert.equal(JSON.parse(b.view().unknownRaw).opacity,31);
  assert.equal(b.view().state,'write-unknown');assert.equal(b.view().busy,false);
});
test('separate actual preference namespaces are independent of another live flush',async()=>{
  const gate=deferred(),a=fixture({async flush(n,b){await gate.promise;b.disk=b.cache;}});await a.model.restore();const pending=a.model.save(snapshot({opacity:31}));await tick();
  const b=fixture();assert.equal(await b.model.restore(),true);assert.equal(await b.model.save(snapshot({opacity:53})),true);
  assert.equal(b.model.view().current.opacity,53);assert.equal(a.model.view().busy,true);gate.resolve();await pending;
});
