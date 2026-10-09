'use strict';
// Verbatim current Index fields/methods + actual ETS preference model. ArkData,
// lifecycle siblings and ArkUI attributes are controlled seams, not device UI.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const sourcePath = path.resolve(__dirname,'../entry/src/main/ets/pages/Index.ets'), source = fs.readFileSync(sourcePath,'utf8');
const modelRoot = path.resolve(__dirname,'../entry/src/main/ets/model');
const modelSources = new Map(['Appearance','AppearancePreferences','UiStrings'].map(name=>[name,fs.readFileSync(path.join(modelRoot,name+'.ets'),'utf8')]));
function member(name, property=false) {
  const pattern=new RegExp('^  (?:@State\\s+)?(?:private\\s+)?(?:async\\s+)?'+name+'(?=\\(|\\s*:)','m'),match=pattern.exec(source);
  assert.ok(match,'actual Index member exists: '+name);const prefix='class Actual {\n';
  const parsed=ts.createSourceFile('actual.ts',prefix+source.slice(match.index)+'\n}',ts.ScriptTarget.ES2020,true,ts.ScriptKind.TS);
  const node=parsed.statements[0]?.members?.[0];assert.ok(node&&(property?ts.isPropertyDeclaration(node):ts.isMethodDeclaration(node)),'actual boundary '+name);
  assert.equal(node.name.getText(parsed),name);return source.slice(match.index,match.index+node.end-prefix.length).replace(/@State\s+/g,'');
}
function compile(input,filename) {
  const result=ts.transpileModule(input,{fileName:filename,reportDiagnostics:true,
    compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.CommonJS}});
  assert.deepEqual((result.diagnostics||[]).filter(x=>x.category===ts.DiagnosticCategory.Error),[],'actual source transpilation');return result.outputText;
}
function builder(name) {
  const start=source.indexOf('  '+name+'() {'),end=source.indexOf('\n  }',start);assert.ok(start>=0&&end>start,'actual builder '+name);
  const actual=source.slice(start,end+5);assert.equal((actual.match(/Column\(\{ space: 8 \}\) \{/g)||[]).length,1);
  // Only replace ArkUI's outer child syntax; keep every actual condition,
  // component, attribute, enabled predicate and click handler verbatim.
  return actual.replace('Column({ space: 8 }) {','const actualColumn = Column({ space: 8 }); {').replace('}.alignItems(','}\n    actualColumn.alignItems(');
}
const fieldNames=['appearanceStatus','appearanceView','preferenceStore','appearancePreferences','appearancePage','appearanceEpoch',
  'appearanceInputRevision','appearanceInputError','appearanceOperation','pageAlive','dark','theme','mode','glassOpacity','radius','themeColor','toneGray','lightness',
  'canvasBackground','solid','canvasLiquid','canvasBlur','canvasOpacity','fontFamily','locale','daily','materials','visualStyle','styleDepth'];
const methodNames=['appearanceOwner','appearanceOwned','appearanceStore','appearanceModel','refreshAppearanceState','waitAppearanceForeground','applyAppearance','restoreAppearance',
  'appearanceUiValue','persistAppearance','recoverAppearance','saveAppearancePreview','aboutToAppear','aboutToDisappear','foregroundChanged','t'];
const pageCode=compile('export class ActualIndexAppearance {\n'+fieldNames.map(name=>member(name,true)).join('\n')+'\n'+
  methodNames.map(name=>member(name)).join('\n')+'\n'+builder('AppearanceState')+'\n}',sourcePath);
const plain=value=>value===undefined?undefined:JSON.parse(JSON.stringify(value));
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
async function settle(){for(let i=0;i<80;i++)await Promise.resolve();}
function harness(options={}) {
  const modules=new Map(),events=[],nodes=[],host={page:'controlled-ui-context'},backend={cache:Object.hasOwn(options,'raw')?options.raw:undefined};
  backend.disk=backend.cache;let opens=0,has=0,gets=0,puts=0,flushes=0,uuid=0,page;
  function load(name){name=name.replace(/^\.\//,'');if(modules.has(name))return modules.get(name);assert.ok(modelSources.has(name));
    const exports={};modules.set(name,exports);vm.runInNewContext(compile(modelSources.get(name),path.join(modelRoot,name+'.ets')),
      {exports,require:load});return exports;}
  const actual={...load('Appearance'),...load('AppearancePreferences'),...load('UiStrings')};
  const store={
    hasSync(key){const n=++has;events.push({kind:'has',key,n});assert.equal(key,actual.appearancePreferenceKey);return options.has?options.has(n,backend):backend.cache!==undefined;},
    getSync(key,fallback){const n=++gets;events.push({kind:'get',key,fallback,n});assert.equal(key,actual.appearancePreferenceKey);return options.get?options.get(n,backend):backend.cache;},
    async put(key,raw){const n=++puts;events.push({kind:'put',key,raw,n});assert.equal(key,actual.appearancePreferenceKey);
      if(options.put)await options.put(raw,n,backend);else backend.cache=raw;},
    async flush(){const n=++flushes;events.push({kind:'flush',n});if(options.flush)await options.flush(n,backend);else backend.disk=backend.cache;}
  };
  const preferences={getPreferencesSync(context,{name}){const n=++opens;events.push({kind:'open',context,name,n});
    assert.equal(context,host);assert.equal(name,actual.appearancePreferenceName);if(options.open)options.open(n,backend);return store;}};
  function component(type,...args){const node={type,args,attributes:{}};nodes.push(node);let proxy;
    proxy=new Proxy({},{get(_,key){return(...values)=>{node.attributes[key]=values;return proxy;};}});return proxy;}
  const stop=()=>events.push({kind:'sibling-stop'}),controller={stop,dispose:stop};
  class MusicWorkbench{constructor(){events.push({kind:'sibling-music-create'});}dispose(){events.push({kind:'sibling-music-dispose'});return Promise.resolve();}
    background(){return Promise.resolve();}foreground(){} }
  class AttachmentFiles{previewCleanupPending(){return false;}}
  class AttachmentOpen{snapshot(){return{};}reconcile(){}}
  const globals={exports:{},...actual,preferences,util:{generateRandomUUID(){return'appearance-page-'+(++uuid);}},MusicWorkbench,AttachmentFiles,AttachmentOpen,
    $rawfile:value=>value,native:{releasePreview(){events.push({kind:'sibling-preview-release'});}},clearTimeout(){},
    Color:{Transparent:'transparent'},ButtonType:{Normal:'normal'},HorizontalAlign:{Start:'start'},
    Column:(...args)=>component('Column',...args),Text:(...args)=>component('Text',...args),Button:(...args)=>component('Button',...args)};
  vm.runInNewContext(pageCode,globals,{filename:sourcePath});
  function newPage() { const result=new globals.exports.ActualIndexAppearance();
  Object.assign(result,{foreground:true,musicEpoch:0,editorInputEpoch:0,editorOpen:false,workspaceRestoreTimer:-1,
    getUIContext:()=>({getHostContext:()=>host,getFont:()=>({registerFont(){events.push({kind:'font-register'});}})}),
    refreshPreview(){events.push({kind:'preview'});},restoreCardOrder(){},start(){},
    directInput:controller,inputPolicy:controller,fieldPolicy:controller,todoDragPosition(){},closeAttachmentPreview(){},
    mediaPlayback:{pauseForBackground(){}},exitMediaFullscreen(){},bindInputRules(){},refreshFieldCounts(){},
    markdown:controller,inlineImages:controller,queries:controller,cardOrder:controller,
    muted:()=> '#777777',accent:()=> '#7662BA'}); return result; }
  page=newPage();
  return{page,newPage,store,backend,events,nodes,actual,counts:()=>({opens,has,gets,puts,flushes}),
    async boot(){page.aboutToAppear();await settle();},
    render(){nodes.length=0;page.AppearanceState();return nodes;},
    click(id){const node=nodes.find(x=>x.attributes.id?.[0]===id);assert.ok(node,'actual rendered button '+id);
      assert.equal(node.attributes.enabled?.[0],true,'button enabled');return node.attributes.onClick[0]();},
    foreground(value){page.foreground=value;page.foregroundChanged();},close(){page.aboutToDisappear();},
    assertFrozen(){assert.equal(fs.readFileSync(sourcePath,'utf8'),source);for(const[name,text]of modelSources)
      assert.equal(fs.readFileSync(path.join(modelRoot,name+'.ets'),'utf8'),text);}};
}
module.exports={harness,member,source,sourcePath,modelSources,plain,deferred,settle,fieldNames,methodNames};
