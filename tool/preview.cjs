// Local-only server for the compiled Flutter Web preview.
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../build/web');
const types = {'.html':'text/html; charset=utf-8','.js':'text/javascript','.json':'application/json','.wasm':'application/wasm','.svg':'image/svg+xml','.png':'image/png','.jpg':'image/jpeg','.gif':'image/gif','.webp':'image/webp','.mp4':'video/mp4','.webm':'video/webm','.woff2':'font/woff2','.ttf':'font/ttf'};
const port = Number(process.env.PORT || 8765);
http.createServer((req, res) => {
  let urlPath;
  try { urlPath = decodeURIComponent(new URL(req.url, 'http://localhost').pathname); }
  catch { res.writeHead(400).end(); return; }
  const target = path.resolve(root, '.' + (urlPath === '/' ? '/index.html' : urlPath));
  if (!target.startsWith(root + path.sep)) { res.writeHead(403).end(); return; }
  fs.stat(target, (error, stat) => {
    if (error || !stat.isFile()) { res.writeHead(404).end('Not found'); return; }
    res.writeHead(200, {'Content-Type': types[path.extname(target)] || 'application/octet-stream', 'Cache-Control': 'no-cache'});
    const stream = fs.createReadStream(target);
    stream.on('error', () => res.destroy());
    stream.pipe(res);
  });
}).listen(port, '127.0.0.1', () => console.log(`daemon preview: http://127.0.0.1:${port}`));
