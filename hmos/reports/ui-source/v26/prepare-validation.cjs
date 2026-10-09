'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const old = path.resolve(__dirname, '../v25');
const scope = 'v26 foundation only: strict current V2 body editing preserving current TaskIds and metadata bytes; exact read-only parent/retirement history and current-child recovery model; Index new-mode/restart/reopen integration OPEN; unsigned, uninstalled, new-native device acceptance NOT_RUN; full Flutter/Windows parity OPEN';
function fresh(name, value) { fs.writeFileSync(path.join(__dirname, name), value, { flag: 'wx' }); }
let checkpoint = fs.readFileSync(path.join(old, 'checkpoint.cjs'), 'utf8');
checkpoint = checkpoint.replace("const baseline = '08fcea98cd1a99decee099ff91754ff0137ba971';", "const baseline = '19582cc1fc66dca96cb979753f35d579112081ef';")
  .replace(/dev25-business-handoff-checkpoint-retry2/g, 'dev26-recovery-foundation')
  .replace(/dev25-business-handoff-checkpoint/g, 'dev26-recovery-foundation')
  .replace(/ui-source\/v25\//g, 'ui-source/v26/')
  .replace('hmos/reports/ui-source/v24/native-build-inputs.json', 'hmos/reports/ui-source/v25/native-build-inputs.json')
  .replace('hmos/reports/ui-source/v26/editor-handoff-store-fixture.json', 'hmos/reports/ui-source/v25/editor-handoff-store-fixture.json')
  .replace(/fresh frozen v25 native archive/g, 'fresh frozen v26 native archive')
  .replace(/^const scope = .*;$/m, 'const scope = ' + JSON.stringify(scope) + ';');
assert.ok(checkpoint.includes('19582cc1fc66dca96cb979753f35d579112081ef'));
assert.ok(checkpoint.includes('ui-source/v25/native-build-inputs.json'));
fresh('checkpoint.cjs', checkpoint);
fresh('build-native.ps1', fs.readFileSync(path.join(old, 'build-native.ps1'), 'utf8')
  .replace(/dev25-business-handoff-checkpoint/g, 'dev26-recovery-foundation')
  .replace(/Business handoff checkpoint Rust/g, 'Recovery foundation Rust'));
fresh('verify-package.ps1', fs.readFileSync(path.join(old, 'verify-package.ps1'), 'utf8')
  .replace(/dev25-business-handoff-checkpoint-retry2/g, 'dev26-recovery-foundation')
  .replace(/dev25-business-handoff-checkpoint/g, 'dev26-recovery-foundation')
  .replace(/fresh v25/g, 'fresh v26'));
console.log('Fresh v26 validation scripts prepared; previous evidence retained.');
