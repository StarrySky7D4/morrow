// Device-local identity custody, independent of where application services run.
// This module has no network calls. A non-extractable WebCrypto wrapping key is
// stored by IndexedDB; this is browser-origin protection, not Windows DPAPI or
// a claim of hardware protection against code executing in the same origin.
const DATABASE = 'morrow-device-identities-v1';
const STORE = 'libraries';
const PREFIX = Uint8Array.of(0x30,0x2e,0x02,0x01,0x00,0x30,0x05,0x06,0x03,0x2b,0x65,0x70,0x04,0x22,0x04,0x20);
// RFC 8410 Ed25519 PKCS#8: 16-byte header, followed by the 32-byte seed.
const equal = (a,b) => a.length === b.length && a.every((v,i)=>v===b[i]);
const hex = bytes => [...bytes].map(v=>v.toString(16).padStart(2,'0')).join('');
function nameOf(name) {
  if (typeof name !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(name)) throw Error('Invalid local library name');
  return name;
}
function validate(record, name) {
  if (!record || record.version !== 1 || record.name !== nameOf(name) ||
      !['prepared','ready'].includes(record.phase) || typeof record.logId !== 'string' ||
      !/^morrow-[a-f0-9-]{36}$/.test(record.logId) ||
      !(record.publicKey instanceof Uint8Array) || record.publicKey.length !== 32 ||
      !(record.iv instanceof Uint8Array) || record.iv.length !== 12 ||
      !(record.sealed instanceof Uint8Array) || record.sealed.length !== PREFIX.length+32+16 ||
      !(record.wrappingKey instanceof CryptoKey) || record.wrappingKey.extractable ||
      record.wrappingKey.type !== 'secret' || record.wrappingKey.algorithm.name !== 'AES-GCM' ||
      record.wrappingKey.algorithm.length !== 256 ||
      record.wrappingKey.usages.length !== 2 || !record.wrappingKey.usages.includes('encrypt') || !record.wrappingKey.usages.includes('decrypt')) {
    throw Error('Local identity is missing, damaged, or unsupported; recovery is required');
  }
  return record;
}
function publicInfo(record) {
  return {name:record.name, logId:record.logId, publicKey:record.publicKey.slice(), phase:record.phase};
}
function aad(record) {
  return new TextEncoder().encode(JSON.stringify(['morrow-device-identity',1,record.name,record.logId,hex(record.publicKey),record.phase]));
}
function openDatabase() {
  return new Promise((resolve,reject)=>{
    let rejected=false;
    const request=indexedDB.open(DATABASE,1);
    request.onupgradeneeded=event=>{
      if(event.oldVersion !== 0 || request.result.objectStoreNames.length !== 0) {
        request.transaction.abort(); return;
      }
      request.result.createObjectStore(STORE,{keyPath:'name'});
    };
    request.onblocked=()=>{rejected=true;reject(Error('Local identity database is busy'));};
    request.onerror=()=>reject(Error('Cannot open local identity database'));
    request.onsuccess=()=>{
      const db=request.result;
      if(rejected){db.close();return;}
      if(db.objectStoreNames.length!==1 || !db.objectStoreNames.contains(STORE)) {
        db.close();reject(Error('Unsupported local identity database'));return;
      }
      db.onversionchange=()=>db.close();
      resolve(db);
    };
  });
}
async function transaction(mode, body) {
  const db=await openDatabase();
  try {
    return await new Promise((resolve,reject)=>{
      const tx=db.transaction(STORE,mode,{durability:'strict'});
      let result, failure;
      tx.oncomplete=()=>resolve(result);
      tx.onabort=()=>reject(failure ?? Error('Local identity transaction failed'));
      tx.onerror=()=>{}; // Abort is the authoritative failed transaction result.
      const finish=value=>{result=value;};
      const fail=error=>{failure=error;tx.abort();};
      try {body(tx.objectStore(STORE),finish,fail);} catch(error){fail(error);}
    });
  } finally {db.close();}
}
async function read(name, allowMissing=false) {
  nameOf(name);
  const record=await transaction('readonly',(store,finish)=>{
    const request=store.get(name);request.onsuccess=()=>finish(request.result);
  });
  if (record === undefined && allowMissing) return null;
  return validate(record,name);
}
async function encrypt(record, pkcs8) {
  record.iv=crypto.getRandomValues(new Uint8Array(12));
  record.sealed=new Uint8Array(await crypto.subtle.encrypt({name:'AES-GCM',iv:record.iv,additionalData:aad(record),tagLength:128},record.wrappingKey,pkcs8));
}
async function unlock(record) {
  let pkcs8;
  try {
    pkcs8=new Uint8Array(await crypto.subtle.decrypt({name:'AES-GCM',iv:record.iv,additionalData:aad(record),tagLength:128},record.wrappingKey,record.sealed));
    if(pkcs8.length!==PREFIX.length+32 || !equal(pkcs8.subarray(0,PREFIX.length),PREFIX)) throw Error('Unsupported private key format');
    const key=await crypto.subtle.importKey('pkcs8',pkcs8,'Ed25519',false,['sign']);
    const publicKey=await crypto.subtle.importKey('raw',record.publicKey,'Ed25519',false,['verify']);
    const challenge=crypto.getRandomValues(new Uint8Array(32));
    const signature=await crypto.subtle.sign('Ed25519',key,challenge);
    if(!await crypto.subtle.verify('Ed25519',publicKey,signature,challenge)) throw Error('Identity key mismatch');
    return pkcs8;
  } catch {
    pkcs8?.fill(0);
    throw Error('Local identity cannot be unlocked; recovery is required');
  }
}

/// Explicit creation only. Concurrent creation uses add(), never overwrite.
export async function createDeviceIdentity(name) {
  nameOf(name);
  const pair=await crypto.subtle.generateKey('Ed25519',true,['sign','verify']);
  const pkcs8=new Uint8Array(await crypto.subtle.exportKey('pkcs8',pair.privateKey));
  try {
    if(pkcs8.length!==PREFIX.length+32 || !equal(pkcs8.subarray(0,PREFIX.length),PREFIX)) throw Error('Unsupported private key format');
    const record={version:1,name,logId:`morrow-${crypto.randomUUID()}`,phase:'prepared',
      publicKey:new Uint8Array(await crypto.subtle.exportKey('raw',pair.publicKey)),
      wrappingKey:await crypto.subtle.generateKey({name:'AES-GCM',length:256},false,['encrypt','decrypt'])};
    await encrypt(record,pkcs8);
    await transaction('readwrite',(store,finish)=>{
      const request=store.add(record);request.onsuccess=()=>finish(null);
    });
    return publicInfo(record);
  } finally {pkcs8.fill(0);}
}

/// Missing identity never falls back to creating a new key or an empty library.
export async function inspectDeviceIdentity(name) {
  const record=await read(name);const pkcs8=await unlock(record);
  try {return publicInfo(record);} finally {pkcs8.fill(0);}
}

// The managed browser workspace may be created only when both its identity
// and the application OPFS pool are absent. Losing IndexedDB alone must never
// hide existing content behind a new identity.
export async function inspectDeviceLibrary(name) {
  const record=await read(name,true);
  if(record) {
    const pkcs8=await unlock(record);
    try { return record.phase; } finally { pkcs8.fill(0); }
  }
  const root=await navigator.storage.getDirectory();
  let directory;
  try { directory=await root.getDirectoryHandle('morrow-workbench-v1'); }
  catch(error) { if(error?.name==='NotFoundError') return 'empty'; throw error; }
  for await (const entry of directory.values()) { if(entry) return 'orphaned'; }
  return 'empty';
}

/// For trusted local consumers only. Never send this seed through a cloud API.
/// Mutable temporary bytes are cleared on success and failure; browser-managed
/// crypto and structured-clone copies are outside JavaScript's erasure guarantee.
export async function withDeviceIdentity(name, consume) {
  const record=await read(name);const pkcs8=await unlock(record);
  const seed=pkcs8.slice(PREFIX.length);
  try {return await consume({...publicInfo(record),seed});}
  finally {seed.fill(0);pkcs8.fill(0);}
}

/// Call only after the content store has durably bound this exact identity.
/// A prepared record survives an interrupted initialization for explicit recovery.
export async function markDeviceIdentityReady(expected) {
  const record=await read(expected.name);
  if(record.logId!==expected.logId || !(expected.publicKey instanceof Uint8Array) || !equal(record.publicKey,expected.publicKey)) throw Error('Selected identity changed');
  const pkcs8=await unlock(record);
  try {
    if(record.phase==='ready') return publicInfo(record);
    const next={...record,phase:'ready'};
    await encrypt(next,pkcs8);
    await transaction('readwrite',(store,finish,fail)=>{
      const request=store.get(record.name);
      request.onsuccess=()=>{
        try {
          const current=validate(request.result,record.name);
          if(current.logId!==record.logId || current.phase!==record.phase || !equal(current.publicKey,record.publicKey) || !equal(current.iv,record.iv) || !equal(current.sealed,record.sealed)) throw Error('Local identity changed during initialization');
          const update=store.put(next);update.onsuccess=()=>finish(null);
        } catch(error){fail(error);}
      };
    });
    return publicInfo(next);
  } finally {pkcs8.fill(0);}
}
