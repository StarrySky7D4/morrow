'use strict';
// Execute the actual product ETS. Controlled SDK/files providers qualify host
// lifecycle behavior only, not AVPlayer codecs, native export, or a device.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.HMOS_TYPESCRIPT_PATH || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const MODEL = path.resolve(__dirname, '../entry/src/main/ets/model/MusicPlayback.ets');
const PLATFORM = path.resolve(__dirname, '../entry/src/main/ets/pages/PlatformMusicPlayer.ets');
function load(file, mocks = {}) {
  const source = fs.readFileSync(file, 'utf8');
  const output = ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020},fileName:file,reportDiagnostics:true});
  assert.deepEqual((output.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error), []);
  const api = {}; vm.runInNewContext(output.outputText, {exports:api,require:key=>{
    assert.ok(Object.hasOwn(mocks,key), `actual module dependency ${key}`); return mocks[key];
  },Error,Number,Promise}, {filename:file}); return api;
}
const api = load(MODEL);
const settle = async () => { for (let i=0;i<100;i++) await Promise.resolve(); };
function deferred() { let resolve,reject; const promise=new Promise((a,b)=>{resolve=a;reject=b;}); return {promise,resolve,reject}; }
function identity(id='A', revision='1') {
  const value=new api.MusicPlaybackIdentity(); value.trackId=id; value.libraryRevision=revision;
  value.importOperation='import-'+id; value.byteLength=10; value.sha256=(id==='B'?'b':'a').repeat(64); return value;
}
function source(id='A', revision='1') {
  const value=new api.MusicPlaybackSource(); Object.assign(value,identity(id,revision)); value.token='lease-'+id; return value;
}
function player() {
  const calls=[],listeners={};
  return {calls,listeners,onState:f=>listeners.state=f,onPosition:f=>listeners.position=f,
    onDuration:f=>listeners.duration=f,onError:f=>listeners.error=f,
    load:async function(){calls.push('load');listeners.state('initialized');},
    prepare:async function(){calls.push('prepare');listeners.duration(9000);listeners.state('prepared');},
    play:async function(){calls.push('play');listeners.state('playing');},
    pause:async function(){calls.push('pause');listeners.state('paused');},
    seek:async value=>calls.push(['seek',value]),release:async()=>calls.push('release')};
}
function fixture(options={}) {
  const calls=[],players=[],sources=[],owned=new Set(),views=[],admissions=[];
  const f={owner:true,options,calls,players,sources,owned,views,admissions};
  const hooks={isCurrentOwner:()=>f.owner,changed:()=>views.push(f.model.view()),
    // Controlled admission seam only: product uses the real Library.select CAS.
    admitSelection:async id=>{admissions.push(api.musicPlaybackIdentityCopy(id));return options.admitSelection?options.admitSelection(id):api.musicPlaybackIdentityCopy(id);},
    ownsSource:s=>owned.has(s),
    acquire:async id=>{
      calls.push(['acquire',id.trackId,id.libraryRevision]);
      const s=options.acquire?await options.acquire(id):source(id.trackId,id.libraryRevision);
      sources.push(s); owned.add(s); return s;
    },
    createPlayer:async s=>{calls.push(['create',s.token]);const p=options.create?await options.create(s):player();players.push(p);return p;},
    releaseSource:async s=>{assert.ok(owned.has(s),'release exact acquired lease object');calls.push(['release-source',s.token]);
      if(options.releaseSource)await options.releaseSource(s);owned.delete(s);},
    adjacent:(current,direction)=>options.adjacent?options.adjacent(current,direction):identity(current.trackId==='A'?'B':'A','2')};
  f.model=new api.MusicPlayback(hooks); f.hooks=hooks; return f;
}
function platformFixture(options={}) {
  const calls=[],listeners={},src=source(),descriptor=new api.MusicPlaybackDescriptor();
  descriptor.token=src.token;descriptor.fd=77;descriptor.byteLength=10;descriptor.sha256=src.sha256;
  const f={calls,listeners,source:src,descriptor,options,owned:true,descriptorOwned:true};
  const p={duration:9000,currentTime:0,on:(event,callback)=>listeners[event]=callback,
    prepare:async()=>calls.push('prepare'),play:async()=>calls.push('play'),pause:async()=>calls.push('pause'),
    seek:(value,mode)=>calls.push(['seek',value,mode]),release:async()=>{calls.push('player-release');
      if(options.release)await options.release();if(options.releaseFailure)throw new Error('player release rejected');}};
  Object.defineProperty(p,'fdSrc',{set:value=>{calls.push(['fdSrc',value]);listeners.stateChange('initialized');}});
  const files={ownsSource:s=>s===src&&f.owned,open:async s=>{assert.equal(s,src);calls.push('open-owned');return options.open?options.open(s):descriptor;},
    ownsDescriptor:(s,d)=>s===src&&d===descriptor&&f.descriptorOwned,
    close:async d=>{assert.equal(d,descriptor);calls.push('fd-close');if(options.closeFailure)throw new Error('FD close rejected');}};
  const mocks={'../model/MusicPlayback':api,'@kit.BasicServicesKit':{},
    '@kit.CoreFileKit':{fileIo:{stat:async fd=>{calls.push(['stat',fd]);if(options.stat) return options.stat(fd);
      return {isFile:()=>!options.notFile,size:options.wrongLength?11:10};}}},
    '@kit.MediaKit':{media:{SeekMode:{SEEK_CLOSEST:2},createAVPlayer:async()=>{calls.push('player-create');
      if(options.createFailure)throw new Error('create failed');return options.create?options.create():p;}}}};
  f.platform=load(PLATFORM,mocks);f.files=files;f.player=p;return f;
}
module.exports={api,MODEL,PLATFORM,load,settle,deferred,identity,source,player,fixture,platformFixture};
