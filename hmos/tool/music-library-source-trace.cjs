'use strict';
// Observe actual repository reads/module execution; dependencies outside the
// checkout are identified by the runner, not claimed as repository inputs.
const fs = require('node:fs'), path = require('node:path'), Module = require('node:module');
const root = path.resolve(__dirname, '../..'), originalRead = fs.readFileSync;
const observed = new Set();
function record(file) {
  if (typeof file !== 'string') return;
  const absolute = path.resolve(file), relative = path.relative(root, absolute);
  if (relative && !relative.startsWith('..') && !path.isAbsolute(relative) && /\.(ets|cjs|json)$/.test(relative) &&
      !relative.replace(/\\/g, '/').startsWith('hmos/reports/ui-source/v29/music-library-')) observed.add(relative.replace(/\\/g, '/'));
  // Native fixture JSON starts music-store and remains part of actual inputs.
}
fs.readFileSync = function(file, ...rest) { record(file); return originalRead.call(this, file, ...rest); };
const originalExtension = Module._extensions['.js'];
Module._extensions['.js'] = function(module, filename) { record(filename); return originalExtension(module, filename); };
record(__filename);
process.on('exit', () => {
  if (process.env.MUSIC_LIBRARY_TRACE) fs.appendFileSync(process.env.MUSIC_LIBRARY_TRACE,
    JSON.stringify({ pid: process.pid, files: [...observed].sort() }) + '\n');
});
