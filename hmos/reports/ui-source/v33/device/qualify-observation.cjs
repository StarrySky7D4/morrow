'use strict';
// Qualify already captured observations; no device input or provider request.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const folder=__dirname;
const flat=nodes=>nodes.flatMap(n=>[n,...flat(n.children||[])]);
const read=n=>JSON.parse(fs.readFileSync(path.join(folder,n)));
const record=n=>{const b=fs.readFileSync(path.join(folder,n));return {path:n,bytes:b.length,sha256:crypto.createHash('sha256').update(b).digest('hex').toUpperCase()};};
const install=read('installation-dev22.json');assert.equal(install.phase,'INSTALLED_AND_LAUNCHED');assert.equal(install.versionCode,1000022);
assert.equal(install.sourceCommit,'ada95e0f119b3d3c3273b8e5bf6da031d3b10c10');
const settings=flat(read('dev22-flat-settings-restored-b.json')),home=flat(read('dev22-final-home-b.json'));
assert.equal(settings.filter(n=>n.type==='Text'&&n.text==='扁平 · 默认').length,1);
assert.equal(home.filter(n=>n.type==='Text'&&n.text==='14').length,1);
assert.equal(home.filter(n=>n.type==='Button'&&n.text==='草稿 4').length,1);
const actions=fs.readdirSync(folder).filter(n=>/^action-.*\.json$/.test(n)).sort();
for(const name of actions){const action=read(name);assert.equal(action.phase,'ACKNOWLEDGED',name);assert.equal(action.device,install.device);}
const names=['installation-dev22.json','bundle-before-dev22.json','bundle-after-dev22.json',
 'dev22-flat-settings-restored-b.json','dev22-flat-settings-restored-b.png','dev22-final-home-b.json','dev22-final-home-b.png'];
for(const name of names)assert.ok(fs.existsSync(path.join(folder,name)),name);
assert.equal(read('bundle-before-dev22.json').applicationInfo.versionCode,1000021);
assert.equal(read('bundle-after-dev22.json').applicationInfo.versionCode,1000022);
const result={qualifiedUtc:new Date().toISOString(),status:'PASS_SCOPED_OBSERVED_VERSION_AND_DISPLAYED_COUNTS',
 sourceCommit:install.sourceCommit,device:install.device,installedVersion:install.versionName,versionCode:install.versionCode,
 hostHap:{bytes:install.hostBytes,sha256:install.hostSha256},finalStyle:'扁平 · 默认',finalRoute:'home',displayedCards:14,displayedDrafts:4,
 acknowledgedActions:actions.length,inputs:[...names,...actions].map(record),
 supersedes:'final-observation.json retains displayed-count qualification but omitted the before bundle file due to an incorrect optional filename. This a2 requires and checks both actual bundle records.',
 interpretation:'Bundle readback plus host immutable HAP/install ACK and actual captured UI. Displayed counts do not prove all database records or draft body bytes. Pointer/navigation/style-only observations are not business-write, all-page pixel or music acceptance.',
 dev23TaskPreferencesDevice:'NOT_RUN',fullParity:'OPEN',unvisitedScopes:['all routes and dimensions/themes/material combinations','task decisions','Preferences crash/restart persistence','music provider URI/grant and audible playback']};
fs.writeFileSync(path.join(folder,'final-observation-a2.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({status:result.status,version:result.versionCode,style:result.finalStyle,cards:14,drafts:4,actions:actions.length}));
