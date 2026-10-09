'use strict';
const fs = require('node:fs'), path = require('node:path'), cp = require('node:child_process'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const label = process.argv[2]; assert.match(label || '', /^dev33-preferences-tasks-[a-z0-9-]+$/);
const report = __dirname, repo = path.resolve(report, '../../../..'), project = path.join(repo, 'hmos/.build/device-candidates', label);
const startedUtc = new Date().toISOString();
const r = cp.spawnSync('pwsh', ['-NoLogo','-NoProfile','-File',path.join(project, 'scripts/build-hap.ps1')], { cwd: project, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
const log = (r.stdout || '') + (r.stderr || ''), logName = label + '-sdk-build.log'; fs.writeFileSync(path.join(report, logName), log, { flag: 'wx' });
const result = { startedUtc, finishedUtc: new Date().toISOString(), exitCode: r.status, signal: r.signal, status: r.status === 0 && /BUILD SUCCESSFUL/.test(log) ? 'PASS' : 'FAILED_OR_UNKNOWN',
  project, log: logName, sha256: crypto.createHash('sha256').update(log).digest('hex').toUpperCase(), scope: 'Complete live Task/AppearancePreferences API26 product graph; exact reused v29 native archives; not installed or device acceptance.' };
fs.writeFileSync(path.join(report, label + '-sdk-result.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' }); console.log(log.slice(-4800)); console.log(JSON.stringify(result)); assert.equal(result.status, 'PASS');
