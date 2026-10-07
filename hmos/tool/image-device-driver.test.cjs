'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const { test } = require('node:test'), assert = require('node:assert/strict');
const driver = require('./image-device-driver.cjs');
const oldProject = path.resolve(__dirname, 'image-gesture-tester/work/install-test-' + crypto.randomUUID()); fs.mkdirSync(oldProject, { recursive: true });
// Synthetic ZIP manifests exercise host artifact/identity gates. They are not
// installable or device-qualified HAPs; real binary hashes are audited separately.
function zipManifest(bundle, module, type) {
  const content = Buffer.from(JSON.stringify({ app: { bundleName: bundle, debug: true, versionCode: 200001 }, module: { name: module, type } }));
  const name = Buffer.from('module.json'), local = Buffer.alloc(30), central = Buffer.alloc(46), end = Buffer.alloc(22);
  local.writeUInt32LE(0x04034b50); local.writeUInt32LE(content.length, 18); local.writeUInt32LE(content.length, 22); local.writeUInt16LE(name.length, 26);
  central.writeUInt32LE(0x02014b50); central.writeUInt32LE(content.length, 20); central.writeUInt32LE(content.length, 24); central.writeUInt16LE(name.length, 28);
  const front = Buffer.concat([local, name, content]), index = Buffer.concat([central, name]);
  end.writeUInt32LE(0x06054b50); end.writeUInt16LE(1, 8); end.writeUInt16LE(1, 10); end.writeUInt32LE(index.length, 12); end.writeUInt32LE(front.length, 16);
  return Buffer.concat([front, index, end]);
}
const mainFile = path.join(oldProject, 'entry-default-unsigned.hap'), testFile = path.join(oldProject, 'entry-ohosTest-unsigned.hap');
const mainBytes = zipManifest('dev.morrow.hmos.gesturetester', 'entry', 'entry'), testBytes = zipManifest('dev.morrow.hmos.gesturetester', 'entry_test', 'feature');
fs.writeFileSync(mainFile, mainBytes); fs.writeFileSync(testFile, testBytes);
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const install = () => ({ stageLabel: 'mock-owned-install', device: 'mock-device',
  mainHap: mainFile, testHap: testFile, mainSha256: hash(mainBytes), testSha256: hash(testBytes) });
const probe = () => ({ operation: 'pan-two', runLabel: 'mock-only-no-device', expectVersionCode: 1000019,
  expectDraftTitle: 'mock-draft-title', expectImageName: 'mock-fixture.png', left: 119, top: 232, right: 712, bottom: 840 });
function syntheticReport(status = 'ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW') {
  const directory = path.resolve(__dirname, 'image-gesture-tester/work/host-test-' + crypto.randomUUID()); fs.mkdirSync(directory);
  const observation = { versionCode: 1000019, draftTitle: 'mock-draft-title', imageName: 'mock-fixture.png',
    windowFocused: true, zoomPercent: 200, canvas: { left: 119, top: 232, right: 712, bottom: 840 } };
  const report = { status, productBundle: 'dev.morrow.hmos', expectedVersionCode: 1000019, expectedDraftTitle: 'mock-draft-title',
    expectedImageName: 'mock-fixture.png', runLabel: 'mock-only-no-device', operation: 'pan-two',
    before: { ...observation }, intent: { ...observation }, after: { ...observation }, injectionAcknowledged: true,
    tokenStatus: 'NOT_EXPOSED_BY_UI', hapShaStatus: 'HOST_DECLARED_NOT_READ_BACK' };
  const png = Buffer.alloc(33); Buffer.from('89504e470d0a1a0a', 'hex').copy(png); png.write('IHDR', 12); png.writeUInt32BE(1320, 16); png.writeUInt32BE(2232, 20);
  for (const label of ['before', 'after']) {
    fs.writeFileSync(path.join(directory, label + '.png'), png); fs.writeFileSync(path.join(directory, label + '.tree.json'), '[]');
    fs.writeFileSync(path.join(directory, label + '.observation.json'), JSON.stringify(report[label]));
  }
  fs.writeFileSync(path.join(directory, 'input-intent.json'), JSON.stringify({ ...report, status: 'INPUT_INTENT' }));
  const save = () => fs.writeFileSync(path.join(directory, 'result.json'), JSON.stringify(report)); save();
  return { directory, report, save };
}
test('install plan hashes exact standalone artifact manifests and prints only independent install commands', () => {
  const result = driver.installPlan(install()); assert.equal(result.status, 'HOST_HASH_VERIFIED_PLAN_ONLY');
  assert.equal(result.device, 'NOT_RUN'); assert.equal(result.main.bytes, mainBytes.length); assert.equal(result.test.bytes, testBytes.length);
  assert.deepEqual(result.steps[3].hdcArguments.slice(2), ['shell', 'bm', 'install', '-p', result.remoteStagingDirectory]);
  assert.equal(result.steps.some(step => step.hdcArguments.includes('uninstall')), false);
});
test('even a hash-matched product bundle disguised as tester filename is refused', () => {
  const directory = path.join(oldProject, 'foreign'); fs.mkdirSync(directory);
  const bytes = zipManifest('dev.morrow.hmos', 'entry', 'entry'), file = path.join(directory, 'entry-default-unsigned.hap'); fs.writeFileSync(file, bytes);
  assert.throws(() => driver.installPlan({ ...install(), mainHap: file, mainSha256: hash(bytes) }));
});
test('install plans fail before execution for changed bytes, omitted hashes, guessed device and unsafe staging label', () => {
  for (const override of [{ mainSha256: 'a'.repeat(64) }, { testSha256: undefined }, { device: '' },
    { stageLabel: '../foreign' }, { mainHap: path.resolve(__dirname, '../entry/build/default/outputs/default/entry-default-unsigned.hap') }]) {
    assert.throws(() => driver.installPlan({ ...install(), ...override }));
  }
});
test('probe command uses freshly supplied identity/version/bounds and actual ETS multipointer plan', () => {
  const result = driver.probePlan(probe()); assert.equal(result.device, 'NOT_RUN'); assert.equal(result.pointerPlan.fingers, 2);
  assert.equal(result.pixelQualification, 'NOT_RUN'); assert.equal(result.previewToken, 'NOT_EXPOSED_BY_UI');
  assert.ok(result.aaArguments.includes('/ets/testrunner/ImageGestureRunner'));
  const gap = result.pointerPlan.points[1][0].x - result.pointerPlan.points[0][0].x;
  for (let step = 0; step < 9; step++) assert.equal(result.pointerPlan.points[1][step].x - result.pointerPlan.points[0][step].x, gap);
});
test('probe printer rejects missing live expectations and observe never prepares input matrix', () => {
  for (const override of [{ expectVersionCode: undefined }, { expectImageName: '' }, { left: undefined }, { operation: 'auto-replay' }]) {
    assert.throws(() => driver.probePlan({ ...probe(), ...override }));
  }
  assert.equal(driver.probePlan({ ...probe(), operation: 'observe' }).pointerPlan, undefined);
});
test('collected report inspection records hashes/percent scope without claiming pixel displacement or private token identity', () => {
  const fixture = syntheticReport(), result = driver.inspectReport(fixture.directory);
  assert.equal(result.runnerStatus, 'ACKNOWLEDGED_REQUIRES_VISUAL_REVIEW'); assert.equal(result.evidence.length, 8);
  assert.equal(result.zoomReadback.beforePercent, 200); assert.equal(result.zoomReadback.afterPercent, 200);
  assert.match(result.imageRectReadback, /NOT_DECODED_PIXEL_DISPLACEMENT/); assert.match(result.visualQualification, /NOT_QUALIFIED/);
  assert.equal(result.replay, 'DO_NOT_REPLAY_RECONCILE_CURRENT_STATE');
});
test('unknown input remains unknown during readonly inspection and never creates a retry command', () => {
  const fixture = syntheticReport('UNKNOWN_INPUT_EFFECT'), result = driver.inspectReport(fixture.directory);
  assert.equal(result.runnerStatus, 'UNKNOWN_INPUT_EFFECT'); assert.equal(result.replay, 'DO_NOT_REPLAY_RECONCILE_CURRENT_STATE');
  assert.equal(result.aaArguments, undefined);
});
test('collected success requires matching typed observations, complete artifacts and positive injection acknowledgement', () => {
  for (const change of ['wrong-owner', 'missing-after', 'unacknowledged', 'bad-png', 'missing-tree']) {
    const fixture = syntheticReport();
    if (change === 'wrong-owner') fixture.report.after.draftTitle = 'another-draft';
    if (change === 'missing-after') delete fixture.report.after;
    if (change === 'unacknowledged') fixture.report.injectionAcknowledged = false;
    if (change === 'bad-png') fs.writeFileSync(path.join(fixture.directory, 'after.png'), 'bad');
    if (change === 'missing-tree') fs.renameSync(path.join(fixture.directory, 'after.tree.json'), path.join(fixture.directory, 'unrelated-tree.json'));
    fixture.save(); assert.throws(() => driver.inspectReport(fixture.directory), change);
  }
});
