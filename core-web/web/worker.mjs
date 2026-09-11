const parameters=new URL(self.location.href).searchParams;
let core;
let faultPoint;
let sqlDiagnostic;
let poolDiagnostic;
self.__morrowFaultBoundary=name=>{if(name.startsWith('pool:'))poolDiagnostic=name;if(name.startsWith('sqlite-error:'))sqlDiagnostic=name;if(name===faultPoint){self.postMessage({boundary:name});for(;;){/* Wait for parent termination at this exact boundary. */}}};
if(parameters.has('unsupported'))Object.defineProperty(navigator,'storage',{value:undefined});
let store;
const db=parameters.get('db')??'qualification';
const ready=(async()=>{core=await import(parameters.has('fault')?'./fault/morrow_web_core.js':'./main/morrow_web_core.js');await core.default();if(!navigator.storage?.getDirectory)throw Error('OPFS unavailable');if(parameters.has('recordkind')&&parameters.has('fault'))await core.install_record_test_opfs();else await core.install_opfs();store=new core.BrowserStore(db,true,1024);})();
ready.catch(()=>{}); // The queued request reports initialization failures.
let chain=Promise.resolve();
self.onmessage=({data})=>{chain=chain.then(async()=>{
 try{
  await ready;
  if(data instanceof ArrayBuffer){if(data.byteLength>65536)throw Error('Message limit');const reply=store.dispatch(new Uint8Array(data));self.postMessage(reply.buffer,[reply.buffer]);return;}
  if(typeof data==='string'&&data.startsWith('arm:')){faultPoint=data.slice(4);self.postMessage('armed');return;}
  let result;
  switch(data){
   case 'attachment-seed':{const bytes=Uint8Array.from({length:100000},(_,i)=>i%251);const id=store.stage(bytes,0n);store.create_attachment('attachment-seed','payload',id);result='attachment-ready';break;}
   case 'attachment-grant':store.grant_attachment('payload','file',60000);result='granted';break;
   case 'attachment-revoke':store.revoke_attachment('payload','file');result='revoked';break;
   case 'attachment-expire':store.grant_attachment('payload','file',1);await new Promise(r=>setTimeout(r,30));result='expired';break;
   case 'attachment-change':store.grant_rename('payload',60000);store.rename(core.rename_encode('attachment-change','payload',1n,'changed'));store.revoke_rename('payload');result='changed';break;
   case 'records':{
    const before=store.export_card('card');
    for(const id of ['w1','w2']){store.workspace_local('workspace-'+id,id,0n,id);store.placement_local('place-'+id,id,id,'card',-5n);}
    for(let i=0;i<2;i++)store.layout_local('layout','w1',1n,7n,true,3);
    store.draft_local('draft-create','draft',before,new Uint8Array([0,255,1]));
    for(let i=0;i<2;i++)store.draft_save_local('draft-save','draft',1n,new Uint8Array([3,2,1]));
    const after=store.export_card('card');if(after.length!==before.length||after.some((v,i)=>v!==before[i]))throw Error('Records changed card');
    if(store.record_revision_local(2,'w1')!==2n||store.record_revision_local(2,'w2')!==1n||store.record_revision_local(3,'draft')!==2n)throw Error('Independent revisions');
    let conflict=false;try{store.draft_save_local('stale','draft',1n,new Uint8Array([9]));}catch(e){if(!String(e).includes('RevisionConflict'))throw e;conflict=true;}if(!conflict)throw Error('Stale draft overwrite');
    store.grant_rename('card',60000);store.rename(core.rename_encode('record-base-change','card',store.card_revision('card'),'base updated'));store.revoke_rename('card');
    if(store.draft_local('draft-create','draft',before,new Uint8Array([0,255,1]))!==1n)throw Error('Draft creation retry changed with base');
    store.grant_rename('card',60000);store.rename(core.rename_encode('record-title-restore','card',store.card_revision('card'),'普通 JS 已提交 🧭'));store.revoke_rename('card');
    store.check();result='records-ok';break;
   }
   case 'records-restored':if(store.record_revision_local(1,'w1')!==1n||store.record_revision_local(2,'w1')!==2n||store.record_revision_local(3,'draft')!==2n)throw Error('Lost record after reopen');result='records-restored';break;
   case 'record-seed':store.create_local('card','card','original');store.workspace_local('w','w',0n,'old');store.placement_local('p','p','w','card',0n);store.draft_local('d','d',store.export_card('card'),new Uint8Array([0]));result='seeded';break;
   case 'record-write':switch(parameters.get('recordkind')){case '1':result=store.workspace_local('edit','w',1n,'new');break;case '2':result=store.layout_local('edit','p',1n,3n,true,2);break;case '3':result=store.draft_save_local('edit','d',1n,new Uint8Array([1,2,3]));break;default:throw Error('Record kind');}break;
   case 'record-state':{const kind=Number(parameters.get('recordkind'));result=store.record_revision_local(kind,kind===1?'w':kind===2?'p':'d');break;}
   case 'high-seed':{const bytes=new Uint8Array(await(await fetch('./vectors/high.morrow')).arrayBuffer());store.import_card('import-high',bytes);store.grant_rename('high',60000);result='high-ready';break;}
   case 'export-high':{const bytes=store.export_card('high');self.postMessage(bytes.buffer,[bytes.buffer]);return;}
   case 'blob-count':result=store.first_blob_page_count();break;
   case 'stage-only':{const bytes=Uint8Array.from({length:100000},(_,i)=>i&255);const id=store.stage(bytes,0n);const raw=store.export_blob(id);if(raw.length!==bytes.length||raw.some((v,i)=>v!==bytes[i]))throw Error('Staged raw mismatch');result=id;break;}
   case 'seed': store.create_local('create','card','old');store.grant_rename('card',60000);result='ready';break;
   case 'read':result=store.card_title('card');break;
   case 'check':store.check();result='ok';break;
   case 'second-connection':{let denied=false;try{new core.BrowserStore('another',true,10);}catch(error){if(!String(error).includes('already open'))throw error;denied=true;}if(!denied)throw Error('Second store accepted');store.check();result='single-owner';break;}
   case 'grant-query':store.grant_query('card',60000);result='query-granted';break;
   case 'revoke-query':store.revoke_query('card');result='query-revoked';break;
   case 'expire-query':store.grant_query('card',1);await new Promise(r=>setTimeout(r,30));result='query-expired';break;
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
 }catch(error){self.postMessage({error:String(error)+(sqlDiagnostic?' ['+sqlDiagnostic+']':'')+(poolDiagnostic?' ['+poolDiagnostic+']':'')});}
});};
