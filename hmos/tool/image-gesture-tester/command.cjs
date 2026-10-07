const fs = require('node:fs');
const operations = new Set(['observe', 'pinch-out', 'pinch-in', 'pinch-moving-focal', 'pan-one', 'pan-two', 'pan-reverse-one', 'pan-reverse-two', 'boundary-pan-two', 'double-tap-reset']);
function command(config) {
  if (!operations.has(config.operation)) throw new Error('Unknown probe operation');
  if (!/^[A-Za-z0-9_-]{1,80}$/.test(config.runLabel || '')) throw new Error('A fresh runLabel is mandatory');
  for (const key of ['expectDraftTitle', 'expectImageName']) {
    if (typeof config[key] !== 'string' || !config[key] || config[key].length > 512 || /[\r\n\0]/.test(config[key])) throw new Error('Exact fixture identity required: ' + key);
  }
  if (!Number.isSafeInteger(config.expectVersionCode) || config.expectVersionCode <= 0) throw new Error('Actual product versionCode is mandatory');
  for (const key of ['left', 'top', 'right', 'bottom']) if (!Number.isFinite(config[key]) || config[key] < 0) throw new Error('Fresh observed px bounds required: ' + key);
  if (config.right - config.left < 120 || config.bottom - config.top < 120) throw new Error('Unusable canvas bounds');
  const args = ['aa', 'test', '-b', 'dev.morrow.hmos.gesturetester', '-m', 'entry_test', '-s', 'unittest', '/ets/testrunner/ImageGestureRunner'];
  for (const key of ['operation', 'runLabel', 'expectVersionCode', 'expectDraftTitle', 'expectImageName', 'left', 'top', 'right', 'bottom', 'expectToken', 'expectHapSha']) {
    if (config[key] !== undefined) args.push('-s', key, String(config[key]));
  }
  args.push('-s', 'timeout', '120000');
  const quote = value => "'" + value.replace(/'/g, "'\\''") + "'";
  return { device: 'NOT_RUN', install: 'NOT_RUN', operation: config.operation, runLabel: config.runLabel,
    aaArguments: args, remoteShellCommand: args.map(quote).join(' '),
    targetBundle: 'dev.morrow.hmos', testerBundle: 'dev.morrow.hmos.gesturetester',
    evidenceLocation: 'Read actual reportDirectory from the runner result: tester getAppContext().filesDir/image-gestures/' + config.runLabel,
    hostSandboxMapping: 'NOT_RUN; no physical device path is guessed' };
}
module.exports = { command };
if (require.main === module) {
  if (process.argv.length !== 3) throw new Error('Usage: node command.cjs observed-probe-config.json (prints only; no device action)');
  process.stdout.write(JSON.stringify(command(JSON.parse(fs.readFileSync(process.argv[2], 'utf8').replace(/^\uFEFF/, ''))), null, 2) + '\n');
}
