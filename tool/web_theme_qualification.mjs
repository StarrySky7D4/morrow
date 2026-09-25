import {readFile,writeFile} from 'node:fs/promises';
import path from 'node:path';
import {createHash} from 'node:crypto';

export async function qualifyTheme({call,sessionId,root,evaluate,until,rect,enable,waitLabel,click,reopenPage}) {
  const name='中秋 · 月满庭 / Moonlit Court';
  const caption='MOONLIT COURT / MID-AUTUMN';
  const fixture=path.join(root,'test/fixtures/plugins/morrow-mid-autumn-1.0.0.morrowplugin');
  const bytes=await readFile(fixture);
  if(bytes.length!==661049||createHash('sha256').update(bytes).digest('hex')!=='b9c8dd591ef88e2d14be4ee7f4b402ff5d6563530618f71cbaf1e4d1b61d9751')throw Error('Original theme fixture changed');
  const idle=()=>until(()=>evaluate('(()=>{const a=globalThis.__mediaWorkerActivity;return a.sent>0&&a.pending===0&&performance.now()-a.last>1000;})()'),'theme host receipts');
  const select=async(file,label='Choose theme plugin')=>{
    await call('Page.setInterceptFileChooserDialog',{enabled:true},sessionId);
    await click(label);
    await until(()=>evaluate("!!document.querySelector('input[type=file]')"),'theme file picker');
    const input=await call('Runtime.evaluate',{expression:"document.querySelector('input[type=file]')"},sessionId);
    await call('DOM.setFileInputFiles',{files:[file],objectId:input.result.objectId},sessionId);
    await call('Page.setInterceptFileChooserDialog',{enabled:false},sessionId);
  };
  const reload=async()=>{
    // Recreate the page and Worker, retaining the browser's static-code cache.
    // Device data must still be read afresh from persistent storage.
    await call('Page.reload',{},sessionId);await enable();await waitLabel('New idea');await idle();
    if(await rect('Review save'))throw Error('Theme operation broke settings persistence');
  };
  const manager=async()=>{await click('Plugins and services');await waitLabel('Choose theme plugin');await idle();};
  const expand=async()=>{await click(name);await idle();};
  await click('Create local workspace');await waitLabel('New idea');await idle();
  console.log('Theme acceptance: workspace ready; checking previews and rejection');
  await manager();
  // Inspection and cancellation do not install a package.
  await select(fixture);await waitLabel('Import');await click('Cancel');await idle();
  if(await rect(name))throw Error('Cancelled theme preview installed a package');
  const bad=path.join(root,'build/bad-theme.morrowplugin');await writeFile(bad,Buffer.from([1,2,3]));
  await select(bad);await until(()=>evaluate("document.body.innerText.includes('container header')"),'corrupt theme rejection');await idle();
  if(await rect('Import'))throw Error('Corrupt theme admitted');
  await select(path.join(root,'sdk/compat/guest-v1-rc1/rust-transform.mplugin'));
  await until(()=>evaluate("document.body.innerText.includes('pure theme plugins only')"),'non-theme rejection');
  await idle();
  const oversized=path.join(root,'build/oversized-theme.morrowplugin');await writeFile(oversized,Buffer.alloc(4276931));
  await select(oversized);await until(()=>evaluate("document.body.innerText.includes('size exceeds')"),'oversized theme rejection');
  await idle();
  // Lose only the UI receipt after the real Worker commits. Reload must discover
  // the installed-but-disabled package without replaying or granting approval.
  await select(fixture);await waitLabel('Import');await idle();
  console.log('Theme acceptance: valid preview; simulating lost import receipt');
  await evaluate('globalThis.__holdThemeReceipt=true');await click('Import');
  await until(()=>evaluate('globalThis.__themeReceiptHeld===true'),'held theme import receipt');
  await reload();
  if(await rect(caption))throw Error('Unconfirmed import auto-enabled theme');
  console.log('Theme acceptance: interrupted import recovered disabled; enabling');
  await manager();await expand();await click('Approve and enable');await waitLabel('Disable');await idle();
  await reload();
  if(await rect('Interface style: Flat · Default'))await click('Interface style: Flat · Default');
  await waitLabel(caption);
  console.log('Theme acceptance: light theme restored; closing and reopening tab');
  const light=await call('Page.captureScreenshot',{format:'png'},sessionId);
  await writeFile(path.join(root,'build/web-theme-light.png'),Buffer.from(light.data,'base64'));
  await reopenPage();await enable();await waitLabel('New idea');await idle();await waitLabel(caption);
  console.log('Theme acceptance: new tab restored theme; checking dark mode');
  await click('Dark');await idle();await reload();await waitLabel(caption);
  const dark=await call('Page.captureScreenshot',{format:'png'},sessionId);
  await writeFile(path.join(root,'build/web-theme-dark.png'),Buffer.from(dark.data,'base64'));
  console.log('Theme acceptance: dark theme restored; disabling and uninstalling');
  await manager();await expand();await click('Disable');await waitLabel('Approve and enable');await idle();await reload();
  if(await rect(caption))throw Error('Disabled theme still active after reopen');
  await manager();await expand();await click('Uninstall (keep content)');await idle();await reload();
  if(await rect(caption))throw Error('Uninstalled theme still active');
  await manager();if(await rect(name))throw Error('Uninstalled theme still registered');
  await reload();await click('White');await idle();await reload();
  console.log('Theme acceptance: uninstalled; checking content and media saves');
  // The same library remains writable after every rejected/interrupted import.
  await click('New idea');await waitLabel('Save idea');
  await until(()=>evaluate("['INPUT','TEXTAREA'].includes(document.activeElement?.tagName)"),'recovery editor focus');
  await call('Input.insertText',{text:'Theme recovery content'},sessionId);
  await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Tab',code:'Tab',windowsVirtualKeyCode:9},sessionId);
  await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Tab',code:'Tab',windowsVirtualKeyCode:9},sessionId);
  await evaluate('new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');
  await click('Save idea');await waitLabel('Theme recovery content');await idle();
  await click('Texture');await select(path.join(root,'test/fixtures/texture.png'),'Local media');await waitLabel('texture.png');await idle();
  const wav=Buffer.alloc(1644);wav.write('RIFF');wav.writeUInt32LE(wav.length-8,4);wav.write('WAVEfmt ',8);wav.writeUInt32LE(16,16);wav.writeUInt16LE(1,20);wav.writeUInt16LE(1,22);wav.writeUInt32LE(8000,24);wav.writeUInt32LE(16000,28);wav.writeUInt16LE(2,32);wav.writeUInt16LE(16,34);wav.write('data',36);wav.writeUInt32LE(1600,40);
  const music=path.join(root,'build/theme-recovery-music.wav');await writeFile(music,wav);
  await select(music,'Import music');await waitLabel('theme-recovery-music');await idle();
  await reload();await waitLabel('Theme recovery content');await waitLabel('texture.png');await waitLabel('theme-recovery-music');
  console.log('Theme acceptance: all lifecycle and persistence assertions passed');
}
