'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const hash = name => crypto.createHash('sha256').update(fs.readFileSync(name)).digest('hex');
const project = path.resolve(process.argv[2] || '');
const work = path.resolve(__dirname, '../../.build/checkpoint-sdk-smoke');
if (!process.argv[2] || !project.startsWith(work + path.sep)) throw new Error('Supply the exact isolated smoke project');
const manifest = JSON.parse(fs.readFileSync(path.join(project, 'source-copy-manifest.json'), 'utf8'));
if (path.resolve(manifest.project) !== project) throw new Error('Manifest project mismatch');
const generated = new Map(manifest.generated_harness.map(item => [item.path, item]));
const copied = [], harness = [];
for (const item of manifest.copied_inputs) {
  const target = path.resolve(project, item.path);
  if (!target.startsWith(project + path.sep)) throw new Error('Manifest copy escapes project');
  const sourceSha = hash(item.source_path);
  if (sourceSha !== item.sha256 || item.copied_sha256 !== item.sha256) throw new Error('Product source changed since copy: ' + item.path);
  if (!generated.has(item.path) && hash(target) !== item.sha256) throw new Error('Independent copy changed: ' + item.path);
  copied.push({ path: item.path, sha256: item.sha256, source_match: true, copy_match: generated.has(item.path) ? 'HARNESS_OVERRIDE' : true });
}
for (const item of manifest.generated_harness) {
  const target = path.resolve(project, item.path);
  if (!target.startsWith(project + path.sep) || hash(target) !== item.sha256) throw new Error('Harness changed: ' + item.path);
  harness.push({ path: item.path, sha256: item.sha256, match: true });
}
process.stdout.write(JSON.stringify({ status: 'PASS', project, copied_inputs: copied.length, generated_harness: harness.length,
  copied, harness, device: 'NOT_RUN', installation: 'NOT_RUN', scope: 'Exact source-copy/explicit isolated harness verification' }, null, 2) + '\n');
