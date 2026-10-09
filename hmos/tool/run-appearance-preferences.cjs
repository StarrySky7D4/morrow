'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), cp = require('node:child_process');
const assert = require('node:assert/strict'), root = path.resolve(__dirname, '..'), repo = path.resolve(root, '..');
const output = path.join(root, 'reports/ui-source/v33'); fs.mkdirSync(output, { recursive: true });
const label = process.argv[2] || 'initial'; assert.match(label, /^[A-Za-z0-9_-]+$/); const base = path.join(output, 'appearance-preferences-' + label);
for(const suffix of ['-inputs-before.json','-inputs-after.json','-tests.log','-result.json']) assert.ok(!fs.existsSync(base+suffix),'preserve report '+base+suffix);
const tsRoot = process.env.HMOS_TYPESCRIPT || 'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/build-tools/ets-loader/node_modules/typescript';
const inputs = [__filename,path.join(__dirname,'appearance-preferences-test-harness.cjs'),path.join(__dirname,'appearance-preferences.test.cjs'),
  path.join(root,'entry/src/main/ets/model/Appearance.ets'),path.join(root,'entry/src/main/ets/model/AppearancePreferences.ets'),
  path.join(tsRoot,'package.json'),require.resolve(tsRoot)];
const references = [path.join(repo,'build/win-cloud-20261005/lib/main.dart'),path.join(repo,'build/win-cloud-20261005/lib/storage.dart'),
  path.join(root,'shared/plugins/workbench/src/preferences.rs'),
  'C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/ets/api/@ohos.data.preferences.d.ts'];
const digest=filenames=>filenames.map(file=>{const bytes=fs.readFileSync(file);return{path:path.relative(repo,file).replaceAll('\\','/'),byte_length:bytes.length,
  sha256:crypto.createHash('sha256').update(bytes).digest('hex')};});
const before={observed_at:new Date().toISOString(),inputs:digest(inputs),references:digest(references)};
fs.writeFileSync(base+'-inputs-before.json',JSON.stringify(before,null,2)+'\n');
const run=cp.spawnSync(process.execPath,['--test',path.join(__dirname,'appearance-preferences.test.cjs')],{encoding:'utf8',maxBuffer:8*1024*1024});
const log=(run.stdout||'')+(run.stderr||'');fs.writeFileSync(base+'-tests.log',log);
const after={observed_at:new Date().toISOString(),inputs:digest(inputs),references:digest(references)};
fs.writeFileSync(base+'-inputs-after.json',JSON.stringify(after,null,2)+'\n');
const count=regexp=>Number(log.match(regexp)?.[1]||0),tests=count(/(?:ℹ|#) tests (\d+)/),passed=count(/(?:ℹ|#) pass (\d+)/),failed=count(/(?:ℹ|#) fail (\d+)/),
  skipped=count(/(?:ℹ|#) skipped (\d+)/),cancelled=count(/(?:ℹ|#) cancelled (\d+)/);
const exact=JSON.stringify(before.inputs)===JSON.stringify(after.inputs)&&JSON.stringify(before.references)===JSON.stringify(after.references);
const summary={qualification:'PASS_SCOPED_ACTUAL_ETS_APPEARANCE_PREFERENCES',observed_at:after.observed_at,exit_code:run.status,
  tests,passed,failed,skipped,cancelled,duration_ms:Number(log.match(/(?:ℹ|#) duration_ms ([\d.]+)/)?.[1]||0),input_count:inputs.length,reference_count:references.length,
  source_exact_before_after:exact,node_version:process.version,typescript_version:JSON.parse(fs.readFileSync(path.join(tsRoot,'package.json'),'utf8')).version,
  log_sha256:crypto.createHash('sha256').update(log).digest('hex'),inputs:after.inputs,references:after.references,
  sdk_build:'NOT_RUN',device_persistence:'NOT_RUN',index_integration:'NOT_RUN',
  assertions:['typed complete snapshots and legacy-only defaults','all bounded values and material graph','unreadable original blocks all writes',
    'separate current, candidate, observed, original and issued unknown literals','literal unknown-field retention including large integers',
    'stable material-id retention across edits/reorder/removal','serialized puts and flushes with exact source preflight/readback',
    'put/flush/readback fault vectors stop all later queued writes','explicit reread and durable acknowledgement for unknown recovery',
    'no automatic replay, default replacement or rollback','foreground/page owner guards and stale/disposed response suppression',
    'same-namespace in-process live operation barrier across page/model replacement','fixed terminal Unknown transfer requiring explicit recovery'],
  controlled_seams:['preferences read/cache/put/flush/exact readback','owned page/foreground token','deferred operation completion','UI change observer'],
  limitations:['The actual ETS source is transpiled and executed; the storage port is a controlled in-memory fault backend.',
    'No ArkUI rendering, real preferences service durability, SDK build, device install, Rust business mutation or Windows parity acceptance is claimed.',
    'The shared same-namespace barrier enforces in-process model exclusivity; it is not cross-process CAS or a crash/restart journal.',
    'Recovery flush confirms the observed cache value after a terminal failed/unknown operation; it cannot reconcile a still-running platform call.']};
if(run.status!==0||tests!==61||passed!==61||failed!==0||skipped!==0||cancelled!==0||!exact)summary.qualification='FAILED';
fs.writeFileSync(base+'-result.json',JSON.stringify(summary,null,2)+'\n');console.log(JSON.stringify(summary,null,2));if(summary.qualification==='FAILED')process.exitCode=1;
