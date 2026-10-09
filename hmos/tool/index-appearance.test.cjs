'use strict';
const assert=require('node:assert/strict'),{test}=require('node:test');
const{harness,plain,deferred,settle,source}=require('./index-appearance-test-harness.cjs');
function raw(patch={}) {const h=harness();return JSON.stringify(Object.assign(new h.actual.AppearanceSnapshot(),patch));}
function preview(h,patch){Object.assign(h.page,patch);h.page.persistAppearance();}
const buttons=h=>h.render().filter(n=>n.type==='Button').map(n=>n.attributes.id?.[0]);

test('real aboutToAppear restores complete preferences through actual namespace/has/get',async()=>{
  const h=harness({raw:raw({theme:'深色',opacity:34,radius:8,locale:'ja',font:'Stored family',visualStyle:'paper'})});await h.boot();
  assert.equal(h.page.appearanceView.state,'ready');assert.equal(h.page.theme,'深色');assert.equal(h.page.dark,true);assert.equal(h.page.glassOpacity,34);
  assert.equal(h.page.radius,8);assert.equal(h.page.locale,'ja');assert.equal(h.page.fontFamily,'Stored family');assert.equal(h.page.visualStyle,'paper');
  assert.deepEqual(h.counts(),{opens:1,has:1,gets:1,puts:0,flushes:0});assert.equal(h.events.find(x=>x.kind==='open').name,'studio-appearance');h.assertFrozen();
});
test('actual missing key defaults are positively observed and never call get or put',async()=>{
  const h=harness();await h.boot();assert.equal(h.page.appearanceView.state,'ready');assert.equal(h.page.glassOpacity,76);
  assert.deepEqual(h.counts(),{opens:1,has:1,gets:0,puts:0,flushes:0});assert.deepEqual(buttons(h),[]);
});
test('actual existing empty or nonstring records block ordinary UI writes',async()=>{
  for(const value of ['',true,7,[],{}]){const h=harness({raw:value});await h.boot();preview(h,{glassOpacity:32});await settle();
    assert.equal(h.page.appearanceView.state,'read-failed');assert.equal(h.backend.cache,value);assert.equal(h.counts().puts,0);
    assert.match(h.page.appearanceStatus,/原数据仍保留/);assert.deepEqual(buttons(h),['appearance-recover']);}
});
test('actual preference open failure is retained and has no write path',async()=>{
  const h=harness({open(){throw new Error('namespace unavailable');}});await h.boot();preview(h,{glassOpacity:31});await settle();
  assert.equal(h.page.appearanceView.state,'read-failed');assert.deepEqual(h.counts(),{opens:1,has:0,gets:0,puts:0,flushes:0});
  assert.match(h.page.appearanceStatus,/不会覆盖/);
});
test('actual has/get failures cannot become missing defaults',async()=>{
  for(const phase of ['has','get']){const options={raw:raw({opacity:46})};options[phase]=()=>{throw new Error('read service fault');};
    const h=harness(options);await h.boot();preview(h,{glassOpacity:31});await settle();assert.equal(h.page.appearanceView.state,'read-failed');
    assert.equal(h.counts().puts,0);assert.equal(JSON.parse(h.backend.disk).opacity,46);}
});
test('malformed and incomplete originals remain unchanged after UI controls change',async()=>{
  for(const original of ['{','{}',raw({opacity:'bad'})]){const h=harness({raw:original});await h.boot();preview(h,{theme:'深色',glassOpacity:30});await settle();
    assert.equal(h.page.appearanceView.state,'read-failed');assert.equal(h.backend.disk,original);assert.equal(h.counts().puts,0);}
});
test('actual full UI snapshot captures every preference field without shared arrays',async()=>{
  const h=harness();await h.boot();const m=Object.assign(new h.actual.MaterialChoice(),{id:'music',enabled:true,depth:1.5});
  preview(h,{theme:'自定义',mode:'超透',glassOpacity:27,radius:9,themeColor:'#234567',toneGray:37,lightness:35,
    canvasBackground:'透明',solid:'#456789',canvasLiquid:true,canvasBlur:11,canvasOpacity:53,fontFamily:'Local family',locale:'ru',
    daily:['one'],materials:[m],visualStyle:'clay',styleDepth:1.6});
  h.page.daily.push('not captured');h.page.materials[0].opacity=12;await settle();const saved=JSON.parse(h.backend.disk);
  assert.deepEqual(saved,{theme:'自定义',mode:'超透',opacity:27,radius:9,accent:'#234567',grayscale:37,lightness:35,background:'透明',solid:'#456789',
    canvasLiquid:true,canvasBlur:11,canvasOpacity:53,font:'Local family',locale:'ru',daily:['one'],materials:[{id:'music',enabled:true,mode:'跟随主题',blur:22,
      opacity:76,radius:20,color:'',follow:'',depth:1.5}],visualStyle:'clay',styleDepth:1.6});assert.equal(h.page.dark,true);
});
test('normal actual UI save reuses one Preferences instance and confirms flush/readback',async()=>{
  const h=harness();await h.boot();preview(h,{glassOpacity:26});await settle();assert.equal(h.page.appearanceView.current.opacity,26);
  assert.equal(h.counts().opens,1);assert.equal(h.counts().puts,1);assert.equal(h.counts().flushes,1);assert.equal(h.page.appearanceView.dirty,false);
  assert.match(h.page.appearanceStatus,/已在本机确认/);assert.deepEqual(buttons(h),[]);
});
test('actual preview never reports saved while put/flush is pending',async()=>{
  const gate=deferred(),h=harness({async flush(n,b){await gate.promise;b.disk=b.cache;}});await h.boot();preview(h,{glassOpacity:25});await settle();
  assert.equal(h.page.appearanceView.busy,true);assert.equal(h.page.appearanceView.current.opacity,76);assert.equal(h.page.glassOpacity,25);
  assert.match(h.page.appearanceStatus,/正在保存/);assert.equal(h.backend.disk,undefined);h.render();
  assert.equal(h.nodes.find(n=>n.attributes.id?.[0]==='appearance-save-preview').attributes.enabled[0],false);
  gate.resolve();await settle();assert.equal(h.page.appearanceView.current.opacity,25);
});
test('actual rapid UI changes are serialized and older ACK never restores older UI',async()=>{
  const gate=deferred(),h=harness({async flush(n,b){if(n===1)await gate.promise;b.disk=b.cache;}});await h.boot();
  preview(h,{glassOpacity:30});preview(h,{glassOpacity:40});preview(h,{glassOpacity:50});await settle();assert.equal(h.counts().puts,1);
  assert.equal(h.page.glassOpacity,50);assert.equal(h.page.appearanceView.candidate.opacity,50);gate.resolve();await settle();
  assert.deepEqual(h.events.filter(x=>x.kind==='put').map(x=>JSON.parse(x.raw).opacity),[30,40,50]);assert.equal(h.page.glassOpacity,50);
  assert.equal(h.page.appearanceView.current.opacity,50);assert.equal(h.page.appearanceView.busy,false);
});
test('actual put lost ACK preserves origin and blocks later queued UI write',async()=>{
  const h=harness({raw:raw({opacity:66}),put(text,n,b){b.cache=text;throw new Error('put ACK lost');}});await h.boot();
  preview(h,{glassOpacity:30});preview(h,{glassOpacity:40});await settle();assert.equal(h.counts().puts,1);assert.equal(h.counts().flushes,0);
  assert.equal(h.page.appearanceView.state,'write-unknown');assert.equal(h.page.appearanceView.current.opacity,66);
  assert.equal(JSON.parse(h.page.appearanceView.unknownRaw).opacity,30);assert.equal(h.page.glassOpacity,40);
  assert.match(h.page.appearanceStatus,/尚未确认/);assert.deepEqual(buttons(h),['appearance-recover']);
});
test('actual flush unknown leaves saved current separate from visible candidate',async()=>{
  const h=harness({raw:raw({opacity:65}),flush(n,b){b.disk=b.cache;throw new Error('flush ACK lost');}});await h.boot();preview(h,{glassOpacity:30});await settle();
  assert.equal(JSON.parse(h.backend.disk).opacity,30);assert.equal(h.page.appearanceView.current.opacity,65);assert.equal(h.page.glassOpacity,30);
  assert.equal(h.page.appearanceView.state,'write-unknown');assert.match(h.page.appearanceStatus,/尚未确认/);
});
test('real rendered recovery button rereads without automatically putting UI candidate',async()=>{
  let unavailable=true;const h=harness({raw:raw({opacity:60}),open(){if(unavailable)throw new Error('offline');}});await h.boot();
  preview(h,{glassOpacity:24});await settle();unavailable=false;h.render();await h.click('appearance-recover');await settle();
  assert.equal(h.page.appearanceView.state,'ready');assert.equal(h.page.appearanceView.current.opacity,60);assert.equal(h.page.glassOpacity,24);
  assert.equal(h.page.appearanceView.dirty,true);assert.equal(h.counts().puts,0);assert.deepEqual(buttons(h),['appearance-save-preview']);
  await h.click('appearance-save-preview');await settle();assert.equal(h.counts().puts,1);assert.equal(JSON.parse(h.backend.disk).opacity,24);
});
test('actual read-only recovery with no candidate applies complete repaired stored UI',async()=>{
  const h=harness({raw:'{'});await h.boot();h.backend.cache=raw({opacity:42,theme:'深色',locale:'ko'});h.render();await h.click('appearance-recover');await settle();
  assert.equal(h.page.glassOpacity,42);assert.equal(h.page.theme,'深色');assert.equal(h.page.locale,'ko');assert.equal(h.page.appearanceView.dirty,false);
  assert.equal(h.counts().puts,0);assert.equal(h.counts().flushes,0);
});
test('actual unknown recovery confirms observed candidate with fresh flush/readback',async()=>{
  const h=harness({raw:raw({opacity:60}),flush(n,b){if(n===1)throw new Error('lost first flush');b.disk=b.cache;}});await h.boot();
  preview(h,{glassOpacity:33});await settle();h.render();await h.click('appearance-recover');await settle();
  assert.equal(h.page.appearanceView.state,'ready');assert.equal(h.page.appearanceView.current.opacity,33);assert.equal(h.page.glassOpacity,33);
  assert.equal(h.counts().puts,1);assert.equal(h.counts().flushes,2);assert.equal(JSON.parse(h.backend.disk).opacity,33);
});
test('actual repeated recovery failure does not put defaults or old confirmed bytes',async()=>{
  const h=harness({raw:'{'});await h.boot();preview(h,{glassOpacity:31});await settle();h.render();await h.click('appearance-recover');await settle();
  assert.equal(h.page.appearanceView.state,'read-failed');assert.equal(h.backend.disk,'{');assert.equal(h.counts().puts,0);
});
test('initial restore result does not override newer input produced before its await',async()=>{
  const h=harness({raw:raw({opacity:62})});h.page.aboutToAppear();preview(h,{glassOpacity:29});await settle();
  assert.equal(h.page.glassOpacity,29);assert.equal(h.page.appearanceView.current.opacity,62);assert.equal(h.page.appearanceView.candidate.opacity,29);
  assert.match(h.page.appearanceStatus,/尚未保存/);assert.equal(h.counts().puts,0);
});
test('initial restore response loses owner on actual background event and cannot apply UI',async()=>{
  const h=harness({raw:raw({opacity:62})});h.page.aboutToAppear();h.foreground(false);await settle();assert.equal(h.page.glassOpacity,76);
  assert.equal(h.page.appearancePreferences.view().state,'not-loaded');h.foreground(true);await settle();assert.equal(h.page.glassOpacity,62);
});
test('actual background/foreground events do not replay unknown write or recover automatically',async()=>{
  const h=harness({raw:raw(),flush(){throw new Error('unknown');}});await h.boot();preview(h,{glassOpacity:22});await settle();
  const counts=h.counts();h.foreground(false);h.foreground(true);await settle();assert.deepEqual(h.counts(),counts);
  assert.equal(h.page.appearanceView.state,'write-unknown');assert.match(h.page.appearanceStatus,/尚未确认/);
});
test('actual background during pending put prevents flush and later queue dispatch',async()=>{
  const gate=deferred(),h=harness({raw:raw(),async put(text,n,b){b.cache=text;await gate.promise;}});await h.boot();
  preview(h,{glassOpacity:25});preview(h,{glassOpacity:35});await settle();h.foreground(false);gate.resolve();await settle();
  assert.equal(h.counts().puts,1);assert.equal(h.counts().flushes,0);assert.equal(h.page.appearancePreferences.view().state,'write-unknown');
  h.foreground(true);await settle();assert.equal(h.page.appearanceView.current.opacity,76);assert.equal(h.page.glassOpacity,35);
});
test('actual background during pending flush preserves UI and original current after ACK',async()=>{
  const gate=deferred(),h=harness({raw:raw({opacity:63}),async flush(n,b){await gate.promise;b.disk=b.cache;}});await h.boot();
  preview(h,{glassOpacity:25});await settle();h.foreground(false);gate.resolve();await settle();h.foreground(true);await settle();
  assert.equal(h.page.appearanceView.state,'write-unknown');assert.equal(h.page.appearanceView.current.opacity,63);assert.equal(h.page.glassOpacity,25);
});
test('actual page close disposes preference model and prevents late current/UI update',async()=>{
  const gate=deferred(),h=harness({raw:raw({opacity:61}),async put(text,n,b){b.cache=text;await gate.promise;}});await h.boot();
  preview(h,{glassOpacity:25});await settle();const status=h.page.appearanceStatus;h.close();assert.equal(h.page.pageAlive,false);gate.resolve();await settle();
  assert.equal(h.page.appearanceStatus,status);assert.equal(h.page.appearancePreferences.view().current.opacity,61);assert.equal(h.counts().flushes,0);
  preview(h,{glassOpacity:35});await settle();assert.equal(h.counts().puts,1);
});
test('actual recovery late response cannot apply current to new foreground epoch',async()=>{
  const gate=deferred(),h=harness({raw:raw({opacity:60}),flush(n,b){if(n===1)throw new Error('unknown');return gate.promise.then(()=>{b.disk=b.cache;});}});await h.boot();
  preview(h,{glassOpacity:25});await settle();const recovering=h.page.recoverAppearance();await settle();h.foreground(false);h.foreground(true);gate.resolve();await recovering;await settle();
  assert.equal(h.page.glassOpacity,25);assert.equal(h.page.appearanceView.current.opacity,60);assert.equal(h.page.appearanceView.state,'write-unknown');
});
test('invalid latest UI candidate cannot be greenwashed by an older durable ACK',async()=>{
  const gate=deferred(),h=harness({raw:raw(),async flush(n,b){await gate.promise;b.disk=b.cache;}});await h.boot();
  preview(h,{glassOpacity:30});await settle();preview(h,{glassOpacity:NaN});assert.match(h.page.appearanceStatus,/尚未通过校验/);
  gate.resolve();await settle();assert.equal(h.counts().puts,1);assert.equal(h.page.appearanceView.current.opacity,30);assert.ok(Number.isNaN(h.page.glassOpacity));
  assert.match(h.page.appearanceStatus,/尚未通过校验/);assert.ok(!h.page.appearanceStatus.includes('已在本机确认'));assert.deepEqual(buttons(h),['appearance-save-preview']);
});
test('actual save-preview handler cannot overwrite invalid inputs',async()=>{
  const h=harness({raw:raw()});await h.boot();preview(h,{locale:'bad'});await settle();h.render();await h.click('appearance-save-preview');await settle();
  assert.equal(h.counts().puts,0);assert.match(h.page.appearanceStatus,/尚未通过校验/);preview(h,{locale:'en'});await settle();
  assert.equal(h.counts().puts,1);assert.equal(h.page.appearanceView.current.locale,'en');assert.match(h.page.appearanceStatus,/已在本机确认/);
});
test('original external drift blocks real Index save and preserves latest UI',async()=>{
  const h=harness({raw:raw({opacity:63})});await h.boot();h.backend.cache=raw({opacity:48});preview(h,{glassOpacity:27});await settle();
  assert.equal(h.page.appearanceView.state,'read-failed');assert.equal(h.page.appearanceView.current.opacity,63);assert.equal(h.page.glassOpacity,27);
  assert.equal(h.counts().puts,0);assert.equal(JSON.parse(h.backend.cache).opacity,48);
});
test('actual Index narrow edits preserve original top/material future values',async()=>{
  const h0=harness(),m=Object.assign(new h0.actual.MaterialChoice(),{id:'music'});
  const original=raw({materials:[m]}).replace('"id":"music"','"id":"music","future":{"large":9007199254740993123}').slice(0,-1)+',"future":{"raw":9007199254740993123456789}}';
  const h=harness({raw:original});await h.boot();preview(h,{glassOpacity:38});await settle();
  assert.ok(h.backend.disk.includes('"large":9007199254740993123'));assert.ok(h.backend.disk.includes('"raw":9007199254740993123456789'));
  assert.equal(h.page.appearanceView.current.opacity,38);h.assertFrozen();
});
test('actual rendered status contains no unconditional saved fallback or technical raw data',async()=>{
  const h=harness({raw:'{'});await h.boot();const text=h.render().filter(n=>n.type==='Text').map(n=>n.args[0]).join(' ');
  assert.match(text,/不会覆盖/);assert.ok(!text.includes('自动保存在本机'));assert.ok(!text.includes('appearance-v1'));
  assert.match(source,/if \(this\.settingsPage !== '设置'\) \{ this\.AppearanceState\(\) \}/);
});
test('busy unknown recovery button is disabled and cannot start another service operation',async()=>{
  const gate=deferred(),h=harness({raw:raw(),flush(n,b){if(n===1)throw new Error('unknown');return gate.promise.then(()=>{b.disk=b.cache;});}});await h.boot();
  preview(h,{glassOpacity:28});await settle();const recovering=h.page.recoverAppearance();await settle();const counts=h.counts();
  // Loading shows accurate progress; the recovery action itself is guarded.
  await h.page.recoverAppearance();assert.deepEqual(h.counts(),counts);assert.match(h.page.appearanceStatus,/正在读取/);
  gate.resolve();await recovering;await settle();assert.equal(h.counts().puts,1);
});
test('actual new Index page cannot read or write over old closed page live flush',async()=>{
  const gate=deferred(),h=harness({raw:raw(),async flush(n,b){const captured=b.cache;if(n===1)await gate.promise;b.disk=captured;}});await h.boot();
  preview(h,{glassOpacity:30});await settle();h.close();const replacement=h.newPage(),counts=h.counts();replacement.aboutToAppear();await settle();
  replacement.glassOpacity=50;replacement.persistAppearance();await settle();await replacement.recoverAppearance();
  assert.deepEqual(h.counts(),counts);assert.equal(replacement.appearanceView.state,'write-unknown');assert.equal(replacement.appearanceView.busy,true);
  assert.equal(replacement.appearanceView.current.opacity,76);assert.equal(JSON.parse(replacement.appearanceView.unknownRaw).opacity,30);
  assert.equal(replacement.glassOpacity,50);gate.resolve();await settle();assert.equal(replacement.appearanceView.busy,false);
  assert.equal(replacement.appearanceView.state,'write-unknown');assert.equal(replacement.appearanceView.current.opacity,76);
  assert.equal(JSON.parse(h.backend.disk).opacity,30);assert.ok(!replacement.appearanceStatus.includes('已在本机确认'));
});
test('actual replacement page confirms old terminal outcome before explicit new preview save',async()=>{
  const gate=deferred(),h=harness({raw:raw(),async flush(n,b){const captured=b.cache;if(n===1)await gate.promise;b.disk=captured;}});await h.boot();
  preview(h,{glassOpacity:30});await settle();h.close();const replacement=h.newPage();replacement.aboutToAppear();await settle();
  replacement.glassOpacity=50;replacement.persistAppearance();gate.resolve();await settle();await replacement.recoverAppearance();await settle();
  assert.equal(replacement.appearanceView.current.opacity,30);assert.equal(replacement.glassOpacity,50);assert.equal(replacement.appearanceView.dirty,true);assert.equal(h.counts().puts,1);
  replacement.saveAppearancePreview();await settle();assert.equal(replacement.appearanceView.current.opacity,50);assert.equal(JSON.parse(h.backend.disk).opacity,50);
  assert.equal(h.counts().puts,2);assert.equal(h.counts().flushes,3);
});
test('actual new page preserves old terminal unknown bytes and needs user recovery',async()=>{
  const h=harness({raw:raw({opacity:64}),flush(){throw new Error('terminal failed ACK');}});await h.boot();preview(h,{glassOpacity:29});await settle();h.close();
  const replacement=h.newPage(),counts=h.counts();replacement.aboutToAppear();await settle();assert.deepEqual(h.counts(),counts);
  assert.equal(replacement.appearanceView.state,'write-unknown');assert.equal(replacement.appearanceView.current.opacity,64);
  assert.equal(JSON.parse(replacement.appearanceView.unknownRaw).opacity,29);assert.ok(!replacement.appearanceStatus.includes('已在本机确认'));
});
