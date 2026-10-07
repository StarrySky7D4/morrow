// Explicit media qualification stages only. Root owns the device; loading this module does
// not issue device commands. It never seeds, imports, publishes or restarts.
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
process.env.HMOS_DEVICE=process.env.HMOS_DEVICE||'127.0.0.1:5555';
assert.equal(process.env.HMOS_DEVICE,'127.0.0.1:5555','only the root-owned existing device');
process.env.HMOS_REPORT_DIR=process.env.HMOS_REPORT_DIR||path.resolve(__dirname,'../reports/ui-source/v16/device-current');
const {f,a,editorSeek}=require('./inline-device-check.cjs');
const hdc='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe',out=process.env.HMOS_REPORT_DIR;
const fixture='HMOS-media-20261007-A',expectedVersion=process.env.HMOS_EXPECTED_VERSION||'0.1.0-hmos-dev.16';
const packageSha=process.env.HMOS_EXPECTED_ZIP_SHA||process.env.ZIP_SHA||process.env.HMOS_EXPECTED_HAP_SHA256||'NOT_SUPPLIED';
assert.match(expectedVersion,/^0\.1\.0-hmos-dev\.[0-9]+$/,'explicit application version');
assert.ok(packageSha==='NOT_SUPPLIED'||/^[A-Fa-f0-9]{64}$/.test(packageSha),'expected package SHA256');
const assets={audio:{name:'HMOS-dev14-tone.wav',id:'asset-draft-import-62a954f711c5144c3f2e333407ecd11f5f1aad374634f95e42a9787a7742ff4f'},
 video:{name:'HMOS-dev14-bars.mp4',id:'asset-draft-import-eaa74272b78711cf088e3953cd54830e6f3e583084dbe34eaafd4798b9ee9e77'}};
const journal=path.join(out,'progress-media-controls.json');let active,serial=0,journalSerial=0;
const wait=ms=>{assert.ok(ms>=0&&ms<=5000);Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,ms);};
function journalIO(operation){
 // Retry only local journal I/O. Never repeat a device command after any
 // uncertain acknowledgement. Windows can briefly reject a shared file open.
 for(let attempt=0;;attempt++){try{return operation();}catch(error){
  if(attempt>=3||!['UNKNOWN','EBUSY','EACCES','EPERM'].includes(error.code))throw error;wait(50*(attempt+1));
 }}
}
function readProgress(){return journalIO(()=>{try{return JSON.parse(fs.readFileSync(journal,'utf8'));}catch(error){
 if(error.code==='ENOENT')return {fixture,device:process.env.HMOS_DEVICE,stages:[]};throw error;
}});}
function persist(value){
 assert.equal(value.fixture,fixture);assert.equal(value.device,process.env.HMOS_DEVICE);
 const pending=journal+'.'+process.pid+'-'+(++journalSerial)+'.pending';
 try{journalIO(()=>fs.writeFileSync(pending,JSON.stringify(value,null,2)+'\n',{flag:'w'}));journalIO(()=>fs.renameSync(pending,journal));}
 catch(error){error.message+='; prior journal retained; pending local evidence: '+pending;throw error;}
}
function update(){if(active){const p=readProgress(),i=p.stages.findIndex(s=>s.label===active.label);assert.ok(i>=0);p.stages[i]=active;persist(p);}}
function command(...args){
 const action={arguments:args,phase:'INTENT',at:new Date().toISOString()};if(active){active.actions.push(action);update();}
 const reply=cp.execFileSync(hdc,['-t',process.env.HMOS_DEVICE,...args.map(String)],{encoding:'utf8',timeout:30000,maxBuffer:4*1024*1024});
 assert.ok(!/illegal|incorrect|failed|\berror\b|please confirm that the coordinate/i.test(reply)||/no error/i.test(reply),reply);
 action.phase='ACKNOWLEDGED';action.reply=reply;update();return reply;
}
function rawInput(action,...values){return command('shell','uitest','uiInput',action,...values.map(v=>String(Math.round(v))));}
function bounds(value){if(Array.isArray(value))return value.map(Number);const matches=String(value||'').match(/-?\d+(?:\.\d+)?/g);return matches&&matches.length>=4?matches.slice(0,4).map(Number):undefined;}
function normalize(value){
 const p=value.attributes||value,node={type:p.type,id:p.id||undefined,text:p.originalText??p.text??'',bounds:bounds(p.bounds),
  hitTestBehavior:String(p.hitTestBehavior||'').replace('HitTestMode.',''),opacity:p.opacity===undefined?undefined:Number(p.opacity),
  visible:p.visible,enabled:p.enabled,accessibilityText:p.accessibilityText,children:(value.children||[]).map(normalize)};return node;
}
const flat=tree=>f.d.flatten(tree),find=(tree,id)=>flat(tree).find(x=>x.n.id===id)?.n;
function observedNode(tree,id){const n=find(tree,id);assert.ok(n&&n.bounds&&n.bounds.every(Number.isFinite)&&n.bounds[2]>n.bounds[0]&&n.bounds[3]>n.bounds[1],'observed bounds '+id);return n;}
function center(node){const b=node.bounds;return [Math.round((b[0]+b[2])/2),Math.round((b[1]+b[3])/2)];}
function capture(label){
 assert.match(label,/^[A-Za-z0-9-]{1,100}$/);for(const suffix of ['.png','.json','.raw.json','.capture.json'])assert.ok(!fs.existsSync(path.join(out,label+suffix)),'fresh capture '+label+suffix);
 const own='/data/local/tmp/hmos-media-'+process.pid+'-'+(++serial)+'-'+label;
 const png=path.join(out,label+'.png'),rawFile=path.join(out,label+'.raw.json'),started=Date.now();
 // Screenshot comes FIRST. Dumping layout before screenshot can outlast the
 // product's 3-second control timeout; neither capture proves the other's time.
 command('shell','snapshot_display','-t','png','-f',own+'.png');const screenshotAt=Date.now();
 command('shell','uitest','dumpLayout','-p',own+'.json');const layoutAt=Date.now();
 command('file','recv',own+'.png',png);command('file','recv',own+'.json',rawFile);
 const raw=JSON.parse(fs.readFileSync(rawFile,'utf8')),tree=(Array.isArray(raw)?raw:[raw]).map(normalize);
 assert.ok(fs.readFileSync(png).subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])),'actual PNG');
 fs.writeFileSync(path.join(out,label+'.json'),JSON.stringify(tree,null,2)+'\n');
 const metadata={label,started,screenshotAt,layoutAt,order:'screenshot before raw native dump',visualReview:'REQUIRED; node presence is not rendered visibility',
  controls:controlEvidence(tree),files:{png,json:path.join(out,label+'.json'),raw:rawFile}};
 fs.writeFileSync(path.join(out,label+'.capture.json'),JSON.stringify(metadata,null,2)+'\n');
 // Only this invocation's fresh, exactly named capture artifacts are removed.
 command('shell','rm','-f',own+'.png',own+'.json');if(active){active.captures.push(metadata);update();}return tree;
}
function controlEvidence(tree){const n=find(tree,'attachment-media-controls'),opacity=n?.opacity;return {present:!!n,opacity,hitTestBehavior:n?.hitTestBehavior,
  inputState:'UNKNOWN',inputStateReason:'dump hitTestBehavior does not prove dispatch',
  renderState:Number.isFinite(opacity)?opacity<=0.02?'OPACITY_ZERO':opacity>=0.98?'OPACITY_ONE':'OPACITY_TRANSITION':'NOT_EXPOSED; inspect PNG'};}
function assertFixture(tree){
 assert.equal(find(tree,'draft-title')?.text,fixture,'exact existing draft title');const rows=flat(tree).filter(x=>x.n.id?.startsWith('draft-asset:'));
 assert.equal(rows.length,2,'exact two existing pins');for(const asset of Object.values(assets)){
  const row=rows.find(x=>x.n.id==='draft-asset:'+asset.id);assert.ok(row,'immutable observed asset ID '+asset.id);
  assert.ok(flat([row.n]).some(x=>x.n.text===asset.name),'exact asset filename '+asset.name);
 }
 return {title:fixture,assets,draftJournalId:'NOT_EXPOSED_BY_UI; asset IDs are checked'};
}
function modalOwnership(tree,kind){
 assert.equal(find(tree,'draft-title')?.text,fixture,'exact exposed draft title');
 const rows=flat(tree).filter(x=>x.n.id?.startsWith('draft-asset:'));
 if(rows.length===2){return {fixture:assertFixture(tree),currentExposedRows:2,pinEvidence:'FRESH_TWO_PIN_EDITOR_ROWS'};}
 assert.ok(rows.length<2,'unexpected extra attachment rows while observing modal');
 for(const row of rows){const asset=Object.values(assets).find(v=>'draft-asset:'+v.id===row.n.id);assert.ok(asset,'exposed row still has the admitted fixture identity');
  assert.ok(flat([row.n]).some(x=>x.n.text===asset.name),'exposed row filename still matches');}
 // Landscape can clip editor rows behind the modal. This does not re-read the
 // pins: reuse only a completed preview on the exact version/package session,
 // and revalidate the saved pre-preview two-row artifact on the host.
 const evidence=readProgress().stages.filter(s=>s.phase==='PASS'&&s.expectedVersion===expectedVersion&&s.expectedPackageSha===packageSha&&
  typeof s.result?.noAutoplay==='string'&&s.captures?.some(c=>c.label.endsWith('-before')&&c.files?.json)&&
  s.result?.first?.kind===kind&&s.result?.first?.assetId===assets[kind].id&&s.result?.first?.name===assets[kind].name&&
  s.result?.second?.assetId===assets[kind].id&&s.result?.first?.position===0&&s.result?.second?.position===0&&
  s.result?.first?.playing===false&&s.result?.second?.playing===false).at(-1);
 assert.ok(evidence,'same package has an explicitly completed preview for this exact pin');
 const before=evidence.captures.find(c=>c.label.endsWith('-before'));assert.ok(before?.files?.json,'saved pre-preview editor observation');
 const previous=JSON.parse(fs.readFileSync(before.files.json,'utf8'));assertFixture(previous);
 return {title:fixture,currentExposedRows:rows.length,pinEvidence:'PRIOR_CONFIRMED_TWO_PINS; NOT_A_FRESH_PIN_REREAD',
  priorStage:evidence.label,priorEditorJson:before.files.json,currentVisibility:'rows not fully exposed in the current modal viewport'};
}
function modal(tree,kind){
 const asset=assets[kind];assert.ok(asset,'explicit audio/video kind');const ownership=modalOwnership(tree,kind);observedNode(tree,'attachment-preview-dialog');
 assert.ok(flat(tree).some(x=>x.n.text===asset.name&&x.parents.some(p=>p.id==='attachment-preview-dialog')),'exact selected modal filename');
 for(const id of ['attachment-media-read-failure','attachment-media-playback-failure','attachment-fullscreen-failure','attachment-media-loading'])assert.ok(!find(tree,id),'no '+id);
 const time=observedNode(tree,'attachment-media-time').text.match(/^(\d+):(\d+) \/ (\d+):(\d+)$/);assert.ok(time,'actual current/total time');
 const position=Number(time[1])*60+Number(time[2]),duration=Number(time[3])*60+Number(time[4]);assert.equal(duration,12,'known own 12-second fixture');
 const button=observedNode(tree,'attachment-media-toggle'),label=[button.text,button.accessibilityText,...flat(button.children||[]).map(x=>x.n.text)].filter(Boolean).join(' ');
 const playing=/Ⅱ|暂停|Pause/i.test(label),paused=/▷|播放|Play/i.test(label);assert.notEqual(playing,paused,'observed known playing/paused glyph or accessibility label '+label);
 return {kind,name:asset.name,assetId:asset.id,ownership,position,duration,toggleLabel:label,playing,controls:controlEvidence(tree),
  zone:observedNode(tree,'attachment-media-gesture-zone').bounds,fullscreenLabel:observedNode(tree,'attachment-media-fullscreen').text};
}
function quickControl(tree,id,point){
 const state=controlEvidence(tree);assert.ok(Number.isFinite(state.opacity),'fresh numeric control opacity is required; presence and hitTestBehavior are insufficient');
 const target=observedNode(tree,id),zone=observedNode(tree,'attachment-media-gesture-zone'),xy=point||center(target);
 if(state.opacity<=0.02){
  const started=Date.now();rawInput('click',...center(zone));wait(500);
  const revealed=capture((active?.label||'manual')+'-reveal-'+serial);
  assert.ok(controlEvidence(revealed).opacity>=0.98,'stop: reveal tap did not finish displaying the observed controls');
  assert.ok(Date.now()-started<2600,'stop if reveal observation consumed control timeout');
  const freshTarget=observedNode(revealed,id);assert.deepEqual(freshTarget.bounds,target.bounds,'target bounds did not change during reveal');
 }else{assert.ok(state.opacity>=0.98,'stop: controls are in transition; inspect before a new explicit action');}
 // Opacity proves only the displayed target. Stage-specific glyph, time and
 // fullscreen observations below must independently prove the click effect.
 rawInput('click',...xy);
}
function runStage(label,body){
 assert.match(label,/^[A-Za-z0-9-]{1,70}$/);const p=readProgress();assert.ok(!p.stages.some(s=>s.label===label),'stage exists: inspect saved outcome; no automatic replay');
 active={label,phase:'RUNNING',expectedVersion,expectedPackageSha:packageSha,packageIdentityProof:'caller expected hash; install bytes verified separately by root',started:new Date().toISOString(),actions:[],captures:[]};p.stages.push(active);persist(p);
 try{const bundle=command('shell','bm','dump','-n','dev.morrow.hmos');assert.ok(bundle.includes(expectedVersion),'fresh installed expected bundle version '+expectedVersion);
  const result=body(label);active.phase='PASS';active.result=result;active.completed=new Date().toISOString();update();console.log(JSON.stringify(active,null,2));return result;
 }catch(error){active.phase='FAILED_OR_UNKNOWN';active.error=error.message;update();throw error;}finally{active=undefined;}
}
function inspect(label='inspect'){return runStage(label,key=>{const tree=capture(key+'-observed'),modalOpen=!!find(tree,'attachment-preview-dialog');
 const observed=modalOpen?modal(tree,flat(tree).some(x=>x.n.id==='attachment-video-surface')?'video':'audio'):undefined;
 return {fixture:observed?observed.ownership:assertFixture(tree),modalOpen,modal:observed,visualReview:'inspect fresh PNG before claims about hidden controls or decoded video'};});}
function preview(kind,label='preview-'+kind){return runStage(label,key=>{
 const before=capture(key+'-before');assertFixture(before);assert.ok(!find(before,'attachment-preview-dialog'),'do not replay an already open preview');
 const n=editorSeek(n=>n.id==='draft-asset-preview:'+assets[kind].id,'exact '+kind+' preview');rawInput('click',...center(n));wait(250);
 let tree=capture(key+'-prepared');const first=modal(tree,kind);assert.equal(first.playing,false,'no autoplay');assert.equal(first.position,0);
 wait(1500);tree=capture(key+'-still-paused');const second=modal(tree,kind);assert.equal(second.playing,false);assert.equal(second.position,0);
 return {first,second,noAutoplay:'observed zero position and paused glyph across captures',visualReview:'PNG required for hidden controls and video frame'};
});}
function playback(kind,label='playback-'+kind){return runStage(label,key=>{
 let tree=capture(key+'-before'),before=modal(tree,kind);assert.equal(before.playing,false,'stage requires an observed paused session');assert.ok(before.position<9,'leave enough fixture time to observe movement');
 quickControl(tree,'attachment-media-toggle');tree=capture(key+'-playing');const started=modal(tree,kind);assert.equal(started.playing,true);
 wait(1400);tree=capture(key+'-advanced');const advanced=modal(tree,kind);assert.equal(advanced.playing,true);assert.ok(advanced.position>before.position,'actual time advances');
 quickControl(tree,'attachment-media-toggle');tree=capture(key+'-paused');const paused=modal(tree,kind);assert.equal(paused.playing,false);
 wait(1300);tree=capture(key+'-pause-stable');const stable=modal(tree,kind);assert.equal(stable.playing,false);assert.equal(stable.position,paused.position,'no continued playback at whole-second UI resolution');
 return {before,started,advanced,paused,stable,audibleOutput:'NOT_PROVEN_BY_UI'};
});}
function fullscreen(kind,label='fullscreen-'+kind){return runStage(label,key=>{
 let tree=capture(key+'-before'),before=modal(tree,kind);assert.equal(before.playing,false);assert.equal(before.fullscreenLabel,'全屏');
 quickControl(tree,'attachment-media-fullscreen');wait(850);tree=capture(key+'-landscape');const after=modal(tree,kind),root=tree[0].bounds;
 assert.equal(after.fullscreenLabel,'退出全屏');assert.ok(root[2]-root[0]>root[3]-root[1],'actual landscape window bounds');assert.equal(after.position,before.position,'no source position reset');
 return {before,after,normalBounds:before.zone,landscapeBounds:root,visualReview:'inspect PNG for system bars and actual fullscreen fit; player identity not exposed by UI'};
});}
function back(kind,label='back-'+kind){return runStage(label,key=>{
 let tree=capture(key+'-before'),before=modal(tree,kind);assert.equal(before.fullscreenLabel,'退出全屏');command('shell','uitest','uiInput','keyEvent','Back');wait(850);
 tree=capture(key+'-restored');const after=modal(tree,kind),root=tree[0].bounds;assert.equal(after.fullscreenLabel,'全屏');assert.ok(root[3]-root[1]>root[2]-root[0],'restored portrait');
 assert.equal(after.position,before.position);return {before,after,restoredBounds:root,previewRemainsOpen:true,visualReview:'inspect PNG for restored bars/layout'};
});}
function reconcileFullscreen(kind,label='reconcile-fullscreen-'+kind){return runStage(label,key=>{
 // Observation only. In particular, an acknowledged fullscreen entry followed
 // by a clipped-tree failure must never replay the entry button.
 wait(850);let tree=capture(key+'-settled'),first=modal(tree,kind),root=tree[0].bounds;
 assert.equal(first.fullscreenLabel,'退出全屏');assert.equal(first.playing,false);assert.ok(root[2]-root[0]>root[3]-root[1],'observed landscape');
 const previous=readProgress().stages.filter(s=>s.label!==key&&s.expectedVersion===expectedVersion&&s.expectedPackageSha===packageSha&&
  s.captures?.some(c=>c.label.endsWith('-landscape'))).at(-1);
 const attempted=previous?.captures?.find(c=>c.label.endsWith('-landscape'));
 assert.ok(attempted?.files?.json,'explicit preceding fullscreen observation exists');
 const prior=modal(JSON.parse(fs.readFileSync(attempted.files.json,'utf8')),kind);assert.equal(first.position,prior.position,'same paused position survives settled rotation');
 wait(850);tree=capture(key+'-stable');const second=modal(tree,kind),stableRoot=tree[0].bounds;
 assert.deepEqual(stableRoot,root,'window bounds stay stable across settled observations');assert.equal(second.position,first.position);assert.equal(second.playing,false);assert.equal(second.fullscreenLabel,'退出全屏');
 return {first,second,landscapeBounds:root,priorAttempt:previous.label,observationOnly:true,actionsReplayed:0,visualReview:'inspect settled PNGs for rotation, fullscreen fit and system bars'};
});}
function close(kind,label='close-'+kind){return runStage(label,key=>{
 const tree=capture(key+'-before'),before=modal(tree,kind);assert.equal(before.fullscreenLabel,'全屏','exit fullscreen explicitly first');rawInput('click',...center(observedNode(tree,'attachment-preview-close')));wait(250);
 const after=capture(key+'-closed');assertFixture(after);assert.ok(!find(after,'attachment-preview-dialog'));assert.ok(!find(after,'attachment-media-playback-failure'));
 return {before,fixture:assertFixture(after),modalClosed:true,physicalLeaseDeletion:'NOT_PROVEN_BY_UI; reopen is a separate explicit stage'};
});}
function background(kind,label='background-'+kind){return runStage(label,key=>{
 let tree=capture(key+'-before'),before=modal(tree,kind);assert.equal(before.playing,false,'background stage starts from an explicitly observed paused session');
 assert.equal(before.fullscreenLabel,'全屏','exit fullscreen explicitly before background stage');assert.ok(before.position<8,'leave enough fixture time to distinguish pause from natural completion');
 quickControl(tree,'attachment-media-toggle');tree=capture(key+'-playing');const started=modal(tree,kind);assert.equal(started.playing,true,'actual play effect before Home');
 wait(1000);tree=capture(key+'-advanced');const advanced=modal(tree,kind);assert.equal(advanced.playing,true);assert.ok(advanced.position>before.position,'actual playback advances before Home');
 assert.ok(advanced.position<10,'stop before Home if fixture is too near natural completion');
 const homeRequestedAt=Date.now();command('shell','uitest','uiInput','keyEvent','Home');wait(1400);
 const home=capture(key+'-home');assert.ok(!find(home,'attachment-preview-dialog'),'actual app modal is absent from the foreground Home observation');
 command('shell','aa','start','-a','EntryAbility','-b','dev.morrow.hmos');wait(850);
 tree=capture(key+'-returned');const returned=modal(tree,kind);assert.equal(returned.playing,false,'background pauses without foreground autoplay');assert.equal(returned.fullscreenLabel,'全屏');
 assert.ok(returned.position<returned.duration,'paused before natural completion');assert.ok(returned.position>=advanced.position&&returned.position<=advanced.position+1,
  'whole-second position remains near last observed playback during background interval');
 wait(1300);tree=capture(key+'-still-paused');const stable=modal(tree,kind);assert.equal(stable.playing,false);assert.equal(stable.position,returned.position);
 return {before,started,advanced,returned,stable,homeRequestedAt,foregroundPausedBeforeEnd:true,
  noAutoplay:'paused glyph and stable position after foreground return',
  identityEvidence:'same exact pin and filename; private native player identity is not exposed',
  backgroundStateLimit:'pause is qualified by returned state/time before EOF; the private player state while Home is foreground is not exposed'};
});}
function seekGestures(kind,label='seek-gestures-'+kind){return runStage(label,key=>{
 let tree=capture(key+'-before'),before=modal(tree,kind);assert.equal(before.playing,false);assert.equal(before.fullscreenLabel,'退出全屏');
 let slider=observedNode(tree,'attachment-media-seek');quickControl(tree,'attachment-media-seek',center(slider));wait(150);
 tree=capture(key+'-midpoint');let middle=modal(tree,kind);assert.ok(middle.position>=5&&middle.position<=7,'observed six-second midpoint before gestures');
 let zone=observedNode(tree,'attachment-media-gesture-zone').bounds,y=(zone[1]+zone[3])/2;
 rawInput('doubleClick',zone[0]+(zone[2]-zone[0])/6,y);capture(key+'-backward-feedback');wait(650);tree=capture(key+'-backward-result');const backward=modal(tree,kind);assert.equal(backward.position,0);
 zone=observedNode(tree,'attachment-media-gesture-zone').bounds;y=(zone[1]+zone[3])/2;rawInput('doubleClick',zone[0]+(zone[2]-zone[0])*5/6,y);capture(key+'-forward-feedback');wait(650);
 tree=capture(key+'-forward-result');const forward=modal(tree,kind);assert.equal(forward.position,10);
 slider=observedNode(tree,'attachment-media-seek');quickControl(tree,'attachment-media-seek',center(slider));wait(150);tree=capture(key+'-drag-midpoint');middle=modal(tree,kind);
 zone=observedNode(tree,'attachment-media-gesture-zone').bounds;const toggle=observedNode(tree,'attachment-media-toggle').bounds,density=(toggle[2]-toggle[0])/56;
 assert.ok(density>=1&&density<=5,'density inferred from observed fullscreen 56-vp button');const delta=175*density,startX=zone[0]+(zone[2]-zone[0])*.35;
 assert.ok(startX+delta<zone[2]-16,'horizontal gesture fits actual observed bounds');rawInput('swipe',startX,(zone[1]+zone[3])/2,startX+delta,(zone[1]+zone[3])/2,1000);wait(150);
 tree=capture(key+'-horizontal-result');const horizontal=modal(tree,kind);assert.ok(horizontal.position>middle.position,'actual horizontal seek advances paused playback');assert.equal(horizontal.playing,false);
 return {before,middle,backward,forward,horizontal,density,deltaPixels:delta,feedbackVisibility:'inspect fast PNG; 400ms may expire before raw tree, never infer feedback rendering from node presence'};
});}
if(require.main===module){const args=process.argv.slice(2),stage=args[0]?.replace(/^--/,''),labelIndex=args.indexOf('--label'),label=labelIndex>=0?args[labelIndex+1]:undefined;
 if(stage==='inspect')inspect(label||'inspect');else{const match=stage?.match(/^(preview|playback|fullscreen|reconcile-fullscreen|back|background|close|seek-gestures)-(audio|video)$/);assert.ok(match,'explicit observed stage required');
  ({preview,playback,fullscreen,back,background,close,'seek-gestures':seekGestures,'reconcile-fullscreen':reconcileFullscreen})[match[1]](match[2],label||stage);}}
module.exports={f,a,editorSeek,out,fixture,assets,inspect,preview,playback,fullscreen,reconcileFullscreen,back,background,close,seekGestures,capture,modal,modalOwnership,controlEvidence,rawInput,runStage};
