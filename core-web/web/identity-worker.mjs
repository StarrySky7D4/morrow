// Qualification only, served from a fresh browser profile by the local runner.
import {createDeviceIdentity, inspectDeviceIdentity, inspectDeviceLibrary, withDeviceIdentity, markDeviceIdentityReady} from './device-identity.mjs';
const DB='morrow-device-identities-v1';
const equal=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
const assert=(ok,message)=>{if(!ok)throw Error(message);};
async function rejects(action,label) {
  let failed=false;try{await action();}catch{failed=true;}
  assert(failed,label+' unexpectedly succeeded');
}
async function raw(name, value) {
  const db=await new Promise((resolve,reject)=>{const r=indexedDB.open(DB);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
  try{return await new Promise((resolve,reject)=>{
    const tx=db.transaction('libraries',value?'readwrite':'readonly');
    const r=value?tx.objectStore('libraries').put(value):tx.objectStore('libraries').get(name);
    tx.oncomplete=()=>resolve(r.result);tx.onabort=()=>reject(tx.error);
  });}finally{db.close();}
}
async function signAndErase(name) {
  let alias;
  await withDeviceIdentity(name,async identity=>{
    alias=identity.seed;
    const pkcs8=new Uint8Array(48);
    pkcs8.set([0x30,0x2e,0x02,0x01,0x00,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x04,0x22,0x04,0x20]);
    pkcs8.set(alias,16);
    const privateKey=await crypto.subtle.importKey('pkcs8',pkcs8,'Ed25519',false,['sign']);pkcs8.fill(0);
    const publicKey=await crypto.subtle.importKey('raw',identity.publicKey,'Ed25519',false,['verify']);
    const message=new TextEncoder().encode('device-local qualification');
    const signature=await crypto.subtle.sign('Ed25519',privateKey,message);
    assert(await crypto.subtle.verify('Ed25519',publicKey,signature,message),'signature mismatch');
  });
  assert(alias.every(v=>v===0),'successful consumer retained seed');
  await rejects(()=>withDeviceIdentity(name,identity=>{alias=identity.seed;throw Error('consumer failed');}),'consumer failure');
  assert(alias.every(v=>v===0),'failed consumer retained seed');
}
self.onmessage=async({data})=>{
  try {
    if(data.kind==='create') {
      assert(await inspectDeviceLibrary('main')==='empty','fresh workspace not empty');
      await rejects(()=>inspectDeviceIdentity('missing'),'missing identity');
      assert(await raw('missing')===undefined,'inspection created an identity');
      const races=await Promise.allSettled([createDeviceIdentity('main'),createDeviceIdentity('main')]);
      assert(races.filter(r=>r.status==='fulfilled').length===1,'duplicate create did not fail atomically');
      const selected=races.find(r=>r.status==='fulfilled').value;
      assert(selected.phase==='prepared','new identity is not prepared');
      assert(await inspectDeviceLibrary('main')==='prepared','interrupted identity not discoverable');
      const record=await raw('main');
      assert(record.wrappingKey instanceof CryptoKey&&!record.wrappingKey.extractable,'wrapping key is extractable');
      assert(record.sealed.length===64&&!('seed' in record)&&!('privateKey' in record),'raw private identity persisted');
      await rejects(()=>crypto.subtle.exportKey('raw',record.wrappingKey),'wrapping key export');
      await signAndErase('main');
      await rejects(()=>markDeviceIdentityReady({...selected,logId:'morrow-'+crypto.randomUUID()}),'wrong identity admission');
      assert((await inspectDeviceIdentity('main')).phase==='prepared','failed admission changed phase');
      await createDeviceIdentity('interrupted');
      const ready=await markDeviceIdentityReady(selected);
      assert(ready.phase==='ready','identity not ready after binding');
      assert((await markDeviceIdentityReady(selected)).phase==='ready','repeat admission failed');
      assert(await inspectDeviceLibrary('main')==='ready','ready identity not discoverable');
      self.postMessage({ok:true,identity:ready});
    } else if(data.kind==='restore') {
      const selected=await inspectDeviceIdentity('main');
      assert(selected.logId===data.identity.logId&&equal(selected.publicKey,data.identity.publicKey),'new Worker opened a different identity');
      assert(selected.phase==='ready','ready phase not durable');
      assert((await inspectDeviceIdentity('interrupted')).phase==='prepared','interrupted initialization silently completed');
      await signAndErase('main');
      const root=await navigator.storage.getDirectory();
      const orphan=await root.getDirectoryHandle('morrow-workbench-v1',{create:true});
      await orphan.getFileHandle('fixture-orphan',{create:true});
      assert(await inspectDeviceLibrary('missing')==='orphaned','orphaned content allowed a new library');
      const record=await raw('main');
      const mutations=[
        r=>{r.sealed[0]^=1;}, r=>{r.publicKey[0]^=1;}, r=>{r.iv[0]^=1;},
        r=>{r.logId='morrow-'+crypto.randomUUID();}, r=>{r.phase='prepared';},
        r=>{delete r.wrappingKey;}, r=>{r.version=2;},
      ];
      for(const mutate of mutations) {
        const damaged=structuredClone(record);mutate(damaged);await raw('main',damaged);
        await rejects(()=>inspectDeviceIdentity('main'),'damaged identity');
        await rejects(()=>createDeviceIdentity('main'),'overwrite damaged identity');
        const after=await raw('main');
        assert(after.version===damaged.version&&after.logId===damaged.logId&&after.phase===damaged.phase&&equal(after.sealed,damaged.sealed)&&equal(after.publicKey,damaged.publicKey)&&equal(after.iv,damaged.iv)&&('wrappingKey' in after)===('wrappingKey' in damaged),'failure changed damaged record');
        // Restore only the exact saved fixture in this disposable test profile.
        await raw('main',record);
      }
      await signAndErase('main');
      const future=await new Promise((resolve,reject)=>{const r=indexedDB.open(DB,2);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});future.close();
      await rejects(()=>inspectDeviceIdentity('main'),'future database schema');
      const after=await raw('main');assert(equal(after.sealed,record.sealed),'future schema rejected destructively');
      self.postMessage({ok:true});
    } else throw Error('Unknown qualification stage');
  } catch(error) {self.postMessage({ok:false,error:error?.stack??String(error)});}
  finally{self.close();}
};
