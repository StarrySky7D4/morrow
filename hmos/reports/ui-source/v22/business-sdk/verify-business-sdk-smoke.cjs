'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const source = path.resolve(__dirname, '../../../..'), work = path.join(source, '.build/checkpoint-sdk-smoke');
const project = path.resolve(process.argv[2] || ''), sha = value => crypto.createHash('sha256').update(value).digest('hex');
if (!process.argv[2] || !project.startsWith(work + path.sep)) throw new Error('Exact isolated business smoke path required');
const manifest = JSON.parse(fs.readFileSync(path.join(project, 'source-copy-manifest.json'), 'utf8'));
if (manifest.project !== project || manifest.bundle_name !== 'dev.morrow.hmos.editorbusinesssdk' || manifest.business_sdk_smoke?.schema !== 1) throw new Error('Wrong business harness');
const generated = new Set(manifest.generated_harness.map(value => value.path)), copied = [], wrappers = [], liveDifferences = [];
for (const item of manifest.copied_inputs) {
  const target = path.resolve(project, item.path); if (!target.startsWith(project + path.sep)) throw new Error('Copy path escapes isolated project');
  if (item.copied_sha256 !== item.sha256) throw new Error('Initial copy mismatch');
  const content = fs.readFileSync(target);
  if (!generated.has(item.path) && (content.length !== item.bytes || sha(content) !== item.sha256)) throw new Error('Frozen copied snapshot changed: ' + item.path);
  const current = fs.readFileSync(item.source_path), live = sha(current) === item.sha256 && current.length === item.bytes;
  if (!live) liveDifferences.push({ path: item.path, copied_sha256: item.sha256, current_sha256: sha(current), current_bytes: current.length });
  copied.push({ path: item.path, bytes: item.bytes, sha256: item.sha256, copy_match: generated.has(item.path) ? 'HARNESS_OVERRIDE' : true, live_source_match: live });
}
for (const item of manifest.generated_harness) {
  const target = path.resolve(project, item.path); if (!target.startsWith(project + path.sep)) throw new Error('Wrapper path escapes isolated project');
  const content = fs.readFileSync(target); if (content.length !== item.bytes || sha(content) !== item.sha256) throw new Error('Generated wrapper changed'); wrappers.push(item);
}
for (const item of manifest.business_sdk_smoke.actual_compile_inputs) {
  const copiedInput = manifest.copied_inputs.find(value => value.path === item.path);
  if (!copiedInput || copiedInput.bytes !== item.bytes || copiedInput.sha256 !== item.sha256 ||
    liveDifferences.some(value => value.path === item.path)) throw new Error('Business/type dependency source changed: ' + item.path);
}
const page = fs.readFileSync(path.join(project, 'entry/src/main/ets/pages/CheckpointSdkSmoke.ets'), 'utf8');
for (const reference of ['EditorBusinessCoordinator.prepare(', 'EditorBusinessCoordinator.restore(', 'coordinator.save()', 'coordinator.retrySave()',
  'coordinator.inspect(', 'coordinator.retryInspect()', 'coordinator.continueTodos(', 'coordinator.mayConsume(']) if (!page.includes(reference)) throw new Error('Missing actual public API reference');
process.stdout.write(JSON.stringify({ status: 'PASS', project, copied_inputs: copied.length, generated_harness: wrappers.length,
  copied, wrappers, actual_compile_inputs: manifest.business_sdk_smoke.actual_compile_inputs, live_source_differences: liveDifferences,
  rewritten_wrappers: manifest.business_sdk_smoke.rewritten_wrappers, scope: manifest.business_sdk_smoke.scope,
  cpp_native_scope: manifest.business_sdk_smoke.cpp_native_scope, runtime: 'NOT_RUN', device: 'NOT_RUN', installation: 'NOT_RUN' }, null, 2) + '\n');
