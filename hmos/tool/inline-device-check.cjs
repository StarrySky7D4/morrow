// Runs observed stages against one new public development fixture; never
// silently replays a create, import or save after an interrupted observation.
process.env.HMOS_DEVICE=process.env.HMOS_DEVICE||'127.0.0.1:5555';
process.env.HMOS_REPORT_DIR=process.env.HMOS_REPORT_DIR||require('node:path').resolve(__dirname,'../reports/ui-source/v13/device');
const f=require('./card-workflow-device-check.cjs'),a=require('./attachment-device-check.cjs');
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const name='HMOS-inline-20261007-A',image='HMOS-dev13-中文 图片.png',invalid='HMOS-dev13-invalid.png';
const body=`# Inline attachment\n\n![名称](attachment:${encodeURIComponent(image)})\n\n![重复](attachment:${encodeURIComponent(image)})\n\n| Image |\n| --- |\n| ![表格](attachment:${encodeURIComponent(image)}) |\n\n![损坏](attachment:${invalid})\n\n![缺失](attachment:not-imported.png)\n\n![网络](https://example.invalid/image.png)`;
const checks=[];
function record(check,extra={}){const file=path.join(f.out,'progress-inline.json');const prior=fs.existsSync(file)?JSON.parse(fs.readFileSync(file,'utf8')):{name,device:process.env.HMOS_DEVICE,checks:[]};assert.equal(prior.name,name);assert.ok(!prior.checks.some(x=>x.check===check),'no completed stage replay');prior.checks.push({check,...extra});fs.writeFileSync(file,JSON.stringify(prior,null,2)+'\n');console.log('PASS '+check);}
function seed(){f.dismissNotice();f.search(name);assert.equal(f.cards().length,0,'fresh fixture only');f.assertNoDraft(name);f.click(f.text('新建灵感'));f.input('draft-title',name);f.input('draft-description',body);f.capture('raw-markdown-before-assets');record('unique raw Markdown fixture created without business publication');}
function editorSeek(predicate,label,up=false){for(let i=0;i<10;i++){const view=f.view();const item=view.find(x=>predicate(x.n)&&x.n.bounds[3]-x.n.bounds[1]>18);if(item)return item.n;
 const anchor=view.find(x=>x.n.id==='markdown-preview')||view.find(x=>x.n.id?.startsWith('draft-asset:'))||view.find(x=>x.n.id==='editor-import'||x.n.id==='draft-title'||x.n.id==='draft-description');
 const scroll=anchor?.parents.filter(p=>p.type==='Scroll').at(-1);assert.ok(scroll,'observed outer editor scroll '+label);const [l,t,r,b]=scroll.bounds;f.d.run('swipe',String(r-10),String(up?t+100:b-100),String(r-10),String(up?b-100:t+100));}throw Error('visible editor '+label);}
function assertEditor(){assert.equal(editorSeek(n=>n.id==='draft-title','fixture title',true).text,name);}
function pickerOwn(filename,label){assertEditor();f.click(f.seek(n=>n.id==='editor-import','import',false,'editor'));selectObservedPicker(filename,label);}
function selectObservedPicker(filename,label){
 assert.ok(a.pickerTree().some(x=>x.n.text==='仅可访问所选项目'),'already observed system picker only');
 const confirm=a.pickerTree().find(x=>x.n.text==='知道了');if(confirm)f.click(confirm.n);
 const browse=a.pickerTree().find(x=>x.n.text==='浏览');if(browse)f.click(browse.n);
 const phone=a.pickerTree().find(x=>x.n.text==='我的手机');if(phone)f.click(phone.n);
 const download=a.pickerTree().find(x=>x.n.text==='Download');if(download)f.click(download.n);
 const file=a.pickerText(filename);assert.ok(file,'exact own file');f.click(file);
 const selected=a.pickerTree().find(x=>x.n.id==='dialog_confirm');if(selected)f.click(selected.n); // Some single-selection views accept the clicked file directly.
 for(let i=0;i<6;i++)f.nodes();assert.ok(f.nodes().some(n=>n.id?.startsWith('draft-asset:')&&f.d.flatten([n]).some(x=>x.n.text===filename)));
 f.capture('imported-'+label);record('system URI imports '+label+' into confirmed draft');}
function preview(){assertEditor();f.click(editorSeek(n=>n.text==='\uf4a1','observed body preview glyph'));
 f.seek(n=>n.id?.startsWith('markdown-attachment-image:'),'inline image',false,'editor');f.capture('editor-inline-image');
 const imageId=f.nodes().find(n=>n.id?.startsWith('markdown-attachment-image:')).id.split(':')[1];
 f.click(f.seek(n=>n.id==='markdown-attachment-image:'+imageId,'inline click',false,'editor'));
 for(let i=0;i<4;i++)f.nodes();assert.ok(f.id('verified-attachment-image'));assert.ok(!f.id('attachment-image-decode-loading'));assert.ok(!f.id('attachment-image-decode-failure'));
 f.click(f.id('attachment-image-zoom-in'));assert.equal(f.id('attachment-image-zoom-reset')?.text,'125%');f.capture('image-zoom-125');
 f.click(f.id('attachment-image-zoom-reset'));assert.equal(f.id('attachment-image-zoom-reset')?.text,'100%');f.capture('image-zoom-reset');
 f.click(f.text('关闭预览'));record('draft inline image decodes, opens, zooms and resets without business publication',{asset_id:imageId});}
function publish(){assertEditor();f.click(f.seek(n=>n.text==='保存灵感','save',false,'editor'));for(let i=0;i<5&&f.id('draft-title');i++)f.nodes();assert.ok(!f.id('draft-title'));
 f.dismissNotice();f.search(name);assert.equal(f.cards().length,1);const identity=f.cards()[0].id;f.click(f.seek(n=>n.type==='Text'&&n.text===name,'published fixture'));
 f.seek(n=>n.id?.startsWith('markdown-attachment-image:'),'published inline',false,'detail');f.capture('published-inline');record('explicit save publishes exactly one card with inline attachment',{identity});}
function readonly(){assert.ok(f.id('card-detail'));const identities=new Set();
 for(let i=0;i<8;i++){for(const n of f.nodes()){if(n.id?.startsWith('markdown-attachment-image:'))identities.add(n.id);}
  const s=f.id('detail-scroll');assert.ok(s);f.d.run('swipe',String(s.bounds[2]-20),String(s.bounds[3]-100),String(s.bounds[2]-20),String(s.bounds[1]+100));}
 assert.equal(identities.size,1,'three references share exactly one image identity');
 f.seek(n=>n.text==='图片未导入 · 缺失','missing reference',false,'detail');f.capture('published-missing-and-remote');
 f.click(f.id('detail-close'));f.assertNoDraft(name);f.restart();f.assertNoDraft(name);f.search(name);assert.equal(f.cards().length,1);f.click(f.seek(n=>n.type==='Text'&&n.text===name,'restart fixture'));f.capture('reopened-inline');
 record('duplicate/table references share one verified image; missing and remote placeholders stay explicit; read-only restart creates no journal');}
if(require.main===module){if(process.argv.includes('--seed'))seed();else if(process.argv.includes('--choose-image'))selectObservedPicker(image,'image');else if(process.argv.includes('--import-image'))pickerOwn(image,'image');else if(process.argv.includes('--import-invalid'))pickerOwn(invalid,'invalid');else if(process.argv.includes('--preview'))preview();else if(process.argv.includes('--publish'))publish();else if(process.argv.includes('--readonly'))readonly();else throw Error('explicit observed stage required');}
module.exports={f,a,name,image,invalid,body,checks,record,seed,pickerOwn,preview,publish,readonly,editorSeek};
