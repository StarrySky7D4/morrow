'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const assert = require('node:assert/strict'), crypto = require('node:crypto');
const report = __dirname, repo = path.resolve(report, '../../../..'), root = path.join(repo, 'hmos');
const label = process.argv[3]; assert.match(label || '', /^dev31-music-ui-[a-z0-9-]+$/);
const project = path.join(root, '.build/device-candidates', label);
const sha = b => crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
const record = (name, base = repo) => { const b = fs.readFileSync(path.join(base, name)); return { path: name, bytes: b.length, sha256: sha(b) }; };
const git = args => cp.execFileSync('git', ['-C', repo, ...args], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
const write = (name, value) => fs.writeFileSync(path.join(report, label + '-' + name + '.json'), JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const read = name => JSON.parse(fs.readFileSync(path.join(report, label + '-' + name + '.json')));
const verify = (items, base = repo) => { for (const item of items) assert.deepEqual(record(item.path, base), item, 'source drift: ' + item.path); };
const scope = 'Music import durable original request, single-use picker lifecycle and Flutter seven-style inner decoration. API26 unsigned debug SDK check; reused exact v29 Rust archives. Not device playback, signed release or full Flutter/Windows parity.';
function files(directory, base = repo) {
  const result = [];
  for (const name of fs.readdirSync(path.join(base, directory)).sort()) {
    const relative = directory + '/' + name, stat = fs.lstatSync(path.join(base, relative));
    assert.ok(!stat.isSymbolicLink(), 'source file cannot be a symlink');
    if (stat.isDirectory()) result.push(...files(relative, base)); else if (stat.isFile()) result.push(relative);
  }
  return result;
}
if (process.argv[2] === 'prepare') {
  assert.equal(git(['rev-parse', 'HEAD']).trim(), '4c04f97e6beb580d33f14f9540db98129fc50180');
  assert.equal(git(['branch', '--show-current']).trim(), 'codex/ArkTsUI');
  assert.ok(!fs.existsSync(project), 'fresh isolated product required');
  const native = JSON.parse(fs.readFileSync(path.join(root, 'reports/ui-source/v29/native-build-inputs.json')));
  verify(native.repository_source_inventory);
  for (const archive of native.archives) { assert.deepEqual(record(archive.path, root), { path: archive.path, bytes: archive.bytes, sha256: archive.sha256 }); assert.equal(record(archive.production_path, root).sha256, archive.sha256); }
  const tracked = git(['ls-files', '--', 'hmos']).trim().split('\n');
  const product = [...new Set([...tracked.filter(n => ['hmos/AppScope/', 'hmos/entry/', 'hmos/hvigor/'].some(p => n.startsWith(p)) ||
    ['hmos/build-profile.json5','hmos/code-linter.json5','hmos/hvigorfile.ts','hmos/oh-package.json5','hmos/oh-package-lock.json5','hmos/scripts/build-hap.ps1'].includes(n)),
    ...files('hmos/entry/src/main/ets')])].sort();
  const inputs = [...new Set([...product, ...native.repository_source_inventory.map(x => x.path),
    'hmos/reports/ui-source/v31/checkpoint.cjs', 'hmos/reports/ui-source/v31/run-sdk.cjs'])].sort().map(n => record(n));
  write('repository-inputs', { createdUtc: new Date().toISOString(), scope, inputs });
  write('native-reuse', { checkedUtc: new Date().toISOString(), status: 'PASS_EXACT_REUSE', baseline: 'v29',
    inputs: native.repository_source_inventory, archives: native.archives, freshlyRebuilt: false, scope: 'Exact current Rust/C++ inputs and production archives match qualified v29 native build.' });
  const copied = [];
  function copy(relative, bytes, provenance) { const target = path.join(project, relative); fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, bytes, { flag: 'wx' }); copied.push({ path: relative, bytes: bytes.length, sha256: sha(bytes), provenance }); }
  for (const name of product) copy(name.slice(5), fs.readFileSync(path.join(repo, name)), { kind: 'actual repository', source: name });
  function dependency(relative, parents = new Set()) {
    const source = path.join(root, 'oh_modules', relative), stat = fs.statSync(source), real = fs.realpathSync(source);
    if (stat.isDirectory()) { assert.ok(!parents.has(real)); const next = new Set(parents); next.add(real);
      for (const name of fs.readdirSync(source).sort()) if (!['node_modules','build','.hvigor'].includes(name)) dependency(path.join(relative, name), next);
    } else { assert.ok(stat.isFile()); copy('oh_modules/' + relative.replaceAll('\\', '/'), fs.readFileSync(source), { kind: 'installed SDK dependency' }); }
  }
  dependency('');
  for (const item of [...copied].filter(x => x.path.startsWith('entry/src/main/cpp/types/libmorrow/')))
    copy('entry/oh_modules/libmorrow.so/' + item.path.slice('entry/src/main/cpp/types/libmorrow/'.length), fs.readFileSync(path.join(project, item.path)), { kind: 'local NAPI types' });
  for (const archive of native.archives) copy(archive.production_path, fs.readFileSync(path.join(root, archive.path)), { kind: 'exact v29 native archive', ...archive });
  verify(inputs); verify(copied.map(({ path, bytes, sha256 }) => ({ path, bytes, sha256 })), project);
  write('source-copy', { createdUtc: new Date().toISOString(), scope, project, inputs: copied });
  console.log(JSON.stringify({ status: 'PASS', phase: 'prepare', label, repositoryInputs: inputs.length, copied: copied.length }));
} else if (process.argv[2] === 'finalize') {
  const repository = read('repository-inputs'), copied = read('source-copy'), build = read('sdk-result');
  assert.equal(build.status, 'PASS'); verify(repository.inputs); verify(copied.inputs.map(({ path, bytes, sha256 }) => ({ path, bytes, sha256 })), project);
  const archivePath = '.build/artifacts/' + label + '/entry-default-unsigned.hap', destination = path.join(root, archivePath);
  fs.mkdirSync(path.dirname(destination), { recursive: true }); fs.copyFileSync(path.join(project, 'entry/build/default/outputs/default/entry-default-unsigned.hap'), destination, fs.constants.COPYFILE_EXCL);
  const artifact = { ...record(archivePath, root), archive_path: archivePath, signed: false, installed: false, versionName: '0.1.0-hmos-dev.21', versionCode: 1000021 };
  write('artifact', { createdUtc: new Date().toISOString(), scope, ...artifact });
  const manifest = { createdUtc: new Date().toISOString(), artifact, inputs: repository.inputs.map(x => ({ path: x.path.slice(5), sha256: x.sha256 })),
    sourceIdentityVerified: true, uiValidation: 'reports/ui-source/v31/validation.md', deliveryScope: scope,
    nativeInputs: 'reports/ui-source/v31/' + label + '-native-reuse.json', nativeArchives: read('native-reuse').archives };
  fs.writeFileSync(path.join(root, 'reports/build-manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  console.log(JSON.stringify({ status: 'PASS', phase: 'finalize', label, artifact }));
} else throw Error('prepare or finalize required');
