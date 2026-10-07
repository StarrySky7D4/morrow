'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, root = path.resolve(report, '../../..'), repo = path.dirname(root);
const baseline = '8c3059615d9adc1296363cd365267b24929b3cc7';
const manifestPath = path.join(root, 'reports/build-manifest.json'), snapshotPath = path.join(report, 'build-inputs.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const git = args => cp.execFileSync('git', ['-C', repo, ...args], { maxBuffer: 64 * 1024 * 1024 });
const frozen = new Map([
  ['entry/src/main/ets/pages/EditorTodos.ets', 'E0A3DA36C9AEED16B37FCD3BFD46B83EF35A32373933D38E5413692C15C835DE'],
  ['tool/editor-todos-component-methods.test.cjs', 'B03017FC9089AD2393ADB24576CAAFCC5A11FE296BF9F3F22330A7DF097EDC56']
]);
function record(name, base = root) {
  const bytes = fs.readFileSync(path.join(base, name));
  return { path: name, bytes: bytes.length, sha256: sha(bytes) };
}
function verify(items, base = root) {
  assert.equal(new Set(items.map(item => item.path)).size, items.length);
  for (const item of items) assert.deepEqual(record(item.path, base), item, 'source drift ' + item.path);
}
function native() {
  const value = JSON.parse(fs.readFileSync(path.join(root, 'reports/ui-source/v22/native-build-inputs.json')));
  verify(value.repository_source_inventory, repo);
  for (const item of value.repository_source_inventory) assert.equal(sha(git(['show', baseline + ':' + item.path])), item.sha256);
  for (const archive of value.archives) {
    const saved = record(archive.path); assert.equal(saved.bytes, archive.bytes); assert.equal(saved.sha256, archive.sha256);
    assert.equal(record(archive.production_path).sha256, archive.sha256);
  }
  return value;
}
const scope = 'Todo construction-selection admission and complete per-prop snapshot guard; unchanged v22 Rust business foundation; business-intent and Index handoff documents are candidate contracts only; final package device NOT_RUN; prior 3116 candidate restore FAILED_OR_UNKNOWN; full Flutter/Windows parity OPEN';
if (process.argv[2] === 'capture') {
  assert.ok(!fs.existsSync(snapshotPath));
  const old = JSON.parse(git(['show', baseline + ':hmos/reports/build-manifest.json']));
  for (const input of old.inputs) {
    const expected = frozen.get(input.path) || input.sha256;
    assert.equal(record(input.path).sha256, expected, 'undeclared baseline drift ' + input.path);
  }
  native();
  const names = [...new Set([...old.inputs.map(input => input.path), 'reports/ui-source/v23/checkpoint.cjs',
    'reports/ui-source/v23/ui-final/prepare.cjs', 'reports/ui-source/v23/verify-package.ps1'])].sort();
  const snapshot = { capturedUtc: new Date().toISOString(), baseline, scope, inputs: names.map(name => record(name)) };
  fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'PASS', phase: 'capture', inputs: names.length, nativeInputs: native().repository_source_inventory.length }));
} else if (process.argv[2] === 'finalize') {
  const snapshot = JSON.parse(fs.readFileSync(snapshotPath)); verify(snapshot.inputs); const libraries = native();
  const copy = JSON.parse(fs.readFileSync(path.join(report, 'ui-final/source-copy-manifest.json')));
  for (const input of copy.inputs) {
    if (input.provenance.kind === 'Git blob' || input.provenance.kind === 'frozen current capture and Watch repair') {
      assert.equal(record(input.path).sha256, input.sha256, 'isolated product differs from repository ' + input.path);
    }
  }
  const artifact = JSON.parse(fs.readFileSync(path.join(report, 'ui-final/artifact.json')));
  const archive = '.build/artifacts/dev23-todo-watch-final/entry-default-unsigned.hap';
  const bytes = record(archive); assert.equal(bytes.bytes, artifact.bytes); assert.equal(bytes.sha256, artifact.sha256);
  const manifest = JSON.parse(git(['show', baseline + ':hmos/reports/build-manifest.json']));
  manifest.createdUtc = new Date().toISOString(); manifest.inputs = snapshot.inputs.map(({ path, sha256 }) => ({ path, sha256 }));
  manifest.artifact = { path: archive, archive_path: archive, bytes: bytes.bytes, sha256: bytes.sha256, signed: false, installed: false };
  manifest.uiValidation = 'reports/ui-source/v23/validation.md'; manifest.deliveryScope = scope;
  manifest.nativeInputs = 'reports/ui-source/v22/native-build-inputs.json'; manifest.nativeArchives = libraries.archives;
  snapshot.finishedUtc = manifest.createdUtc; snapshot.sourceIdentityVerified = true; snapshot.artifact = manifest.artifact;
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + '\n');
  console.log(JSON.stringify({ status: 'PASS', phase: 'finalize', inputs: manifest.inputs.length, artifact: manifest.artifact }));
} else throw new Error('capture or finalize required');
