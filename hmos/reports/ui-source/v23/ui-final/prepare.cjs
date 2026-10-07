'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, root = path.resolve(report, '../../../..'), repo = path.dirname(root);
const baseline = '8c3059615d9adc1296363cd365267b24929b3cc7';
const project = path.join(root, '.build/device-candidates/dev23-todo-watch-final');
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
if (process.argv[2] === 'prepare') {
  assert.ok(!fs.existsSync(project) && !fs.existsSync(manifestPath), 'fresh isolated UI candidate');
  fs.mkdirSync(project, { recursive: true }); const inputs = [];
  function write(relative, bytes, provenance) {
    const target = path.join(project, relative); fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, bytes, { flag: 'wx' }); inputs.push({ path: relative, bytes: bytes.length, sha256: sha(bytes), provenance });
  }
  const names = git(['ls-tree', '-r', '--name-only', baseline, '--', 'hmos']).toString().trim().split('\n').filter(name =>
    ['hmos/AppScope/', 'hmos/entry/', 'hmos/hvigor/'].some(prefix => name.startsWith(prefix)) ||
    ['hmos/build-profile.json5', 'hmos/code-linter.json5', 'hmos/hvigorfile.ts', 'hmos/oh-package.json5',
      'hmos/oh-package-lock.json5', 'hmos/scripts/build-hap.ps1'].includes(name));
  const override = 'entry/src/main/ets/pages/EditorTodos.ets';
  for (const name of names) {
    const relative = name.slice(5), original = git(['show', baseline + ':' + name]);
    const bytes = relative === override ? fs.readFileSync(path.join(root, relative)) : original;
    if (relative === override) assert.equal(sha(bytes), 'E0A3DA36C9AEED16B37FCD3BFD46B83EF35A32373933D38E5413692C15C835DE');
    write(relative, bytes, relative === override ? { kind: 'frozen current capture and Watch repair', baseline_sha256: sha(original) } : { kind: 'Git blob', baseline, path: name });
  }
  const dependencies = path.join(root, 'oh_modules');
  function dependency(relative, ancestors = new Set()) {
    const source = path.join(dependencies, relative), stat = fs.statSync(source), real = fs.realpathSync(source);
    if (stat.isDirectory()) {
      assert.ok(!ancestors.has(real)); const next = new Set(ancestors); next.add(real);
      for (const name of fs.readdirSync(source).sort()) if (!['node_modules', 'build', '.hvigor'].includes(name)) dependency(path.join(relative, name), next);
    } else {
      assert.ok(stat.isFile()); write('oh_modules/' + relative.replaceAll('\\', '/'), fs.readFileSync(source), { kind: 'installed SDK project dependency', source });
    }
  }
  dependency('');
  for (const input of [...inputs].filter(input => input.path.startsWith('entry/src/main/cpp/types/libmorrow/'))) {
    write('entry/oh_modules/libmorrow.so/' + input.path.slice('entry/src/main/cpp/types/libmorrow/'.length),
      fs.readFileSync(path.join(project, input.path)), { kind: 'entry dependency from Git types', source: input.path, baseline });
  }
  const native = JSON.parse(git(['show', baseline + ':hmos/reports/ui-source/v22/native-build-inputs.json']));
  for (const input of native.repository_source_inventory) assert.equal(sha(git(['show', baseline + ':' + input.path])), input.sha256, 'frozen native source ' + input.path);
  for (const archive of native.archives) {
    const bytes = fs.readFileSync(path.join(root, archive.path)); assert.equal(bytes.length, archive.bytes); assert.equal(sha(bytes), archive.sha256);
    write(archive.production_path, bytes, { kind: 'verified immutable v22 native archive', ...archive });
  }
  const manifest = { capturedUtc: new Date().toISOString(), baseline, project,
    scope: '8c305961 product plus frozen EditorTodos construction-selection and complete per-prop snapshot guard; includes unchanged v22 native foundation; candidate business-intent/handoff contracts are not implemented; final package device NOT_RUN; full parity OPEN',
    override, native_inputs: native.repository_source_inventory.length, native_archives: native.archives, inputs };
  verify(manifest); fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ result: 'PASS', project, inputs: inputs.length, nativeInputs: manifest.native_inputs }));
} else if (process.argv[2] === 'verify') {
  const manifest = JSON.parse(fs.readFileSync(manifestPath)); verify(manifest);
  console.log(JSON.stringify({ result: 'PASS', project, inputs: manifest.inputs.length }));
} else if (process.argv[2] === 'archive') {
  const manifest = JSON.parse(fs.readFileSync(manifestPath)); verify(manifest);
  const source = path.join(project, 'entry/build/default/outputs/default/entry-default-unsigned.hap');
  const archive = path.join(root, '.build/artifacts/dev23-todo-watch-final/entry-default-unsigned.hap');
  fs.mkdirSync(path.dirname(archive), { recursive: true }); fs.copyFileSync(source, archive, fs.constants.COPYFILE_EXCL);
  const bytes = fs.readFileSync(archive), artifact = { createdUtc: new Date().toISOString(), baseline, project,
    inputs: manifest.inputs.length, archive, bytes: bytes.length, sha256: sha(bytes), signed: false, installed: false, scope: manifest.scope };
  fs.writeFileSync(path.join(report, 'artifact.json'), JSON.stringify(artifact, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify(artifact));
} else throw new Error('prepare, verify or archive required');
