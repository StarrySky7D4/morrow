import { UmLibrary } from './um-decrypt.mjs';
self.onmessage = async ({data}) => {
 let library;
 try {
  if (!(data.bytes instanceof Uint8Array) || !data.bytes.length || data.bytes.length > 150*1024*1024) {
   self.postMessage({code:10}); return;
  }
  library = await UmLibrary.load(new URL('./um_decrypt.wasm',import.meta.url));
  const result=library.decrypt(data.bytes,{format:data.format});
  const extensions=['','mp3','flac','ogg','m4a','wav','wma','dff','aac','ape'];
  self.postMessage({code:0,extension:extensions[result.info.audioFormat],bytes:result.bytes},[result.bytes.buffer]);
 } catch(error) {
  self.postMessage({code:Number.isInteger(error.code)?error.code:255});
 } finally { library?.dispose(); }
};
