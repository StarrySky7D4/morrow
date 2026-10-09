'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const out=__dirname,device='127.0.0.1:5555',cli=path.join(process.env.APPDATA,'npm/node_modules/@deveco/deveco-cli/dist/cli.js');
const hdcPath='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
const write=(name,value)=>fs.writeFileSync(path.join(out,name),JSON.stringify(value,null,2)+'\n',{flag:'wx'});
const hdc=(...args)=>cp.execFileSync(hdcPath,['-t',device,...args.map(String)],{encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});
const ui=(...args)=>cp.execFileSync(process.execPath,[cli,'ui',...args.map(String),'--device',device],{encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});
const flat=(nodes,parents=[])=>nodes.flatMap(n=>[{n,parents},...flat(n.children||[],[...parents,n])]);
function read(){const raw=ui('layout','--format','json'),tree=JSON.parse(raw.slice(raw.indexOf('[')));
 assert.ok(!flat(tree).some(x=>(x.n.type||'').startsWith('WindowScene')||x.n.id==='Paf_Permission_Sheet_Window_Builder'),'System or permission surface; inspect without another input');return tree;}
function capture(label){const tree=read();write(label+'.json',tree);ui('screenshot','--path',path.join(out,label+'.png'));
 console.log(JSON.stringify(flat(tree).filter(x=>x.n.id||x.n.type==='Button'||/未完整|原请求|已保存|重新检查/.test(x.n.text||''))
   .map(x=>({id:x.n.id,text:x.n.text,type:x.n.type,bounds:x.n.bounds})),null,2));return tree;}
function once(label,args){assert.match(label,/^[a-z0-9-]+$/);const file=path.join(out,'action-'+label+'.json');assert.ok(!fs.existsSync(file),'Do not replay an issued stage');
 const proof={issuedUtc:new Date().toISOString(),device,args,phase:'ISSUED_UNKNOWN'};write('action-'+label+'.json',proof);
 try {const result=hdc(...args);proof.result=result;proof.acknowledgedUtc=new Date().toISOString();
   assert.ok(/No Error|successfully/i.test(result),'Input/start not acknowledged');proof.phase='ACKNOWLEDGED';return result;
 } catch(error){proof.error=String(error);throw error;} finally {fs.writeFileSync(file,JSON.stringify(proof,null,2)+'\n');}}
function click(label,{id,text,type}={}){const tree=read(),matches=flat(tree).filter(x=>
 (!id||x.n.id===id)&&(!text||x.n.text===text)&&(!type||x.n.type===type)&&x.n.bounds&&x.n.bounds[3]>x.n.bounds[1]);
 assert.equal(matches.length,1,'Exactly one observed target required: '+JSON.stringify({id,text,type}));const [l,t,r,b]=matches[0].n.bounds;
 return once(label,['shell','uitest','uiInput','click',Math.round((l+r)/2),Math.round((t+b)/2)]);}
function input(label,id,value,{replace=false}={}){const tree=read(),matches=flat(tree).filter(x=>x.n.id===id);assert.equal(matches.length,1,'Observed input '+id);
 const n=matches[0].n;assert.ok(n.type==='TextInput'||n.type==='TextArea');const [l,t,r,b]=n.bounds;
 once(label+'-focus',['shell','uitest','uiInput','click',Math.round((l+r)/2),Math.round((t+b)/2)]);
 if(replace)once(label+'-select-all',['shell','uitest','uiInput','keyEvent','2072','2017']);
 else assert.equal(n.text||'','','Appending only to observed empty field');
 const local=path.join(out,label+'-input.sh'),remote='/data/local/tmp/morrow-v28-'+label+'-input.sh';
 fs.writeFileSync(local,"#!/bin/sh\nvalue=$(printf '%s' '"+Buffer.from(value).toString('base64')+"' | base64 -d; printf '.')\nexec uitest uiInput text \"${value%.}\"\n",{flag:'wx'});
 const sent=hdc('file','send',local,remote);assert.ok(/FileTransfer finish|success/i.test(sent),'Input file delivery not acknowledged');
 once(label+'-text',['shell','sh',remote]);const actual=flat(read()).find(x=>x.n.id===id)?.n.text;
 write(label+'-readback.json',{id,expected:value,actual,exact:actual===value});assert.equal(actual,value,'Exact displayed input must match; inspect without replay');}
module.exports={hdc,ui,flat,read,capture,once,click,input,out};
if(require.main===module)capture(process.argv[2]||'observed');
