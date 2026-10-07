'use strict';
// Host-only planning and evidence inspection. Importing or running this file
// performs no HDC, app, provider, installation or input call.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), vm = require('node:vm'), zlib = require('node:zlib');
const { command } = require('./image-gesture-tester/command.cjs');
const ownedWork = path.resolve(__dirname, 'image-gesture-tester/work');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const plain = value => JSON.parse(JSON.stringify(value));
const psQuote = text => "'" + String(text).replace(/'/g, "''") + "'";
function exactHash(value, name) {
  if (typeof value !== 'string' || !/^[a-fA-F0-9]{64}$/.test(value)) throw Error('Exact artifact SHA256 required: ' + name);
  return value.toUpperCase();
}
function hapManifest(bytes) {
  let eocd = -1;
  for (let offset = bytes.length - 22; offset >= Math.max(0, bytes.length - 65557); offset--) {
    if (bytes.readUInt32LE(offset) === 0x06054b50) { eocd = offset; break; }
  }
  if (eocd < 0) throw Error('Tester artifact is not a bounded ZIP HAP');
  const count = bytes.readUInt16LE(eocd + 10); let offset = bytes.readUInt32LE(eocd + 16), found;
  for (let entry = 0; entry < count; entry++) {
    if (offset + 46 > bytes.length || bytes.readUInt32LE(offset) !== 0x02014b50) throw Error('Invalid HAP directory');
    const flags = bytes.readUInt16LE(offset + 8), method = bytes.readUInt16LE(offset + 10);
    const compressedSize = bytes.readUInt32LE(offset + 20), size = bytes.readUInt32LE(offset + 24), nameLength = bytes.readUInt16LE(offset + 28);
    const extraLength = bytes.readUInt16LE(offset + 30), commentLength = bytes.readUInt16LE(offset + 32), localOffset = bytes.readUInt32LE(offset + 42);
    const name = bytes.subarray(offset + 46, offset + 46 + nameLength).toString('utf8');
    if (name === 'module.json') {
      if (found || flags & 1 || size > 65536 || localOffset + 30 > bytes.length || bytes.readUInt32LE(localOffset) !== 0x04034b50) throw Error('Invalid or duplicate bounded HAP manifest');
      const start = localOffset + 30 + bytes.readUInt16LE(localOffset + 26) + bytes.readUInt16LE(localOffset + 28);
      if (start + compressedSize > bytes.length) throw Error('Truncated HAP manifest');
      const compressed = bytes.subarray(start, start + compressedSize);
      const expanded = method === 0 ? compressed : method === 8 ? zlib.inflateRawSync(compressed, { maxOutputLength: 65536 }) : undefined;
      if (!expanded || expanded.length !== size) throw Error('Unsupported HAP manifest encoding');
      found = JSON.parse(expanded.toString('utf8'));
    }
    offset += 46 + nameLength + extraLength + commentLength;
  }
  if (!found) throw Error('HAP manifest absent'); return found;
}
function artifact(file, expected, expectedFilename, expectedModule, expectedType) {
  const absolute = path.resolve(file || '');
  if (!absolute.startsWith(ownedWork + path.sep) || path.basename(absolute) !== expectedFilename) throw Error('Independent owned tester artifact required');
  const real = fs.realpathSync(absolute);
  if (!real.startsWith(fs.realpathSync(ownedWork) + path.sep)) throw Error('Artifact resolves outside owned tester work');
  const bytes = fs.readFileSync(real), digest = sha(bytes);
  if (digest !== exactHash(expected, expectedFilename)) throw Error('Tester artifact changed; install plan refused');
  const manifest = hapManifest(bytes);
  if (manifest.app?.bundleName !== 'dev.morrow.hmos.gesturetester' || manifest.app.debug !== true ||
    manifest.module?.name !== expectedModule || manifest.module.type !== expectedType || !Number.isSafeInteger(manifest.app.versionCode)) {
    throw Error('Artifact is not the independent debug tester module');
  }
  return { path: absolute, bytes: bytes.length, sha256: digest, bundleName: manifest.app.bundleName,
    moduleName: manifest.module.name, versionCode: manifest.app.versionCode,
    signing: 'UNSIGNED_BUILD_EXPECTATION_DEVICE_ACCEPTANCE_NOT_RUN' };
}
function installPlan(config) {
  if (!/^[A-Za-z0-9_-]{1,80}$/.test(config.stageLabel || '')) throw Error('Fresh installation stageLabel required');
  if (typeof config.device !== 'string' || !/^[A-Za-z0-9_.:-]{1,120}$/.test(config.device)) throw Error('Explicit root-owned HDC device required');
  const main = artifact(config.mainHap, config.mainSha256, 'entry-default-unsigned.hap', 'entry', 'entry');
  const test = artifact(config.testHap, config.testSha256, 'entry-ohosTest-unsigned.hap', 'entry_test', 'feature');
  if (main.versionCode !== test.versionCode) throw Error('Independent tester module versions differ');
  const directory = '/data/local/tmp/morrow-image-tester-' + config.stageLabel;
  const steps = [
    { stage: 'create-fresh-directory', args: ['shell', 'mkdir', directory] },
    { stage: 'send-main', args: ['file', 'send', main.path, directory + '/entry-default-unsigned.hap'] },
    { stage: 'send-test', args: ['file', 'send', test.path, directory + '/entry-ohosTest-unsigned.hap'] },
    { stage: 'install-both-independent-modules', args: ['shell', 'bm', 'install', '-p', directory] },
    { stage: 'read-installed-tester', args: ['shell', 'bm', 'dump', '-n', 'dev.morrow.hmos.gesturetester'] }
  ].map(step => ({ stage: step.stage, hdcArguments: ['-t', config.device, ...step.args],
    powershell: '& ' + psQuote(config.hdcPath || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe') +
      ' ' + ['-t', config.device, ...step.args].map(psQuote).join(' ') }));
  return { status: 'HOST_HASH_VERIFIED_PLAN_ONLY', device: 'NOT_RUN', installation: 'NOT_RUN',
    bundleName: 'dev.morrow.hmos.gesturetester', main, test, remoteStagingDirectory: directory, steps,
    instruction: 'Root executes each step once after the prior acknowledgement. Existing directory, rejected signature, provider failure or unknown outcome stops the plan. Reconcile before another attempt. No uninstall or security-setting workaround is included.' };
}
function plans() {
  const ts = require(process.env.HMOS_TYPESCRIPT_PATH || process.env.HMOS_TYPESCRIPT ||
    'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript');
  const file = path.join(__dirname, 'image-gesture-tester/ImagePointerPlan.ets');
  const compiled = ts.transpileModule(fs.readFileSync(file, 'utf8'), { fileName: file, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 } });
  if ((compiled.diagnostics || []).some(item => item.category === ts.DiagnosticCategory.Error)) throw Error('Actual pointer plan did not transpile');
  const exports = {}; vm.runInNewContext(compiled.outputText, { exports, require: () => ({}) }); return exports;
}
function probePlan(config) {
  const output = command(config);
  if (config.operation !== 'observe' && config.operation !== 'double-tap-reset') {
    output.pointerPlan = plain(plans().imagePointerPlan(config.operation,
      { left: config.left, top: config.top, right: config.right, bottom: config.bottom, displayId: config.displayId || 0 }));
  }
  output.fixtureBinding = 'HOST_EXPECTATIONS_REQUIRE_LIVE_RUNNER_AND_ROOT_FIXTURE_EVIDENCE';
  output.pixelQualification = 'NOT_RUN'; output.previewToken = 'NOT_EXPOSED_BY_UI';
  return output;
}
function fileEvidence(directory, name, kind, required) {
  const file = path.join(directory, name);
  if (!fs.existsSync(file)) { if (required) throw Error('Required collected evidence absent: ' + name); return { file, present: false }; }
  const bytes = fs.readFileSync(file), result = { file, present: true, bytes: bytes.length, sha256: sha(bytes) };
  if (kind === 'png') {
    if (bytes.length < 33 || bytes.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a' || bytes.subarray(12, 16).toString('ascii') !== 'IHDR') throw Error('Invalid collected PNG: ' + name);
    result.width = bytes.readUInt32BE(16); result.height = bytes.readUInt32BE(20);
    if (!result.width || !result.height) throw Error('Empty PNG dimensions');
    result.validation = 'PNG_SIGNATURE_AND_IHDR_ONLY_PIXEL_REVIEW_REQUIRED';
  } else if (kind === 'json') { JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, '')); }
  return result;
}
function inspectReport(directory) {
  const absolute = path.resolve(directory), resultFile = path.join(absolute, 'result.json');
  const result = JSON.parse(fs.readFileSync(resultFile, 'utf8').replace(/^\uFEFF/, ''));
  const statuses = ['READONLY_OBSERVED', 'ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW', 'STOPPED', 'UNKNOWN_INPUT_EFFECT'];
  if (!statuses.includes(result.status) || result.productBundle !== 'dev.morrow.hmos') throw Error('Unrecognized image runner evidence');
  const operated = result.status === 'ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW';
  const observed = operated || result.status === 'READONLY_OBSERVED';
  if (observed && (!result.before || !result.intent)) throw Error('Successful runner observation is incomplete');
  if (operated && (!result.after || result.injectionAcknowledged !== true)) throw Error('Acknowledged operation lacks after-observation or injection acknowledgement');
  const evidence = [fileEvidence(absolute, 'result.json', 'json', true),
    fileEvidence(absolute, 'before.png', 'png', observed), fileEvidence(absolute, 'before.tree.json', 'json', observed),
    fileEvidence(absolute, 'before.observation.json', 'json', observed), fileEvidence(absolute, 'input-intent.json', 'json', operated),
    fileEvidence(absolute, 'after.png', 'png', operated), fileEvidence(absolute, 'after.tree.json', 'json', operated),
    fileEvidence(absolute, 'after.observation.json', 'json', operated)];
  for (const observation of [result.before, result.intent, result.after].filter(Boolean)) {
    if (observation.versionCode !== result.expectedVersionCode || observation.draftTitle !== result.expectedDraftTitle ||
      observation.imageName !== result.expectedImageName || observation.windowFocused !== true) throw Error('Collected observation identity is inconsistent');
  }
  let zoomReadback = 'UNKNOWN';
  if (result.before && result.after) zoomReadback = { beforePercent: result.intent?.zoomPercent ?? result.before.zoomPercent,
    afterPercent: result.after.zoomPercent, pixelTransform: 'NOT_READ_BACK_BY_PERCENTAGE' };
  return { status: 'COLLECTED_ARTIFACTS_INSPECTED_ONLY', runnerStatus: result.status, operation: result.operation,
    runLabel: result.runLabel, evidence, zoomReadback, imageRectReadback: 'UITEST_COMPONENT_BOUNDS_ONLY_NOT_DECODED_PIXEL_DISPLACEMENT',
    previewToken: result.tokenStatus, installedArchiveHash: result.hapShaStatus,
    visualQualification: 'NOT_QUALIFIED_REQUIRES_ROOT_SCREENSHOT_REVIEW',
    replay: result.status === 'UNKNOWN_INPUT_EFFECT' || evidence.some(item => item.present && item.file.endsWith('input-intent.json')) ?
      'DO_NOT_REPLAY_RECONCILE_CURRENT_STATE' : 'NO_AUTOMATIC_REPLAY', report: result };
}
module.exports = { installPlan, probePlan, inspectReport };
if (require.main === module) {
  if (process.argv.length !== 4) throw Error('Usage: node image-device-driver.cjs install-plan|probe-plan config.json OR inspect-report collected-directory (prints only; no device calls)');
  const action = process.argv[2], value = process.argv[3];
  const config = () => JSON.parse(fs.readFileSync(value, 'utf8').replace(/^\uFEFF/, ''));
  const output = action === 'install-plan' ? installPlan(config()) : action === 'probe-plan' ? probePlan(config()) :
    action === 'inspect-report' ? inspectReport(value) : (() => { throw Error('Unknown explicit stage'); })();
  process.stdout.write(JSON.stringify(output, null, 2) + '\n');
}
