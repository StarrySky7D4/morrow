'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..'), repo = path.dirname(root);
const report = __dirname, manifestPath = path.join(root, 'reports/build-manifest.json');
const snapshotPath = path.join(report, 'build-inputs.json');
const nativePath = path.join(root, 'reports/ui-source/v20/retirement/native-build-inputs.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const record = (name, base = root) => { const bytes = fs.readFileSync(path.join(base, name)); return { path: name, bytes: bytes.length, sha256: sha(bytes) }; };
const scope = 'UI lease/raw-draft fork integration and mounted-row focus repair; reused unchanged b486 native source/archives; no device acceptance of this new package; development version remains dev19';
function verifyNative() {
  const native = JSON.parse(fs.readFileSync(nativePath));
  for (const input of native.repository_source_inventory) assert.deepEqual(record(input.path, repo), input, 'native source drift ' + input.path);
  assert.equal(native.archives.length, 2);
  for (const archive of native.archives) {
    assert.equal(record(archive.path).sha256, archive.sha256, 'native archive drift');
    assert.equal(record(archive.production_path).sha256, archive.sha256, 'adopted archive drift');
  }
  return { checkedUtc: new Date().toISOString(), status: 'PASS', basis: 'b4860df4b253ff594bfb0811a68adea90d05ce81',
    source_manifest: 'reports/ui-source/v20/retirement/native-build-inputs.json',
    repository_sources: native.repository_source_inventory.length, archives: native.archives, rebuilt: false };
}
function verify(snapshot) { for (const input of snapshot.inputs) assert.deepEqual(record(input.path), input, 'build source drift ' + input.path); }
if (process.argv[2] === 'capture') {
  assert.ok(!fs.existsSync(snapshotPath), 'immutable checkpoint already captured');
  const old = JSON.parse(fs.readFileSync(manifestPath));
  const names = [...new Set([...old.inputs.map(input => input.path), 'entry/src/main/ets/pages/EditorViewLease.ets',
    'tool/index-draft-fork-integration.test.cjs', 'reports/ui-source/v21/checkpoint.cjs', 'reports/ui-source/v21/verify-package.ps1'])];
  const native = verifyNative();
  const snapshot = { capturedUtc: new Date().toISOString(), scope, inputs: names.map(name => record(name)) };
  fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + '\n', { flag: 'wx' });
  fs.writeFileSync(path.join(report, 'native-reuse-check.json'), JSON.stringify(native, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ phase: 'capture', status: 'PASS', inputs: names.length, nativeSources: native.repository_sources, reusedNative: true }));
} else if (process.argv[2] === 'finalize') {
  const snapshot = JSON.parse(fs.readFileSync(snapshotPath)); verify(snapshot); verifyNative();
  const manifest = JSON.parse(fs.readFileSync(manifestPath));
  const archive = '.build/artifacts/dev21-ui-checkpoint/entry-default-unsigned.hap';
  fs.mkdirSync(path.dirname(path.join(root, archive)), { recursive: true });
  fs.copyFileSync(path.join(root, manifest.artifact.path), path.join(root, archive), fs.constants.COPYFILE_EXCL);
  const artifact = record(archive);
  manifest.createdUtc = new Date().toISOString(); manifest.inputs = snapshot.inputs.map(({ path, sha256 }) => ({ path, sha256 }));
  manifest.hmosVersion = JSON.parse(fs.readFileSync(path.join(root, 'AppScope/app.json5'))).app.versionName;
  manifest.artifact = { path: manifest.artifact.path, archive_path: archive, bytes: artifact.bytes, sha256: artifact.sha256, signed: false, installed: false };
  manifest.uiValidation = 'reports/ui-source/v21/validation.md'; manifest.deliveryScope = scope;
  snapshot.finishedUtc = manifest.createdUtc; snapshot.sourceIdentityVerified = true; snapshot.artifact = manifest.artifact;
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + '\n');
  console.log(JSON.stringify({ phase: 'finalize', status: 'PASS', inputs: manifest.inputs.length, artifact: manifest.artifact }));
} else throw new Error('capture or finalize required');
