'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),cp=require('node:child_process'),assert=require('node:assert/strict');
const repo=path.resolve(__dirname,'../../../..'),source=path.join(repo,'hmos/.build/device-candidates/dev29-music-foundation-final');
const target=path.join(repo,'hmos/.build/device-candidates/dev29-music-sdk-probe-final');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex').toUpperCase();
const copied=JSON.parse(fs.readFileSync(path.join(__dirname,'source-copy-manifest-final.json')));
assert.ok(!fs.existsSync(target));const inputs=[];
for(const x of copied.inputs){const b=fs.readFileSync(path.join(source,x.path));assert.equal(b.length,x.bytes);assert.equal(sha(b),x.sha256);const dest=path.join(target,x.path);fs.mkdirSync(path.dirname(dest),{recursive:true});fs.writeFileSync(dest,b,{flag:'wx'});inputs.push({path:x.path,bytes:b.length,sha256:sha(b),provenance:x.provenance});}
const probe=`import { common } from '@kit.AbilityKit';
import { MusicFiles } from '../model/MusicFiles';
import { MusicLibraryView } from '../model/MusicLibrary';
import { MusicPlayer, MusicPlaybackSource } from '../model/MusicPlayback';
import { createMusicPlayer } from './PlatformMusicPlayer';
@Entry
@Component
struct Index {
  // Compilation-only method. No UI handler calls it, and this probe is not installed.
  private compileOnly(context: common.UIAbilityContext): Promise<MusicPlayer> {
    const library = new MusicLibraryView();
    const source = new MusicPlaybackSource(); source.libraryRevision = library.library_revision;
    return createMusicPlayer(source, new MusicFiles(context));
  }
  build() { Column() { Text('Music SDK compile probe') } }
}
`;
fs.writeFileSync(path.join(target,'entry/src/main/ets/pages/Index.ets'),probe);
fs.writeFileSync(path.join(__dirname,'music-sdk-probe-final.ets'),probe,{flag:'wx'});
const index=inputs.find(x=>x.path==='entry/src/main/ets/pages/Index.ets');Object.assign(index,{bytes:Buffer.byteLength(probe),sha256:sha(Buffer.from(probe)),provenance:{kind:'test-only compilation probe',source:'hmos/reports/ui-source/v29/music-sdk-probe-final.ets'}});
function verify(){for(const x of inputs){const b=fs.readFileSync(path.join(target,x.path));assert.equal(b.length,x.bytes);assert.equal(sha(b),x.sha256,x.path);}}
verify();fs.writeFileSync(path.join(__dirname,'music-sdk-probe-final-inputs.json'),JSON.stringify({scope:'Isolated SDK source import/type check only; altered test entry, not final product HAP or device playback',project:target,inputs},null,2)+'\n',{flag:'wx'});
const startedUtc=new Date().toISOString();const r=cp.spawnSync('pwsh',['-NoLogo','-NoProfile','-File',path.join(target,'scripts/build-hap.ps1')],{cwd:target,encoding:'utf8',maxBuffer:64*1024*1024});
const log=(r.stdout||'')+(r.stderr||'');fs.writeFileSync(path.join(__dirname,'music-sdk-probe-final-build.log'),log,{flag:'wx'});verify();
const result={startedUtc,finishedUtc:new Date().toISOString(),exitCode:r.status,signal:r.signal,status:r.status===0&&/BUILD SUCCESSFUL/.test(log)?'PASS':'FAILED_OR_UNKNOWN',sourceIdentityVerified:true,inputs:inputs.length,log:'music-sdk-probe-final-build.log',sha256:sha(Buffer.from(log)),scope:'New four actual music ETS modules imported in altered uninstalled test entry. SDK compile/type checking only; not final app/music UI/device acceptance.'};
fs.writeFileSync(path.join(__dirname,'music-sdk-probe-final-result.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(log.slice(-2400));console.log(JSON.stringify(result));assert.equal(result.status,'PASS');