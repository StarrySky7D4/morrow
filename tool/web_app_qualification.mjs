import {writeFile,readFile,mkdtemp} from 'node:fs/promises';
import path from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';

const legacySnapshot=JSON.stringify({version:1,theme:'white',glass:'frosted',background:'ambient',ideas:[{
  id:'legacy-browser-card',title:'Legacy browser content',description:'Preserved source',category:'灵感',
  time:'刚刚',icon:0,color:4287137450,favorite:false,todos:[],completed:[],stage:'待整理',attachments:[],
}]});
function context(call,sessionId) {
  const evaluate=async expression=>{
    const response=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true},sessionId);
    if(response.exceptionDetails)throw Error(response.exceptionDetails.text+': '+(response.exceptionDetails.exception?.description??''));
    return response.result?.value;
  };
  const until=async(check,label)=>{
    const deadline=Date.now()+45000;
    while(Date.now()<deadline){if(await check())return;await delay(150);}
    throw Error('Timed out: '+label);
  };
  return {evaluate,until};
}
export async function prepareWebApp(call,sessionId,site,variant) {
  const {evaluate,until}=context(call,sessionId);
  // Establish the real HTML origin with app startup blocked until test data
  // is seeded. JSON document viewers differ between browser builds and are
  // unsuitable as fixture documents. Only harness-owned profiles are used.
  await call('Network.setBlockedURLs',{urls:[new URL('flutter_bootstrap.js',site).href+'*']},sessionId);
  const navigation=await call('Page.navigate',{url:site},sessionId);
  if(navigation.errorText)throw Error('Fixture navigation: '+navigation.errorText);
  await until(async()=>{try{return await evaluate(`location.href===${JSON.stringify(site)}&&document.readyState!=='loading'`);}catch{return false;}},'fixture origin');
  if(variant==='legacy')await evaluate(`localStorage.setItem('flutter.daemon.studio.v1',${JSON.stringify(JSON.stringify(legacySnapshot))})`);
  if(variant==='orphan')await evaluate(`(async()=>{const root=await navigator.storage.getDirectory();const d=await root.getDirectoryHandle('morrow-workbench-v1',{create:true});await d.getFileHandle('preserved-fixture',{create:true});})()`);
  await call('Network.setBlockedURLs',{urls:[]},sessionId);
  await call('Emulation.setDeviceMetricsOverride',{width:1280,height:900,deviceScaleFactor:1,mobile:false},sessionId);
}
export async function qualifyWebApp(call,sessionId,base,root,variant) {
  const {evaluate,until}=context(call,sessionId);
  const rect=label=>evaluate(`(()=>{
    const nodes=[...document.querySelectorAll('[aria-label], [role=button], flt-semantics')];
    const e=nodes.find(e=>(e.getAttribute('aria-label')??e.textContent??'').trim().split(/\\r?\\n/).includes(${JSON.stringify(label)})&&e.getBoundingClientRect().width>0);
    if(!e)return null;const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};
  })()`);
  const enable=async()=>{
    await until(async()=>{
      await evaluate("(document.querySelector('flt-semantics-placeholder')??document.querySelector('flt-glass-pane')?.shadowRoot?.querySelector('flt-semantics-placeholder'))?.click()");
      return await evaluate("document.querySelectorAll('flt-semantics').length>0");
    },'Flutter accessibility');
  };
  const waitLabel=label=>until(async()=>!!await rect(label),label);
  const click=async label=>{
    await waitLabel(label);const r=await rect(label);
    await call('Input.dispatchMouseEvent',{type:'mousePressed',x:r.x,y:r.y,button:'left',clickCount:1},sessionId);
    await call('Input.dispatchMouseEvent',{type:'mouseReleased',x:r.x,y:r.y,button:'left',clickCount:1},sessionId);
  };
  const state=()=>evaluate("import('./workbench/device-identity.mjs').then(m=>m.inspectDeviceLibrary('main'))");
  try {
    await enable();
    if(variant==='legacy') {
      await waitLabel('Open existing content');
      if(await rect('Create local workspace'))throw Error('Legacy content offered replacement');
      await click('Open existing content');
      await waitLabel('Legacy browser content');
      const raw=await evaluate("localStorage.getItem('flutter.daemon.studio.v1')");
      if(raw!==JSON.stringify(legacySnapshot)||await state()!=='empty')throw Error('Legacy source was changed or new library was created');
    } else if(variant==='orphan') {
      await waitLabel('Try again');
      if(await rect('Create local workspace')||await state()!=='orphaned')throw Error('Orphaned content was hidden by a new library');
      const entries=await evaluate("(async()=>{const d=await(await navigator.storage.getDirectory()).getDirectoryHandle('morrow-workbench-v1');const names=[];for await(const [name]of d.entries())names.push(name);return names;})()");
      if(entries.length!==1||entries[0]!=='preserved-fixture')throw Error('Orphaned storage modified');
    } else {
      await click('Create local workspace');
      await waitLabel('New idea');
      const identity=await evaluate("import('./workbench/device-identity.mjs').then(m=>m.inspectDeviceIdentity('main')).then(v=>v.logId)");
      if(await state()!=='ready')throw Error('Identity not ready after application startup');
      if(process.argv.includes('--offline-edits')) {
        await call('Network.emulateNetworkConditions',{offline:true,latency:0,downloadThroughput:-1,uploadThroughput:-1},sessionId);
        if(await evaluate('navigator.onLine'))throw Error('Offline acceptance did not disconnect the page');
      }
      await click('New idea');
      await waitLabel('Save idea');
      await until(()=>evaluate("['INPUT','TEXTAREA'].includes(document.activeElement?.tagName)"),'editor focus');
      await call('Input.insertText',{text:'Browser application card'},sessionId);
      const attachmentPath=path.join(root,'build/browser-local-attachment.txt');
      await writeFile(attachmentPath,'Device-only original attachment\n本地原件😀\n');
      await call('Page.setInterceptFileChooserDialog',{enabled:true},sessionId);
      await click('Import file');
      await until(()=>evaluate("!!document.querySelector('input[type=file]')"),'browser file picker');
      const input=await call('Runtime.evaluate',{expression:"document.querySelector('input[type=file]')"},sessionId);
      await call('DOM.setFileInputFiles',{files:[attachmentPath],objectId:input.result.objectId},sessionId);
      await call('Page.setInterceptFileChooserDialog',{enabled:false},sessionId);
      await waitLabel('browser-local-attachment.txt');
      // Commit the text field's focus/selection update before submitting. The
      // editor intentionally keeps a successor draft if selection changes
      // while a save is in flight, so typing and submitting in one frame is
      // not the ordinary settled-input scenario this startup check covers.
      await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Tab',code:'Tab',windowsVirtualKeyCode:9},sessionId);
      await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Tab',code:'Tab',windowsVirtualKeyCode:9},sessionId);
      await evaluate('new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))');
      await click('Save idea');
      await waitLabel('Browser application card');
      if(await rect('Review save')||await rect('Retry query'))throw Error('Committed card left a preferences or query failure');
      if(await evaluate("localStorage.getItem('flutter.daemon.studio.v1')")!==null)throw Error('Application wrote new content to old storage');
      if(process.argv.includes('--offline-edits'))await call('Network.emulateNetworkConditions',{offline:false,latency:0,downloadThroughput:-1,uploadThroughput:-1},sessionId);
      await call('Page.reload',{ignoreCache:true},sessionId);
      await enable();
      await waitLabel('Browser application card');
      if(await rect('Review save')||await rect('Retry query'))throw Error('Reopened workspace has a preferences or query failure');
      await until(()=>evaluate("[...document.querySelectorAll('flt-semantics')].some(e=>(e.getAttribute('aria-label')??e.textContent??'').includes('browser-local-attachment.txt'))"),'reopened attachment in actual card');
      const downloadDirectory=await mkdtemp(path.join(root,'build/browser-attachment-download-'));
      await call('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:downloadDirectory});
      await click('Browser application card');
      await click('Save attachment as');
      await until(async()=>{
        try {return (await readFile(path.join(downloadDirectory,'browser-local-attachment.txt'))).equals(await readFile(attachmentPath));}
        catch(error) {if(error.code==='ENOENT')return false;throw error;}
      },'downloaded original attachment bytes');
      await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);
      await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Escape',code:'Escape',windowsVirtualKeyCode:27},sessionId);
      const reopened=await evaluate("import('./workbench/device-identity.mjs').then(m=>m.inspectDeviceIdentity('main')).then(v=>v.logId)");
      if(reopened!==identity)throw Error('Page reload changed identity');
      // If a legacy snapshot later coexists, both stores must remain reachable.
      await evaluate(`localStorage.setItem('flutter.daemon.studio.v1',${JSON.stringify(JSON.stringify(legacySnapshot))})`);
      await call('Page.reload',{ignoreCache:true},sessionId);await enable();
      await waitLabel('Reopen workspace');await waitLabel('Open existing content');
      if(await rect('Create local workspace'))throw Error('Coexisting stores offered replacement');
      await click('Open existing content');await waitLabel('Legacy browser content');
      await call('Page.reload',{ignoreCache:true},sessionId);await enable();
      await click('Reopen workspace');await waitLabel('Browser application card');
      if(await evaluate("localStorage.getItem('flutter.daemon.studio.v1')")!==JSON.stringify(legacySnapshot))throw Error('Opening the new workspace replaced legacy content');
    }
    const screenshot=await call('Page.captureScreenshot',{format:'png'},sessionId);
    await writeFile(path.join(root,`build/web-bootstrap-${variant}.png`),Buffer.from(screenshot.data,'base64'));
    return `PASS: formal Web application ${variant}${process.argv.includes('--offline-edits')?' (offline editing)':''}: ${variant==='fresh'?'UI file selection/save, OPFS attachment persistence, full page reload, exact original file download and explicit access to coexisting old/new content':variant==='legacy'?'existing content remains accessible and unchanged':'missing identity preserves orphaned data and prevents creation'}`;
  } catch(error) {
    const screenshot=await call('Page.captureScreenshot',{format:'png'},sessionId);
    await writeFile(path.join(root,`build/web-bootstrap-${variant}-failure.png`),Buffer.from(screenshot.data,'base64'));
    await writeFile(path.join(root,`build/web-bootstrap-${variant}-failure.html`),await evaluate('document.body.outerHTML'));
    throw error;
  }
}
