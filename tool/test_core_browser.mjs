// Local test-only HTTP server + headless Chrome CDP runner, no npm dependencies.
import { createServer } from 'node:http';
import { readFile, writeFile, mkdtemp, access } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import {prepareWebApp,qualifyWebApp} from './web_app_qualification.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const candidates = [process.env.CHROME_BIN, process.env.CHROME_EXECUTABLE,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/usr/bin/google-chrome', '/usr/bin/chromium', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'].filter(Boolean);
let chrome;
for (const file of candidates) { try { await access(file); chrome = file; break; } catch { /* Try next explicit path. */ } }
if (!chrome) throw new Error('Set CHROME_BIN to a Chrome/Chromium executable');

const app=process.argv.includes('--app');
const siteArg=process.argv.indexOf('--site');
const remoteSite=siteArg<0?null:new URL(process.argv[siteArg+1]);
if(remoteSite&&(!app||remoteSite.protocol!=='https:'||!remoteSite.pathname.endsWith('/')))throw Error('--site requires --app and an HTTPS directory URL');
const baseArg=process.argv.indexOf('--base-path');
const basePath=remoteSite?.pathname??(baseArg<0?'/preview/':process.argv[baseArg+1]);
if(!/^\/([A-Za-z0-9_.-]+\/)*$/.test(basePath))throw Error('Invalid base path');
const appVariant=process.argv.includes('--legacy')?'legacy':process.argv.includes('--orphan')?'orphan':process.argv.includes('--media')?'media':'fresh';
const webFolder=app?'build/web':process.argv.includes('--channel')?'build/web-channel-parity':process.argv.includes('--identity')?'build/web-identity-parity':process.argv.includes('--workbench')?'build/web-workbench-parity':process.argv.includes('--packages')?'build/web-package-parity':process.argv.includes('--renderer')?'build/ui-renderer/web':process.argv.includes('--ui')?'build/ui-protocol/web':process.argv.includes('--store')?'build/core-test.10/web-store':'build/core-test.10/web';
const allowed = [basePath];
const mime = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript',
  '.wasm': 'application/wasm', '.json': 'application/json', '.bin': 'application/octet-stream' };
const server = createServer(async (request, response) => {
  try {
    const route = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    if(app&&route==='/preview/__fixture__.html') {response.setHeader('Content-Type','text/html');response.end('<!doctype html><title>Isolated fixture origin</title>');return;}
    if(process.argv.includes('--renderer')&&route.startsWith('/preview/assets/test-fonts/')){
      const fonts={'text':process.env.MORROW_UI_TEST_FONT??'C:/Windows/Fonts/msyh.ttc',
        'emoji':process.env.MORROW_UI_TEST_EMOJI_FONT??'C:/Windows/Fonts/seguiemj.ttf'};
      const font=fonts[route.slice('/preview/assets/test-fonts/'.length)];
      if(!font){response.writeHead(404).end();return;}
      response.setHeader('Content-Type','application/octet-stream');
      response.end(await readFile(font));return;
    }
    if(process.argv.includes('--store')&&request.method==='POST'&&route==='/capture/browser-card.morrow'){
      const chunks=[];let size=0;
      for await(const chunk of request){size+=chunk.length;if(size>9*1024*1024){response.writeHead(413).end();return;}chunks.push(chunk);}
      await writeFile(path.join(root,'build/core-test.10/browser-card.morrow'),Buffer.concat(chunks));response.writeHead(204).end();return;
    }
    if (!allowed.some((p) => p.endsWith('/') ? route.startsWith(p) : route === p)) { response.writeHead(404).end(); return; }
    const target = path.resolve(root, webFolder, route.slice(basePath.length) || 'index.html');
    const relative = path.relative(path.join(root, webFolder), target);
    if (relative.startsWith('..') || path.isAbsolute(relative)) { response.writeHead(403).end(); return; }
    response.setHeader('Content-Type', mime[path.extname(target)] ?? 'application/octet-stream');
    response.setHeader('Cache-Control', 'no-store');
    response.end(await readFile(target));
  } catch { response.writeHead(404).end(); }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}`;
const site=remoteSite?.href??`${base}${basePath}`;
const profile = await mkdtemp(path.join(root, 'build/browser-core-'));
const process_ = spawn(chrome, ['--headless=new', '--no-first-run', '--no-default-browser-check',
  ...(app?['--lang=en-US']:[]),
  '--disable-background-networking', '--disable-extensions', '--autoplay-policy=no-user-gesture-required', '--remote-debugging-port=0',
  `--user-data-dir=${profile}`, 'about:blank'], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
let socket;
const pending = new Map();
let nextId = 1;
const appDiagnostics=[];
const appRequests=[];
function call(method, params = {}, sessionId) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
    pending.set(id, { resolve: (v) => { clearTimeout(timer); resolve(v); }, reject: (e) => { clearTimeout(timer); reject(e); } });
    socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
}
try {
  const endpoint = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('Chrome startup timeout')), 20000);
    let logs = '';
    process_.once('error', reject);
    process_.once('exit', (code) => { clearTimeout(timeout); reject(new Error(`Chrome exited: ${code}; ${logs}`)); });
    process_.stderr.on('data', (chunk) => {
      logs += chunk;
      const match = logs.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) { clearTimeout(timeout); resolve(match[1]); }
    });
  });
  socket = new WebSocket(endpoint);
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
  socket.addEventListener('message', ({ data }) => {
    const reply = JSON.parse(data);
    if(app&&reply.method==='Network.requestWillBeSent') {
      const r=reply.params.request;
      if(/^https?:/.test(r.url))appRequests.push({url:r.url,method:r.method,hasPostData:!!r.hasPostData});
    }
    if(app && ['Runtime.exceptionThrown','Log.entryAdded','Runtime.consoleAPICalled'].includes(reply.method) && appDiagnostics.length<30) {
      const p=reply.params;
      if(reply.method!=='Runtime.consoleAPICalled'||['error','warning'].includes(p.type))appDiagnostics.push(JSON.stringify(p).slice(0,3500));
    }
    const waiter = pending.get(reply.id);
    if (!waiter) return;
    pending.delete(reply.id);
    if (reply.error) waiter.reject(new Error(JSON.stringify(reply.error)));
    else waiter.resolve(reply.result);
  });
  const version = await call('Browser.getVersion');
  for (const mode of (process.argv.includes('--store')?['store','restore']:['worker'])) {
    const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
    await call('Page.enable', {}, sessionId);
    if(app) {
      await call('Runtime.enable',{},sessionId);await call('Log.enable',{},sessionId);await call('Network.enable',{},sessionId);
      // Edge can follow the OS language despite --lang. Keep semantic-label
      // fixtures deterministic without modifying application preferences.
      await call('Network.setUserAgentOverride',{userAgent:version.userAgent,acceptLanguage:'en-US,en'},sessionId);
      await call('Emulation.setLocaleOverride',{locale:'en_US'},sessionId);
      if(process.argv.includes('--slow-ui'))await call('Emulation.setCPUThrottlingRate',{rate:4},sessionId);
    }
    if(app) {
      try {await prepareWebApp(call,sessionId,site,appVariant);}
      catch(error) {
        console.error(appDiagnostics.join('\n'));
        const frames=await call('Page.getFrameTree',{},sessionId);console.error(JSON.stringify(frames));
        const shot=await call('Page.captureScreenshot',{format:'png'},sessionId);
        await writeFile(path.join(root,`build/web-bootstrap-${appVariant}-preparation-failure.png`),Buffer.from(shot.data,'base64'));
        throw error;
      }
    }
    if(process.argv.includes('--renderer')) await call('Emulation.setDeviceMetricsOverride',{width:360,height:640,deviceScaleFactor:1,mobile:false},sessionId);
    await call('Page.navigate', { url: `${site}${mode==='restore'?'?restore=1':''}` }, sessionId);
    let result;
    if(app) {
      try {result=await qualifyWebApp(call,sessionId,base,root,appVariant);}
      catch(error){console.error(appDiagnostics.join('\n'));throw error;}
    }
    const deadline = Date.now() + (process.argv.includes('--store') ? 300000 : 60000);
    while (!result && Date.now() < deadline) {
      const evaluation = await call('Runtime.evaluate', { expression: 'globalThis.coreProbeResult ?? null', returnByValue: true }, sessionId);
      result = evaluation.result?.value;
      if (result) break;
      await delay(150);
    }
    if(process.argv.includes('--renderer') && result==='READY'){
      const evaluate=async expression=>(await call('Runtime.evaluate',{expression,returnByValue:true},sessionId)).result?.value;
      const clickLabel=async label=>{
        let rect; const until=Date.now()+10000;
        while(Date.now()<until){
          rect=await evaluate(`(()=>{
            const matches=[...document.querySelectorAll('[aria-label], [role=button], [role=switch]')]
              .filter(e=>(e.getAttribute('aria-label')??e.textContent??'').includes(${JSON.stringify(label)}));
            const e=matches.find(e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0;});
            if(!e)return null;
            const r=e.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};
          })()`);
          if(rect)break; await delay(100);
        }
        if(!rect){
          await writeFile(path.join(root,'build/ui-renderer/browser-dom.html'),await evaluate('document.body.outerHTML'));
          const shot=await call('Page.captureScreenshot',{format:'png'},sessionId);
          await writeFile(path.join(root,'build/ui-renderer/browser-debug.png'),Buffer.from(shot.data,'base64'));
          throw Error('Missing rendered control: '+label);
        }
        await call('Input.dispatchMouseEvent',{type:'mousePressed',x:rect.x,y:rect.y,button:'left',clickCount:1},sessionId);
        await call('Input.dispatchMouseEvent',{type:'mouseReleased',x:rect.x,y:rect.y,button:'left',clickCount:1},sessionId);
      };
      await clickLabel('标题');
      await call('Input.dispatchKeyEvent',{type:'keyDown',key:'a',code:'KeyA',modifiers:2,windowsVirtualKeyCode:65},sessionId);
      await call('Input.dispatchKeyEvent',{type:'keyUp',key:'a',code:'KeyA',modifiers:2,windowsVirtualKeyCode:65},sessionId);
      await call('Input.insertText',{text:'浏览器编辑'},sessionId);
      await delay(200);await clickLabel('置顶');await delay(200);await clickLabel('应用');await delay(200);
      const records=await evaluate('globalThis.rendererEdits??[]');
      for(const expected of ['editText|title|浏览器编辑|false','setToggle|pinned||true','activate|apply||false'])
        if(!records.includes(expected))throw Error('Missing real widget event '+expected+': '+JSON.stringify(records));
      const state=await evaluate('globalThis.coreProbeResult');if(state!=='READY')throw Error(state);
      const screenshot=await call('Page.captureScreenshot',{format:'png'},sessionId);
      await writeFile(path.join(root,'build/ui-renderer/browser-narrow.png'),Buffer.from(screenshot.data,'base64'));
      result='PASS: actual Flutter Web form, Unicode keyboard edit, toggle and button events at 360px';
    }
    if (!result?.startsWith('PASS:')) throw new Error(result ?? 'Core protocol probe timed out');
    if(app) {
      await writeFile(path.join(root,`build/web-network-${appVariant}.json`),JSON.stringify({site,requests:appRequests},null,2));
      if(appRequests.some(r=>!['GET','HEAD'].includes(r.method)||r.hasPostData))throw Error('Unexpected outbound data request during local-only acceptance');
    }
    console.log(version.product + ': ' + result);
    if(process.argv.includes('--ui')){
      const response=await call('Runtime.evaluate',{expression:'Array.from(globalThis.uiEvent)',returnByValue:true},sessionId);
      const bytes=response.result?.value;
      if(!Array.isArray(bytes)||bytes.length<8||bytes.length>65536||bytes.some(n=>!Number.isInteger(n)||n<0||n>255))throw Error('Invalid browser event bytes');
      await writeFile(path.join(root,'build/ui-protocol/browser-event.capnp'),Buffer.from(bytes));
    }
    await call('Target.closeTarget', { targetId });
  }
} finally {
  if (socket?.readyState === WebSocket.OPEN) {
    // Browser.close may terminate before a reply; do not wait for an RPC timeout.
    socket.send(JSON.stringify({ id: nextId++, method: 'Browser.close' }));
    if (process_.exitCode === null) {
      await Promise.race([new Promise((resolve) => process_.once('exit', resolve)), delay(3000)]);
    }
    socket.close();
  }
  for (const waiter of pending.values()) waiter.reject(new Error('Browser runner closed'));
  process_.kill();
  await new Promise((resolve) => server.close(resolve));
}
