'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..'), repo = path.dirname(root), report = __dirname;
const manifestPath = path.join(root, 'reports/build-manifest.json');
const nativePath = path.join(report, 'native-build-inputs.json'), snapshotPath = path.join(report, 'build-inputs.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
function record(name, base = root) {
  const bytes = fs.readFileSync(path.join(base, name));
  return { path: name, bytes: bytes.length, sha256: sha(bytes) };
}
function verify(items, base = root) {
  assert.equal(new Set(items.map(item => item.path)).size, items.length, 'unique input paths');
  for (const item of items) assert.deepEqual(record(item.path, base), item, 'input drift ' + item.path);
}
function files(dir, base = root) {
  return fs.readdirSync(path.join(base, dir), { withFileTypes: true }).flatMap(item =>
    item.isDirectory() ? (['target', '.build', '.git'].includes(item.name) ? [] : files(dir + '/' + item.name, base)) : [dir + '/' + item.name]);
}
function verifyNative(adopted = true) {
  const native = JSON.parse(fs.readFileSync(nativePath));
  verify(native.repository_source_inventory, repo);
  const discovered = [...files('hmos/rust', repo), ...files('hmos/shared', repo)].filter(name =>
    /\.(rs|proto|capnp)$/.test(name) || name.endsWith('/Cargo.toml'));
  const names = new Set(native.repository_source_inventory.map(item => item.path));
  for (const name of discovered) assert.ok(names.has(name), 'missing native input ' + name);
  for (const archive of native.archives) {
    const input = record(archive.path);
    assert.equal(input.bytes, archive.bytes); assert.equal(input.sha256, archive.sha256);
    if (adopted) assert.equal(record(archive.production_path).sha256, archive.sha256, 'adopted archive drift');
  }
  return native;
}
const scope = 'Restored editor reactive admission fix and strict development editor business foundation; editor_save/continued business not wired in Index; product package device NOT_RUN; full Flutter/Windows parity OPEN';
const action = process.argv[2];
if (action === 'native') {
  assert.ok(!fs.existsSync(nativePath), 'native checkpoint already exists');
  const previous = JSON.parse(fs.readFileSync(path.join(root, 'reports/ui-source/v20/retirement/native-build-inputs.json')));
  const owned = JSON.parse(fs.readFileSync(path.join(report, 'editor-business-source-inputs.json'))).entries;
  verify(owned, repo);
  const changed = new Set(owned.map(item => item.path));
  for (const item of previous.repository_source_inventory) {
    if (!changed.has(item.path)) assert.deepEqual(record(item.path, repo), item, 'unrelated native drift ' + item.path);
  }
  const names = [...new Set([...previous.repository_source_inventory.map(item => item.path), ...changed,
    'hmos/reports/ui-source/v22/build-native.ps1'])].sort();
  const archives = ['arm64-v8a', 'x86_64'].map(abi => ({ abi,
    ...record('.build/editor-input-native/dev22-business-checkpoint/' + abi + '/libmorrow_hmos.a'),
    production_path: 'entry/src/main/cpp/rust/' + abi + '/libmorrow_hmos.a' }));
  const native = { capturedUtc: new Date().toISOString(), scope,
    identity_scope: 'Frozen owned Rust inputs checked after complete host regression; all previous native inputs unchanged except declared owned changes; fresh dual ABI release archives; pinned Cargo graph unchanged',
    repository_source_inventory: names.map(name => record(name, repo)), archives,
    builds: ['native-arm64-build.log', 'native-x64-build.log'], device: 'NOT_RUN' };
  fs.writeFileSync(nativePath, JSON.stringify(native, null, 2) + '\n', { flag: 'wx' });
  verifyNative(false);
  // Root owns archive adoption; preserved historical archives remain immutable.
  for (const archive of archives) fs.copyFileSync(path.join(root, archive.path), path.join(root, archive.production_path));
  verifyNative();
  fs.writeFileSync(path.join(report, 'native-adoption-check.json'), JSON.stringify({ status: 'PASS',
    repository_sources: names.length, inventory_complete: true, archives: 2, production_archives: 2,
    archive_identity: archives, device: 'NOT_RUN' }, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ phase: action, status: 'PASS', nativeInputs: names.length, archives }));
} else if (action === 'capture') {
  assert.ok(!fs.existsSync(snapshotPath), 'product checkpoint already exists');
  const old = JSON.parse(fs.readFileSync(manifestPath)), native = verifyNative();
  const names = [...new Set([...old.inputs.map(item => item.path),
    ...native.repository_source_inventory.map(item => item.path.replace(/^hmos\//, '')),
    'entry/src/main/ets/model/EditorBusiness.ets', 'tool/editor-business-model.test.cjs',
    'reports/ui-source/v22/checkpoint.cjs', 'reports/ui-source/v22/verify-package.ps1'])].sort();
  const snapshot = { capturedUtc: new Date().toISOString(), scope, inputs: names.map(name => record(name)) };
  fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ phase: action, status: 'PASS', inputs: names.length }));
} else if (action === 'finalize') {
  const snapshot = JSON.parse(fs.readFileSync(snapshotPath)); verify(snapshot.inputs); const native = verifyNative();
  const manifest = JSON.parse(fs.readFileSync(manifestPath));
  const archive = '.build/artifacts/dev22-business-checkpoint/entry-default-unsigned.hap';
  fs.mkdirSync(path.dirname(path.join(root, archive)), { recursive: true });
  fs.copyFileSync(path.join(root, manifest.artifact.path), path.join(root, archive), fs.constants.COPYFILE_EXCL);
  const artifact = record(archive);
  manifest.createdUtc = new Date().toISOString();
  manifest.inputs = snapshot.inputs.map(({ path, sha256 }) => ({ path, sha256 }));
  manifest.hmosVersion = JSON.parse(fs.readFileSync(path.join(root, 'AppScope/app.json5'))).app.versionName;
  manifest.artifact = { path: manifest.artifact.path, archive_path: archive, bytes: artifact.bytes,
    sha256: artifact.sha256, signed: false, installed: false };
  manifest.uiValidation = 'reports/ui-source/v22/validation.md'; manifest.deliveryScope = scope;
  manifest.nativeInputs = 'reports/ui-source/v22/native-build-inputs.json'; manifest.nativeArchives = native.archives;
  snapshot.finishedUtc = manifest.createdUtc; snapshot.sourceIdentityVerified = true; snapshot.artifact = manifest.artifact;
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  fs.writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2) + '\n');
  console.log(JSON.stringify({ phase: action, status: 'PASS', inputs: manifest.inputs.length, artifact: manifest.artifact }));
} else throw new Error('native, capture or finalize required');
