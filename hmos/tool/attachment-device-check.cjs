// Dev12 product checks use observed app/picker controls; interrupted stages
// resume only after inspecting the saved result. Never reseed named cards.
process.env.HMOS_DEVICE=process.env.HMOS_DEVICE||'127.0.0.1:5557';
process.env.HMOS_REPORT_DIR=process.env.HMOS_REPORT_DIR||require('node:path').resolve(__dirname,'../reports/ui-source/v12/device');
const f=require('./card-workflow-device-check.cjs'),assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const name='HMOS-attachment-20261005-A';
function pickerTree(){const raw=f.d.run('layout','--format','json');const tree=JSON.parse(raw.slice(raw.indexOf('[')));return f.d.flatten(tree);}
function pickerCapture(label){const tree=pickerTree();fs.writeFileSync(path.join(f.out,label+'.json'),JSON.stringify(tree.map(x=>x.n),null,2)+'\n');f.d.run('screenshot','--path',path.join(f.out,label+'.png'));return tree;}
function seed(){f.dismissNotice();f.search(name);assert.equal(f.cards().length,0,'no replay of existing fixture');f.click(f.text('新建灵感'));
 f.input('draft-title',name);f.input('draft-description','# Attachment fixture\n\nPreserve 中文😀 and the existing card identity.');f.click(f.text('保存灵感'));assert.ok(!f.id('draft-title'));f.dismissNotice();
 f.click(f.seek(n=>n.type==='Text'&&n.text===name,name));f.click(f.id('detail-edit'));assert.equal(f.id('draft-title').text,name);f.capture('editor-before-picker');f.pass('unique existing-card fixture prepared through actual editor');}
function open(){f.click(f.id('editor-import'));pickerCapture('document-picker');}
function record(label,details={}){const file=path.join(f.out,'release-checks.json');const prior=fs.existsSync(file)?JSON.parse(fs.readFileSync(file,'utf8')):{device:process.env.HMOS_DEVICE,fixture:name,checks:[]};assert.ok(!prior.checks.some(x=>x.label===label),'never replay a completed stage');prior.checks.push({label,details});fs.writeFileSync(file,JSON.stringify(prior,null,2)+'\n');console.log('PASS '+label);}
function pickerText(value){return pickerTree().find(x=>x.n.text===value&&x.parents.some(p=>p.type==='SheetPage'))?.n;}
function restore(){assert.ok(f.text('保留的草稿'));const title=f.view().find(x=>x.n.text===name&&x.parents.some(p=>p.id?.startsWith('draft-row:')));assert.ok(title);const row=title.parents.find(p=>p.id?.startsWith('draft-row:'));f.click(f.d.flatten([row]).find(x=>x.n.text==='恢复编辑').n);
 assert.equal(f.id('draft-title').text,name);assert.equal(f.nodes().filter(n=>n.id?.startsWith('draft-asset:')).length,1);assert.ok(f.text('草稿已保留'));f.capture('release-restored-pin');
 f.click(f.text('预览'));assert.ok(f.id('verified-attachment-image'));f.capture('release-restored-pin-preview');f.click(f.text('关闭预览'));record('final HAP restores and previews retained image pin after process restart');}
function chooseDocument(){const filename='HMOS-dev12-20261005-中文 文档.txt';
 if(!pickerText(filename)){f.click(pickerText('浏览'));f.click(pickerText('我的手机'));f.click(pickerText('Download'));}
 pickerCapture('release-document-uri');f.click(pickerText(filename));
 for(let i=0;i<4&&!f.text('草稿已保留');i++)f.nodes();
 assert.equal(f.nodes().filter(n=>n.id?.startsWith('draft-asset:')).length,2);assert.ok(f.text('草稿已保留'));assert.ok(f.nodes().some(n=>n.text?.endsWith('38 B')));
 f.capture('release-two-draft-assets');record('final HAP imports actual Unicode document URI and retains two draft attachments');}
function publish(){assert.equal(f.id('draft-title')?.text,name);assert.equal(f.nodes().filter(n=>n.id?.startsWith('draft-asset:')).length,2);assert.ok(f.text('草稿已保留'));
 f.click(f.text('保存灵感'));for(let i=0;i<5&&f.id('draft-title');i++)f.nodes();assert.ok(!f.id('draft-title'),'confirmed publication closes consumed draft');f.dismissNotice();f.search(name);assert.equal(f.cards().length,1);
 const identity=f.cards()[0].id.slice('workspace-card:'.length);assert.equal(identity,'204235e9-2eb4-4b94-91d3-5a84ff881bf9');f.click(f.seek(n=>n.type==='Text'&&n.text===name,name));assert.equal(f.id('detail-title').text,name);
 assert.equal(f.nodes().filter(n=>n.id?.startsWith('saved-asset:')).length,2);assert.ok(f.text('Preserve 中文😀 and the existing card identity.'));f.capture('release-published-two-assets');record('final HAP publishes two attachments to the original card identity and retains Markdown body',{card_id:identity});}
function saveExportAs(originalName,target,label){assert.match(target,/^HMOS-dev12-exported-[A-Za-z0-9.-]+$/);
 let tree=pickerTree();const folder=tree.find(x=>x.n.id==='Download'&&x.n.type==='Row');assert.ok(folder);f.click(folder.n);
 tree=pickerTree();const input=tree.find(x=>x.n.type==='TextInput'&&x.n.text===originalName);assert.ok(input);f.click(input.n);f.key(2072,2017);
 const cp=require('node:child_process'),hdc='C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/toolchains/hdc.exe';
 cp.execFileSync(hdc,['-t',process.env.HMOS_DEVICE,'shell','uitest','uiInput','text',target]);f.key('Back');
 assert.ok(pickerTree().some(x=>x.n.type==='TextInput'&&x.n.text===target));pickerCapture('release-save-'+label+'-target');
 f.click(pickerTree().find(x=>x.n.id==='dialog_confirm').n);assert.ok(f.text('附件已导出。'));f.capture('release-exported-'+label);record('final HAP exports '+label+' through actual save URI',{filename:target});}
function compareExports(){const crypto=require('node:crypto'),dir=path.resolve(__dirname,'../reports/ui-source/v12/fixtures');const pairs=[['HMOS-dev12-20261005-中文 图片.png','HMOS-dev12-exported-image-20261005-A.png'],['HMOS-dev12-20261005-中文 文档.txt','HMOS-dev12-exported-document-20261005-A.txt']];
 const files=pairs.map(([source,target])=>{const original=fs.readFileSync(path.join(dir,source)),returned=fs.readFileSync(path.join(dir,target));assert.deepEqual(returned,original);return {source,target,bytes:original.length,sha256:crypto.createHash('sha256').update(returned).digest('hex')};});
 fs.writeFileSync(path.join(f.out,'export-byte-comparison.json'),JSON.stringify({result:'PASS',files},null,2)+'\n');record('actual provider image and document exports match original bytes and SHA256',{files});}
function cancelReadOnly(){assert.ok(f.id('card-detail'));f.click(f.id('detail-close'));f.restart();f.search(name);assert.equal(f.cards().length,1);f.click(f.seek(n=>n.type==='Text'&&n.text===name,name));assert.equal(f.nodes().filter(n=>n.id?.startsWith('saved-asset:')).length,2);f.capture('release-reopened-two-assets');
 f.click(f.id('detail-edit'));assert.equal(f.nodes().filter(n=>n.id?.startsWith('draft-asset:')).length,2);f.click(f.id('editor-import'));pickerCapture('release-cancel-picker');f.key('Back');assert.equal(f.nodes().filter(n=>n.id?.startsWith('draft-asset:')).length,2);assert.ok(!f.id('draft-status'));f.capture('release-cancel-unchanged');
 f.click(f.text('保留草稿'));assert.ok(!f.id('draft-title'));f.assertNoDraft(name);record('final HAP reopens two published assets; cancelling chooser and unchanged editor creates no journal');
 f.dismissNotice();f.click(f.seek(n=>n.type==='Text'&&n.text===name,name));}
function finalRead(){f.search(name);assert.equal(f.cards().length,1);assert.equal(f.cards()[0].id,'workspace-card:204235e9-2eb4-4b94-91d3-5a84ff881bf9');f.click(f.seek(n=>n.type==='Text'&&n.text===name,name));
 assert.equal(f.nodes().filter(n=>n.id?.startsWith('saved-asset:')).length,2);assert.ok(f.text('TXT · 38 B'));assert.ok(f.text('PNG · 1.8 MiB'));f.capture('icons-final-two-assets');
 const image=f.view().find(x=>x.n.text==='HMOS-dev12-20261005-中文 图片.png'&&x.parents.some(p=>p.id?.startsWith('saved-asset:')));const id=image.parents.find(p=>p.id?.startsWith('saved-asset:')).id.slice('saved-asset:'.length);
 f.click(f.id('attachment-preview:'+id));assert.ok(f.id('verified-attachment-image'));f.capture('icons-final-image-preview');f.click(f.text('关闭预览'));record('final icon package reads published attachments and previews image',{built_hap_sha256:'07AE2562E04CBE189F04BFE1B60F1CC7458CDECBFE3ECF02F3BC0B7C2D83A467'});}
function removeOne(){assert.equal(f.nodes().filter(n=>n.id?.startsWith('saved-asset:')).length,2);f.click(f.id('detail-edit'));
 const image=f.view().find(x=>x.n.text==='HMOS-dev12-20261005-中文 图片.png'&&x.parents.some(p=>p.id?.startsWith('draft-asset:')));assert.ok(image);const id=image.parents.find(p=>p.id?.startsWith('draft-asset:')).id.slice('draft-asset:'.length);f.click(f.id('draft-asset-remove:'+id));
 assert.equal(f.nodes().filter(n=>n.id?.startsWith('draft-asset:')).length,1);assert.ok(f.text('TXT · 38 B'));f.capture('icons-final-remove-reference');f.click(f.text('保存灵感'));for(let i=0;i<5&&f.id('draft-title');i++)f.nodes();assert.ok(!f.id('draft-title'));f.dismissNotice();
 f.restart();f.search(name);assert.equal(f.cards().length,1);assert.equal(f.cards()[0].id,'workspace-card:204235e9-2eb4-4b94-91d3-5a84ff881bf9');f.click(f.seek(n=>n.type==='Text'&&n.text===name,name));
 assert.equal(f.nodes().filter(n=>n.id?.startsWith('saved-asset:')).length,1);assert.ok(f.text('TXT · 38 B'));assert.ok(!f.text('HMOS-dev12-20261005-中文 图片.png'));assert.ok(f.text('Preserve 中文😀 and the existing card identity.'));f.capture('icons-final-one-reference-reopened');record('final icon package removes only image reference; document and body survive business restart',{built_hap_sha256:'07AE2562E04CBE189F04BFE1B60F1CC7458CDECBFE3ECF02F3BC0B7C2D83A467'});}
if(require.main===module){if(process.argv.includes('--seed'))seed();else if(process.argv.includes('--open-picker'))open();else throw Error('explicit observed stage required');}
module.exports={f,name,pickerTree,pickerCapture,pickerText,record,restore,chooseDocument,publish,saveExportAs,compareExports,cancelReadOnly,finalRead,removeOne};
