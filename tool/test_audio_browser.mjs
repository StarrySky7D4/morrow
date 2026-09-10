// Local test-only HTTP server + headless Chrome CDP runner, no npm dependencies.
import { createServer } from 'node:http';
import { readFile, mkdtemp, access } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const root = fileURLToPath(new URL('../', import.meta.url));
const candidates = [process.env.CHROME_BIN, process.env.CHROME_EXECUTABLE,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/usr/bin/google-chrome', '/usr/bin/chromium', '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'].filter(Boolean);
let chrome;
for (const file of candidates) { try { await access(file); chrome = file; break; } catch { /* Try next explicit path. */ } }
if (!chrome) throw new Error('Set CHROME_BIN to a Chrome/Chromium executable');

const allowed = ['/preview/', '/fixtures/'];
const mime = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript',
  '.wasm': 'application/wasm', '.json': 'application/json', '.bin': 'application/octet-stream' };
const server = createServer(async (request, response) => {
  try {
    const route = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    if (!allowed.some((p) => p.endsWith('/') ? route.startsWith(p) : route === p)) { response.writeHead(404).end(); return; }
    const target = route.startsWith('/preview/')
      ? path.resolve(root, 'build/audio-web-probe', route.slice('/preview/'.length) || 'index.html')
      : path.resolve(root, 'test/fixtures/audio', route.slice('/fixtures/'.length));
    const relative = path.relative(root, target);
    if (relative.startsWith('..') || path.isAbsolute(relative)) { response.writeHead(403).end(); return; }
    response.setHeader('Content-Type', mime[path.extname(target)] ?? 'application/octet-stream');
    response.setHeader('Cache-Control', 'no-store');
    response.end(await readFile(target));
  } catch { response.writeHead(404).end(); }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}`;
const profile = await mkdtemp(path.join(root, 'build/browser-audio-'));
const process_ = spawn(chrome, ['--headless=new', '--no-first-run', '--no-default-browser-check',
  '--disable-background-networking', '--disable-extensions', '--autoplay-policy=no-user-gesture-required', '--remote-debugging-port=0',
  `--user-data-dir=${profile}`, 'about:blank'], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
let socket;
const pending = new Map();
let nextId = 1;
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
    const waiter = pending.get(reply.id);
    if (!waiter) return;
    pending.delete(reply.id);
    if (reply.error) waiter.reject(new Error(JSON.stringify(reply.error)));
    else waiter.resolve(reply.result);
  });
  const version = await call('Browser.getVersion');
  for (const mode of ['worker']) {
    const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
    await call('Page.enable', {}, sessionId);
    await call('Page.navigate', { url: `${base}/preview/` }, sessionId);
    let result;
    const deadline = Date.now() + 60000;
    while (Date.now() < deadline) {
      const evaluation = await call('Runtime.evaluate', { expression: 'globalThis.audioProbeResult ?? null', returnByValue: true }, sessionId);
      result = evaluation.result?.value;
      if (result) break;
      await delay(150);
    }
    if (!result?.startsWith('PASS:')) throw new Error(result ?? 'Audio probe timed out');
    console.log(version.product + ': ' + result);
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
