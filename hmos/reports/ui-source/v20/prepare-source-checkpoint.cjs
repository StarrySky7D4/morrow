'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex').toUpperCase();
const manifestFile = path.join(root, 'reports/build-manifest.json');
const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
const version = JSON.parse(fs.readFileSync(path.join(root, 'AppScope/app.json5'), 'utf8')).app;
assert.equal(version.versionName, '0.1.0-hmos-dev.19'); assert.equal(version.versionCode, 1000019);
assert.equal(hash(path.join(root, '.build/artifacts/dev19/entry-default-unsigned.hap')),
  'F7A913980CF3213536820727A677EEC04EA169FE2B65CE16E305F352D14BE4EC', 'preserved published dev19');
const artifact = 'entry/build/default/outputs/default/entry-default-unsigned.hap';
assert.ok(process.argv.slice(2).every(arg => arg === '--final'), 'only the final archive selector is accepted');
const archive = process.argv.includes('--final')
  ? '.build/artifacts/dev20-source-checkpoint-final/entry-default-unsigned.hap'
  : '.build/artifacts/dev20-source-checkpoint/entry-default-unsigned.hap';
const artifactFile = path.join(root, artifact), archiveFile = path.join(root, archive);
fs.mkdirSync(path.dirname(archiveFile), { recursive: true });
if (fs.existsSync(archiveFile)) assert.equal(hash(archiveFile), hash(artifactFile), 'never replace a different checkpoint archive');
else fs.copyFileSync(artifactFile, archiveFile);
const additions = ['entry/src/main/ets/model/EditorInputPolicy.ets', 'entry/src/main/ets/model/EditorInputHash.ets',
  'entry/src/main/ets/model/EditorTodos.ets', 'entry/src/main/ets/pages/EditorTodos.ets',
  'rust/src/editor_input.rs', 'rust/src/editor_input/tests.rs', 'rust/src/create_todos.rs', 'rust/src/create_todos/tests.rs'];
manifest.inputs = [...new Set([...manifest.inputs.map(input => input.path), ...additions])]
  .map(input => ({ path: input, sha256: hash(path.join(root, input)) }));
manifest.createdUtc = new Date().toISOString();
manifest.hmosVersion = version.versionName;
manifest.deliveryScope = 'dev20 source checkpoint, not a dev20 release; Index has not adopted the new EditorTodos/InputPolicy UI';
manifest.artifact = { path: artifact, archive_path: archive, bytes: fs.statSync(artifactFile).size,
  sha256: hash(artifactFile), signed: false, installed: false };
manifest.uiValidation = 'reports/ui-source/v20/validation.md';
const encoded = JSON.stringify(manifest, null, 2) + '\n';
fs.writeFileSync(manifestFile, encoded); fs.writeFileSync(path.join(__dirname, 'checkpoint-build-manifest.json'), encoded);
console.log(JSON.stringify({ result: 'PASS', version: manifest.hmosVersion, inputs: manifest.inputs.length, artifact: manifest.artifact }));
