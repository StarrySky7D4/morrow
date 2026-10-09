'use strict';
// Record actual product/fixture bytes read by the composed source tests.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),Module=require('node:module');
const root=path.resolve(__dirname,'../..'),read=fs.readFileSync,observed=new Map();
function eligible(file){if(typeof file!=='string')return '';const absolute=path.resolve(file),relative=path.relative(root,absolute).replace(/\\/g,'/');
 return relative&&!relative.startsWith('../')&&!path.isAbsolute(relative)&&/\.(ets|cjs|json)$/.test(relative)&&!relative.startsWith('hmos/reports/ui-source/v30/music-workbench-')?relative:'';}
function record(file,bytes){const relative=eligible(file);if(!relative)return;const b=Buffer.isBuffer(bytes)?bytes:Buffer.from(bytes),sha256=crypto.createHash('sha256').update(b).digest('hex');
 const previous=observed.get(relative);if(previous&&(previous.bytes!==b.length||previous.sha256!==sha256))throw Error('Actual test input changed during reads: '+relative);
 observed.set(relative,{path:relative,bytes:b.length,sha256});}
fs.readFileSync=function(file,...rest){const bytes=read.call(this,file,...rest);record(file,bytes);return bytes;};
const js=Module._extensions['.js'];Module._extensions['.js']=function(module,file){if(eligible(file))record(file,read(file));return js(module,file);};
record(__filename,read(__filename));
process.on('exit',()=>{if(process.env.MUSIC_WORKBENCH_TRACE)fs.appendFileSync(process.env.MUSIC_WORKBENCH_TRACE,JSON.stringify({pid:process.pid,files:[...observed.values()].sort((a,b)=>a.path.localeCompare(b.path))})+'\n');});
