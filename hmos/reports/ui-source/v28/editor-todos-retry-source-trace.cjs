'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const repo = process.env.TODO_RETRY_REPO, output = process.env.TODO_RETRY_TRACE;
if (!repo || !output) throw new Error('Explicit source trace paths required');
const read = fs.readFileSync, reads = new Map(), compiled = new Set();
const relative = filename => typeof filename === 'string' ? path.relative(repo, path.resolve(filename)).replace(/\\/g, '/') : '';
fs.readFileSync = function (filename, ...args) {
  const data = read.call(this, filename, ...args), name = relative(filename);
  if (name.startsWith('hmos/') && /\.(?:ets|cjs|json)$/.test(name)) {
    const bytes = Buffer.isBuffer(data) ? data : Buffer.from(data);
    if (!reads.has(name)) reads.set(name, { path: name, bytes: bytes.length,
      sha256: crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase() });
  }
  return data;
};
const ts = require(process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
const transpile = ts.transpileModule, Module = require('node:module'), originalLoad = Module._load;
const facade = { ...ts, transpileModule: function (input, options, ...args) {
  const name = relative(options && options.fileName);
  if (name.startsWith('hmos/') && name.endsWith('.ets')) compiled.add(name);
  return transpile.call(this, input, options, ...args);
} };
Module._load = function (...args) {
  const value = originalLoad.apply(this, args); return value === ts ? facade : value;
};
process.on('exit', () => fs.writeFileSync(path.join(output, process.pid + '.json'),
  JSON.stringify({ reads: [...reads.values()], compiled: [...compiled] }, null, 2) + '\n', { flag: 'wx' }));
