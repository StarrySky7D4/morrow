'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, root = path.resolve(report, '../../..'), repo = path.dirname(root);
const baseline = '19582cc1fc66dca96cb979753f35d579112081ef';
const project = path.join(root, '.build/device-candidates/dev26-recovery-foundation-retry1');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const git = args => cp.execFileSync('git', ['-C', repo, ...args], {maxBuffer: 64 * 1024 * 1024});
const json = name => JSON.parse(fs.readFileSync(path.join(report, name)));
const writeJson = (name, value, fresh = true) => fs.writeFileSync(path.join(report, name), JSON.stringify(value, null, 2) + '\n', fresh ? {flag: 'wx'} : undefined);
const record = (name, base = repo) => {const b = fs.readFileSync(path.join(base, name)); return {path:name, bytes:b.length, sha256:sha(b)};};
function verify(items, base = repo) {
  assert.equal(new Set(items.map(x => x.path)).size, items.length);
  for (const item of items) assert.deepEqual(record(item.path, base), item, 'input drift: ' + item.path);
}
const scope = "v26 foundation only: strict current V2 body editing preserving current TaskIds and metadata bytes; exact read-only parent/retirement history and current-child recovery model; Index new-mode/restart/reopen integration OPEN; unsigned, uninstalled, new-native device acceptance NOT_RUN; full Flutter/Windows parity OPEN";
const nativeFile = 'native-build-inputs.json';
if (process.argv[2] === 'native-capture') {
  const frozen = json('editor-handoff-native-inputs.json'); verify(frozen);
  const owned = new Map(frozen.map(x => [x.path, x]));
  const old = JSON.parse(git(['show', baseline + ':hmos/reports/ui-source/v25/native-build-inputs.json']));
  for (const input of old.repository_source_inventory) assert.deepEqual(record(input.path), owned.get(input.path) || input, 'undeclared native drift');
  const names = [...new Set([...old.repository_source_inventory.map(x => x.path),
    ...frozen.filter(x => x.path.startsWith('hmos/rust/')).map(x => x.path), 'hmos/reports/ui-source/v26/build-native.ps1'])].sort();
  writeJson(nativeFile, {capturedUtc:new Date().toISOString(), baseline, scope, repository_source_inventory:names.map(x => record(x)), archives:[], device:'NOT_RUN'});
  console.log(JSON.stringify({status:'PASS',phase:'native-capture',inputs:names.length}));
} else if (process.argv[2] === 'native-adopt') {
  const native = json(nativeFile); verify(native.repository_source_inventory);
  assert.equal(native.archives.length, 0, 'already adopted');
  for (const abi of ['arm64-v8a','x86_64']) {
    const name = '.build/editor-input-native/dev26-recovery-foundation-retry1/' + abi + '/libmorrow_hmos.a';
    const archive = record(name, root), production = 'entry/src/main/cpp/rust/' + abi + '/libmorrow_hmos.a';
    fs.copyFileSync(path.join(root, name), path.join(root, production));
    assert.equal(record(production, root).sha256, archive.sha256);
    native.archives.push({abi, ...archive, production_path:production});
  }
  verify(native.repository_source_inventory);
  native.finishedUtc = new Date().toISOString(); native.builds = ['native-arm64-build.log','native-x64-build.log'];
  writeJson(nativeFile, native, false); console.log(JSON.stringify({status:'PASS',phase:'native-adopt',archives:native.archives}));
} else if (process.argv[2] === 'prepare') {
  assert.ok(!fs.existsSync(project), 'isolated checkpoint must be fresh');
  const native = json(nativeFile); verify(native.repository_source_inventory); assert.equal(native.archives.length,2);
  const old = JSON.parse(git(['show', baseline + ':hmos/reports/build-manifest.json']));
  // Build manifests also list Rust/tool inputs; obtain the full product file set separately.
  const productNames = git(['ls-tree','-r','--name-only',baseline,'--','hmos']).toString().trim().split('\n').filter(x =>
    ['hmos/AppScope/','hmos/entry/','hmos/hvigor/'].some(p => x.startsWith(p)) ||
    ['hmos/build-profile.json5','hmos/code-linter.json5','hmos/hvigorfile.ts','hmos/oh-package.json5','hmos/oh-package-lock.json5','hmos/scripts/build-hap.ps1'].includes(x));
  const names = [...new Set([...old.inputs.map(x => 'hmos/' + x.path), ...productNames, ...native.repository_source_inventory.map(x => x.path),
    'hmos/entry/src/main/ets/model/EditorBusinessSession.ets', 'hmos/tool/editor-business-session-model.test.cjs',
    'hmos/entry/src/main/ets/model/EditorBusinessHandoff.ets', 'hmos/tool/editor-business-handoff-model.test.cjs',
    'hmos/tool/index-business-integration.test.cjs', 'hmos/tool/index-business-test-harness.cjs',
    'hmos/reports/ui-source/v25/editor-handoff-store-fixture.json',
    'hmos/reports/ui-source/v26/checkpoint.cjs','hmos/reports/ui-source/v26/verify-package.ps1'])]
    // Tool test inputs have their own before/after inventory in run-models.cjs;
    // they are not compiler inputs to the product SDK copy.
    .filter(x => !x.startsWith('hmos/tool/')).sort();
  const snapshot = {capturedUtc:new Date().toISOString(),baseline,scope,inputs:names.map(x => record(x))};
  writeJson('build-inputs.json',snapshot);
  const inputs = [];
  function copy(relative, bytes, provenance) {
    const target = path.join(project, relative); fs.mkdirSync(path.dirname(target), {recursive:true});
    fs.writeFileSync(target,bytes,{flag:'wx'}); inputs.push({path:relative,bytes:bytes.length,sha256:sha(bytes),provenance});
  }
  const product = names.filter(x => ['hmos/AppScope/','hmos/entry/','hmos/hvigor/'].some(p => x.startsWith(p)) ||
    ['hmos/build-profile.json5','hmos/code-linter.json5','hmos/hvigorfile.ts','hmos/oh-package.json5','hmos/oh-package-lock.json5','hmos/scripts/build-hap.ps1'].includes(x));
  for (const name of product) copy(name.slice(5),fs.readFileSync(path.join(repo,name)),{kind:'frozen current repository',source:name});
  const dependencies = path.join(root,'oh_modules');
  function dependency(relative, parents = new Set()) {
    const source = path.join(dependencies,relative), stat = fs.statSync(source), real = fs.realpathSync(source);
    if (stat.isDirectory()) {
      assert.ok(!parents.has(real)); const next = new Set(parents); next.add(real);
      for (const n of fs.readdirSync(source).sort()) if (!['node_modules','build','.hvigor'].includes(n)) dependency(path.join(relative,n),next);
    } else {assert.ok(stat.isFile()); copy('oh_modules/' + relative.replaceAll('\\','/'),fs.readFileSync(source),{kind:'installed SDK dependency'});}
  }
  dependency('');
  for (const input of [...inputs].filter(x => x.path.startsWith('entry/src/main/cpp/types/libmorrow/')))
    copy('entry/oh_modules/libmorrow.so/' + input.path.slice('entry/src/main/cpp/types/libmorrow/'.length),fs.readFileSync(path.join(project,input.path)),{kind:'local type dependency',source:input.path});
  for (const archive of native.archives) {
    assert.equal(record(archive.path,root).sha256,archive.sha256); assert.equal(record(archive.production_path,root).sha256,archive.sha256);
    copy(archive.production_path,fs.readFileSync(path.join(root,archive.path)),{kind:'fresh frozen v26 native archive',...archive});
  }
  verify(inputs.map(({path,bytes,sha256}) => ({path,bytes,sha256})),project); verify(snapshot.inputs);
  writeJson('source-copy-manifest.json',{capturedUtc:new Date().toISOString(),baseline,scope,project,inputs});
  console.log(JSON.stringify({status:'PASS',phase:'prepare',inputs:inputs.length,repositoryInputs:names.length,project}));
} else if (process.argv[2] === 'finalize') {
  const snapshot = json('build-inputs.json'), native = json(nativeFile), copied = json('source-copy-manifest.json');
  verify(snapshot.inputs); verify(native.repository_source_inventory);
  verify(copied.inputs.map(({path,bytes,sha256}) => ({path,bytes,sha256})),project);
  const archive = '.build/artifacts/dev26-recovery-foundation-retry1/entry-default-unsigned.hap';
  fs.mkdirSync(path.dirname(path.join(root,archive)),{recursive:true});
  fs.copyFileSync(path.join(project,'entry/build/default/outputs/default/entry-default-unsigned.hap'),path.join(root,archive),fs.constants.COPYFILE_EXCL);
  const artifact = {...record(archive,root),archive_path:archive,signed:false,installed:false};
  const manifest = JSON.parse(git(['show',baseline + ':hmos/reports/build-manifest.json']));
  manifest.createdUtc = new Date().toISOString(); manifest.artifact = artifact;
  manifest.inputs = snapshot.inputs.map(x => ({path:x.path.slice(5),sha256:x.sha256}));
  manifest.uiValidation = 'reports/ui-source/v26/validation.md'; manifest.deliveryScope = scope;
  manifest.nativeInputs = 'reports/ui-source/v26/native-build-inputs.json'; manifest.nativeArchives = native.archives;
  fs.writeFileSync(path.join(root,'reports/build-manifest.json'),JSON.stringify(manifest,null,2)+'\n');
  writeJson('artifact.json',{createdUtc:manifest.createdUtc,...artifact,scope});
  snapshot.finishedUtc = manifest.createdUtc; snapshot.sourceIdentityVerified = true; snapshot.artifact = artifact;
  writeJson('build-inputs.json',snapshot,false);
  console.log(JSON.stringify({status:'PASS',phase:'finalize',artifact}));
} else throw new Error('native-capture, native-adopt, prepare or finalize required');
