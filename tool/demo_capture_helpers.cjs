// Load inside a Playwright session for local Flutter Web demo capture.
module.exports = async function({page,context,root,fs}) {
  const dir=root+'/build/demo-video';
  const cdp=await context.newCDPSession(page);
  let active=null,pending=[];
  cdp.on('Page.screencastFrame',event=>{
    if(active){
      const f=String(active.frames.length).padStart(5,'0')+'.jpg';
      active.frames.push({file:f,time:event.metadata.timestamp});
      pending.push(fs.writeFile(active.path+'/'+f,Buffer.from(event.data,'base64')));
    }
    cdp.send('Page.screencastFrameAck',{sessionId:event.sessionId}).catch(()=>{});
  });
  async function cursor(){await page.evaluate(()=>{
    document.getElementById('demo-cursor')?.remove();
    const el=document.createElement('div');el.id='demo-cursor';
    el.style.cssText='position:fixed;width:18px;height:18px;border:2px solid #7960bb;border-radius:50%;background:#ffffff99;left:720px;top:420px;z-index:2147483647;pointer-events:none;box-shadow:0 0 0 5px #aa88dd22;transition:transform .18s,background .18s';
    document.body.append(el);
    window.addEventListener('pointermove',e=>{el.style.left=e.clientX-9+'px';el.style.top=e.clientY-9+'px'});
    window.addEventListener('pointerdown',()=>{el.style.transform='scale(1.8)';el.style.background='#9676dd99'});
    window.addEventListener('pointerup',()=>{el.style.transform='scale(1)';el.style.background='#ffffff99'});
  })}
  async function start(id){
    await cursor();const path=dir+'/takes/'+id;await fs.mkdir(path,{recursive:true});
    active={id,path,frames:[]};pending=[];
    await cdp.send('Page.startScreencast',{format:'jpeg',quality:92,maxWidth:1440,maxHeight:850,everyNthFrame:1});
    await page.waitForTimeout(300);
  }
  async function stop(){
    await page.waitForTimeout(500);await cdp.send('Page.stopScreencast');
    await Promise.all(pending);
    const last=String(active.frames.length).padStart(5,'0')+'.jpg';
    await page.screenshot({path:active.path+'/'+last,type:'jpeg',quality:94});
    const now=await page.evaluate(()=>Date.now()/1000);
    active.frames.push({file:last,time:now});
    await fs.writeFile(active.path+'/frames.json',JSON.stringify(active.frames,null,2));
    console.log('Captured',active.id,active.frames.length,'frames');active=null;
  }
  async function click(x,y){await page.mouse.move(x,y,{steps:20});await page.waitForTimeout(220);await page.mouse.down();await page.waitForTimeout(120);await page.mouse.up();await page.waitForTimeout(450)}
  async function text(s){for(const char of s){await page.keyboard.insertText(char);await page.waitForTimeout(65)}}
  async function wait(ms=1000){await page.waitForTimeout(ms)}
  return {start,stop,click,text,wait,cursor};
};
