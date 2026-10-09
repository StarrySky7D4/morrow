'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const repo=path.resolve(__dirname,'../..'),sourcePath=path.join(repo,'hmos/entry/src/main/ets/pages/Index.ets');
const baselinePath=path.join(repo,'hmos/.build/appearance-integration/v33/index-task-baseline.ets');
const sha=value=>crypto.createHash('sha256').update(value).digest('hex');
function verify(){const oldBytes=fs.readFileSync(baselinePath),newBytes=fs.readFileSync(sourcePath);
  assert.equal(oldBytes.length,350258);assert.equal(sha(oldBytes),'619592c0e8483199ea1ca92ab48fa6eed9efa25be37051e368060e6410786664');
  const old=oldBytes.toString('utf8').replaceAll('\r\n','\n');let restored=newBytes.toString('utf8').replaceAll('\r\n','\n');
  const replace=(value,next)=>{assert.equal(restored.split(value).length-1,1,'unique permitted change: '+value.slice(0,100));restored=restored.replace(value,next);};
  replace("import { AppearancePreferences, AppearancePreferenceView, appearancePreferenceStatus, appearancePreferenceName,\n  appearancePreferenceKey, validateAppearanceCandidate } from '../model/AppearancePreferences';\n",'');
  replace("  @State appearanceStatus: string = '外观设置尚未读取，当前改动未保存。';","  @State appearanceStatus: string = '';");
  for(const name of ['appearanceView','appearancePreferences','appearancePage','appearanceEpoch','appearanceInputRevision','appearanceInputError','appearanceOperation']){
    const pattern=new RegExp('^  (?:@State )?(?:private )?'+name+'[^\\n]*\\n','m');const match=pattern.exec(restored);assert.ok(match,'permitted field '+name);replace(match[0],'');}
  replace('  private settingsScroller: Scroller = new Scroller();','  private appearanceWrites: Promise<void> = Promise.resolve();\n  private settingsScroller: Scroller = new Scroller();');
  replace('  aboutToDisappear(): void {\n    this.appearanceEpoch++;\n    this.appearancePreferences?.dispose();','  aboutToDisappear(): void {');
  replace("  private foregroundChanged(): void {\n    this.appearanceEpoch++;\n    if (this.appearancePreferences && this.pageAlive && this.foreground) {\n      this.refreshAppearanceState();\n      if (this.appearanceView.state === 'not-loaded') { this.restoreAppearance(); }\n      if (this.appearanceView.busy) { this.waitAppearanceForeground(); }\n    }",'  private foregroundChanged(): void {');
  const start=restored.indexOf('  private appearanceOwner(): string'),end=restored.indexOf('  private backSettings(): void',start);
  const oldStart=old.indexOf('  private restoreAppearance(): void'),oldEnd=old.indexOf('  private backSettings(): void',oldStart);
  assert.ok(start>0&&end>start&&oldStart>0&&oldEnd>oldStart);replace(restored.slice(start,end),old.slice(oldStart,oldEnd));
  replace('      this.AppearanceState()\n',"      Text(this.appearanceStatus || '✓ 外观设置自动保存在本机').fontFamily(this.fontFamily || 'HarmonyOS Sans').fontSize(9).fontColor(this.muted()).margin({ top: 13 }).width('100%')\n");
  const builderStart=restored.indexOf('  @Builder\n  AppearanceState() {'),builderEnd=restored.indexOf('  @Builder\n  DailyPanel() {',builderStart);
  assert.ok(builderStart>0&&builderEnd>builderStart);replace(restored.slice(builderStart,builderEnd),'');
  replace("          if (this.settingsPage !== '设置') { this.AppearanceState() }\n",'');
  assert.equal(restored,old,'Every byte outside declared appearance changes preserved after CRLF normalization');
  return{qualification:'PASS_DECLARED_APPEARANCE_CHANGES_ONLY',baseline:{path:path.relative(repo,baselinePath).replaceAll('\\','/'),byte_length:oldBytes.length,sha256:sha(oldBytes)},
    current:{path:path.relative(repo,sourcePath).replaceAll('\\','/'),byte_length:newBytes.length,sha256:sha(newBytes)},
    old_canonical_sha256:sha(old),reversed_canonical_sha256:sha(restored),task_business_music_paint_and_other_fields_preserved:true,
    scope:'Reversing only the listed appearance import/fields/methods/lifecycle/UI changes reproduces the whole frozen task Index exactly after newline normalization.'};}
module.exports={verify,baselinePath};if(require.main===module)console.log(JSON.stringify(verify(),null,2));
