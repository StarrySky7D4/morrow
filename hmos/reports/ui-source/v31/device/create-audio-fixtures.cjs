'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../../..'),id=crypto.randomUUID().replaceAll('-','').slice(0,12),folder=path.join(root,'.build/device-fixtures/v31-'+id);
fs.mkdirSync(folder,{recursive:false});
function wave(seconds,frequency,name) {
  const rate=16000,count=rate*seconds,b=Buffer.alloc(44+count*2);
  b.write('RIFF');b.writeUInt32LE(b.length-8,4);b.write('WAVE',8);b.write('fmt ',12);b.writeUInt32LE(16,16);b.writeUInt16LE(1,20);b.writeUInt16LE(1,22);
  b.writeUInt32LE(rate,24);b.writeUInt32LE(rate*2,28);b.writeUInt16LE(2,32);b.writeUInt16LE(16,34);b.write('data',36);b.writeUInt32LE(count*2,40);
  for(let n=0;n<count;n++){const fade=Math.min(1,n/(rate*.05),(count-1-n)/(rate*.05));b.writeInt16LE(Math.round(6553*fade*Math.sin(2*Math.PI*frequency*n/rate)),44+n*2);}
  return save(name,b,{seconds,encoding:'RIFF/WAVE PCM16 mono 16000Hz; generated sine with fade; public non-sensitive test asset'});
}
function save(name,b,description) {const file=path.join(folder,name);fs.writeFileSync(file,b,{flag:'wx'});return {name,localPath:file,remotePath:'/data/service/el2/100/hmdfs/account/files/Docs/Download/'+name,bytes:b.length,sha256:crypto.createHash('sha256').update(b).digest('hex'),...description};}
const files=[wave(60,440,'HMOS-v31-'+id+'-long.wav'),wave(4,660,'HMOS-v31-'+id+'-short.wav'),
  save('HMOS-v31-'+id+'.lrc',Buffer.from('[00:00.00]起点 · HMOS v31\n[00:03.00]三秒 · 完整原文歌词\n[00:12.00]十二秒 · seek verification\n[00:45.00]四十五秒 · 后台返回不自动续播\n'),{encoding:'UTF-8 LRC with LF; generated public test asset'})];
fs.writeFileSync(path.join(__dirname,'audio-fixtures.json'),JSON.stringify({createdUtc:new Date().toISOString(),id,folder,files,scope:'Generated files only; no codec, speaker output or successful picker qualification'},null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({id,folder,files}));
