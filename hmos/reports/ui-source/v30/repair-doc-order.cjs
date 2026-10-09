'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
for (const name of ['README.md', 'docs/PARITY.md', 'docs/ALIGNMENT_PLAN.md']) {
  const file = path.join(root, name), old = fs.readFileSync(file, 'utf8');
  const start = old.indexOf('2026-10-09 **v30 当前音乐界面检查点**'), end = old.indexOf('2026-10-09 **v28 当前分支检查点**', start);
  assert.ok(start > 0 && end > start); const block = old.slice(start, end), rest = old.slice(0, start) + old.slice(end);
  const header = /^#[^\r\n]+\r?\n(?:\r?\n)+/.exec(rest); assert.ok(header);
  const result = (rest.slice(0, header[0].length) + block + rest.slice(header[0].length))
    .replace('**v29 当前音乐基础检查点**', '**v29 历史音乐基础检查点**');
  fs.writeFileSync(file, result);
  assert.ok(result.indexOf('**v30 当前') < result.indexOf('**v29 历史'));
}
