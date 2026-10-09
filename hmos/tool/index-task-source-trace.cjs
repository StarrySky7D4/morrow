'use strict';
// Observe actual repository input bytes of the composed Index source tests.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),Module=require('node:module');
const root=path.resolve(__dirname,'../..'),originalRead=fs.readFileSync,observed=new Map();
function relative(file){
 if(typeof file!=='string')return '';
 const name=path.relative(root,path.resolve(file)).replace(/\\/g,'/');
 return name&&!name.startsWith('../')&&!path.isAbsolute(name)&&/\.(ets|cjs|json)$/.test(name)?name:'';
}
function record(file,bytes){
 const name=relative(file);if(!name)return;
 const input=Buffer.isBuffer(bytes)?bytes:Buffer.from(bytes),item={path:name,bytes:input.length,sha256:crypto.createHash('sha256').update(input).digest('hex').toUpperCase()};
 const prior=observed.get(name);if(prior&&(prior.bytes!==item.bytes||prior.sha256!==item.sha256))throw Error('Input changed during actual reads: '+name);
 observed.set(name,item);
}
fs.readFileSync=function(file,...args){const bytes=originalRead.call(this,file,...args);record(file,bytes);return bytes;};
const originalExtension=Module._extensions['.js'];
Module._extensions['.js']=function(module,file){if(relative(file))record(file,originalRead(file));return originalExtension(module,file);};
record(__filename,originalRead(__filename));
process.on('exit',()=>{if(process.env.INDEX_TASK_SOURCE_TRACE)fs.appendFileSync(process.env.INDEX_TASK_SOURCE_TRACE,
 JSON.stringify({pid:process.pid,files:[...observed.values()].sort((a,b)=>a.path.localeCompare(b.path))})+'\n');});
