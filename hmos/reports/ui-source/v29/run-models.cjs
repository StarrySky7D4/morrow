'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process');
const crypto = require('node:crypto'), assert = require('node:assert/strict');
const report = __dirname, repo = path.resolve(report, '../../../..');
const phase = process.argv[2] || 'final';
assert.match(phase, /^[a-z0-9-]+$/);
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();
const tracked = cp.execFileSync('git', ['-C', repo, 'ls-files', '--', 'hmos'], {encoding:'utf8'}).trim().split('\n');
const additional = [...fs.readdirSync(path.join(repo,'hmos/tool')).filter(x=>/music-.*\.cjs$/.test(x)).map(x=>'hmos/tool/'+x), 'hmos/entry/src/main/ets/model/MusicFiles.ets','hmos/entry/src/main/ets/model/MusicLibrary.ets','hmos/entry/src/main/ets/model/MusicPlayback.ets','hmos/entry/src/main/ets/pages/PlatformMusicPlayer.ets', ...fs.readdirSync(report).filter(x=>/^music-store-fixture.*\.json$/.test(x)).map(x=>'hmos/reports/ui-source/v29/'+x),'hmos/reports/ui-source/v29/run-models.cjs'];
const names = [...new Set([...tracked.filter(name => name.startsWith('hmos/tool/') ||
  name.startsWith('hmos/entry/src/main/ets/') || /store-fixture\.json$/.test(name)), ...additional])].sort();
const inventory = () => names.map(name => { const bytes = fs.readFileSync(path.join(repo,name));
  return {path:name,bytes:bytes.length,sha256:sha(bytes)}; });
const write = (name,value) => fs.writeFileSync(path.join(report,'models-' + phase + '-' + name + '.json'),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
const before = inventory(); write('inputs-before',before);
const tests = fs.readdirSync(path.join(repo,'hmos/tool')).filter(name => name.endsWith('.test.cjs')).sort()
  .map(name => path.join(repo,'hmos/tool',name));
assert.ok(tests.length >= 36);
const started = new Date().toISOString();
const result = cp.spawnSync(process.execPath,['--test',...tests],{cwd:repo,encoding:'utf8',maxBuffer:64*1024*1024});
const log = (result.stdout || '') + (result.stderr || '');
const logName = 'models-' + phase + '-tests.log';
fs.writeFileSync(path.join(report,logName),log,{flag:'wx'});
const after = inventory(); write('inputs-after',after);
const unchanged = JSON.stringify(after) === JSON.stringify(before);
const proof = {startedUtc:started,finishedUtc:new Date().toISOString(),exitCode:result.status,signal:result.signal,
  suites:tests.length,inputs:names.length,sourceIdentityVerified:unchanged,log:logName,sha256:sha(Buffer.from(log)),
  scope:'Actual-source host tool/models and Index methods using Store-produced fixture DTOs. Controlled lifecycle/transport/render seams are not device acceptance. Full Flutter/Windows parity OPEN.'};
write('result',proof); console.log(log.slice(-1700)); console.log(JSON.stringify(proof));
assert.ok(unchanged,'Actual repository model/test input drift');
assert.equal(result.status,0,'Model test failure');
