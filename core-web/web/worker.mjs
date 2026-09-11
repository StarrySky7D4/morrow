const parameters=new URL(self.location.href).searchParams;
let core;
let faultPoint;
self.__morrowFaultBoundary=name=>{if(name===faultPoint){self.postMessage({boundary:name});for(;;){/* Wait for parent termination at this exact boundary. */}}};
if(parameters.has('unsupported'))Object.defineProperty(navigator,'storage',{value:undefined});
let store;
const db=parameters.get('db')??'qualification';
const ready=(async()=>{core=await import(parameters.has('fault')?'./fault/morrow_web_core.js':'./main/morrow_web_core.js');await core.default();if(!navigator.storage?.getDirectory)throw Error('OPFS unavailable');await core.install_opfs();store=new core.BrowserStore(db,true,1024);})();
ready.catch(()=>{}); // The queued request reports initialization failures.
let chain=Promise.resolve();
self.onmessage=({data})=>{chain=chain.then(async()=>{
 try{
  await ready;
  if(data instanceof ArrayBuffer){if(data.byteLength>65536)throw Error('Message limit');const reply=store.dispatch(new Uint8Array(data));self.postMessage(reply.buffer,[reply.buffer]);return;}
  if(typeof data==='string'&&data.startsWith('arm:')){faultPoint=data.slice(4);self.postMessage('armed');return;}
  let result;
  switch(data){
   case 'high-seed':{const bytes=new Uint8Array(await(await fetch('./vectors/high.morrow')).arrayBuffer());store.import_card('import-high',bytes);store.grant_rename('high',60000);result='high-ready';break;}
   case 'export-high':{const bytes=store.export_card('high');self.postMessage(bytes.buffer,[bytes.buffer]);return;}
   case 'blob-count':result=store.first_blob_page_count();break;
   case 'stage-only':{const bytes=Uint8Array.from({length:100000},(_,i)=>i&255);const id=store.stage(bytes,0n);const raw=store.export_blob(id);if(raw.length!==bytes.length||raw.some((v,i)=>v!==bytes[i]))throw Error('Staged raw mismatch');result=id;break;}
   case 'seed': store.create_local('create','card','old');store.grant_rename('card',60000);result='ready';break;
   case 'read':result=store.card_title('card');break;
   case 'check':store.check();result='ok';break;
   case 'second-connection':{let denied=false;try{new core.BrowserStore('another',true,10);}catch(error){if(!String(error).includes('already open'))throw error;denied=true;}if(!denied)throw Error('Second store accepted');store.check();result='single-owner';break;}
   case 'grant-read':store.grant_read('card',60000);result='read-granted';break;
   case 'revoke-read':store.revoke_read('card');result='read-revoked';break;
   case 'expire-read':store.grant_read('card',1);await new Promise(r=>setTimeout(r,30));result='read-expired';break;
   case 'publish':{const id=store.stage(Uint8Array.from({length:100000},(_,i)=>i&255),0n);result=store.create_attachment('publish','files',id);break;}
   case 'remove':result=store.clear_attachments('remove','files',1n);break;
   case 'publish-state':result=store.lookup_revision('publish')===undefined?'absent':store.attachment_count('files');break;
   case 'remove-state':result=store.lookup_revision('remove')===undefined?'absent':store.attachment_count('files');break;
   case 'gc-seed':{const id=store.stage(Uint8Array.from({length:100000},(_,i)=>i&255),0n);store.retire(id,2n);result='retired';break;}
   case 'gc-run':result=store.collect(60002n);break;
   case 'grant':store.grant_rename('card',60000);result='granted';break;
   case 'revoke':store.revoke_rename('card');result='revoked';break;
   case 'expiry':store.grant_rename('card',1);await new Promise(r=>setTimeout(r,30));result='expired';break;
   case 'attachments':{
    const bytes=Uint8Array.from({length:100000},(_,i)=>(i*37)&255);const id=store.stage(bytes,0n);
    if(store.stage(bytes,1n)!==id)throw Error('Blob dedup failed');
    store.create_attachment('attach','files',id);store.clear_attachments('clear','files',1n);
    let retained=false;try{store.retire(id,2n);}catch(error){if(!String(error).includes('Retained'))throw error;retained=true;}if(!retained)throw Error('History payload was retired');
    const restored=store.export_blob(id);if(restored.length!==bytes.length||restored.some((v,i)=>v!==bytes[i]))throw Error('Raw bytes changed');
    if(store.collect(60002n)!==0)throw Error('Live payload collected');store.check();result='attachments-ok';break;
   }
   case 'capacity':{
    store.free();store=new core.BrowserStore('capacity',true,1);store.create_local('only','card','only');store.grant_rename('card',60000);
    const request=core.rename_encode('over','card',1n,'must rollback');let rejected=false;try{store.rename(request);}catch(error){if(!String(error).includes('Capacity'))throw error;rejected=true;}
    if(!rejected||store.card_revision('card')!==1n||store.lookup_revision('over')!==undefined)throw Error('Capacity boundary failed');store.check();result='capacity-ok';break;
   }
   case 'close':store.free();store=undefined;result='closed';break;
   default:throw Error('Unknown test control');
  }self.postMessage(result);
 }catch(error){self.postMessage({error:String(error)});}
});};
