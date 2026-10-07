'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), { spawnSync } = require('node:child_process');
const projectSource = path.resolve(__dirname, '../../../..'), project = path.resolve(process.argv[2] || '');
const work = path.join(projectSource, '.build/checkpoint-sdk-smoke');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
if (!process.argv[2] || !project.startsWith(work + path.sep)) throw new Error('Supply the exact isolated fork project');
const baseVerify = path.join(projectSource, 'tool/checkpoint-sdk-smoke/verify.cjs');
const base = spawnSync(process.execPath, [baseVerify, project], { encoding: 'utf8' });
if (base.status !== 0) throw new Error(base.stderr || base.stdout || 'Base source-copy verifier failed');
const result = JSON.parse(base.stdout), manifest = JSON.parse(fs.readFileSync(path.join(project, 'source-copy-manifest.json'), 'utf8'));
if (manifest.bundle_name !== 'dev.morrow.hmos.draftforksdk' || manifest.fork_sdk_smoke?.schema !== 1) throw new Error('Not the explicit fork harness');
for (const item of [...manifest.copied_inputs, ...manifest.generated_harness]) {
  const target = path.resolve(project, item.path);
  if (!target.startsWith(project + path.sep)) throw new Error('Manifest path outside isolated project');
  if (!manifest.generated_harness.some(wrapper => wrapper.path === item.path) || manifest.generated_harness.includes(item)) {
    if (fs.statSync(target).size !== item.bytes) throw new Error('Final copy/wrapper byte length differs: ' + item.path);
  }
}
for (const item of manifest.fork_sdk_smoke.actual_models) {
  const bytes = fs.readFileSync(path.join(project, item.path));
  if (bytes.length !== item.bytes || hash(bytes) !== item.sha256) throw new Error('Actual model differs');
}
const page = fs.readFileSync(path.join(project, 'entry/src/main/ets/pages/CheckpointSdkSmoke.ets'), 'utf8');
for (const reference of ['EditorDraftForkCoordinator', 'DraftForkHooks', 'new EditorDraftForkCoordinator', 'fork.begin()', 'fork.retry()',
  'fork.openChild()', 'fork.retirementCommand(', 'fork.releaseRejectedParent()', 'parent.pauseWrites()', 'parent.resumeWrites()']) {
  if (!page.includes(reference)) throw new Error('Actual fork API reference absent: ' + reference);
}
process.stdout.write(JSON.stringify({ ...result, fork_models: manifest.fork_sdk_smoke.actual_models,
  generated_harness: manifest.generated_harness, rewritten_wrappers: manifest.fork_sdk_smoke.rewritten_wrappers,
  base_verify_sha256: hash(fs.readFileSync(baseVerify)), scope: manifest.fork_sdk_smoke.scope,
  cpp_native_scope: manifest.fork_sdk_smoke.cpp_native_scope, runtime: 'NOT_RUN' }, null, 2) + '\n');
