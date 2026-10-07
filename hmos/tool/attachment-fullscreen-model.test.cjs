const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {test}=require('node:test'),assert=require('node:assert/strict');
const ts=require('C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../entry/src/main/ets/model/AttachmentFullscreen.ets'),'utf8');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText;
function load(mocks={}){const api={};vm.runInNewContext(compiled,{exports:api,Error,AppStorage:mocks.storage||{get:()=>undefined},require:key=>key==='@kit.ArkUI'?{window:mocks.window||{}}:{}});return api;}
const api=load(),settle=async()=>{for(let i=0;i<40;i++)await Promise.resolve();};
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
function fixture(options={}){
  const baseline={windowId:8,layoutFullscreen:false,immersive:false,orientation:9,statusBar:true,navigationBar:true,...options.baseline};
  const state={...baseline},calls=[],failures=new Map(),pauses=new Map(),views=[];let factories=0,captures=0;
  const write=async(key,value)=>{calls.push([key,value]);state[key]=value;
    if(pauses.has(key))await pauses.get(key).promise;
    if(failures.has(key))throw new Error(failures.get(key));};
  const port={capture:()=>{captures++;return baseline;},setLayout:value=>write('layoutFullscreen',value),setImmersive:value=>write('immersive',value),
    setBars:async(status,navigation)=>{calls.push(['bars',status,navigation]);state.statusBar=status;state.navigationBar=navigation;
      if(pauses.has('bars'))await pauses.get('bars').promise;if(failures.has('bars'))throw new Error(failures.get('bars'));},
    setOrientation:value=>write('orientation',value)};
  const model=new api.AttachmentFullscreen(async()=>{factories++;return options.factory?await options.factory(port):port;},7,()=>views.push(model.view()));
  return {model,baseline,state,calls,failures,pauses,views,port,get factories(){return factories;},get captures(){return captures;}};
}

test('native fullscreen hides bars and changes orientation, then restores each original value',async()=>{
  const f=fixture();await f.model.set(true);
  assert.equal(f.model.view().phase,'fullscreen');assert.equal(f.model.view().active,true);
  assert.deepEqual(f.calls,[['layoutFullscreen',true],['immersive',true],['bars',false,false],['orientation',7]]);
  await f.model.exit();assert.deepEqual(f.state,f.baseline);assert.equal(f.model.view().active,false);assert.equal(f.model.snapshot(),undefined);
  assert.deepEqual(f.calls.slice(-4),[['bars',true,true],['orientation',9],['layoutFullscreen',false],['immersive',false]]);
});

test('pre-existing immersive request and asymmetric bar policy survive snapshot and restoration',async()=>{
  const f=fixture({baseline:{layoutFullscreen:false,immersive:true,orientation:0,statusBar:false,navigationBar:true}});
  await f.model.set(true);await f.model.exit();assert.deepEqual(f.state,f.baseline);
});

test('repeated enter neither recreates the window port nor resets an existing fullscreen session',async()=>{
  const f=fixture();await f.model.set(true);const calls=f.calls.length;await f.model.set(true);
  assert.equal(f.factories,1);assert.equal(f.captures,1);assert.equal(f.calls.length,calls);await f.model.exit();
});

test('original snapshot and UI snapshot are immutable from caller mutations',async()=>{
  const f=fixture();await f.model.set(true);f.baseline.orientation=66;f.baseline.statusBar=false;
  const snapshot=f.model.snapshot();snapshot.orientation=100;const view=f.model.view();view.active=false;view.phase='idle';
  assert.equal(f.model.snapshot().orientation,9);assert.equal(f.model.view().phase,'fullscreen');
  await f.model.exit();assert.equal(f.state.orientation,9);assert.equal(f.state.statusBar,true);
});

test('exit before pending admission avoids any native writes',async()=>{
  const f=fixture(),enter=f.model.set(true),exit=f.model.exit();await Promise.all([enter,exit]);
  assert.equal(f.factories,0);assert.deepEqual(f.calls,[]);assert.equal(f.model.view().phase,'idle');
});

test('exit while the app window is being resolved cancels late creation without entering',async()=>{
  const gate=deferred(),f=fixture({factory:()=>gate.promise}),enter=f.model.set(true);await settle();
  const exit=f.model.exit();gate.resolve(f.port);await Promise.all([enter,exit]);
  assert.equal(f.captures,0);assert.deepEqual(f.calls,[]);assert.equal(f.model.view().retained,false);
});

test('exit during an accepted layout mutation waits then restores and never subsequently hides bars',async()=>{
  const f=fixture(),gate=deferred();f.pauses.set('layoutFullscreen',gate);const enter=f.model.set(true);await settle();
  const exit=f.model.exit();assert.equal(f.model.view().retained,true);gate.resolve();await Promise.all([enter,exit]);
  assert.equal(f.calls.some(x=>x[0]==='bars'&&x[1]===false),false);assert.deepEqual(f.state,f.baseline);
});

test('exit during accepted landscape change restores all prior values and does not reenter',async()=>{
  const f=fixture(),gate=deferred();f.pauses.set('orientation',gate);const enter=f.model.set(true);await settle();
  const exit=f.model.exit();gate.resolve();await Promise.all([enter,exit]);
  assert.deepEqual(f.state,f.baseline);assert.equal(f.model.view().phase,'idle');assert.equal(f.factories,1);
});

test('rapid fullscreen toggles cancel pending enter rather than performing two hidden transitions',async()=>{
  const f=fixture();await Promise.all([f.model.toggle(),f.model.toggle()]);assert.deepEqual(f.calls,[]);
  await f.model.toggle();assert.equal(f.model.view().phase,'fullscreen');await f.model.toggle();assert.equal(f.model.view().phase,'idle');
});

test('an enter API rejection which may already have changed the window is restored before returning failure',async()=>{
  const f=fixture();let n=0;f.port.setLayout=async value=>{f.calls.push(['layoutFullscreen',value]);f.state.layoutFullscreen=value;if(++n===1)throw new Error('ack lost');};
  await assert.rejects(f.model.set(true),/无法进入全屏/);assert.deepEqual(f.state,f.baseline);
  assert.equal(f.model.view().phase,'enter_failed');assert.equal(f.model.view().retained,false);assert.equal(f.factories,1);
});

test('failed restore attempts every remaining independent setting, retains the original window and requires explicit retry',async()=>{
  const f=fixture();await f.model.set(true);f.failures.set('bars','unknown bars');
  await assert.rejects(f.model.exit(),/Window restoration failed: system bars/);
  assert.deepEqual(f.calls.slice(-4),[['bars',true,true],['orientation',9],['layoutFullscreen',false],['immersive',false]]);
  assert.equal(f.model.view().phase,'restore_failed');assert.equal(f.model.view().active,true);assert.equal(f.model.snapshot().windowId,8);
  const count=f.calls.length;await settle();assert.equal(f.calls.length,count);assert.equal(f.factories,1);
  f.failures.clear();await f.model.exit();assert.equal(f.model.view().retained,false);assert.deepEqual(f.state,f.baseline);
  assert.equal(f.factories,1);assert.equal(f.captures,1);
});

test('failed cancellation restore is not automatically replayed by the enter catch handler',async()=>{
  const f=fixture(),gate=deferred();f.pauses.set('layoutFullscreen',gate);const enter=f.model.set(true);await settle();
  // Invalidate the running admission without queuing a second explicit exit.
  const nextEnter=f.model.set(true);f.failures.set('bars','restore rejected');gate.resolve();
  await assert.rejects(enter,/全屏布局尚未恢复/);await assert.rejects(nextEnter,/全屏布局尚未恢复/);
  assert.equal(f.calls.filter(x=>x[0]==='bars').length,1);assert.equal(f.model.view().phase,'restore_failed');
});

test('failed original enter and failed restoration retain obligation rather than recapturing the altered window',async()=>{
  const f=fixture();f.failures.set('orientation','orientation rejected');await assert.rejects(f.model.set(true));
  assert.equal(f.model.view().phase,'restore_failed');assert.equal(f.model.snapshot().orientation,9);
  const count=f.calls.length;await assert.rejects(f.model.set(true),/先重试退出/);assert.equal(f.calls.length,count);assert.equal(f.factories,1);
  f.failures.clear();await f.model.toggle();assert.equal(f.model.view().phase,'idle');assert.equal(f.captures,1);
});

test('invalid or unavailable baseline cannot mutate the window',async()=>{
  const f=fixture({baseline:{windowId:0}});await assert.rejects(f.model.set(true));assert.deepEqual(f.calls,[]);
  assert.equal(f.model.view().retained,false);assert.equal(f.model.view().phase,'enter_failed');
  const unavailable=fixture({factory:async()=>{throw new Error('window unavailable');}});await assert.rejects(unavailable.model.set(true));
  assert.equal(unavailable.model.view().active,false);assert.equal(unavailable.model.snapshot(),undefined);
});

test('actual WindowKit adapter binds only the exact initialized app main window',async()=>{
  const calls=[],storage=new Map([['hmos-fullscreen-policy-ready',true],['hmos-fullscreen-window-id',44]]);
  const main={getWindowProperties:()=>({id:44,isLayoutFullScreen:false}),getImmersiveModeEnabledState:()=>false,getPreferredOrientation:()=>9,
    setWindowLayoutFullScreen:async value=>calls.push(['layout',value]),setImmersiveModeEnabledState:value=>calls.push(['immersive',value]),
    setWindowSystemBarEnable:async names=>calls.push(['bars',...names]),setPreferredOrientation:async value=>calls.push(['orientation',value])};
  const actual=load({storage:{get:key=>storage.get(key)},window:{getLastWindow:async()=>main}}),port=await actual.createFullscreenWindow({});
  assert.deepEqual(JSON.parse(JSON.stringify(port.capture())),{windowId:44,layoutFullscreen:false,immersive:false,orientation:9,statusBar:true,navigationBar:true});
  await port.setBars(false,false);await port.setBars(true,true);await port.setLayout(true);await port.setImmersive(true);await port.setOrientation(7);
  assert.deepEqual(calls,[['bars'],['bars','status','navigation'],['layout',true],['immersive',true],['orientation',7]]);
  storage.set('hmos-fullscreen-policy-ready',false);await assert.rejects(actual.createFullscreenWindow({}),/窗口尚未准备好/);
  storage.set('hmos-fullscreen-policy-ready',true);storage.set('hmos-fullscreen-window-id',99);
  await assert.rejects(actual.createFullscreenWindow({}),/应用窗口已变化/);
});
