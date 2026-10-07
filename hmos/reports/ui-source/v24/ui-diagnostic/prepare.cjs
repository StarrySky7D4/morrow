'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, root = path.resolve(report, '../../../..'), repo = path.dirname(root);
const baseline = '7f07d07480660bbc868369b9565dc98a47e8af08';
const project = path.join(root, '.build/device-candidates/dev24-capture-diagnostic');
const manifestPath = path.join(report, 'source-copy-manifest.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const git = args => cp.execFileSync('git', ['-C', repo, ...args], { maxBuffer: 64 * 1024 * 1024 });
function verify(manifest) {
  assert.equal(new Set(manifest.inputs.map(input => input.path)).size, manifest.inputs.length);
  for (const input of manifest.inputs) {
    const bytes = fs.readFileSync(path.join(project, input.path));
    assert.equal(bytes.length, input.bytes); assert.equal(sha(bytes), input.sha256, input.path);
  }
}
function diagnostic(original) {
  let source = original.toString('utf8');
  const selection = '  private leaseSelectionChanged(owner: string, name: string, start: number, end: number): void {\n';
  const uncaptured = '  private leaseUncaptured(owner: string, name: string, event: string): void {\n';
  assert.equal(source.split(selection).length, 2); assert.equal(source.split(uncaptured).length, 2);
  source = source.replace(selection, selection + "    console.info('HMOS_CAPTURE_DIAG selection name=' + name + ' start=' + start + ' end=' + end + ' length=' + this.draftField(name).text.length + ' focused=' + this.draftFocusedFields.has(name) + ' restore=' + this.draftRestoreInput + ' owned=' + this.ownsEditorView(owner));\n");
  source = source.replace(uncaptured, uncaptured + "    const eventKind: string = event.indexOf('\\\"kind\\\":\\\"input\\\"') >= 0 ? 'todo_input' : event.indexOf('\\\"kind\\\":\\\"selection\\\"') >= 0 ? 'todo_selection' : event.indexOf('\\\"start\\\"') >= 0 ? 'field_selection' : 'other';\n" +
    "    console.info('HMOS_CAPTURE_DIAG uncaptured name=' + name + ' kind=' + eventKind + ' focused=' + this.draftFocusedFields.has(name) + ' restore=' + this.draftRestoreInput + ' owned=' + this.ownsEditorView(owner) + ' event_length=' + event.length);\n");
  return Buffer.from(source);
}
const scope = 'Frozen 7f07d074 product plus metadata-only capture diagnostics in isolated Index; no live Index/Rust edits adopted; unchanged v22 native; no text or preview payload logged; not a release or queue-drain proof';
if (process.argv[2] === 'prepare') {
  assert.ok(!fs.existsSync(project) && !fs.existsSync(manifestPath)); fs.mkdirSync(project, { recursive: true }); const inputs = [];
  function write(relative, bytes, provenance) {
    const target = path.join(project, relative); fs.mkdirSync(path.dirname(target), { recursive: true }); fs.writeFileSync(target, bytes, { flag: 'wx' });
    inputs.push({ path: relative, bytes: bytes.length, sha256: sha(bytes), provenance });
  }
  const names = git(['ls-tree', '-r', '--name-only', baseline, '--', 'hmos']).toString().trim().split('\n').filter(name =>
    ['hmos/AppScope/', 'hmos/entry/', 'hmos/hvigor/'].some(prefix => name.startsWith(prefix)) ||
    ['hmos/build-profile.json5', 'hmos/code-linter.json5', 'hmos/hvigorfile.ts', 'hmos/oh-package.json5', 'hmos/oh-package-lock.json5', 'hmos/scripts/build-hap.ps1'].includes(name));
  for (const name of names) {
    const relative = name.slice(5), original = git(['show', baseline + ':' + name]), edited = relative === 'entry/src/main/ets/pages/Index.ets';
    if (edited) assert.equal(sha(original), '34DDECA5A375328ADB1A7EC53DE7F8E5D1A90804CD8634FAA85672D7A588160C');
    write(relative, edited ? diagnostic(original) : original, { kind: edited ? 'isolated diagnostic of Git blob' : 'Git blob', baseline, path: name, original_sha256: sha(original) });
  }
  const dependencies = path.join(root, 'oh_modules');
  function dependency(relative, ancestors = new Set()) {
    const source = path.join(dependencies, relative), stat = fs.statSync(source), real = fs.realpathSync(source);
    if (stat.isDirectory()) {
      assert.ok(!ancestors.has(real)); const next = new Set(ancestors); next.add(real);
      for (const name of fs.readdirSync(source).sort()) if (!['node_modules', 'build', '.hvigor'].includes(name)) dependency(path.join(relative, name), next);
    } else { assert.ok(stat.isFile()); write('oh_modules/' + relative.replaceAll('\\', '/'), fs.readFileSync(source), { kind: 'installed SDK project dependency', source }); }
  }
  dependency('');
  for (const input of [...inputs].filter(input => input.path.startsWith('entry/src/main/cpp/types/libmorrow/'))) {
    write('entry/oh_modules/libmorrow.so/' + input.path.slice('entry/src/main/cpp/types/libmorrow/'.length), fs.readFileSync(path.join(project, input.path)), { kind: 'entry dependency from Git types', source: input.path, baseline });
  }
  const native = JSON.parse(git(['show', baseline + ':hmos/reports/ui-source/v22/native-build-inputs.json']));
  for (const input of native.repository_source_inventory) assert.equal(sha(git(['show', baseline + ':' + input.path])), input.sha256);
  for (const archive of native.archives) {
    const bytes = fs.readFileSync(path.join(root, archive.path)); assert.equal(bytes.length, archive.bytes); assert.equal(sha(bytes), archive.sha256);
    write(archive.production_path, bytes, { kind: 'verified immutable v22 native archive', ...archive });
  }
  const manifest = { capturedUtc: new Date().toISOString(), baseline, project, scope, native_inputs: native.repository_source_inventory.length, inputs };
  verify(manifest); fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' }); console.log(JSON.stringify({ status: 'PASS', inputs: inputs.length, project }));
} else if (process.argv[2] === 'archive') {
  const manifest = JSON.parse(fs.readFileSync(manifestPath)); verify(manifest);
  const archive = path.join(root, '.build/artifacts/dev24-capture-diagnostic/entry-default-unsigned.hap'); fs.mkdirSync(path.dirname(archive), { recursive: true });
  fs.copyFileSync(path.join(project, 'entry/build/default/outputs/default/entry-default-unsigned.hap'), archive, fs.constants.COPYFILE_EXCL);
  const bytes = fs.readFileSync(archive), artifact = { createdUtc: new Date().toISOString(), baseline, project, inputs: manifest.inputs.length, archive, bytes: bytes.length, sha256: sha(bytes), signed: false, installed: false, scope };
  fs.writeFileSync(path.join(report, 'artifact.json'), JSON.stringify(artifact, null, 2) + '\n', { flag: 'wx' }); console.log(JSON.stringify(artifact));
} else throw new Error('prepare or archive required');
